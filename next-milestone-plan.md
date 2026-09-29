# Next milestone plan: personal reading tools

> Plans the work after in-document find and the PDF toolbar (both done). Read the
> [feature roadmap](feature-roadmap.md) first; this file orders what remains and
> puts a licensing gate in front of every new dependency, font and data file.
> Facts marked *verified* were checked on 2026-09-29 (`cargo deny list` on the
> current lockfile, `cargo info` on crates.io); everything else is a decision or
> a thing to verify when the work starts.

## Goal

Make simPl comfortable for long reading sessions and study, without adding a
network dependency, a database or a licensing obligation we cannot meet:
adjustable reading appearance, TXT and Markdown files, and bookmarks with
highlights and notes. Collections are optional. Dictionary and text-to-speech
are **not** in this milestone; they only get a licensing research spike so the
next milestone starts with a decision instead of a surprise.

## Licensing: where we stand

- **Our terms.** Source is PolyForm Noncommercial 1.0.0 (`LICENSE.md`); official
  binaries are free under `LICENSE-BINARY.txt`; a separate commercial license is
  offered by contact. Third-party code and fonts keep their own licenses, which
  ship in `third-party/` with every package (`scripts/package.ps1`,
  `scripts/collect-licenses.ps1`, `assets/licenses/`, PDFium notices).
- **Dependencies today** (*verified*): everything in the lockfile is permissive
  (MIT, Apache-2.0, BSD-2/3, ISC, Zlib, 0BSD, Unlicense, BSL-1.0, Unicode-3.0).
  Two crates offer a copyleft option next to a permissive one and we must keep
  choosing the permissive one: `self_cell` (Apache-2.0 **OR** GPL-2.0-only) and
  `r-efi` (MIT OR Apache-2.0 **OR** LGPL-2.1-or-later). `mac` declares
  MIT/Apache-2.0 but ships no license file; `collect-licenses.ps1` already has
  an explicit entry for it.
- **What is missing.** There is no automated policy: no `deny.toml`, no CI
  license check, and the font notice list in `package.ps1` is hard-coded, so a
  newly bundled font would ship without its notice unless someone remembers.
  There is no `CONTRIBUTING` file, yet we offer a commercial license: outside
  contributions cannot be relicensed without the contributor's grant.

## Rules for this milestone

1. **No copyleft in the binary.** No GPL, LGPL, AGPL, MPL (unless the owner
   accepts it in writing), CC-BY-SA, "non-commercial" or field-of-use licenses,
   including for data files and fonts. If a crate offers `X OR GPL`, record that
   we use `X`.
2. **Every new crate is checked before it is added:** license on crates.io *and*
   an actual license text in the package (the collection script fails
   otherwise), its whole new dependency tree, and `cargo deny check licenses`.
3. **Every new font, icon or data file** gets a notice file in
   `assets/licenses/`, an entry in `Typeface-SOURCES.txt`, and is shipped by the
   package script. Only OFL 1.1 / Apache-2.0 / public-domain assets. New icons
   come from the Material Symbols subset with the change recorded in
   `Material-Symbols-CHANGES.txt`.
4. **Nothing copyrighted enters the repository as a fixture.** Test books are
   authored here or clearly public domain with provenance noted, as in
   `fixtures/`.
5. **No code copied from other readers.** Most well-known open readers are GPL
   (Calibre, Foliate, Sioyek, SumatraPDF, Zathura). Use them for behavior ideas
   only, never for code, and never by pasting from memory.
6. **No system voices, dictionaries or fonts are redistributed.** Using what
   Windows already provides is fine; bundling it is not.
7. **User data stays local.** Notes and highlights quote copyrighted books, so
   they live only on the user's disk and are exported only by the user's action.
8. **The name and logo rule stays.** New features do not add third-party
   trademarks to the UI or docs.

## Work packages, in order

### WP0. License gate (do first, about a day)
- Add `deny.toml` for the shipped target (`x86_64-pc-windows-msvc`): an allow
  list of the licenses above, `unknown-registry`/`unknown-git` denied, private
  workspace crates ignored (they are `publish = false`).
- Run `cargo deny check licenses bans sources` in the release workflow before
  packaging; fail the build on a new license.
- Make the font notice list in `package.ps1` come from `assets/fonts/` and fail
  if a font has no notice.
- Regenerate `crates/iced-shell/dependency-inventory.txt` and note how it is
  produced.
- Decide the contribution policy (see open decisions) and add `CONTRIBUTING.md`.
- **Done when:** a deliberately added GPL crate fails CI, and removing a font
  notice fails packaging.

### WP1. Reading appearance settings
- Sepia theme, line spacing, margins and font choice among bundled faces
  (Literata, Inter, Geist), remembered per book; two columns only if pagination
  can stay stable (the page map must not change on zoom).
- **Licensing:** no new crate. If a font is added it must be OFL 1.1 or
  Apache-2.0, with the reserved-font-name rules read first; otherwise use faces
  already bundled. System font enumeration is allowed, since nothing is shipped.
- **Research:** which settings change pagination and so invalidate the cached
  page map (`book_map.rs`); per-book storage next to reading position.

### WP2. TXT and Markdown
- Map both to the existing reflow items; register `.txt`/`.md` in imports
  (`managed.rs`), `document_kind` and the installer associations.
- **Encoding:** UTF-8 with or without BOM, UTF-16 BOM, then the Windows ANSI code
  page through `MultiByteToWideChar`. `reader-document` already uses
  `windows-sys` for globalization, so no new crate is needed.
- **Markdown parser:** `pulldown-cmark` 0.13.4 (*verified*: MIT; with default
  features off it adds only `unicase`, MIT OR Apache-2.0; `bitflags` and
  `memchr` are already in the tree). Fallback: `comrak` (BSD-2-Clause,
  *verified*), heavier. Avoid `encoding_rs`: `(Apache-2.0 OR MIT) AND
  BSD-3-Clause` is acceptable but two notices for a job Win32 already does.
- **Scope:** headings, paragraphs, lists, quotes, code, links and local images;
  no raw HTML passthrough, no remote resources, as with HTML today.
- **Fixtures:** author them; do not import sample books.

### WP3. Bookmarks, highlights and notes
- Keyed by book fingerprint, one small versioned JSON file per book beside the
  reading position (`position.rs` is the model). Anchor by item id and byte
  range for reflow, by page and glyph range for PDF, plus the quoted text so a
  changed parser can re-find it.
- Highlight color and note in a small panel; a list of marks per book; jump
  from list to place. Reuse the find highlight and exact-scroll code.
- Export to Markdown on demand (user action only).
- **Licensing:** no new dependency. Store only what the user marked; never send
  it anywhere; say so in the README.
- **Risk:** anchors drifting when parsing changes. Version records and keep the
  quote.

### WP4. Collections and tags (optional, if time remains)
- Shelves and filters in `shelf.rs`; extend the library entry format without
  breaking existing files (`library.rs` validates size and entry count).
- No licensing impact.

### Research spikes (documents only, no product code)
- **Dictionary data.** Bundled data is the risky part, and for Turkish users the
  obvious sources are not free: proprietary national dictionaries cannot be
  shipped; Wiktionary-derived data is CC BY-SA (share-alike); FreeDict is
  mostly GPL. Candidate policy: **ship no dictionary, and let the user import
  their own StarDict-style files**, so the user holds the data license. Confirm
  the file format is free to implement and pick a parser; check WordNet's
  license only if English data is wanted.
- **Text to speech.** Use Windows' own engines (SAPI or WinRT
  `SpeechSynthesizer`) so no voice is bundled; check the `windows` crate
  (0.62.2, MIT OR Apache-2.0, *verified*) features needed and how much a
  second `windows-*` version adds to the tree. Rule out espeak-ng (GPL-3.0) and
  third-party neural voices (per-voice licenses, some restricted).
- Output: one short decision note each, added to the next milestone plan.

## Order and gates

`WP0` → `WP1` → `WP2` → `WP3` → `WP4`, with the spikes alongside. A package is
done only when all of these hold:

1. `cargo fmt`, `cargo clippy --workspace --all-targets` and
   `cargo test --workspace` are clean.
2. `cargo deny check licenses` is green with the new work included.
3. `scripts/package.ps1` produces a package whose `third-party/` contains a
   notice for every new crate, font or icon change.
4. The README and `LICENSE-BINARY.txt` still describe the product truthfully;
   they change only if a real term changes.
5. Behavior is tried in the release build (`target\release\iced-shell.exe`), not
   only in tests.

## Open decisions for the owner

- **Contributions.** Either accept no outside pull requests for now, or require
  a contributor grant that allows relicensing (needed for the commercial
  license). Which one?
- **MPL.** Rule 1 refuses MPL-licensed crates. Keep it that strict?
- **Dictionary policy.** Is "user imports their own dictionaries, nothing
  bundled" acceptable as the default?
- **Scope.** Is WP4 in or out of this milestone?
- **Windows-only.** Text to speech and the Win32 encoding path assume Windows;
  fine while the product is Windows-only, but worth stating.

## Risks

- A transitive dependency changes license between releases: the pinned lockfile
  and the CI check catch it, but only if WP0 lands first.
- Bundled font families carrying reserved names or non-OFL terms: verify the
  license text of the exact file, not the family page.
- Stored anchors and highlight data outliving a parser change: version and test
  them.
- Notes exports containing large excerpts of copyrighted books: keep export a
  deliberate action and warn in the UI text.
