# Visual correctness / BiDi reference (reader-workload fixture)

Fixture revision: `reader-workload-fx-3`.

Visual correctness is tracked separately from plain-text copying (see
`selection-cases.md`). This table specifies the direction/font/style inputs
and properties to inspect for each curated fixture item. It is a specification,
not a test-run report: use the Iced real-window drivers to check the current
renderer, font resolution, wrapping, selection, and clipboard behavior.

## Pinned reference sources

- Unicode 17.0.0, UAX #9 revision 51:
  https://www.unicode.org/reports/tr9/tr9-51.html
  (short excerpts below; redistributed under the Unicode License,
  see `../licenses/Unicode-License.txt`).
- Unicode 17.0.0 UCD `BidiCharacterTest.txt`
  (https://www.unicode.org/Public/17.0.0/ucd/BidiCharacterTest.txt,
  dated 2025-07-30): an 11-line verbatim excerpt is checked in as
  `BidiCharacterTest-excerpt.txt`. These lines are authoritative
  expected outputs (resolved levels + visual reordering) for their
  exact code-point sequences, verified per UAX #9 through rule L2
  inclusively. Out of scope of that file and of the ordering claims
  below: rule L4 (mirroring of paired brackets) and rule L3's display
  adjustment — L3 itself states that combining marks applied to a
  right-to-left base character *precede their base character* at this
  stage and that the mark/base order must be reversed if the rendering
  engine expects marks to follow their base in final display.
- Unicode 17.0.0, UAX #14 revision 55,
  https://www.unicode.org/reports/tr14/tr14-55.html (§3.1), quoted for
  the East Asian line-break expectation below: “In these scripts, lines
  can break anywhere, except before or after certain characters. The
  precise set of prohibited line breaks may depend on user preference
  or local custom and is commonly tailorable.”

Excerpts (UAX #9 rev 51, quoted with attribution):

- §3.3.6 Resolving Implicit Levels: “Right-to-left text will always end
  up with an odd level, and left-to-right and numeric text will always
  end up with an even level. In addition, numeric text will always end
  up with a higher level than the paragraph level.” Rules I1/I2 follow.
- §3.4 rule L3: “Combining marks applied to a right-to-left base
  character will at this point precede their base character. If the
  rendering engine expects them to follow the base characters in the
  final display process, then the ordering of the marks and the base
  character must be reversed.”
- §3.5 Shaping: “Cursively connected scripts, such as Arabic or Syriac,
  require the selection of positional character shapes that depend on
  adjacent characters (see Section 9.2, Arabic of [Unicode]). Shaping is
  logically applied after Rule I2 of the Bidirectional Algorithm and is
  limited to characters within the same level run.”
- §4.4 Bidirectional Conformance: “The Unicode Character Database [UCD]
  includes two files that provide conformance tests for implementations
  of the Bidirectional Algorithm [Tests9]. One of the test files,
  BidiTest.txt, comprises exhaustive test sequences of bidirectional
  types up to a given length … The other test file, BidiCharacterTest.txt,
  contains test sequences of explicit code points, including, for
  example, bracket pairs.”

## Controlled font baseline: roles, instance, and fallback order

This is the single font policy for both adapters. It is a PoC comparison
input, not a product typography decision, and it prescribes no shaping
engine.

- Requested instances: `NotoSansJP[wght].ttf` is a variable font whose
  *intrinsic* default instance is `wght=100` (the name-table face is
  Thin); the fixture baseline explicitly requests `wght=400` for all
  Japanese text. Static Latin/Arabic/Hebrew faces carry no instance
  choice.
- Ordered selection per text, by script of each logical run:
  1. Latin letters, digits, and Latin punctuation → the Latin face
     matching the requested style: regular for normal text, `latin-bold`
     for bold runs, `latin-italic` for italic runs; headings use
     `latin-bold` at heading size (heading runs request bold).
  2. Arabic-script runs (including Arabic-Indic digits) →
     `NotoSansArabic-Regular.ttf` at its supplied regular weight.
  3. Hebrew-script runs → `NotoSansHebrew-Regular.ttf` at its supplied
     regular weight.
  4. Japanese/CJK runs → `NotoSansJP[wght].ttf` at requested
     `wght=400`, even where a style run requests bold/italic.
- Where a style requests a face that is not vendored (bold or italic
  Arabic/Hebrew/Japanese), the supplied regular script face is used;
  adapters must **not** synthesize weight or slant and must not
  silently substitute a machine font where one of these files covers
  the text.
- Characters outside this set (e.g. U+1D11E, U+1F642) have **no**
  baseline coverage: their appearance (tofu, fallback to a system font,
  or omission) is recorded as an observation, not a pass/fail.

| Role | File | Requested instance |
|---|---|---|
| latin-regular | `assets/fonts/NotoSans-Regular.ttf` | static, as-is |
| latin-bold | `assets/fonts/NotoSans-Bold.ttf` | static, as-is |
| latin-italic | `assets/fonts/NotoSans-Italic.ttf` | static, as-is |
| arabic | `assets/fonts/NotoSansArabic-Regular.ttf` | static, as-is |
| hebrew | `assets/fonts/NotoSansHebrew-Regular.ttf` | static, as-is |
| japanese | `assets/fonts/NotoSansJP[wght].ttf` | variable; baseline requests `wght=400` (file's intrinsic default is 100) |

## Visual inspection table

Item IDs below reference actual workload items; `fixture_consistency.rs`
asserts that every ID in this table resolves. Logical indices in the
“expected properties” cells are 0-based positions into the item's
logical text.

| Item | Direction / style inputs | Inspect | Expected properties |
|---|---|---|---|
| `p-00001` (English) | LTR; latin-regular + latin-bold run | Styled Latin phrase, wrapping | The `stationery shop` phrase uses the bold face without changing logical letter order. Text before and after the bold run uses the regular face; wrapping at 480 DIP does not break selection across lines. |
| `p-00002` (Arabic + Latin + digits) | RTL base; italic run on `Latin` | Base direction, shaping, embedding | Paragraph lays out right-to-left. Arabic letters join into their contextual positional forms (UAX #9 §3.5; Unicode §9.2); the shadda in `مدرّسة` sits above its base, not displaced. `42` and `Latin` form LTR runs embedded in the RTL line (numeric text at a level higher than the paragraph level, UAX #9 §3.3.6). |
| `p-00003` (exact excerpt sequence) | RTL base | Segment ordering | Exact sequence of `BidiCharacterTest-excerpt.txt` source line 252 (`(اא) 1-2`, pd=RTL). Logical indices: 0 `(`, 1 `ا`, 2 `א`, 3 `)`, 4 space, 5 `1`, 6 `-`, 7 `2`. Recorded resolved levels: `1 1 1 1 1 2 2 2`. Recorded visual left-to-right order of logical indices: `5 6 7 4 3 2 1 0`. A renderer's line must match the recorded sequence (subject only to the out-of-scope L4 mirroring of the bracket pair). |
| `p-00004` (exact excerpt sequence) | RTL base | Mixed Hebrew/Arabic/Latin | Exact sequence of `BidiCharacterTest-excerpt.txt` source line 49 (pd=RTL). Logical indices and code points: 0–2 = `א ב ג` (U+05D0 U+05D1 U+05D2), 3 space, 4 `(`, 5–7 = `d e f`, 8 space, 9–11 = `ا ب ج` (U+0627 U+0628 U+062C), 12 `)`, 13 space, 14–16 = `a b c`. Recorded resolved levels: `1 1 1 1 1 2 2 2 1 1 1 1 1 1 2 2 2`. Recorded visual left-to-right order of logical indices: `14 15 16 13 12 11 10 9 8 5 6 7 4 3 2 1 0`. The recorded sequence is the oracle; any prose reading of it is an aid, not the oracle. Ordering claim only: L4 mirroring is out of scope. |
| `p-00005` (Hebrew + Latin + digits) | RTL base | Base direction, neutrals | Hebrew runs RTL; `123` and `English` embed as LTR sub-runs; sentence colon and period resolve to the RTL base level. |
| `p-00006` (Japanese) | LTR; japanese face at `wght=400`, latin faces for embedded Latin/digits | CJK glyphs, wrapping | Japanese glyphs come from `NotoSansJP[wght].ttf` at the requested instance. Break opportunities are East Asian style (UAX #14 rev 55 §3.1): lines can break anywhere *except before or after certain characters* — e.g. no break immediately before `、` or `。`. The *selected* wrap positions among the legal opportunities are tailorable/engine-dependent and not prescribed; Latin/digit runs must not be split mid-run. No blanket any-character claim is made. |
| `p-00007` (decomposed) | LTR | Combining marks | `café`/`naïve` are stored decomposed (`e`+U+0301, `i`+U+0308); marks attach to their preceding base with no stray/box mark, and selection offsets keep base and mark as distinct scalars. Original normalization is preserved; NFC rendering is not required. |
| `p-00008` (ligature candidate) | LTR; latin-regular/bold | Ligature shaping | `ffi` may or may not form a ligature (font/shaper choice); it must not render with overlapping or substituted wrong glyphs. Non-gating: a non-ligating result is acceptable. |
| `p-00009` (style-run rendering) | LTR; latin-regular/bold/italic | Style boundaries | The bold run and the italic run render with their requested Latin faces; normal text between them uses `latin-regular`; run edges must land on the recorded character boundaries (no glyph of a neighboring run leaks into the styled range). |
| `p-00010` (long paragraph) | LTR; latin-regular + latin-italic run on `several lines` | Multi-line wrap | At the 480 DIP content width the paragraph wraps across several lines; the *selected* wrap positions are engine-dependent and not prescribed; no hyphen insertion is expected from the fixture text itself. |
| `p-00011` (probes) | LTR | Supplementary scalar + emoji | U+1D11E and U+1F642 are **not** covered by the baseline fonts; their rendering (fallback, tofu, omission) is a recorded non-gating observation. The byte-offset selection contract must still hold. |
| `p-00012` (digit probes) | LTR; latin-regular/bold, arabic regular | AN/EN digit runs, bold policy | `١-1` mirrors the class sequence of `BidiCharacterTest-excerpt.txt` source line 142 (`0661 002D 0031`, pd=LTR: levels `2 0 0`, visual order `0 1 2`). Within the bold run over `١-1`: the Arabic-Indic `١` (U+0661) uses the supplied regular Arabic face (no synthesized weight), while the ASCII `-` and `1` use `latin-bold`. ASCII digits elsewhere use the Latin faces. |
| `h-0001`, `h-0002` (headings) | LTR; latin-bold at heading size | Heading weight/size | Headings render with `NotoSans-Bold.ttf` at the recipe's heading size/line height; no machine-font substitution. |
| `img-0001` (image block) | n/a | Image rendering | The manifest-listed PNG renders at the recipe's 240×160 display size, stretched per the documented display size (aspect: intrinsic 480×320 matches 240×160 aspect 1.5); nonuniform content must remain visibly nonuniform (no solid fill). |

## Explicit non-claims

- Correct resolved levels/reordering do not prove glyph shaping, font
  fallback, hit testing, or native window rendering. Arabic joining,
  mark placement, and CJK/wrapping expectations above describe what to
  inspect; actual results must identify their build, fixture, and host.
- Pixel-perfect output across renderers is not required; directional order,
  mark placement, font-role selection, and wrapping properties are.
- Emoji rendering is a non-gating probe; the baseline font set does not
  promise coverage.
