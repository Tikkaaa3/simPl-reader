# Shared reader layout

`reader-layout` owns the book section model, bundled reading fonts, paragraph
layout, typography/themes, height index, canonical atlas and theme adaptation.
It uses Iced's core/widget layout and tiny-skia renderer without `iced_winit`,
an event loop or a native window. The patched cosmic-text stack is the same as
the desktop's. Both Android ABIs compile this crate through `reader-ffi`.

The desktop retains its selectable widgets, highlighting and chrome. Its
paragraph widget and this crate share the shaping request and layout rule in
`paragraph`; a test compares their measured heights for every golden book and
reading option. Desktop modules re-export the moved data and atlas functions.

## Canonical pages

- Measure at the default typography: 720 px paper, 48 px margins, 20 px body.
- Reflow long paragraphs on the line grid. Source page lists and PDF pages keep
  their own labels and boundaries.
- Adapt each section to a theme/options by mapping canonical cuts to matching
  paragraph lines. Global numbers and labels stay fixed.
- Keep the existing `v1-<fingerprint>.json` schema and PDF conversion version.
  Desktop cache paths remain unchanged; Android uses `cacheDir/simPl/page-maps`.
- `atlas::build_with_cache(..., false)` avoids reading or writing user caches
  when comparing layouts. Cancellation is checked between items/sections,
  including before accepting a cached result.

## Parity tests

`golden` writes eight project-authored fixtures from embedded inputs: structured
HTML/EPUB, long prose HTML, chapter EPUBs with/without source pages, imported TXT,
imported Markdown and PDF Book. ZIP creator metadata is fixed to the original
Windows value, so Android produces identical input fingerprints.

`tests/golden/*.json` was recorded with the desktop measurement path before its
move. It includes canonical atlases and six theme/font/size/margin/spacing
adaptations. The Windows test compares all eight fixtures byte-for-byte. The
device test embeds the recorded JSON and requires the six portable fixtures to
match byte-for-byte; the two structured fixtures include system monospace or
script fallback and are reported separately. System-font coverage remains the
known limit (including Arabic, Hebrew and CJK text absent from bundled faces).
PDFium must be staged beside host tests for PDF Book; device tests require it.

```powershell
cargo test -p reader-layout
cargo test -p iced-shell golden_atlas
powershell -NoProfile -ExecutionPolicy Bypass -File scripts/android.ps1 test
```

Only an intentional layout change should replace the reference JSON:

```powershell
$env:SIMPL_RECORD_GOLDEN = '1'
cargo test -p iced-shell desktop_page_atlases_match_the_golden_books
Remove-Item Env:SIMPL_RECORD_GOLDEN
```

## Android API

`reader-ffi::open_book(path)` immediately returns an `OpenBookTask`. Poll
`status()` and `result()` from a Kotlin background coroutine; errors are
`CoreException.Failed`. `cancel()` or closing a pending task requests cooperative
cancellation. PDF conversion finishes before layout observes cancellation.

The resulting `OpenBook` exposes its fingerprint, `atlas()`, exact `atlas_json()`
and `page(number)`. The global `atlas(fingerprint)` looks up a live book; retain
the `OpenBook` handle while using it. Page numbers are **one-based** at the FFI
boundary. A publisher page can span chapters, so `page` returns all section
fragments with rows, labels and start/end paragraph cuts (`row`, `line`, `lines`).

`adapt(theme, LayoutOptions)` starts an `AdaptBookTask`. Its result is a separate
`AdaptedBook`, preserving the canonical atlas and page identities. Unknown themes
fall back to Default; options use the existing validation bounds. PDF Book keeps
canonical typography and physical pages. Close task/book/adaptation handles when
finished (`use` in Kotlin). Rendering and gestures belong to later milestones.
