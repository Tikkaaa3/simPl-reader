# Selection/copy reference cases (reader-workload fixture)

Fixture revision: `reader-workload-fx-3`.

Endpoints are stable text-item IDs plus zero-based UTF-8 byte
offsets; ranges are start-inclusive/end-exclusive. Offsets are valid
character boundaries and never bisect a base-plus-combining-mark
sequence. These conventions are fixture-only, not a production
locator contract.

Plain-text copy semantics: preserve logical character order and
original normalization; concatenate style runs without extra
separators; join selected text items (including headings) with one
LF; add no terminal LF unless present in the selected source text;
image items contribute no text and no additional separators;
reversing anchor/focus yields the same text; a collapsed range
yields the empty string. Canonical references are LF-only; a future
Windows clipboard transport may normalize LF to CRLF on write
without changing these files.

Expected copied strings are stored as checked-in UTF-8 golden files
in `expected-copy/<case>.txt`, produced by this authoring script
directly from the semantics above — not by the Rust extraction
logic under test.

| Case | Anchor (item @ byte) | Focus (item @ byte) | Intent |
|---|---|---|---|
| `collapsed` | p-00010 @ 217 | p-00010 @ 217 | Zero-length selection: anchor and focus are the same byte offset; the copied text is empty. |
| `style-boundary` | p-00009 @ 22 | p-00009 @ 56 | Within one paragraph: starts inside the bold run, ends before the trailing plain tail; plain text between the styled runs is included. |
| `long-within-paragraph` | p-00010 @ 99 | p-00010 @ 392 | Long range inside one paragraph; at the 480 DIP content width it is intended to span more wrapped lines than one screen row shows. |
| `adjacent-paragraphs` | p-00005 @ 0 | p-00006 @ 152 | Two consecutive paragraphs joined by exactly one LF. |
| `cross-paragraph-partial` | p-00005 @ 32 | p-00006 @ 77 | Selection starts mid-paragraph and ends mid-paragraph across the paragraph boundary. |
| `cross-image-heading` | p-00008 @ 0 | p-00009 @ 85 | Range crosses an intervening image item (contributes no text, no separator) and a heading (contributes its text, joined with LF). |
| `across-viewport` | p-00001 @ 0 | p-00012 @ 67 | Twelve paragraphs plus the intervening heading at the shared layout recipe exceed the 600 DIP viewport, so this range requires scrolling; the intervening image contributes nothing. |
| `reversed` | p-00006 @ 152 | p-00005 @ 0 | Same content as adjacent-paragraphs with anchor and focus swapped; the copied text is identical. |
| `rtl-mixed` | p-00002 @ 0 | p-00002 @ 150 | Whole Arabic paragraph with Latin and digits embedded; stored in logical order. |
| `cjk` | p-00006 @ 0 | p-00006 @ 152 | Whole Japanese paragraph with Latin and digits embedded. |
| `combining` | p-00007 @ 10 | p-00007 @ 29 | Range covers complete base-plus-combining-mark sequences; endpoints must not bisect a mark from its base. |
| `supplementary` | p-00011 @ 22 | p-00011 @ 41 | Range covers an astral-plane scalar (U+1D11E) so byte offsets and scalar indexing stay distinct. |
