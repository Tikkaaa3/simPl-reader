# simPl book parser roadmap

> This file retains milestone decisions and implementation history. The current
> reading model uses fixed global pages, zoom without repagination, one visible
> paper, and PDF prose with separate illustrations. PDF conversions are cached;
> blank source pages remain blank. Later follow-ups supersede earlier reflow and
> text-only conversion plans. See the [README](README.md) for current usage.

Status: milestones 1–3 implemented (2026-09-27), with automated checks and
CPU-rendered visual previews verified. Live desktop interaction qualification
remains pending; milestone 4 is planned.
This extends the existing [project roadmap](roadmap.md).

## Goal

Give simPl its own quiet, consistent book-reading experience while keeping
original-layout document reading available where supported. Build on the
existing HTML/EPUB extractor and native reflow renderer.

The first deliverable is a real EPUB or HTML document displayed in simPl's
default minimalist book style, with working light/dark switching and reliable
reading-position preservation.

## Reading modes

| Mode | Behavior | Initial availability |
|---|---|---|
| Document | Preserve the source page layout, fonts, figures, tables, and formulas through the existing PDF renderer. | Default for PDF. |
| Book | Render structured content using simPl's typography, spacing, reading width, and palette. Reflow when size or viewport changes. | Default for reflowable EPUB and HTML; explicit opt-in for suitable PDFs. |

- Remember an explicit mode choice per document once both modes are supported.
- Offer the mode switch only when that document has two usable representations.
- Keep source files unchanged; book rendering is a view of the content.
- The existing HTML/EPUB view already extracts and reflows content. Faithful
  publisher CSS/layout rendering for these formats is separate future work;
  an original-layout HTML/EPUB mode is not part of the first milestone.
- Preserve a source location when changing modes. Keep each mode's last
  position and use source mapping to move to the corresponding passage where
  possible, falling back to a known source page when an exact match is absent.

## Default style: simPl Minimal

Ship one built-in book style initially: **simPl Minimal**, selected by default.
These are starting values to refine with actual books and user feedback. Store
them in one style definition so later changes do not require parser changes.

| Property | Starting value |
|---|---|
| Reading font | Bundled Literata Regular; existing bold and italic faces for emphasis. Preserve script-aware fallback. |
| Controls | Bundled Inter, matching the existing application. |
| Body size | 20 logical pixels by default; preserve the existing 12–36 size controls. |
| Line height | 1.6 times body size. |
| Reading column | Centered, maximum 36 em, shrinking to the available viewport. At the default size this is 720 logical pixels. |
| Side padding | 32 logical pixels on wide windows, reducing to 16 on narrow windows. |
| Paragraphs | 0.75 em after paragraphs, no first-line indent, start-aligned text with a ragged trailing edge. Respect RTL direction. |
| Headings | Literata Medium; chapter title 1.6 em, lower headings 1.3/1.1 em; generous spacing and a clear hierarchy. |
| Images | Preserve aspect ratio, fit within the reading column, avoid enlarging small images unnecessarily. |
| Captions | 0.85 em with secondary text color. |
| Decoration | Plain reading surface, restrained separators, no paper texture, drop caps, ornamental frames, or page-turn animation. |
| Navigation | PDF Book preserves source pages as separate paper sheets with previous/next controls; EPUB/HTML reflow into paper sheets with page controls and Contents. |

The style has two palettes, sharing the same typography and geometry:

| Token | Light | Dark |
|---|---|---|
| Reading background | `#F6F3EC` — warm off-white | `#171A1E` — soft charcoal |
| Main text | `#282723` | `#DDDAD2` |
| Secondary text | `#66635D` | `#AAA79F` |
| Link/accent | `#355F85` | `#8DB8DE` |

Use **Light** on a fresh installation unless an explicit appearance preference
has already been saved. Subsequent launches restore the user's choice. Sepia
and additional styles can follow after the two initial palettes work well.

## Upper-right toolbar: font size and light/dark controls

- [x] In Book mode, show separate font-size decrease (`−`) and increase (`+`)
  buttons beside the light/dark toggle in the upper-right toolbar.
- [x] Reuse the existing font-size controls and 12–36 logical-pixel limits;
  disable the respective button at its limit. Provide `Decrease font size` and
  `Increase font size` tooltips, descriptive labels, and keyboard focus.
- [x] Reflow immediately after a font-size change while preserving the current
  reading passage, and save the chosen size with the document's preferences.
  PDF Document mode retains its existing zoom controls.

- [x] Add one compact light/dark toggle at the right end of the top toolbar.
- [x] Use a sun/moon icon with a tooltip describing the action: `Switch to dark
  mode` or `Switch to light mode`. Include keyboard focus and a descriptive label.
- [x] Switch instantly without reopening or reparsing the document, resetting
  selection, changing reading position, or triggering a layout change.
- [x] Apply the selected appearance consistently to the application chrome and
  the Book reading surface, using separate tokens for each surface.
- [x] In PDF Document mode, change surrounding controls and background while
  preserving the original page colors and embedded images.
- [x] Persist appearance as a global preference across documents and launches.
  Keep it independent of the per-document Document/Book choice.
- [x] Keep these controls reachable through the existing toolbar reveal behavior
  when reading controls are collapsed.
- [x] Verify both palettes for legible text, selection, and focus states. Internal
  link behavior remains in milestone 2.

## Architecture

Keep three responsibilities separate:

1. **Format loading and extraction:** EPUB/HTML adapters produce structured
   content; the existing PDF page renderer remains available independently.
2. **Book model:** content identity, chapter order, semantic blocks, inline
   emphasis, links, and source locations. It contains no palette or font choices.
3. **Presentation:** book style plus reader preferences drive the existing
   native layout, selection, and rendering path.

Extend `reader-document` and the current renderer incrementally. Reuse the
existing HTML parser, EPUB package loader, bundled fonts, and PDF worker.
Keep expensive loading/conversion off the UI thread, retain bounded caches,
and preserve on-demand EPUB chapter loading and viewport virtualization.

Treat typography settings and palette changes separately: typography can
require reflow; changing only colors should not. Retain content-based reading
anchors through reflow and reopening. Version persisted preferences and provide
defaults for existing records without discarding valid saved positions.

## Delivery order

### 1. Minimal Book view and appearance controls

- [x] Extract the default book style into a single presentation definition.
- [x] Apply simPl Minimal to the existing EPUB/HTML reading path.
- [x] Implement upper-right font-size `−` / `+` buttons, the light/dark toggle,
  and preference persistence.
- [x] Keep existing font-size settings, contents, selection/copy, and resume.
- [x] Make the style independent from source-document styling and shell colors.

**Done when:** a real EPUB and local HTML document open in the default style;
light/dark works immediately; resizing or changing font size keeps the same
passage in view; closing and reopening restores the passage and preferences.

### 2. Richer book structure

- [x] Extend the current heading/paragraph/image model with lists, quotations,
  scene breaks, captions, and preformatted text without losing source order.
- [x] Preserve stable source identities for chapters, blocks, and link targets.
- [x] Support internal links and footnotes, including returning to the passage
  from which a footnote was opened.
- [x] Define explicit handling for tables and formulas; preserve meaningful
  content and report unsupported structures rather than silently dropping them.
- [x] Style the added structures through the same simPl Minimal definition.

**Done when:** representative structured EPUB/HTML books retain their meaning,
emphasis, navigation, images, and logical copy order under both palettes.

### 3. PDF to Book conversion

- [x] Start with text-based, single-column books and an explicit Book action.
- [x] Use existing PDF text extraction as input; reconstruct lines and
  paragraphs, accounting for repeated headers/footers and line-end hyphenation.
- [x] Keep cleanup conservative: preserve meaningful punctuation and text, and
  verify against the source instead of assuming every line break is accidental.
- [x] Preserve source page and available text-range mappings for converted blocks.
- [x] Support returning to the corresponding original page from Book mode.
- [x] Preserve relevant illustrations or clearly explain conversion limitations
  before presenting a result as usable Book content.
- [x] Keep Document mode available when conversion fails or is unsuitable;
  preserve the currently readable view and reading position on failure.
- [x] Respect existing PDF extraction permissions and input/resource limits.

**Done when:** selected real single-column PDF books can be read in Book mode
with checked text order, reliable source-page navigation, and independent
position persistence for both modes.

Scanned/image-only PDFs need a future OCR stage. Complex columns, equations,
tables, and general academic PDF conversion are outside this first conversion
milestone. Their original pages remain readable in Document mode.

### 4. External themes and later plugins

- [ ] Define a versioned declarative theme format for typography, spacing,
  palette, and supported block styles after the built-in style is established.
- [ ] Add local theme packages and a chooser; keep simPl Minimal as the fallback.
- [ ] Validate theme values and font assets before activation; report invalid
  packages without interrupting reading.
- [ ] Keep theme loading independent from parsing and source-content changes.
- [ ] Introduce broader plugin extension points only when concrete use cases
  require them; initial external themes need no executable plugin code.

**Done when:** a local theme can change a book's presentation without changing
its content, selection/copy order, or saved reading location.

## Validation for each implemented milestone

Use focused automated checks for parsing, source mappings, preference migration,
and anchor restoration, plus visual inspection of actual book content. Cover
narrow/wide windows, both palettes, font-size changes, RTL/mixed-script text,
long chapters, images, selection/copy, and close/reopen behavior as relevant.

Check that the reader stays responsive and idle without continuous redraws.
Run the repository's required formatting, lint, and test checks when code
changes land. Updating this roadmap alone does not implement these features.

## Milestone 1 implementation and evidence

- `crates/iced-shell/src/book_style.rs` defines the built-in reading typography,
  responsive column, heading spacing, and light/dark book surfaces.
- The toolbar, shell, library, settings, and surrounding PDF controls share
  theme-aware chrome colors. Existing original-layout PDF page rendering is retained.
- `crates/reader-document/src/preferences.rs` stores global appearance separately
  from existing HTML/EPUB/PDF reading positions. Older position records and saved
  font sizes remain usable; missing preferences default to Light.
- Automated checks cover unchanged geometry and selection on appearance changes,
  reflow anchors, font bounds, serialized appearance writes, late startup reads,
  missing/older preference records, corrupt files, and visible save failures.
- Real local Project Gutenberg Alice HTML and EPUB were rendered through the
  production widget tree in both palettes, at 1280/540 px widths, and at the
  maximum 36 px body size. Selection, focus outlines, toolbar wrapping, text
  contrast, and scrollbar clearance were inspected in the generated PNGs.
- Machine-local evidence: `target/book-milestone/previews/`,
  `target/book-milestone-check.log`, and `target/book-milestone-build.log`.
  Reproduction instructions are in the [reader README](crates/iced-shell/README.md#book-appearance-visual-qa).
- Native mouse/keyboard interaction, live close/reopen, and cross-DPI inspection
  remain a desktop verification follow-up. Offscreen rendering and state/storage
  tests are not represented as successful live desktop runs.

## Milestone 2 implementation and evidence

- `reader-document` retains structural metadata alongside the existing selectable
  items: list/quote nesting, figure associations, captions, scene breaks,
  preformatted text, footnotes, table rows and formula fallbacks. Inline links
  carry UTF-8 ranges and reference/note/backlink roles. Presentation is separate.
- Identity is scoped by source fingerprint, canonical EPUB section href, item ID
  and source fragment. Reparsing unchanged input is deterministic; font/palette
  changes never regenerate identities. List markers join their first paragraph,
  retaining old paragraph ordinals and aliases for the former marker item.
- Text links navigate within the current HTML or between EPUB sections. Reflowable
  manifest notes outside the linear spine are loaded on demand without entering
  Next/Previous chapter order. External/unsupported targets report an error.
- Click opens a link; dragging selects its text. Visible links participate in
  Tab/Enter navigation, with native glyph bounds used for hit testing and focus.
  The toolbar return arrow and Alt+Left restore the previous item and fractional
  offset, including after font changes. The session history holds at most 64
  passages; failed/stale loads do not commit or consume it.
- Tables use selectable rows in source order with an explicit layout notice.
  MathML uses supplied `alttext` or its serialized source, with an explicit
  mathematical-layout notice. This is not table-grid or formula typesetting.
  Image-only links, publisher CSS and original HTML/EPUB layout remain outside
  this implementation; the original PDF view is unchanged.
- simPl Minimal styles all added blocks: indented quotations/nested lists,
  smaller captions/notes, subdued separators, and shaded code/table/formula
  blocks. Code uses platform monospace, preserving bold/italic emphasis.
- Validation: 231 workspace tests passed, formatting and Clippy passed. Tests
  cover deterministic extraction, Unicode link ranges, internal/auxiliary EPUB
  targets, rejected paths, click versus drag (including RTL), clipped hits,
  logical copy order, keyboard link activation, return anchors, and failed/stale
  navigation. Native widget previews cover both palettes, 1280/540 px widths,
  20/36 px text, nested lists, captions/images, formulas, notes and link focus.
- Reproducible authored input: `fixtures/book-structure/`. Machine-local evidence:
  `target/book-milestone2-check.log`, `target/book-milestone2-build.log` and
  `target/book-milestone2/previews*`. These offscreen renders and state tests do
  not claim live desktop interaction, cross-DPI or clean-machine qualification.


## Milestone 3 implementation and evidence

- PDF opens in Document mode by default. **Book** shows the text-only conversion
  boundaries before extraction; **Cancel** or Escape leaves the original view.
  A successful conversion uses simPl Minimal, the existing font −/+ controls,
  light/dark appearance, selection/copy and reflow anchors.
- `reader-pdf::book` reconstructs prose on the PDF worker from native text and
  geometry. It removes repeated margin templates, distinguishes paragraph gaps,
  indents and larger headings, and conservatively joins line-end hyphens. A
  visible hyphen is removed only with evidence of the joined word elsewhere in
  the document; source punctuation otherwise remains. Source page boundaries remain intact, including sentences continuing onto the next page. This is heuristic prose reconstruction,
  not recovery of publisher semantics or inline bold/italic styling.
- Every block keeps versioned, fingerprint-scoped identity and source page,
  UTF-8 text ranges and normalized vertical bounds. **Original page N** opens
  the source near the current passage; this mapping is approximate within a
  reflowed paragraph. **Document** restores that mode's separate last position
  and zoom. The explicit mode choice, Book font/anchor and Document page/zoom
  survive reopening and fingerprint-checked file relocation independently.
- Image-only or unsupported individual pages receive explicit original-page
  reminders. If more than one quarter of the pages are unusable, conversion
  fails and retains the original reader and position. No OCR, image extraction,
  multi-column reading-order recovery, formula layout or PDF annotation import
  is claimed. Conversion notices remain available after opening the Book view.
- Extraction respects PDF permissions, the existing source/page limits, and
  conversion caps of 2,000 pages, 16 MiB text and 100,000 lines. Native handles
  stay on the worker; cancellation is checked between pages and cleanup passes.
  Request identity prevents canceled/stale loads from replacing the view.
- Validation: formatting, Clippy and 246 workspace tests passed. Native PDFium
  conversion and production Iced CPU previews also passed for authored prose,
  [Alice (111 pages)](https://www.planetebook.com/alices-adventures-in-wonderland/)
  and [Frankenstein (277 pages)](https://www.planetebook.com/frankenstein/).
  Every converted range was checked against extracted source text for byte
  boundaries, geometry and text order, allowing only documented whitespace and
  hyphen cleanup. Both books keep a visible reminder for their cover page.
  Both palettes, 1280/540 px widths and 20/36 px fonts were inspected. Isolated
  save/reopen checks exercised both mode preferences and independent positions.
- Authored column, image-only, permission-restricted and 5,000-page PDFs were rejected while
  the original page remained renderable. Reproducible inputs and instructions
  are in [fixtures/pdf-book](fixtures/pdf-book/README.md) and the
  [reader README](crates/iced-shell/README.md#pdf-book-conversion-and-qa).
  Machine-local evidence is in `target/book-milestone3/`,
  `target/book-milestone3-check.log` and `target/book-milestone3-build.log`.
  The release build passed and `target/portable/simPl/simPl.exe` was refreshed;
  its SHA-256 matches the release binary:
  `06a90ba5aa27a682b27d3bdb1abab84860ffb9d579fa8f6df26a24f466a330a9`.
  These are native offscreen and state/storage checks; live desktop input,
  cross-DPI and clean-Windows qualification remain separate follow-ups.


## PDF Book page presentation follow-up

- Preserve each source PDF page as a separate paper surface, with visible edges,
  contrasting surroundings, margins, a footer number and a gap between pages.
  Each page has a portrait minimum height and expands when larger text needs
  more room; content is never cropped to force a fixed paper size.
- Previous/next buttons and Left/Right keys navigate source pages. The toolbar
  shows current/total pages, and disables navigation at the ends. Page Up/Down
  and wheel scrolling remain available within long pages.
- Retain virtualized paragraphs and sheets. Geometry maps paper coordinates back
  to content anchors so resizing, text size, source navigation and saved Book
  positions remain meaningful. Converter v2 stops merging across source pages;
  saved v1 positions migrate to their first source block.
- Native checks cover navigation boundaries, both palettes, 1280/540 px windows,
  20/36 px fonts, source text/ranges, position migration and independent reopen.
  Evidence: `target/book-pages/`, `target/book-pages-check.log` and
  `target/book-pages-build.log`. Live desktop/DPI qualification remains pending.

The page-presentation release build passed and the portable executable was
refreshed. Formatting, Clippy and all 246 workspace tests passed.


## Reader chrome refinement (2026-09-27)

User-requested presentation update: hide routine document notice banners, open
PDF Book directly without a preflight panel, remove Copy buttons (retain Ctrl+C),
and keep the PDF Book toolbar on one line. Document now returns to the mapped
source page; the redundant Original page button is removed. Library appearance
moves beside Search in the existing titlebar; reading appearance stays beside
font controls. These supersede the earlier preflight/two-button UI description.

The real Crime and Punishment EPUB passed native whitespace-selection, multiline
drag, exact copy and clipped-hit checks. Both-palette previews show the highlight.
Its HTML counterpart retains all 55 headings. The continuous presentation gap
identified here is resolved by the follow-up below. Evidence: `target/reader-cleanup/`.


## HTML/EPUB pages and library ownership (2026-09-27)

- [x] Paginate HTML and EPUB into visible paper sheets with top/bottom margins;
  retain all blocks; long paragraphs now continue across sheets at line boundaries.
- [x] Add HTML heading-based Contents; center page controls and Contents in Book
  mode, with `book ~ chapter` in the top window title.
- [x] Left/Right turns pages; Up/Down scrolls inside pages. EPUB page navigation
  crosses chapters; its counter is per section.
- [x] Add Document and drops create private copies under
  `%LOCALAPPDATA%\simPl\documents`; copy accepted HTML local images too.
  Existing library entries migrate when opened from the shelf.
- [x] Hover/focus card Favourite and Remove actions, persistent Favourites section,
  physical deletion restricted to owned copies, originals preserved.
- [x] Remove Recent from Settings and rename keyboard help to Shortcuts.

Native QA covers the real Downloads Crime and Punishment HTML/EPUB, both palettes,
narrow/wide views, page/chapter navigation, favourite persistence, action-button
hit handling and managed-copy removal. Evidence: `target/reader-library/`.

Validation: formatting, Clippy with warnings denied, and all 249 workspace tests
passed. Real-book native previews and managed-library workflow checks also passed.


## Long-paragraph pagination correction (2026-09-27)

The initial block-only paginator could produce extremely tall sheets for source
paragraphs spanning thousands of characters (reproduced in the user's saved
Crime and Punishment CHAPTER III). Reflow now splits these at native line-height
boundaries, keeps source text/IDs unchanged, clips input and drawing to each
sheet, and maps fractional paragraph anchors to the correct continuation page.
PDF Book source-page preservation is unchanged. Native QA checks the real chapter
at 12/20/36 px, navigation, selection/copy and margin clipping.

Library action icons now use hover styling without tooltip text. Remove opens a
compact Cancel/Remove confirmation; cancel, Escape and backdrop dismissal leave
the copy intact. Explicit Remove starts the existing owned-copy deletion worker.


### Final-sheet navigation correction

Count the most visible sheet and reserve trailing scroll space so the last sheet
can align with the viewport top. Fixes the 21/22 counter and blocked next-chapter
transition at the end of Crime and Punishment CHAPTER III. Regression QA verifies
22/22 and the transition to CHAPTER IV, plus PDF Book navigation.


### Stable chapter totals

Replace estimated HTML/EPUB heights with a complete native measurement pass on
the task executor before displaying pages. Commit the height map atomically and
keep it fixed during scrolling. Reuse it for same-width navigation; recompute on
font or width changes. Cancel obsolete work and preserve source anchors. Native
regression checks audit visible layout and verify page totals/boundaries across
scrolling and chapter changes. Evidence: `target/stable-pagination/`.


## Fixed global pages and zoom (2026-09-27)

This supersedes chapter-local pagination and font-size-driven reflow.

- [x] PDF Book uses the original PDF page boundaries and count.
- [x] EPUB 3 page-list, EPUB 2 NCX pageList, and HTML page lists/pagebreak markers preserve publisher labels and boundaries. Inline page targets split logical text at the actual source boundary.
- [x] Without publisher pages, measure the whole book once at the canonical Literata 20 px, 720 px paper / 624 px text width. Persist the map by content fingerprint in the app's `page-maps` directory. Chapters retain lazy live content; the map retains only row heights and page ranges.
- [x] Global page input (Enter; Ctrl+L to focus) resolves pages across EPUB sections. Previous/next crosses section boundaries. Contents remains chapter navigation.
- [x] +/− scales the completed paper (40–300%) and pointer coordinates. Zoom, window resizing and scrolling never regenerate the map. Zoom is session-scoped; reading anchors remain saved per document.
- [x] Publisher labels are accepted in the input (including roman numerals); numeric ordinal is the fallback. Supplemental EPUB notes do not change the main-book page count.
- [x] Regression tests cover publisher lists, pagebreak boundaries, fixed geometry, cancellation, global navigation, reopen, and transformed native text selection.

Native QA on the supplied Crime and Punishment files: EPUB 886 pages; HTML 892 pages. Page 400 resolves across chapters; the different files retain separate maps. Evidence: `target/global-pages-qa/`, `target/global-pages-qa.log`, `target/global-pages-check.log`. These are real Iced layouts/renders and state tests, not live desktop input qualification.


## Brand and source-page synchronization (2026-09-27)

- [x] Explicit PDF Document ↔ Book switches follow the currently displayed source page;
  old saved positions apply to reopening, not deliberate view switches.
- [x] Normalize numeric/Roman publisher label decorations (`{45}` → `45`) in display and
  cached maps. Pride and Prejudice's 453 publisher pages and boundaries remain unchanged.
- [x] Apply supplied light/dark monochrome brand tokens throughout chrome, controls,
  library and Book paper. UI uses Geist 400/560; reading remains Literata.
- [x] Validate PDF page 38 → Book page 38 and Book page 45 → Document page 45 with native
  reader state, and render both themes with the real Pride and Prejudice EPUB.


## Single-page Book reading and toolbar collapse (2026-09-27)

Book mode now renders only the selected paper (EPUB, HTML and PDF Book). Wheel,
Up/Down and Page Up/Down scroll within that paper; Left/Right and the global page
input turn pages. Existing source/generated page maps, counts and zoom remain
unchanged. Native scroll offsets are page-local while saved source anchors remain
stable; stale scroll notifications cannot move a newly selected page.

The reader toolbar collapses to zero height. Its restore/hide control lives in the
existing title bar; F8 also toggles it. The paper gains the entire toolbar height,
and the scrollable retains its widget position/state. Hidden controls leave keyboard
focus navigation; Contents and Help close with the toolbar.

Validation: required fmt/Clippy/workspace tests passed. Native widget-tree QA checked
both Alice EPUB editions, Let's Go, Pride and Prejudice, Crime and Punishment, the
EPUB smoke book, two HTML files and the 111-page Alice PDF Book. At 40%, 100%, and
180% zoom the scroll content contains exactly one paper, toolbar collapse releases
its height, and page navigation preserves total counts. Evidence:
`target/single-page-check.log`, `target/single-page-books-qa.log`,
`target/single-page-pdf-qa.log`, and `target/single-page-qa/`.
