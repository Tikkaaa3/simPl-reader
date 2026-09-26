//! CLI argument parsing and validation (task T-004 launch semantics).
//!
//! Grammar (deliberately small; no CLI framework dependency):
//!
//! ```text
//! process-measure --out <dir> [--interval-ms <ms>] [--duration-ms <ms>]
//!                 [--cwd <dir>] [--stdio <null|inherit|file>]
//!                 [--declare <key>=<value>]...
//!                 [--] <target-exe> [target args...]
//! ```
//!
//! The first bare `--` is the delimiter: every token after it belongs to the
//! child argument vector verbatim (including tokens that look like options
//! and additional `--` strings). Tool options are only recognized before the
//! delimiter. Parsing is pure; filesystem checks (existence of the target,
//! creation of a new output directory) happen later in `run`.

use std::ffi::{OsStr, OsString};
use std::fmt;
use std::os::windows::ffi::OsStrExt;
use std::path::PathBuf;

/// Documented finite upper bounds; the tool is a bounded sampler, not a
/// resident monitor. Validated before anything is launched.
pub const MAX_INTERVAL_MS: u64 = 3_600_000; // 1 hour
pub const MAX_DURATION_MS: u64 = 86_400_000; // 24 hours

/// Child stdio policy. `null` (default) connects the child's std streams to
/// the NUL device; `inherit` shares the sampler's console; `file` streams
/// stdout/stderr into run-directory files. None of the three buffers output
/// in memory.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StdioMode {
    Null,
    Inherit,
    File,
}

impl StdioMode {
    pub fn as_str(self) -> &'static str {
        match self {
            StdioMode::Null => "null",
            StdioMode::Inherit => "inherit",
            StdioMode::File => "file",
        }
    }
}

/// Operator-declared run context. Values supplied on the command line are
/// labeled declarations, never observed facts; absent context stays
/// explicitly unknown in the manifest.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DeclaredKey {
    SourceRevision,
    BuildProfile,
    FixtureRevision,
    /// Any other `--declare key=value` (recorded verbatim).
    Other(String),
}

impl DeclaredKey {
    pub fn as_str(&self) -> &str {
        match self {
            DeclaredKey::SourceRevision => "source_revision",
            DeclaredKey::BuildProfile => "build_profile",
            DeclaredKey::FixtureRevision => "fixture_revision",
            DeclaredKey::Other(k) => k,
        }
    }
}

/// A fully validated argument vector, ready for `run::run_collection`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ValidatedArgs {
    pub out_dir: PathBuf,
    pub target_exe: OsString,
    pub target_args: Vec<OsString>,
    /// `None` means the child inherits the sampler's working directory.
    pub target_cwd: Option<PathBuf>,
    pub interval_ms: u64,
    pub duration_ms: u64,
    pub stdio: StdioMode,
    pub declarations: Vec<(DeclaredKey, String)>,
}

/// Parse outcome: run, or `--help` (which must exit 0 without a run).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Parsed {
    Run(Box<ValidatedArgs>),
    Help,
}

/// A CLI usage problem. Reported on stderr; the tool exits with the
/// documented validation exit code (2) and never launches a child.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ArgsError {
    pub message: String,
}

impl fmt::Display for ArgsError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.message)
    }
}

const USAGE: &str = "\
usage: process-measure --out <new-dir> [--interval-ms <ms>] [--duration-ms <ms>]
                       [--cwd <dir>] [--stdio <null|inherit|file>]
                       [--declare <key>=<value>]...
                       -- <target-exe> [args...]

Samples one launched child's raw user/kernel CPU time (100 ns units) and
private working set / private commit memory (bytes) until natural exit, the
duration limit, or an interruption, then writes manifest.json and
samples.jsonl into the new output directory. The child owns no shell; its
process tree is terminated and cleaned up by the sampler. Defaults:
interval 250 ms, duration 10 s, stdio null.
";

/// True when the token contains an interior NUL; such strings cannot be
/// passed through Windows string APIs and must be rejected before launch.
pub fn has_interior_nul(os: &OsStr) -> bool {
    os.encode_wide().any(|w| w == 0)
}

fn err<T>(message: impl Into<String>) -> Result<T, ArgsError> {
    Err(ArgsError {
        message: message.into(),
    })
}

/// Parse a raw argv (without the program name) into a validated run
/// configuration.
pub fn parse(argv: &[OsString]) -> Result<Parsed, ArgsError> {
    parse_inner(argv)
}

fn parse_inner(argv: &[OsString]) -> Result<Parsed, ArgsError> {
    // Split at the first bare `--`: everything before is tool options,
    // everything after is the child argv, verbatim.
    let split_at = argv.iter().position(|t| t == "--");
    let (option_tokens, child_tokens) = match split_at {
        Some(i) => (&argv[..i], &argv[i + 1..]),
        None => (argv, &[][..]),
    };

    // --help before the delimiter exits successfully without a run; after
    // the delimiter it is an ordinary child argument.
    if argv.len() == 1 && argv[0] == "--help" {
        return Ok(Parsed::Help);
    }

    let mut out_dir: Option<PathBuf> = None;
    let mut interval_ms: Option<u64> = None;
    let mut duration_ms: Option<u64> = None;
    let mut target_cwd: Option<PathBuf> = None;
    let mut stdio: Option<StdioMode> = None;
    let mut declarations: Vec<(DeclaredKey, String)> = Vec::new();

    let mut i = 0;
    while i < option_tokens.len() {
        let token = option_tokens[i].to_string_lossy().into_owned();
        let value_of = |i: &mut usize, name: &str| -> Result<OsString, ArgsError> {
            *i += 1;
            option_tokens.get(*i).cloned().ok_or_else(|| ArgsError {
                message: format!(
                    "missing value for {name} (run `process-measure --help` for usage)"
                ),
            })
        };
        match token.as_str() {
            "--out" => {
                if out_dir.is_some() {
                    return err("option --out given twice");
                }
                let v = value_of(&mut i, "--out")?;
                if v.is_empty() || has_interior_nul(&v) {
                    return err("option --out requires a non-empty path without NUL");
                }
                out_dir = Some(PathBuf::from(v));
            }
            "--interval-ms" => {
                if interval_ms.is_some() {
                    return err("option --interval-ms given twice");
                }
                let v = value_of(&mut i, "--interval-ms")?;
                interval_ms = Some(parse_u64_option("--interval-ms", &v)?);
            }
            "--duration-ms" => {
                if duration_ms.is_some() {
                    return err("option --duration-ms given twice");
                }
                let v = value_of(&mut i, "--duration-ms")?;
                duration_ms = Some(parse_u64_option("--duration-ms", &v)?);
            }
            "--cwd" => {
                if target_cwd.is_some() {
                    return err("option --cwd given twice");
                }
                let v = value_of(&mut i, "--cwd")?;
                if v.is_empty() || has_interior_nul(&v) {
                    return err("option --cwd requires a non-empty path without NUL");
                }
                target_cwd = Some(PathBuf::from(v));
            }
            "--stdio" => {
                if stdio.is_some() {
                    return err("option --stdio given twice");
                }
                let v = value_of(&mut i, "--stdio")?;
                let v = v.to_string_lossy().into_owned();
                stdio = Some(match v.as_str() {
                    "null" => StdioMode::Null,
                    "inherit" => StdioMode::Inherit,
                    "file" => StdioMode::File,
                    _ => return err("option --stdio must be one of: null, inherit, file"),
                });
            }
            "--declare" => {
                let v = value_of(&mut i, "--declare")?;
                let v = v.to_string_lossy().into_owned();
                let (key, value) = match v.split_once('=') {
                    Some((k, val)) => (k, val),
                    None => {
                        return err("option --declare requires key=value");
                    }
                };
                if key.is_empty() {
                    return err("option --declare requires a non-empty key");
                }
                declarations.push((declared_key(key), value.to_string()));
            }
            "--help" => {
                return err("option --help must be given without any other arguments");
            }
            "--" => unreachable!("delimiters were split before option parsing"),
            other => {
                return err(format!(
                    "unknown option {other} (tool options go before the `--` delimiter; run `process-measure --help` for usage)"
                ));
            }
        }
        i += 1;
    }

    let Some(out_dir) = out_dir else {
        return err(
            "required option --out <dir> is missing (the run directory must be a new directory; run `process-measure --help` for usage)",
        );
    };

    let Some(exe) = child_tokens.first() else {
        return err(
            "missing target executable after the `--` delimiter (expected: -- <target-exe> [args...])",
        );
    };
    if has_interior_nul(exe) {
        return err("target executable path contains an embedded NUL");
    }
    for arg in &child_tokens[1..] {
        if has_interior_nul(arg) {
            return err("a target argument contains an embedded NUL");
        }
    }

    let interval_ms = interval_ms.unwrap_or(250);
    let duration_ms = duration_ms.unwrap_or(10_000);
    if interval_ms == 0 {
        return err("option --interval-ms must be greater than zero");
    }
    if interval_ms > MAX_INTERVAL_MS {
        return err(format!(
            "option --interval-ms must not exceed the documented maximum of {MAX_INTERVAL_MS} ms (bounded sampler, not a resident monitor)"
        ));
    }
    if duration_ms == 0 {
        return err("option --duration-ms must be greater than zero");
    }
    if duration_ms > MAX_DURATION_MS {
        return err(format!(
            "option --duration-ms must not exceed the documented maximum of {MAX_DURATION_MS} ms (bounded sampler, not a resident monitor)"
        ));
    }
    if duration_ms < interval_ms {
        return err(format!(
            "option --duration-ms ({duration_ms}) must be at least --interval-ms ({interval_ms})"
        ));
    }

    Ok(Parsed::Run(Box::new(ValidatedArgs {
        out_dir,
        target_exe: exe.clone(),
        target_args: child_tokens[1..].to_vec(),
        target_cwd,
        interval_ms,
        duration_ms,
        stdio: stdio.unwrap_or(StdioMode::Null),
        declarations,
    })))
}

fn parse_u64_option(name: &str, value: &OsStr) -> Result<u64, ArgsError> {
    let Some(text) = value.to_str() else {
        return err(format!(
            "option {name} requires an unsigned decimal integer"
        ));
    };
    text.parse::<u64>().map_err(|_| ArgsError {
        message: format!("option {name} requires an unsigned decimal integer"),
    })
}

fn declared_key(key: &str) -> DeclaredKey {
    match key {
        "source_revision" => DeclaredKey::SourceRevision,
        "build_profile" => DeclaredKey::BuildProfile,
        "fixture_revision" => DeclaredKey::FixtureRevision,
        other => DeclaredKey::Other(other.to_string()),
    }
}

/// Render the usage text (also used by `--help`).
pub fn usage_text() -> &'static str {
    USAGE
}

#[cfg(test)]
mod tests {
    use super::*;

    fn os(v: &str) -> OsString {
        OsString::from(v)
    }

    fn ok(argv: &[OsString]) -> ValidatedArgs {
        match parse(argv).expect("parses") {
            Parsed::Run(a) => *a,
            Parsed::Help => panic!("unexpected help"),
        }
    }

    fn err_of(argv: &[OsString]) -> String {
        parse(argv).expect_err("must fail").message
    }

    #[test]
    fn minimal_run_with_delimiter() {
        let a = ok(&[os("--out"), os(r"target\runs\a"), os("--"), os("child.exe")]);
        assert_eq!(a.out_dir, PathBuf::from(r"target\runs\a"));
        assert_eq!(a.target_exe, os("child.exe"));
        assert!(a.target_args.is_empty());
        assert_eq!(a.interval_ms, 250);
        assert_eq!(a.duration_ms, 10_000);
        assert_eq!(a.stdio, StdioMode::Null);
        assert!(a.target_cwd.is_none());
        assert!(a.declarations.is_empty());
    }

    #[test]
    fn child_arguments_after_delimiter_are_verbatim() {
        let a = ok(&[
            os("--out"),
            os("d"),
            os("--"),
            os("child.exe"),
            os("--interval-ms"),
            os("999"),
            os(""),
            os("a b"),
            os("he said \"hi\""),
            os("trailing\\"),
            os("--"),
            os("é🙂"),
        ]);
        assert_eq!(a.target_exe, os("child.exe"));
        assert_eq!(
            a.target_args,
            vec![
                os("--interval-ms"),
                os("999"),
                os(""),
                os("a b"),
                os("he said \"hi\""),
                os("trailing\\"),
                os("--"),
                os("é🙂"),
            ]
        );
    }

    #[test]
    fn all_options_parse_with_custom_values() {
        let a = ok(&[
            os("--interval-ms"),
            os("500"),
            os("--duration-ms"),
            os("4000"),
            os("--cwd"),
            os(r"D:\work dir"),
            os("--stdio"),
            os("file"),
            os("--out"),
            os("d"),
            os("--declare"),
            os("source_revision=abc123"),
            os("--declare"),
            os("gpu=NVIDIA Test 1"),
            os("--"),
            os("child.exe"),
        ]);
        assert_eq!(a.interval_ms, 500);
        assert_eq!(a.duration_ms, 4000);
        assert_eq!(a.target_cwd, Some(PathBuf::from(r"D:\work dir")));
        assert_eq!(a.stdio, StdioMode::File);
        assert_eq!(
            a.declarations,
            vec![
                (DeclaredKey::SourceRevision, "abc123".to_string()),
                (
                    DeclaredKey::Other("gpu".to_string()),
                    "NVIDIA Test 1".to_string()
                ),
            ]
        );
    }

    #[test]
    fn duration_equal_to_interval_is_accepted() {
        let a = ok(&[
            os("--interval-ms"),
            os("300"),
            os("--duration-ms"),
            os("300"),
            os("--out"),
            os("d"),
            os("--"),
            os("x"),
        ]);
        assert_eq!(a.interval_ms, 300);
        assert_eq!(a.duration_ms, 300);
    }

    #[test]
    fn zero_interval_rejected() {
        assert!(
            err_of(&[
                os("--interval-ms"),
                os("0"),
                os("--out"),
                os("d"),
                os("--"),
                os("c")
            ])
            .contains("interval")
        );
    }

    #[test]
    fn zero_duration_rejected() {
        assert!(
            err_of(&[
                os("--duration-ms"),
                os("0"),
                os("--out"),
                os("d"),
                os("--"),
                os("c")
            ])
            .contains("duration")
        );
    }

    #[test]
    fn overflowing_values_rejected() {
        assert!(
            err_of(&[
                os("--interval-ms"),
                os("99999999999999999999999"),
                os("--out"),
                os("d"),
                os("--"),
                os("c")
            ])
            .contains("interval")
        );
        assert!(
            err_of(&[
                os("--duration-ms"),
                os("99999999999999999999"),
                os("--out"),
                os("d"),
                os("--"),
                os("c")
            ])
            .contains("duration")
        );
    }

    #[test]
    fn nonnumeric_and_negative_values_rejected() {
        assert!(
            err_of(&[
                os("--interval-ms"),
                os("250ms"),
                os("--out"),
                os("d"),
                os("--"),
                os("c")
            ])
            .contains("interval")
        );
        assert!(
            err_of(&[
                os("--interval-ms"),
                os("-5"),
                os("--out"),
                os("d"),
                os("--"),
                os("c")
            ])
            .contains("interval")
        );
    }

    #[test]
    fn duration_shorter_than_interval_rejected() {
        let msg = err_of(&[
            os("--interval-ms"),
            os("500"),
            os("--duration-ms"),
            os("250"),
            os("--out"),
            os("d"),
            os("--"),
            os("c"),
        ]);
        assert!(msg.contains("duration"), "got: {msg}");
    }

    #[test]
    fn documented_upper_bounds_are_enforced() {
        assert!(
            err_of(&[
                os("--interval-ms"),
                os("3600001"),
                os("--out"),
                os("d"),
                os("--"),
                os("c")
            ])
            .contains("interval")
        );
        assert!(
            err_of(&[
                os("--duration-ms"),
                os("86400001"),
                os("--out"),
                os("d"),
                os("--"),
                os("c")
            ])
            .contains("duration")
        );
        // At the cap they are accepted.
        let a = ok(&[
            os("--interval-ms"),
            os("3600000"),
            os("--duration-ms"),
            os("86400000"),
            os("--out"),
            os("d"),
            os("--"),
            os("c"),
        ]);
        assert_eq!(a.interval_ms, 3_600_000);
    }

    #[test]
    fn missing_output_directory_rejected() {
        let msg = err_of(&[os("--"), os("c.exe")]);
        assert!(msg.contains("--out"), "got: {msg}");
    }

    #[test]
    fn unknown_option_before_delimiter_rejected() {
        let msg = err_of(&[
            os("--wat"),
            os("1"),
            os("--out"),
            os("d"),
            os("--"),
            os("c"),
        ]);
        assert!(msg.contains("--wat"), "got: {msg}");
    }

    #[test]
    fn duplicate_option_rejected() {
        assert!(
            err_of(&[
                os("--out"),
                os("a"),
                os("--out"),
                os("b"),
                os("--"),
                os("c")
            ])
            .contains("--out")
        );
    }

    #[test]
    fn missing_option_value_rejected() {
        assert!(err_of(&[os("--out")]).contains("--out"));
        assert!(err_of(&[os("--interval-ms")]).contains("--interval-ms"));
    }

    #[test]
    fn missing_delimiter_rejected() {
        let msg = err_of(&[os("--out"), os("d"), os("child.exe")]);
        assert!(msg.contains("--"), "got: {msg}");
    }

    #[test]
    fn empty_target_argv_rejected() {
        assert!(err_of(&[os("--out"), os("d")]).contains("target"));
    }

    #[test]
    fn interior_nul_rejected_everywhere() {
        // Windows OsString is WTF-8, where U+0000 is the raw NUL byte, so a
        // test-only wide constructor can build the forbidden value.
        let nul = unsafe { os_with_nul("a") };
        assert!(has_interior_nul(&nul));
        assert!(err_of(&[os("--out"), nul.clone(), os("--"), os("c")]).contains("NUL"));
        assert!(err_of(&[os("--out"), os("d"), os("--"), nul]).contains("NUL"));
        assert!(!has_interior_nul(&os("normal")));
    }

    // Test-only constructor that builds an OsString containing an interior
    // NUL without private std APIs: Windows OsString is WTF-8, where U+0000
    // is encoded as the raw NUL byte.
    unsafe fn os_with_nul(text: &str) -> OsString {
        use std::os::windows::ffi::OsStringExt;
        let mut wide: Vec<u16> = text.encode_utf16().collect();
        wide.push(0);
        wide.push(b'x' as u16);
        OsString::from_wide(&wide)
    }

    #[test]
    fn bad_stdio_value_rejected() {
        let msg = err_of(&[
            os("--stdio"),
            os("capture"),
            os("--out"),
            os("d"),
            os("--"),
            os("c"),
        ]);
        assert!(msg.contains("--stdio"), "got: {msg}");
    }

    #[test]
    fn help_before_delimiter_is_help() {
        assert_eq!(parse(&[os("--help")]), Ok(Parsed::Help));
    }

    #[test]
    fn help_after_delimiter_is_a_child_argument() {
        let a = ok(&[
            os("--out"),
            os("d"),
            os("--"),
            os("child.exe"),
            os("--help"),
        ]);
        assert_eq!(a.target_args, vec![os("--help")]);
    }

    #[test]
    fn second_delimiter_belongs_to_the_child() {
        let a = ok(&[
            os("--out"),
            os("d"),
            os("--"),
            os("child.exe"),
            os("--"),
            os("--x"),
        ]);
        assert_eq!(a.target_args, vec![os("--"), os("--x")]);
    }

    #[test]
    fn declaration_errors() {
        assert!(
            err_of(&[
                os("--out"),
                os("d"),
                os("--declare"),
                os("novalue"),
                os("--"),
                os("c")
            ])
            .contains("--declare")
        );
        assert!(
            err_of(&[
                os("--out"),
                os("d"),
                os("--declare"),
                os("=value"),
                os("--"),
                os("c")
            ])
            .contains("--declare")
        );
    }

    #[test]
    fn declare_value_may_contain_equals() {
        let a = ok(&[
            os("--out"),
            os("d"),
            os("--declare"),
            os("note=k=v"),
            os("--"),
            os("c"),
        ]);
        assert_eq!(
            a.declarations,
            vec![(DeclaredKey::Other("note".to_string()), "k=v".to_string())]
        );
    }
}
