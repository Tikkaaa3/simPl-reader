# Next milestone: reading appearance settings

> Milestone 3 of the [feature roadmap](feature-roadmap.md) and nothing else
> (toolbar and find are done). Later items (TXT/Markdown, notes, collections,
> dictionary, text to speech) stay out. Facts below come from reading the code
> and running `cargo deny list` on 2026-09-29; anything else is a decision to
> make when the work starts.

## Goal

Let people make the reader comfortable for their eyes, and remember it:

- **Theme:** Light and Dark exist; add **Sepia**.
- **Font:** choose among a few bundled reading fonts.
- **Line spacing** and **margins** (narrow / normal / wide).
- **Text size:** already exists as Book zoom, but only for the current session;
  remember it too.
- Settings apply to all books by default, and a book can keep its own.

Two columns are **not** in this milestone (see "Left out").

## Licensing: what we must not break

Keep it simple. This milestone should add **no new Rust crates**, only a few
font files, so the checks are short.

1. **Nothing paid.** Only fonts under the SIL Open Font License 1.1 or
   Apache-2.0 (or public domain). No commercial fonts (for example Bookerly),
   no font subscriptions, no "free for personal use" fonts from font websites,
   and no bundling of Windows fonts such as Georgia or Cambria. We already get
   fonts from the Google Fonts repository at a pinned commit
   (`assets/licenses/Typeface-SOURCES.txt`); keep doing that.
2. **Nothing against our own license.** Our source license (PolyForm
   Noncommercial) and the free official-release terms (`LICENSE-BINARY.txt`)
   need no change: a bundled OFL font stays under its own license, like Inter,
   Literata and Geist do today. Every current dependency is permissive
   (*verified*), and no new dependency means no new risk.
3. **What OFL asks of us, in plain words.** We may embed the font in the app and
   ship the app freely. We must ship the font's copyright line and license text,
   and not sell the font on its own. If a font declares a *Reserved Font Name*
   and we change the file (for example by cutting a static weight out of a
   variable font, as we did for Geist), the changed file needs a different
   family name. We already did this: `simPl UI 560`.
4. **For each new font, before adding it:** read the license file of the exact
   file, write down the copyright line and whether a Reserved Font Name is
   declared, then add the license to `assets/licenses/`, an entry to
   `Typeface-SOURCES.txt`, and the file to the notice list in
   `scripts/package.ps1`. That list is hard-coded, so a font added without it
   would ship without its notice; add a small check that fails packaging when a
   file in `assets/fonts/` has no notice.
5. **Theme names and looks** are our own ("Sepia", our colors). Do not name or
   copy the styling of another reader.

## What the code tells us

- Book text uses Literata (five bundled faces). `reader.rs` builds the fonts
  from the family name in one place. Inter is in `assets/fonts/` with its
  license already shipped, but only Regular/Medium/SemiBold, so it has no bold
  or italic for reading.
- Layout numbers are constants: `MINIMAL` in `book_style.rs` (line height 1.6,
  column width, paragraph gap) is used 27 times in `app.rs` and `book_map.rs`,
  and `PAPER`/`MARGIN`/`TEXT` (720 / 48) about a dozen times. They must become
  values that can change.
- **The page map is cached on disk per book** (`page-maps/v1-<fingerprint>.json`)
  and is checked only against the book's fingerprint. Change line spacing,
  margins or font, and the cached pages are wrong until the cache knows which
  settings it was built for. This is the real risk of the milestone.
- Themes: `Appearance` is Light or Dark, saved in `preferences.json` (small,
  versioned). `ui::palette` decides the colors by asking whether the iced theme
  is dark, which cannot express a third theme. The sun/moon button flips
  between two states.
- The place for the controls exists: the *Reading Settings* panel already holds
  Book zoom (session only) and window controls.

## Work, in order

### 1. Settings model and storage
- A `ReadingStyle` value: theme, font, line spacing, margin, text size.
- Global defaults in `preferences.json`; new fields are optional so old files
  still load. Per-book override in a small versioned file keyed by the book
  fingerprint, written with the same atomic-write helper.
- **Done when:** an old `preferences.json` loads unchanged, a book's override
  wins over the default, and unknown values fall back safely (tests).

### 2. Layout values instead of constants, and a safe page cache
- Replace `MINIMAL`/`PAPER`/`MARGIN`/`TEXT` with a style value carried by the
  reader and passed to measuring, pagination and rendering, so all three always
  agree.
- Put a **layout signature** (line spacing, margins, font, text width) inside
  the cached page map and rebuild when it differs. Keep one file per book and
  overwrite it, so the cache does not grow with every setting change.
- The reading position is saved by item and fraction, so it survives the
  repagination; page **numbers** will change, so say so in the settings text.
- **Done when:** changing spacing changes the page count, the reader stays on the
  same passage, and a stale cache is never used (tests, plus the existing
  preview harness in `book_preview.rs`).

### 3. Sepia and the theme selector
- Add `Sepia` to `Appearance` (old files stay valid) and choose palettes by
  theme, not by "is dark". Replace the two-state button with a small selector.
- Pick colors by measured contrast: body text at least 4.5:1 on its background
  (a unit test computes it), secondary text at least 3:1.
- The theme also colors the app chrome; PDF **Document** view shows page
  images and is unchanged, PDF **Book** view follows the theme.

### 4. Reading fonts
- Decide the list at the start, **at most three families** (each is about
  1 MB for four faces): Literata stays the default; add one more serif and one
  clear sans, or an easy-to-read face such as Atkinson Hyperlegible. Every
  family must have regular, bold, italic and bold-italic, all OFL, checked as
  in the licensing rules. Inter would need bold and italic faces added the same
  way we made the existing ones (`Typeface-SOURCES.txt` records the method).
- Measure each family with the same renderer used for pagination, since font
  metrics change every page break.

### 5. Settings panel
- In *Reading Settings*: theme, font, line spacing, margins, text size, a
  "This book only / All books" choice and "Reset". Show a live sample like the
  existing Literata preview.
- Keyboard: every control gets a `Control` entry so Tab and Enter work, and the
  F1 help lists it.

### 6. Check and package
- Full test, clippy and `cargo deny list` (expect no new crates).
- Run `scripts/package.ps1`; confirm `third-party/fonts` has a notice for every
  bundled font and the font check from the licensing rules passes.
- Look at every theme and font in the release build, on a short book and on a
  long EPUB and a PDF in Book view.

## Left out, on purpose

- **Two columns.** The reader is built around one 720-wide page with a global
  page map, selection and find on top of it; a second column touches all of
  them. Revisit it as its own milestone.
- **Dark rendering of PDF Document pages** (inverting page images).
- Other roadmap items, including anything to do with dictionaries.

## Risks

- **Stale page maps** if the signature misses a setting that affects layout:
  list the settings once and test each one.
- **Slow first open after a change**, because pages are measured again: reuse
  the existing "Preparing pages…" state and do not block the window.
- **Font metrics** differ per family; measure with the real font, never with an
  estimate.
- **Binary size and notices** grow with each font: cap at three families.
- A change of page numbers can confuse anyone who noted "page 214"; keep the
  page-number field working and explain it in the panel.
