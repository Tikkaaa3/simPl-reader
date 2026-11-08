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

`shelf::Card { Write, Refresh }` decides what a finished load may do to the library. `Storage::card()` is the only map: `Import` writes a card and `InPlace` refreshes one. A `Refresh` updates a card that already exists for the opened or the replaced path and never inserts one. The shelf decides at write time. Writes that arrive while the library is still loading wait in its queue and are decided after the load. This stops the startup open, which can finish first, from adding a card.

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
