//! One reader window per user, Windows session and profile. A later launch hands
//! its request to that window over a local named pipe and exits.

use std::fs::File;
use std::io::{self, Read, Write};
use std::path::{Path, PathBuf};
use std::sync::{Mutex, PoisonError};
use std::time::{Duration, Instant};

use iced_futures::futures::channel::mpsc::{UnboundedReceiver, UnboundedSender, unbounded};
use iced_futures::futures::{Stream, StreamExt, stream};
use sha2::{Digest, Sha256};

/// What another launch asks the running window to do.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Request {
    Raise,
    /// Open this path, then come to the front. Absolute when it came from
    /// another process: `claim` absolutizes it and `wire::decode` refuses
    /// relative paths.
    Open(PathBuf),
}

/// What `main` should do after `claim`.
#[derive(Debug, PartialEq, Eq)]
pub enum Claim {
    /// Run the reader. Forwards, if this process serves them, arrive through `forwarded`.
    Primary,
    /// The running window has the request; exit.
    Forwarded,
}

const CONNECT_TIMEOUT: Duration = Duration::from_millis(500);

static INBOX: Mutex<Option<UnboundedReceiver<Request>>> = Mutex::new(None);

/// Become the instance for this user, session and profile, or forward `path`
/// (or a focus request when `None`) to the one that already is. Every failure
/// along the way means "run normally": a launch is never blocked or lost.
pub fn claim(path: Option<&Path>) -> Claim {
    let Some(name) = PipeName::current() else {
        return Claim::Primary;
    };
    match listen(&name, true) {
        Ok(first) => {
            let (sender, receiver) = unbounded();
            let served = std::thread::Builder::new()
                .name("simPl single instance".into())
                .spawn(move || serve(name, first, sender));
            if served.is_ok() {
                *INBOX.lock().unwrap_or_else(PoisonError::into_inner) = Some(receiver);
            }
            Claim::Primary
        }
        Err(_) => {
            let request = match path {
                None => Some(Request::Raise),
                Some(path) => std::path::absolute(path).ok().map(Request::Open),
            };
            match request {
                Some(request) if forward(&name, &request, CONNECT_TIMEOUT).is_ok() => {
                    Claim::Forwarded
                }
                _ => Claim::Primary,
            }
        }
    }
}

/// Requests from later launches, for `Subscription::run`. Empty when this
/// process does not serve (an elevated instance holds the pipe, or setup failed).
pub fn forwarded() -> impl Stream<Item = Request> {
    let receiver = INBOX.lock().unwrap_or_else(PoisonError::into_inner).take();
    stream::iter(receiver).flatten()
}

#[derive(Clone)]
struct PipeName {
    text: String,
    wide: Vec<u16>,
}

impl PipeName {
    fn new(text: String) -> Self {
        let wide = text.encode_utf16().chain([0]).collect();
        Self { text, wide }
    }

    fn current() -> Option<Self> {
        let (sid, session) = win32::user_and_session().ok()?;
        let profile = std::path::absolute(reader_profile::storage_base()).ok()?;
        Some(Self::new(pipe_name(&sid, session, &profile)))
    }
}

/// Changing the frame format needs a new tag, so old and new builds never share a pipe.
const PIPE_VERSION: &[u8] = b"simPl.Reader.v1\0";

fn pipe_name(sid: &[u8], session: u32, profile: &Path) -> String {
    let mut hash = Sha256::new();
    hash.update(PIPE_VERSION);
    hash.update((sid.len() as u32).to_le_bytes());
    hash.update(sid);
    for unit in normalize_profile(profile) {
        hash.update(unit.to_le_bytes());
    }
    let digest = hash.finalize();
    let hex: String = digest[..16]
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect();
    format!(r"\\.\pipe\simPl.Reader.{session}.{hex}")
}

/// ASCII case folded, `/` as `\`, no trailing separator.
fn normalize_profile(profile: &Path) -> Vec<u16> {
    use std::os::windows::ffi::OsStrExt;
    let backslash = u16::from(b'\\');
    let mut units: Vec<u16> = profile
        .as_os_str()
        .encode_wide()
        .map(|unit| match u8::try_from(unit) {
            Ok(b'/') => backslash,
            Ok(byte) => u16::from(byte.to_ascii_lowercase()),
            Err(_) => unit,
        })
        .collect();
    while units.last() == Some(&backslash) {
        units.pop();
    }
    units
}

/// Keeps one instance listening at every moment: the next instance is created
/// before the connected one is read, so a newcomer sees ERROR_PIPE_BUSY (and
/// waits), never ERROR_FILE_NOT_FOUND (and starts a second window).
fn serve(name: PipeName, first: File, inbox: UnboundedSender<Request>) {
    let mut current = first;
    loop {
        let came = win32::accept(&current);
        let next = listen(&name, false);
        if came.is_ok()
            && let Some(request) = read_request(&current)
            && inbox.unbounded_send(request).is_err()
        {
            return;
        }
        drop(current);
        let Ok(next) = next else {
            return;
        };
        current = next;
    }
}

fn read_request(pipe: &File) -> Option<Request> {
    let mut frame = Vec::new();
    pipe.take(wire::MAX_FRAME as u64 + 1)
        .read_to_end(&mut frame)
        .ok()?;
    wire::decode(&frame)
}

/// One pipe instance. `first` adds FILE_FLAG_FIRST_PIPE_INSTANCE, so exactly one
/// process can create the first one; the others get ERROR_ACCESS_DENIED.
fn listen(name: &PipeName, first: bool) -> io::Result<File> {
    win32::create_pipe(name, first)
}

fn forward(name: &PipeName, request: &Request, timeout: Duration) -> io::Result<()> {
    let frame = wire::encode(request).ok_or(io::ErrorKind::InvalidInput)?;
    let deadline = Instant::now() + timeout;
    let mut pipe = loop {
        match win32::connect(name) {
            Ok(pipe) => break pipe,
            Err(error) if error.raw_os_error() == Some(win32::PIPE_BUSY) => {
                let left = deadline.saturating_duration_since(Instant::now());
                if left.is_zero() {
                    return Err(io::ErrorKind::TimedOut.into());
                }
                win32::wait(name, left)?;
            }
            Err(error) => return Err(error),
        }
    };
    if let Ok(server) = win32::server_process(&pipe) {
        win32::allow_foreground(server);
    }
    pipe.write_all(&frame)
}

mod wire {
    use super::Request;
    use std::os::windows::ffi::OsStrExt;
    use std::path::PathBuf;

    pub(super) const MAX_PAYLOAD: usize = 32 * 1024;
    pub(super) const MAX_FRAME: usize = 4 + MAX_PAYLOAD;

    pub(super) fn encode(request: &Request) -> Option<Vec<u8>> {
        let units: Vec<u16> = match request {
            Request::Raise => Vec::new(),
            Request::Open(path) => path.as_os_str().encode_wide().collect(),
        };
        let length = units.len() * 2;
        if length > MAX_PAYLOAD {
            return None;
        }
        let mut frame = Vec::with_capacity(4 + length);
        frame.extend_from_slice(&(length as u32).to_le_bytes());
        for unit in units {
            frame.extend_from_slice(&unit.to_le_bytes());
        }
        Some(frame)
    }

    pub(super) fn decode(frame: &[u8]) -> Option<Request> {
        let (head, body) = frame.split_first_chunk::<4>()?;
        let length = u32::from_le_bytes(*head) as usize;
        if length != body.len() || !length.is_multiple_of(2) || length > MAX_PAYLOAD {
            return None;
        }
        if length == 0 {
            return Some(Request::Raise);
        }
        let units: Vec<u16> = body
            .chunks_exact(2)
            .map(|pair| u16::from_le_bytes([pair[0], pair[1]]))
            .collect();
        let text = String::from_utf16(&units).ok()?;
        if text.contains('\0') {
            return None;
        }
        let path = PathBuf::from(text);
        path.is_absolute().then_some(Request::Open(path))
    }
}

mod win32 {
    use super::PipeName;
    use std::fs::{File, OpenOptions};
    use std::io;
    use std::os::windows::fs::OpenOptionsExt;
    use std::os::windows::io::{AsRawHandle, FromRawHandle, OwnedHandle};
    use std::time::Duration;
    use windows_sys::Win32::Foundation::{
        ERROR_NO_DATA, ERROR_PIPE_BUSY, ERROR_PIPE_CONNECTED, HANDLE, INVALID_HANDLE_VALUE,
    };
    use windows_sys::Win32::Security::{
        GetLengthSid, GetTokenInformation, TOKEN_QUERY, TOKEN_USER, TokenSessionId, TokenUser,
    };
    use windows_sys::Win32::Storage::FileSystem::{
        FILE_FLAG_FIRST_PIPE_INSTANCE, FILE_GENERIC_WRITE, FILE_READ_ATTRIBUTES,
        PIPE_ACCESS_INBOUND, SECURITY_IDENTIFICATION,
    };
    use windows_sys::Win32::System::Pipes::{
        ConnectNamedPipe, CreateNamedPipeW, GetNamedPipeServerProcessId, PIPE_READMODE_BYTE,
        PIPE_REJECT_REMOTE_CLIENTS, PIPE_TYPE_BYTE, PIPE_UNLIMITED_INSTANCES, PIPE_WAIT,
        WaitNamedPipeW,
    };
    use windows_sys::Win32::System::Threading::{GetCurrentProcess, OpenProcessToken};
    use windows_sys::Win32::UI::WindowsAndMessaging::AllowSetForegroundWindow;

    pub(super) const PIPE_BUSY: i32 = ERROR_PIPE_BUSY as i32;
    const IN_BUFFER: u32 = 64 * 1024;
    const _: () = assert!(IN_BUFFER as usize > super::wire::MAX_FRAME);

    pub(super) fn user_and_session() -> io::Result<(Vec<u8>, u32)> {
        let mut raw: HANDLE = std::ptr::null_mut();
        // SAFETY: the pseudo handle of the current process needs no closing, and
        // `raw` is a valid out pointer for the opened token.
        if unsafe { OpenProcessToken(GetCurrentProcess(), TOKEN_QUERY, &mut raw) } == 0 {
            return Err(io::Error::last_os_error());
        }
        // SAFETY: OpenProcessToken succeeded, so `raw` is an open handle owned here.
        let token = unsafe { OwnedHandle::from_raw_handle(raw) };
        let mut size = 0_u32;
        // SAFETY: a null buffer of length 0 only asks for the required size.
        unsafe {
            GetTokenInformation(
                token.as_raw_handle(),
                TokenUser,
                std::ptr::null_mut(),
                0,
                &mut size,
            )
        };
        let mut buffer = vec![0_u64; (size as usize).div_ceil(8)];
        // SAFETY: `buffer` is u64-aligned and at least `size` bytes long.
        if unsafe {
            GetTokenInformation(
                token.as_raw_handle(),
                TokenUser,
                buffer.as_mut_ptr().cast(),
                size,
                &mut size,
            )
        } == 0
        {
            return Err(io::Error::last_os_error());
        }
        // SAFETY: the call filled `buffer` with a TOKEN_USER whose SID points
        // into the same buffer, which outlives these reads.
        let sid = unsafe {
            let user = &*buffer.as_ptr().cast::<TOKEN_USER>();
            let length = GetLengthSid(user.User.Sid) as usize;
            std::slice::from_raw_parts(user.User.Sid.cast::<u8>(), length).to_vec()
        };
        let mut session = 0_u32;
        // SAFETY: TokenSessionId writes one u32 into `session`.
        if unsafe {
            GetTokenInformation(
                token.as_raw_handle(),
                TokenSessionId,
                (&raw mut session).cast(),
                size_of::<u32>() as u32,
                &mut size,
            )
        } == 0
        {
            return Err(io::Error::last_os_error());
        }
        Ok((sid, session))
    }

    pub(super) fn create_pipe(name: &PipeName, first: bool) -> io::Result<File> {
        let open_mode = PIPE_ACCESS_INBOUND
            | if first {
                FILE_FLAG_FIRST_PIPE_INSTANCE
            } else {
                0
            };
        // SAFETY: `wide` is NUL-terminated and outlives the call; null security
        // attributes give the default DACL of this process's token.
        let handle = unsafe {
            CreateNamedPipeW(
                name.wide.as_ptr(),
                open_mode,
                PIPE_TYPE_BYTE | PIPE_READMODE_BYTE | PIPE_WAIT | PIPE_REJECT_REMOTE_CLIENTS,
                PIPE_UNLIMITED_INSTANCES,
                0,
                IN_BUFFER,
                0,
                std::ptr::null(),
            )
        };
        if handle == INVALID_HANDLE_VALUE {
            return Err(io::Error::last_os_error());
        }
        // SAFETY: CreateNamedPipeW succeeded, so `handle` is an open handle owned here.
        Ok(File::from(unsafe { OwnedHandle::from_raw_handle(handle) }))
    }

    /// ERROR_PIPE_CONNECTED and ERROR_NO_DATA mean a client came first.
    pub(super) fn accept(pipe: &File) -> io::Result<()> {
        // SAFETY: the handle stays open for the borrow, and a null OVERLAPPED
        // makes this a blocking wait on a synchronous handle.
        if unsafe { ConnectNamedPipe(pipe.as_raw_handle(), std::ptr::null_mut()) } != 0 {
            return Ok(());
        }
        let error = io::Error::last_os_error();
        match error.raw_os_error().map(|code| code as u32) {
            Some(ERROR_PIPE_CONNECTED | ERROR_NO_DATA) => Ok(()),
            _ => Err(error),
        }
    }

    /// Reading attributes lets the client ask for the server's process id.
    pub(super) fn connect(name: &PipeName) -> io::Result<File> {
        OpenOptions::new()
            .access_mode(FILE_GENERIC_WRITE | FILE_READ_ATTRIBUTES)
            .security_qos_flags(SECURITY_IDENTIFICATION)
            .open(&name.text)
    }

    /// At least 1 ms, because 0 means the pipe default.
    pub(super) fn wait(name: &PipeName, left: Duration) -> io::Result<()> {
        let milliseconds = left.as_millis().clamp(1, 60_000) as u32;
        // SAFETY: `wide` is NUL-terminated and outlives the call.
        if unsafe { WaitNamedPipeW(name.wide.as_ptr(), milliseconds) } == 0 {
            return Err(io::Error::last_os_error());
        }
        Ok(())
    }

    pub(super) fn server_process(pipe: &File) -> io::Result<u32> {
        let mut process = 0_u32;
        // SAFETY: the handle stays open for the borrow, and `process` is a valid out pointer.
        if unsafe { GetNamedPipeServerProcessId(pipe.as_raw_handle(), &mut process) } == 0 {
            return Err(io::Error::last_os_error());
        }
        Ok(process)
    }

    /// Best effort: on failure Windows keeps its foreground lock and the window may only flash.
    pub(super) fn allow_foreground(process: u32) {
        // SAFETY: takes a plain process id.
        unsafe { AllowSetForegroundWindow(process) };
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::{BufRead, BufReader};
    use std::process::{Child, ChildStdin, Command, Stdio};
    use std::sync::atomic::{AtomicU32, Ordering};
    use std::sync::mpsc;

    const WAIT: Duration = Duration::from_secs(20);

    fn test_name() -> PipeName {
        static NEXT: AtomicU32 = AtomicU32::new(0);
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        PipeName::new(format!(
            r"\\.\pipe\simPl.test.{}.{nanos}.{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ))
    }

    fn serving(name: &PipeName) -> UnboundedReceiver<Request> {
        let first = listen(name, true).unwrap();
        let (sender, receiver) = unbounded();
        let name = name.clone();
        std::thread::spawn(move || serve(name, first, sender));
        receiver
    }

    fn received(receiver: UnboundedReceiver<Request>, count: usize) -> Vec<Request> {
        let (done, result) = mpsc::channel();
        std::thread::spawn(move || {
            let requests =
                iced_futures::futures::executor::block_on(receiver.take(count).collect::<Vec<_>>());
            let _ = done.send(requests);
        });
        result.recv_timeout(WAIT).expect("the forwards arrived")
    }

    fn open(path: &str) -> Request {
        Request::Open(PathBuf::from(path))
    }

    fn frame(text: &str) -> Vec<u8> {
        units_frame(&text.encode_utf16().collect::<Vec<_>>())
    }

    fn units_frame(units: &[u16]) -> Vec<u8> {
        let mut frame = ((units.len() * 2) as u32).to_le_bytes().to_vec();
        for unit in units {
            frame.extend_from_slice(&unit.to_le_bytes());
        }
        frame
    }

    #[test]
    fn frames_encode_a_path_and_a_focus_request() {
        assert_eq!(
            wire::encode(&open(r"C:\a")),
            Some(vec![8, 0, 0, 0, b'C', 0, b':', 0, b'\\', 0, b'a', 0])
        );
        assert_eq!(wire::encode(&Request::Raise), Some(vec![0, 0, 0, 0]));
    }

    #[test]
    fn frames_round_trip_unicode_paths() {
        let path = r"C:\Bücher\Ölçü ✓.pdf";
        assert_eq!(
            wire::decode(&wire::encode(&open(path)).unwrap()),
            Some(open(path))
        );
    }

    #[test]
    fn frames_that_lie_about_their_length_are_dropped() {
        for frame in [
            &[][..],
            &[0, 0, 0],
            &[4, 0, 0, 0, b'C', 0],
            &[2, 0, 0, 0, b'C', 0, 0, 0],
            &[1, 0, 0, 0, b'C'],
        ] {
            assert_eq!(wire::decode(frame), None, "{frame:?}");
        }
        assert_eq!(wire::decode(&[0, 0, 0, 0]), Some(Request::Raise));
    }

    #[test]
    fn frames_over_32_kib_are_dropped() {
        let fits = format!(r"C:\{}", "a".repeat(16_381));
        assert_eq!(wire::decode(&frame(&fits)), Some(open(&fits)));
        let over = format!(r"C:\{}", "a".repeat(16_382));
        assert_eq!(frame(&over).len(), 4 + 32_770);
        assert_eq!(wire::decode(&frame(&over)), None);
    }

    #[test]
    fn invalid_utf16_nul_and_relative_paths_are_dropped() {
        assert_eq!(wire::decode(&units_frame(&[0xD800])), None);
        assert_eq!(wire::decode(&frame("C:\\a\0b")), None);
        assert_eq!(wire::decode(&frame("book.pdf")), None);
        assert_eq!(wire::decode(&frame(r"C:\a")), Some(open(r"C:\a")));
    }

    #[test]
    fn encode_refuses_paths_over_the_limit() {
        let over = format!(r"C:\{}", "a".repeat(16_383));
        assert_eq!(wire::encode(&open(&over)), None);
        let fits = format!(r"C:\{}", "a".repeat(16_381));
        assert_eq!(
            wire::encode(&open(&fits)).map(|frame| frame.len()),
            Some(32_772)
        );
    }

    #[test]
    fn pipe_name_ignores_case_and_trailing_separators() {
        let base = pipe_name(&[1, 2, 3], 1, Path::new(r"C:\Users\A\AppData\Local\"));
        assert_eq!(
            base,
            pipe_name(&[1, 2, 3], 1, Path::new("c:/users/a/appdata/local"))
        );
        assert_eq!(
            base,
            r"\\.\pipe\simPl.Reader.1.3fe4ada8cddb553f857b876ddb6d15b8"
        );
    }

    #[test]
    fn pipe_name_separates_users_sessions_and_profiles() {
        let profile = Path::new(r"C:\Users\A\AppData\Local");
        let base = pipe_name(&[1, 2, 3], 1, profile);
        let user = pipe_name(&[1, 2, 4], 1, profile);
        let session = pipe_name(&[1, 2, 3], 2, profile);
        let other = pipe_name(&[1, 2, 3], 1, Path::new(r"D:\QA\profile"));
        assert_ne!(user, base);
        assert_ne!(session, base);
        assert_ne!(other, base);
        assert!(
            session.starts_with(r"\\.\pipe\simPl.Reader.2."),
            "{session}"
        );
    }

    #[test]
    fn a_forward_reaches_the_server_and_it_keeps_listening() {
        let name = test_name();
        let receiver = serving(&name);
        assert_eq!(
            forward(&name, &open(r"C:\a"), CONNECT_TIMEOUT).ok(),
            Some(())
        );
        assert_eq!(
            forward(&name, &Request::Raise, CONNECT_TIMEOUT).ok(),
            Some(())
        );
        assert_eq!(received(receiver, 2), vec![open(r"C:\a"), Request::Raise]);
    }

    #[test]
    fn a_burst_of_sixteen_forwards_all_arrive() {
        let name = test_name();
        let receiver = serving(&name);
        let paths: Vec<String> = (0..16).map(|n| format!(r"C:\book{n}.pdf")).collect();
        let clients: Vec<_> = paths
            .iter()
            .map(|path| {
                let (name, request) = (name.clone(), open(path));
                std::thread::spawn(move || forward(&name, &request, CONNECT_TIMEOUT).ok())
            })
            .collect();
        for client in clients {
            assert_eq!(client.join().unwrap(), Some(()));
        }
        let mut arrived = received(receiver, 16);
        arrived.sort_by_key(|request| format!("{request:?}"));
        let mut expected: Vec<Request> = paths.iter().map(|path| open(path)).collect();
        expected.sort_by_key(|request| format!("{request:?}"));
        assert_eq!(arrived, expected);
    }

    #[test]
    fn a_forward_written_before_the_server_accepts_is_received() {
        let name = test_name();
        let first = listen(&name, true).unwrap();
        assert_eq!(
            forward(&name, &open(r"C:\early.pdf"), CONNECT_TIMEOUT).ok(),
            Some(())
        );
        let (sender, receiver) = unbounded();
        let serving = name.clone();
        std::thread::spawn(move || serve(serving, first, sender));
        assert_eq!(received(receiver, 1), vec![open(r"C:\early.pdf")]);
    }

    #[test]
    fn the_client_learns_the_server_process() {
        let name = test_name();
        let _first = listen(&name, true).unwrap();
        let client = win32::connect(&name).unwrap();
        assert_eq!(
            win32::server_process(&client).ok(),
            Some(std::process::id())
        );
    }

    #[test]
    fn malformed_frames_are_dropped_and_serving_continues() {
        let name = test_name();
        let receiver = serving(&name);
        let mut raw = win32::connect(&name).unwrap();
        raw.write_all(&[9, 0, 0, 0]).unwrap();
        drop(raw);
        assert_eq!(
            forward(&name, &open(r"C:\b"), CONNECT_TIMEOUT).ok(),
            Some(())
        );
        assert_eq!(received(receiver, 1), vec![open(r"C:\b")]);
    }

    #[test]
    fn only_one_first_instance_can_exist() {
        let name = test_name();
        let _first = listen(&name, true).unwrap();
        assert_eq!(
            listen(&name, true)
                .map(drop)
                .map_err(|error| error.raw_os_error()),
            Err(Some(5))
        );
    }

    #[test]
    fn forward_without_a_server_fails_at_once() {
        let started = Instant::now();
        let result = forward(&test_name(), &Request::Raise, CONNECT_TIMEOUT);
        assert_eq!(result.map_err(|error| error.raw_os_error()), Err(Some(2)));
        assert!(started.elapsed() < Duration::from_millis(100));
    }

    #[test]
    fn forward_to_a_stuck_server_gives_up_at_the_deadline() {
        let name = test_name();
        let _first = listen(&name, true).unwrap();
        let _holder = win32::connect(&name).unwrap();
        let started = Instant::now();
        let result = forward(&name, &Request::Raise, Duration::from_millis(200));
        let elapsed = started.elapsed();
        assert!(result.is_err());
        assert!(
            elapsed >= Duration::from_millis(190) && elapsed < Duration::from_millis(600),
            "{elapsed:?}"
        );
    }

    const CLAIM_PATH: &str = "SIMPL_CLAIM_PATH";
    const CLAIM_BARRIER: &str = "SIMPL_CLAIM_BARRIER";
    const PREFIX: &str = "simpl-instance-";

    struct Claimer {
        child: Child,
        stdin: Option<ChildStdin>,
        lines: mpsc::Receiver<String>,
    }

    impl Claimer {
        fn start(root: &Path, path: &Path, barrier: Option<&Path>) -> Self {
            let mut command = Command::new(std::env::current_exe().unwrap());
            command
                .args([
                    "--ignored",
                    "--exact",
                    "instance::tests::claimer",
                    "--test-threads=1",
                    "--nocapture",
                ])
                .env("LOCALAPPDATA", root.join("profile"))
                .env(CLAIM_PATH, path)
                .env_remove(CLAIM_BARRIER)
                .stdin(Stdio::piped())
                .stdout(Stdio::piped());
            if let Some(barrier) = barrier {
                command.env(CLAIM_BARRIER, barrier);
            }
            let mut child = command.spawn().unwrap();
            let stdout = child.stdout.take().unwrap();
            let (send, lines) = mpsc::channel();
            std::thread::spawn(move || {
                for line in BufReader::new(stdout).lines().map_while(Result::ok) {
                    let _ = send.send(line);
                }
            });
            let stdin = child.stdin.take();
            Self {
                child,
                stdin,
                lines,
            }
        }

        fn next(&self, key: &str) -> String {
            let deadline = Instant::now() + WAIT;
            loop {
                let left = deadline.saturating_duration_since(Instant::now());
                let line = self.lines.recv_timeout(left).expect(key);
                // libtest prints `test <name> ... ` in front of the first line.
                if let Some((_, value)) = line.split_once(key) {
                    return value.to_owned();
                }
            }
        }

        fn finish(mut self) {
            drop(self.stdin.take());
            assert!(self.child.wait().unwrap().success());
        }
    }

    fn temp_root(test: &str) -> PathBuf {
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let root =
            std::env::temp_dir().join(format!("{PREFIX}{test}-{}-{nanos}", std::process::id()));
        std::fs::create_dir_all(root.join("profile")).unwrap();
        root
    }

    #[test]
    #[ignore = "runs in a child process with a temp profile through the claim tests"]
    fn claimer() {
        let path = PathBuf::from(std::env::var_os(CLAIM_PATH).expect("started by Claimer"));
        let profile = PathBuf::from(std::env::var_os("LOCALAPPDATA").unwrap());
        let root = profile.parent().unwrap();
        assert!(
            root.parent() == Some(std::env::temp_dir().as_path())
                && root
                    .file_name()
                    .and_then(|name| name.to_str())
                    .is_some_and(|name| name.starts_with(PREFIX)),
            "the claimer must run under a temp profile, not {}",
            profile.display()
        );
        if let Some(barrier) = std::env::var_os(CLAIM_BARRIER).map(PathBuf::from) {
            std::fs::write(barrier.join(format!("ready-{}", std::process::id())), "").unwrap();
            while !barrier.join("go").exists() {
                std::thread::sleep(Duration::from_millis(1));
            }
        }
        let started = Instant::now();
        let claim = claim(Some(&path));
        println!("claim={claim:?}");
        println!("elapsed_ms={}", started.elapsed().as_millis());
        if claim == Claim::Primary {
            std::thread::spawn(|| {
                let mut requests = std::pin::pin!(forwarded());
                while let Some(request) = iced_futures::futures::executor::block_on(requests.next())
                {
                    match request {
                        Request::Open(path) => println!("received={}", path.display()),
                        Request::Raise => println!("received=raise"),
                    }
                }
            });
            let _ = io::stdin().read_to_end(&mut Vec::new());
        }
    }

    #[test]
    fn simultaneous_first_launches_elect_one_server() {
        let root = temp_root("election");
        let barrier = root.join("barrier");
        std::fs::create_dir(&barrier).unwrap();
        let paths = [root.join("a.md"), root.join("b.md")];
        let claimers = paths
            .each_ref()
            .map(|path| Claimer::start(&root, path, Some(&barrier)));
        let deadline = Instant::now() + WAIT;
        while std::fs::read_dir(&barrier).unwrap().count() < 2 {
            assert!(Instant::now() < deadline, "both claimers got ready");
            std::thread::sleep(Duration::from_millis(5));
        }
        std::fs::write(barrier.join("go"), "").unwrap();

        let claims = claimers.each_ref().map(|claimer| claimer.next("claim="));
        let mut sorted = claims.clone();
        sorted.sort();
        assert_eq!(sorted, ["Forwarded", "Primary"]);
        let primary = claims.iter().position(|claim| claim == "Primary").unwrap();
        assert_eq!(
            claimers[primary].next("received="),
            paths[1 - primary].display().to_string()
        );
        for claimer in claimers {
            claimer.finish();
        }
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn crashed_primary_does_not_block() {
        let root = temp_root("crash");
        let mut first = Claimer::start(&root, Path::new(r"C:\first.md"), None);
        assert_eq!(first.next("claim="), "Primary");
        let second = Claimer::start(&root, Path::new(r"C:\second.md"), None);
        assert_eq!(second.next("claim="), "Forwarded");
        assert_eq!(first.next("received="), r"C:\second.md");
        second.finish();

        first.child.kill().unwrap();
        first.child.wait().unwrap();
        let third = Claimer::start(&root, Path::new(r"C:\third.md"), None);
        assert_eq!(third.next("claim="), "Primary");
        let elapsed: u64 = third.next("elapsed_ms=").parse().unwrap();
        assert!(elapsed < 500, "{elapsed} ms");
        third.finish();
        std::fs::remove_dir_all(root).unwrap();
    }
}
