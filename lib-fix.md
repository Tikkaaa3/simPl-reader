# Library fix plan

Branch: `fix/library` (from `main` at `1d51def`).
Issue: [#1 Prevent SimPL from creating copies of every document](https://github.com/Tikkaaa3/simPl-reader/issues/1).

> "I need SimPL to read the actual pdf where I put it, not from a copy on C:"

## Goals

1. **Open** reads a document from where it is. Nothing is copied, and the book appears in **Recent** only.
2. **Add document** imports a copy into the simPl library. This is today's behaviour. The library screen lists imported books only.
3. Double-clicking a file in Explorer, **Open with → simPl**, and drag-and-drop all use the **Open** path.
4. **Settings → Library & data → Library location** chooses where imported copies live.
   Changing it moves the existing library to the new folder safely.

Non-goals:
- Editing PDFs.
- Watching originals for changes.
- Syncing libraries between PCs.
- Library cards for books that weren't imported (see Decisions, item 4).

## How it works today (verified)

All locations are on `fix/library`.

| Entry point | Code | Copies? |
|---|---|---|
| Add document card / Ctrl+O | `app.rs:3770` → `open_with_import(path, prior, import)`, where `import = locate.is_none()` | yes |
| Drag and drop | `app.rs:4738` → `open_with_import(path, None, true)` | yes |
| Explorer double-click / Open with (`simPl.exe "%1"`) | `main.rs` passes the path to `app::run` → `app.rs:6745` `open_with_import(path, None, true)` | yes |
| Click a library card | `app.rs:3420` → `open_with_import(path, Some(prior), true)` | yes, even after **Locate** |
| Recent → Open | `app.rs:3712` → `open(entry.path, None)` | no |
| Locate (library or recent) | `app.rs:1936` → dialog → `open_with_import(.., import = false)` | no |

- The copy happens in `managed::import` (`reader-document/src/managed.rs:75`), called from `app.rs:1839`.
  It writes `%LOCALAPPDATA%\simPl\documents\<sha256(path + bytes)>\<name>` (`managed.rs:206`).
- Editing the original creates a **second** full copy, because the folder hash includes the content.
  A scratch test showed this: two copies after one edit.
- **Every successful open adds or updates a library card**, including ones that don't copy: `load_document` always returns `catalog: Some(catalog_entry(..))` (`app.rs:424`, `app.rs:473`). The `Loaded` handler then calls `shelf.remember`.
- **Bug:** Locate points a library entry at the original. The next click on that card calls `OpenLibrary` with `import = true`, so the copy comes back and replaces the entry.
- The installer already registers simPl under "Open with" for `.pdf`, `.html`, `.epub`, `.txt` and `.md`, using `"{app}\simPl.exe" "%1"` (`installer/simPl.iss:83-123`).
- There is no single-instance handling. The mutex in `platform.rs` only marks that the app is running. Each double-click starts a new window.
- Storage root: `reader_profile::storage_base()` returns `LOCALAPPDATA`, or roots configured on Android. `managed::root()` is `<base>\simPl\documents`.
- **Reading positions are keyed by a hash of the canonical path** (`position.rs:295-303`). The content fingerprint is only used to validate a record. Notes and highlights (`annotations.rs`) and per-book typography (`reading.rs`) are keyed by content fingerprint.
  Any change of path (moving the library, Locate) must carry the position records across.
  `position::transfer_saved_position` (`position.rs:167`) and `recent::relocate` (`recent.rs:70`) already do this for Locate.
- Already in place for files outside simPl:
  - `managed::remove` only takes such a file off the list (`managed.rs:320`).
  - Backup skips linked originals, and restore maps them to `documents/.missing` until Locate (`backup.rs:529`).

## Target behaviour

The library screen shows imported books. Recent shows everything you read, including files opened without copying.

| | **Add document** (import) | **Open** (in place) |
|---|---|---|
| File simPl reads | copy under the library location | the original, where it is |
| Appears in | Library + Recent | Recent only |
| Shelves, favourites, cover grid | yes | no (use **Add to library** from Recent) |
| Reading position, notes, highlights | yes | yes (same storage as today) |
| Remove | deletes simPl's copy (current text) | Recent → Remove deletes the history entry. It also removes a matching library card, which is today's behaviour, but an in-place book has none. |
| Original moved or deleted | unaffected | Recent row shows it as missing → **Locate** (current behaviour) |
| Backup "Include book files" | bundled | path only (unchanged) |
| Library location move | moved | untouched |

Entry points:

- **Ctrl+O, drag-and-drop, Explorer double-click, Open with, the startup argument** → Open (in place).
- **Add document** card → Import, using the file dialog.
- **Recent row** gets an **Add to library** action. It imports a copy, the same as Add document, and carries the reading position across.
- **Library card → Locate** (the copy is missing, for example after restoring a backup without book files) **imports** the chosen file. That keeps every library card backed by a copy.
- **Recent row → Locate** keeps today's behaviour: it repoints the history entry to the chosen file without copying.
- TXT, Markdown and single-file HTML opened in place are converted **in memory** on every load. `text::to_html` and `html::load_html_bytes(source, bytes)` already resolve images relative to the source, so nothing is written.
- HTML book folders need a packaged EPUB, so a dropped folder, or a folder passed at startup, is still imported (copy and card). Reading folders in place, with the EPUB in the disposable cache, is a follow-up.

Legacy library entries that already point outside `managed::root()` (made by today's Locate, or restored from a backup) stay on the library screen. They open in place and are never silently replaced by a copy.

## Phase LF-1: Open in place

Status: implemented on `fix/library`.

### What each entry point does

`app.rs` has one type per decision. Each entry point builds an `OpenAs`, and the file dialog and the load carry it unchanged.

- `Storage { InPlace, Import }` says whether the file is copied first.
- `Relocation { Library(Entry), Recent(Entry), AddToLibrary(Entry) }` is an open that moves an existing entry to another file. `Relocation::storage()` maps Library and AddToLibrary to `Import` and Recent to `InPlace`, so an in-place Library Locate can't be written.
- `OpenAs { New(Storage), Relocate(Relocation) }` is the whole intent. `OpenAs::external(path)` handles drop and the startup argument: a file opens in place, a folder is imported.

| Entry point | `OpenAs` | Copies | Library card |
|---|---|---|---|
| Library card | `New(InPlace)` | no | refreshed |
| Recent → Open | `New(InPlace)` | no | refreshed if one exists |
| Open file card, Ctrl+O | `New(InPlace)` | no | refreshed if one exists |
| Add document card | `New(Import)` | yes | written |
| Add document on a file that already has a card | `Relocate(Library)` | yes | moved to the copy |
| Drop or startup: a file | `New(InPlace)` | no | refreshed if one exists |
| Drop or startup: a folder | `New(Import)` | yes | written |
| Library card → Locate | `Relocate(Library)` | yes | moved to the copy |
| Recent → Locate | `Relocate(Recent)` | no | moved if one exists |
| Recent → Add to library | `Relocate(AddToLibrary)` | yes | written |

The missing-file notice offers the Locate the failed open carried. For a plain open, a matching library card wins over a Recent row.

### Library cards

`shelf::Card { Write, UpdateExisting }` decides what a finished load may do to the library. `Storage::card()` is the only map: `Import` writes a card and `InPlace` updates one. `UpdateExisting` updates a card that already exists for the opened or the replaced path and never inserts one. The shelf decides at write time. Writes that arrive while the library is still loading wait in its queue and are decided after the load. This stops the startup open, which can finish first, from adding a card.

A missing `library.json` is rebuilt from Recent, but only from simPl's own copies (`library::bootstrap`). On first run the startup open can write `recent.json` before the library is read, and a profile that only uses Open never writes `library.json`.

### Text and Markdown

`load_html` also reads `.txt`, `.text`, `.md` and `.markdown`. It converts the file in memory with `text::read_page`, which `managed::import_at` also uses. Both hash the same bytes, so an in-place file and its imported copy share a fingerprint, notes and per-book typography. `DocumentKind::from_path` is the one extension table.

### Tests

- `app.rs` entry-point tests check the `OpenAs` each message produces.
- `isolated.rs` re-runs a test in a child process whose `LOCALAPPDATA` is a fresh temp folder. Tests that open real files use it, so they never touch the real profile. The regression test for the original bug is `library_card_outside_the_library_opens_in_place_without_a_copy`.
- A manual check of the release build is not committed. It launched PDF, Markdown, TXT and HTML files the way Explorer does, and each left no copy and no card, with Recent pointing at the original. It also ran Add to library from the Recent panel, which made one copy and one card.

### Accepted for now

- A relocation that fails after importing, because the fingerprint doesn't match, leaves the new copy behind.
- Library Locate on a card whose copy still exists leaves the old copy without a card.
- `load_html` refuses UNC paths, so HTML, TXT and Markdown can't be opened in place from a network share. Add document still works for TXT and Markdown.
- Recent shows TXT and Markdown files with the "HTML" tag.
- Links and junctions are not rejected for in-place reads. OneDrive placeholders are reparse points, and Recent already opens them today.

## Phase LF-2: Explorer double-click and Open with

Status: implemented on `feat/lf2-open-with`.

### One window per profile

`crates/iced-shell/src/instance.rs` owns single-instance behaviour. `main()` calls `instance::claim(path)` in normal mode only, after mode detection and before iced starts. It returns `Claim::Primary` (run the reader) or `Claim::Forwarded` (exit 0). The diagnostic modes never create or contact the pipe.

- **Key.** The pipe name is `\\.\pipe\simPl.Reader.<session>.<hash>`. The hash covers a version tag, the user's binary SID and the normalized profile folder (`reader_profile::storage_base()`). Scripts that set their own `LOCALAPPDATA` therefore get their own window and never reach yours.
- **Election.** A launch first tries to create the pipe with `FILE_FLAG_FIRST_PIPE_INSTANCE`, so Windows decides which of two simultaneous launches becomes the server. The loser forwards.
- **Server.** A dedicated thread accepts one request per connection. It creates the next pipe instance before reading the current one, so a burst of launches waits briefly instead of finding no server. A client that wrote and closed before the server accepted still delivers: `ConnectNamedPipe` returns `ERROR_NO_DATA` and the bytes are readable.
- **Client.** The second launch resolves the path against its own working directory, connects within 500 ms, lets the server take the foreground (`AllowSetForegroundWindow`) and writes one frame. Any failure, including an elevated first instance, means it starts normally.
- **Wire.** A 4-byte length, then UTF-16 (at most 32 KiB). Zero length means "come to the front". `wire::decode` is the only place that checks incoming bytes.

### In the running window

`instance::forwarded` feeds an unconditional subscription that sends `Message::Forwarded(Request)`. The startup argument, file drops and forwards all go through one `Inbox { open, raise }`:

- `Reader::can_open()` is the single rule for when a document may replace what is shown. It covers saving, a file dialog, a failed close, a removal, a remove confirmation and a backup or restore. `Reader::open` uses it, and the inbox waits for it.
- The inbox holds one path, so the newest one wins. A file the user picks directly replaces a waiting one.
- `update()` drains the inbox after every message. The window is restored if minimized, then focused. Raising waits while a file picker or the backup picker is open, because those pickers have no owner window.

### Shortcut

**Add document** is Ctrl+I. The plan said Ctrl+Shift+O, but another program on the development machine had registered it as a global hotkey (`RegisterHotKey` returns `ERROR_HOTKEY_ALREADY_REGISTERED`), so simPl never received it. Global hotkeys rarely use a single modifier.

### Installer

`.htm` and `.xhtml` use the existing `simPl.Reader.HTML` ProgID and `.markdown` uses `simPl.Reader.MD`. `scripts/test-installer.ps1` checks all eight extensions from one ordered map.

`SupportedTypes` under `Software\Classes\Applications\simPl.exe` was dropped. The Installer QA and production setups both install `simPl.exe`, so they would share that key, and Windows can launch the installed copy when a user picks a portable `simPl.exe`. `OpenWithProgids` and `Capabilities\FileAssociations` already list simPl in Open with. Windows doesn't let an app make itself the default, so users choose **Always** in the Open with dialog.

### Checks

- `instance.rs` tests use throwaway pipe names: framing and rejection, write-before-accept, a burst of 16, the stuck-server deadline, and the first-instance flag. Child processes with a temporary `LOCALAPPDATA` test a simultaneous election, a crashed primary and a forward into a running reader.
- `crates/iced-shell/tests/release-smoke.ps1` launches a second copy with a file, after minimizing, and while the file picker is open, then checks that one reader process remains.
- `scripts/test-installer.ps1` passed with the eight extensions.
- A manual check of the release build is not committed. It covered forwarding, a relative path, restore and front while minimized, a maximized window staying maximized, a launch with no file, a burst and two profiles.

### Accepted for now

- A client that connects and never writes stalls later forwards until they time out and start their own window. Only a hostile process of the same user can do this.
- A forward that reaches a window while it is closing is lost; there is no acknowledgement.
- Selecting several files in Explorer opens one of them.
- A forwarded open while a note is being written does not save the draft first. Drops have the same gap.

## Phase LF-3: Library location setting

Status: implemented on `feat/lf3-library-location`.

The library folder holds imported copies only. Settings, positions, history, notes, covers and caches stay in `%LOCALAPPDATA%\simPl`. Books opened in place are never moved.

### The setting

`crates/reader-document/src/library_location.rs` owns everything below. Its record is `%LOCALAPPDATA%\simPl\library-location.json`:

- `root` is `Default` (`<profile>\documents`, never stored as a path) or `Folder(<canonical …\simPl Library>)`.
- `moving` is the move journal: `None`, `Copying { to }` or `Cleaning { from, positions_copied }`.
- Every write is atomic (temp file plus `MoveFileExW`). The record is not in `backup::allowed()`, so a backup never carries another PC's path.

Only a missing file means the default folder. A file that can't be read or parsed fails closed: `managed::root()` returns an error, imports and removals refuse with "Library settings could not be read: …", and `source_kind` returns `None`.

`managed::root()` is now `Result<PathBuf, String>` and reads the record on every call, like `storage_base()`. `managed::import` refuses a custom folder that went missing ("Library folder not found: …") instead of recreating it. `library::load` reads the root once and passes it to the source-format backfill; `load_at`, used by backup validation and the move, does no backfill.

Android has no record and keeps the default folder.

### Validation (`plan`)

Change… uses `<picked>\simPl Library`. Nothing else is written into the picked folder. `plan` refuses:

- a missing folder, a file, a link or a junction;
- a network folder (`\\?\UNC\…` or `GetDriveTypeW == DRIVE_REMOTE`), because the HTML, TXT, MD and EPUB loaders reject UNC paths;
- a target that already holds a simPl copy (an owned `<64-hex id>` folder) or a `.incoming` folder, because rollback and staging delete those. Other files in it are allowed and left alone;
- the current folder, a folder inside it or around it;
- simPl's data folder or its install folder, or a folder around either;
- an unavailable current folder, a pending journal, and a drive without the bytes to copy plus max(64 MiB, 2 %).

Paths are compared by component, case-insensitively, with and without the `\\?\` prefix.

### The move (`run_move`)

The move holds an exclusive `library-location.lock` handle; recovery takes the same lock.

1. Create `to`, check it holds no copies, and write `Copying { to }`.
2. For each owned folder in the current root, copy every file into `to\.incoming\<id>` with `create_new`, hashing as it goes. `.simpl-owned` is copied last. Each file is synced, read back and compared by size and SHA-256. Then `.incoming\<id>` is renamed to `to\<id>`. Cancel is checked between 64 KiB chunks and once more before the commit.
3. Write `{ root: to, moving: Cleaning { from, positions_copied: false } }`. This write is the only commit point. Before it, cancel, failure and a crash all end in `rollback(to)`: delete the copies and `.incoming` in `to`, and remove `to` only when it is empty.
4. `finish`:
   - Copy every position record of library and Recent paths under `from` to the new key, replacing what is there. Then write `positions_copied: true`, so a rerun never overwrites a newer position.
   - Rewrite `library.json` and `recent.json`, dropping an entry that duplicates an earlier one.
   - Re-read both files. Delete a source copy only when `to` owns the same id and no entry still points at it.
   - Clear the journal.

`run_move` returns `Moved`, `Cancelled { kept }`, or `Committed { to, error }` when `finish` failed after the commit. A failure before the commit is an `Err`.

`recover()` runs in `main` after `instance::claim` and before the window opens. It undoes `Copying`, reruns `finish` for `Cleaning`, and returns the one notice the app shows. It never overwrites an unreadable record, and it skips this start when another process holds the lock.

### Unavailable folder

The banner and Settings say "Library folder not found: …" with Retry. Settings adds:

- **Find it…** (`relink`). It accepts the picked `simPl Library`, or a folder holding one, when it has an owned copy, with its book file, for every library and Recent path under the old folder. It writes `Cleaning` with the old folder as `from` and runs `finish`, which copies positions and rewrites paths but deletes nothing. Nothing is copied.
- **Stop using this folder…** (`forget`). It switches to the default folder without moving or deleting anything. It also works mid-cleanup when the new folder is gone, which is the only way out of a lost drive.

### Backup and restore

- **Backup.** With a custom folder, "Include book files" reads the owned copies from it and stores them as `documents/<id>/…`. `library.json` and `recent.json` are archived from memory with their paths rewritten to `<source_root>\documents\<id>\…`, and the position records follow the rewritten keys. The archive is an ordinary v1 backup that Android and older builds restore. Including books with the folder unplugged fails with "The library folder … isn't connected. Connect it or turn off Include book files."
- **Restore.** The setting never changes on restore: the current record is copied into the stage. With a custom folder, each owned book in the archive goes through the same copy-verify-rename routine (`placed_id`, `place_book`). A folder with an identical tree is reused, and a differing one gets a fresh id (SHA-256 of the archive id and the tree digest). Nothing in the folder is deleted. Paths under the archive's `documents\<id>` map to the placed copy. A backup without books maps them to the copies already in the folder. A books archive is refused while the folder is unplugged.
- Backup and restore are refused while a journal is pending.

### App (`crates/iced-shell`)

- `platform::pick_folder` is `IFileOpenDialog` with `FOS_PICKFOLDERS`, on its own thread with its own COM apartment. `platform::open_in_explorer` runs `explorer <path>`.
- `library_ui.rs` has the Settings section above Library backup, the banner, and the move job. The job copies `word_translation::start_package_job`: a named thread, a bounded channel, an abortable task and a generation guard.
- A move sets `profile.busy`, so `interactive`, `can_open`, `exit_ready` and `picker_open` hold everything else, and forwarded opens wait in the inbox. Cancel is always clickable. Closing the window sets the cancel flag and `pending_exit`.
- `profile_ready()` is false while a journal is pending, so Change, Use default folder, backup and restore show "Finish the library move first." Retry and Stop using this folder use `profile_settled()`, the same rule without the journal check.
- A finished move, Retry, Find it and Stop using all reload Recent and the library through `reload_profile()`, the helper a restore uses.

### Checks

- `library_location.rs` tests run in a child test process with a temporary `LOCALAPPDATA`:
  - the default folder without a record;
  - a missing custom folder that is never recreated;
  - an unreadable record that fails closed;
  - `plan` refusals, including a junction;
  - a move with literal paths, progress and positions;
  - a round trip that keeps the latest position;
  - cancel before the first file, mid-book and after the last book;
  - recovery of `Copying` and `Cleaning`, including an unreachable target twice and then Stop using;
  - Find it;
  - backup to a default-layout archive and its restore on a default profile;
  - an unplugged folder with "Include book files";
  - restore into a custom folder.
- Fault injection (`every_interrupted_step_converges`) fails, then crashes, every step of `run_move` and `finish` in both directions. After each one, `recover()` runs twice. Every card and Recent entry must open, every book's bytes must be in the current folder, positions must resolve, and the other folder must hold no copies.
- `isolated.rs`: a missing folder refuses Add document and can be forgotten. A move through the app messages holds a forwarded open until it ends, then opens it, and the card opens from the new folder.
- `settings_profile.rs` renders the new section and still expects one damage region.
- `release-smoke.ps1` scrolls Settings to the library folder and checks that showing it wrote no setting.

### Accepted for now

- A crash between restore's two renames (an existing gap) now also loses the setting, so the next start uses the default folder. Books in the custom folder are untouched, and the previous profile is still in `.simPl-before-restore-*`.
- Two simPl processes on one profile (only after an LF-2 forward fails) can overwrite each other's library edits, as they already can. The lock only keeps two moves, or a move and a recovery, apart.
- `recover()` runs before the window opens. After a crash mid-move, rolling back or finishing a large library delays the first window.
- A restore that fails after placing books in a custom folder leaves those copies there without cards. A later move copies them too.
- Every move copies, even on the same drive, so it needs twice the space while it runs.
- A card whose path can't be rewritten (its copy is missing from the new folder) keeps reading the old copy, which stays in the old folder.

## Phase LF-4: Docs and release

- README "Library and local data" (`README.md:185`):
  - Open vs Add document.
  - Library vs Recent.
  - Add to library.
  - The library location setting.
  - How to make simPl the default ("Always open with simPl").
- Release notes for 0.1.6. Reply on issue #1 linking the release.
- Existing users: nothing is migrated automatically. Their library books stay imported. To read a PDF from its original location, they Open it. Opened that way, it starts a new reading position, because positions are keyed by path.
  Follow-up idea: when an opened file's content matches an imported book, offer to carry over its position.

## Order and size

| Phase | Size | Depends on |
|---|---|---|
| LF-1 step 2 (OpenLibrary bug) | small | — |
| LF-1 rest | medium | — |
| LF-2 | medium | LF-1 (in-place mode) |
| LF-3 | large | LF-1 |
| LF-4 | small | all |

LF-1 + LF-2 solve issue #1. LF-3 can ship in a later release.

## Decisions

1. **Drag-and-drop opens without copying.** It behaves the same as Open. There's no modifier for importing; use **Add document** to import.
2. **Library location moves imported book files only** (`documents\`). Settings, positions, history, notes, covers and caches stay in `%LOCALAPPDATA%\simPl`.
3. **A second launch reuses the running window** (single instance, LF-2 step 2).
4. **Opened books appear in Recent only (option B).** The library screen lists imported books. Recent rows offer **Add to library**, which imports a copy.
   We may revisit this later with "linked" library cards (option A): cards that point at the original, with shelves and favourites but no copy.
