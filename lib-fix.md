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
| Remove | deletes simPl's copy (current text) | Recent → Remove deletes the history entry only (current behaviour) |
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
- HTML book folders need a packaged EPUB. Opened in place, it is generated into the disposable cache (`cache_base()\simPl\converted\<key>`) and rebuilt when the folder changes. The Recent entry still points at the folder.

Legacy library entries that already point outside `managed::root()` (made by today's Locate, or restored from a backup) stay on the library screen. They open in place and are never silently replaced by a copy.

## Phase LF-1: Open in place

1. Replace `open_with_import(path, locate, import: bool)` with an explicit mode: `enum Storage { InPlace, Import }`.
   Every call site picks a mode. Nothing infers it from the presence of `locate`.
2. **Fix the bug in `OpenLibrary` (`app.rs:3420`).** Never call `managed::import` for an existing library entry. A card backed by a copy opens that copy; a legacy entry outside `root()` opens in place.
   Ship this first, on its own commit, with a regression test.
3. **Library cards only for imports.** `load_document` returns `catalog: Some(..)` only when one of these holds:
   - the mode is `Import`;
   - the opened path is inside `managed::root()`;
   - the path already belongs to a library entry, so its progress and cover stay updated.
   Otherwise it returns `catalog: None`. Recent is still updated via `remember_recent`.
4. Loading in place inside `load_document`:
   - canonicalize the path;
   - reject links and junctions (reuse `managed::regular`);
   - keep the 512 MiB limit;
   - convert TXT and MD in memory;
   - package HTML folders into the cache.
5. UI:
   - Point Ctrl+O at Open: a file dialog, then open in place.
   - Point drop at Open, and update the drop overlay text.
   - The **Add document** card in `shelf.rs:936` keeps importing. Its subtitle becomes "Keeps a copy in your simPl library". Add an **Open file** card next to it with the subtitle "Reads it from where it is, without copying".
6. **Add to library** action on Recent rows. It calls `managed::import`. Then `recent::relocate`/`transfer_saved_position` moves the position, so the history entry points at the copy and keeps its place.
7. **Library card → Locate** imports the chosen file (see Target behaviour). Recent → Locate is unchanged.
8. Tests:
   - Each entry point produces the expected mode.
   - `OpenLibrary` never calls `managed::import`.
   - In-place opens add no library entry and write nothing under `root()`.
   - TXT/MD in-place opens write no files.
   - Opening an existing library book still updates its card's progress.
   - Add to library keeps the reading position.
   - Library Locate produces a copy; Recent Locate doesn't.

## Phase LF-2: Explorer double-click and Open with

1. In `app.rs:6745`, the startup path opens **in place**.
2. Single instance: when simPl is already running, a second launch forwards the path to it and exits. The running window then opens the file and comes to the front.
   - Use a per-user named pipe, `\\.\pipe\simPl-<user SID>`, created by the first instance. An iced subscription reads paths from it and sends `Message::OpenExternal(path)`.
   - The second process connects, writes the path (UTF-16, length-prefixed, at most 32 KiB), and exits.
     If it can't connect within about 500 ms, it starts normally. A crashed first instance must never block a launch.
   - The diagnostic modes (`--reader-poc` and others) skip single-instance handling.
   - If a book is open, use the normal close/save flow (`capture_progress`) before switching. If a modal is open (settings, a backup in progress, a remove confirmation), queue the path and open it once the modal closes.
3. Installer, `installer/simPl.iss`:
   - Also register `.htm`, `.xhtml` and `.markdown` (the README lists them as supported).
   - Add `SupportedTypes` under `Software\Classes\Applications\simPl.exe`, so the portable exe also shows up properly in "Open with → Choose another app".
   - Windows doesn't let an app make itself the default. Users choose "Always" in the Open with dialog. Document this in the README.
4. Tests:
   - Argument parsing still accepts `simPl.exe "<path>"` and `-- <path>`.
   - Pipe protocol unit tests: framing, oversize payloads, invalid UTF-16.
   - `scripts/test-installer.ps1` checks the new registry values.
   - A native QA run: double-click a PDF while simPl is closed, then again while it's open.

## Phase LF-3: Library location setting

Scope: the **library location** holds imported copies only (`documents\`). Settings, positions, history, notes, covers and caches stay in `%LOCALAPPDATA%\simPl`.
- This keeps the setting's pointer in a fixed place.
- Small state stays on the local disk.
- Large book files can live on another drive.

### Storage

- `preferences.json` gets `library_location: Option<PathBuf>`. `None` means the default `%LOCALAPPDATA%\simPl\documents`.
- `managed::root()` reads the configured location. It's resolved once at startup and after a successful move, and cached in a `RwLock`, not re-read per call.
- The chosen folder gets a subfolder `simPl Library\` with a `.simpl-library` marker file. simPl never writes to, or deletes, the user's chosen folder itself.
  Choosing a folder that already contains a `simPl Library` with a marker means "use the existing library", for example after a reinstall.
- The app mirrors the path to `HKCU\Software\simPl\Reader\LibraryLocation`, so the uninstaller can find it (Inno can't parse JSON easily).

### Settings UI (`app.rs:5687`, Library & data)

- Shows the current path, with **Change…**, **Open folder** and **Reset to default**.
- Shows the size and the number of imported books.
- Available only with no book open (same rule as Backup).

### Move procedure

1. Validate the target:
   - It's a local or mapped folder and writable.
   - It isn't inside the current library, and the current library isn't inside it.
   - It isn't inside the install folder or the profile's other folders.
   - There's enough free space (sum of `documents\` plus a margin).
   - It's not a link or junction.
2. Write a journal, `%LOCALAPPDATA%\simPl\library-move.json`, with `{from, to, state}`.
3. Transfer:
   - On the same volume, `fs::rename` of each `<sha256>` folder.
   - Across volumes, copy each folder to `to\.incoming\<id>`, verify size and SHA-256, then rename into place.
   - Show progress with Cancel, which rolls back.
4. Rewrite paths in `library.json`, `recent.json` and `shelves.json` by prefix (`from\` → `to\`), using the same normalisation as `backup::remap_paths`.
   Move the position records with `transfer_saved_position`, because the path key changes. Write everything atomically: temp file plus rename, as the existing `save_at` functions do.
5. Save `library_location` and the registry value.
6. Only then delete the old folders. Use the `owned_folder` and `check_tree` checks, never a blind recursive delete.
7. Delete the journal.

At startup, if a journal exists, either finish the move (the copy is verified and paths were rewritten) or roll back. Do this before the library loads, and tell the user what happened.

### Unavailable location (unplugged USB drive, offline network share)

- Don't silently reset to the default.
- Show imported entries as missing, with a banner: **Library folder not found** → Retry / Choose folder… / Reset to default.
- Recent entries for files opened in place keep working.

### Other code that assumes the default location

- `book_map.rs` checks `managed::root()`.
- `managed::source_kind`.
- `backup.rs`:
  - "Include book files" must read from the configured location.
  - The manifest records `library_root` alongside `source_root`; bump the manifest version, keep reading v1.
  - Restore writes book files into the current library location.
- The uninstaller, `simPl.iss:151` `ProfilePath`: also offer to delete the library location from the registry value. Delete only the `simPl Library` folder that has the marker.

### Tests

- Moves on the same volume and across volumes (use two temp roots; across volumes, force the copy path).
- Cancel and failure at each step roll back.
- A journal with each `state` resumes or rolls back correctly.
- Paths and position records are rewritten.
- Recent entries outside the library are untouched.
- An existing marked library is adopted.
- Refuses nested or invalid targets.
- Backup and restore round-trip with a non-default location.
- Uninstall QA in `scripts/test-installer.ps1`.

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
