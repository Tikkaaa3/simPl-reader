#!/usr/bin/env python3
"""Authoring-time generator for the reader-workload fixture package.

This script is the single authoring source for:

* ``crates/reader-workload/src/curated.rs``      (curated prelude texts + style runs)
* ``crates/reader-workload/src/selection_cases.rs`` (named selection/copy cases)
* ``fixtures/reader-workload/manifest.txt``       (asset manifest with SHA-256)
* ``fixtures/reader-workload/references/selection-cases.md``
* ``fixtures/reader-workload/references/expected-copy/<case>.txt`` golden files

It is NOT part of the Rust library or its tests: normal generation and
verification never run it. Golden expectations are produced here, by an
independent Python implementation of the documented copy semantics, so
they do not come from the Rust extraction logic that validates them.

Run from the repository root after (re)defining curated content or
replacing an asset:

    python fixtures/reader-workload/tools/build_fixture_data.py
    cargo fmt --all

(The generated Rust files are emitted unformatted; run `cargo fmt --all`
afterwards so the workspace's format check stays green.)

The script is deterministic and offline. It refuses to run if a
referenced asset or license file is missing.
"""

import hashlib
import pathlib
import sys

REPO = pathlib.Path(__file__).resolve().parent.parent.parent.parent
FIXTURE = REPO / "fixtures" / "reader-workload"
CRATE_SRC = REPO / "crates" / "reader-workload" / "src"

FIXTURE_REVISION = "reader-workload-fx-3"

# ---------------------------------------------------------------------------
# Pinned upstream sources (recorded in the manifest).
# notofonts.github.io snapshot that provided the Latin/Arabic/Hebrew binaries:
NOTOFONTS_REF = "29e0840b6666c8603be41de7d5c8ac5bc2d64699"
# notofonts family source repos providing the authoritative OFL texts:
LGC_REF = "023d7b73d5c1a5ed6489bd04a120244c1f2bff3f"
ARABIC_REF = "834a11352b9ee26f827c745757dc380ef8df68bf"
HEBREW_REF = "a9e04ff21deca0b7814ab041886b90b296e7034f"
# google/fonts snapshot that provided the Japanese variable font:
GOOGLE_FONTS_REF = "e44c4b011a820c2cbe2fd2cfa8052037d7edb571"
# Unicode 17.0.0 UCD snapshot providing BidiCharacterTest.txt:
UNICODE_UCD = "https://www.unicode.org/Public/17.0.0/ucd/BidiCharacterTest.txt"

NOTOFONTS_URL = (
    "https://github.com/notofonts/notofonts.github.io/tree/"
    + NOTOFONTS_REF
    + "/fonts"
)
JP_URL = (
    "https://github.com/google/fonts/tree/"
    + GOOGLE_FONTS_REF
    + "/ofl/notosansjp"
)

# ---------------------------------------------------------------------------
# Curated workload content. Text is stored in logical Unicode order.
# Arabic/Hebrew content is NEVER visually pre-reversed.
# ---------------------------------------------------------------------------

IMAGE_ASSET_PATH = "assets/images/reader-sample.png"
IMAGE_INTRINSIC = (480, 320)  # pixels

HEADINGS = [
    ("h-0001", "Reader Workload Fixture", 1),
    ("h-0002", "Styled Passages", 1),
]

# (id, text, base_direction, [(styled substring, style), ...])
BOLD, ITALIC = "Bold", "Italic"
PARAGRAPHS = [
    (
        "p-00001",
        "The little stationery shop glowed beside the rain-soaked street "
        "as evening readers passed by.",
        "Ltr",
        [("stationery shop", BOLD)],
    ),
    (
        "p-00002",
        "هذا نص تجريبي بالعربية، وفي وسطه الرقم 42 ومصطلح Latin، والكلمة "
        "مدرّسة تحمل علامة شدة.",
        "Rtl",
        [("Latin", ITALIC)],
    ),
    # Exact character sequence of BidiCharacterTest.txt line 252 (pd = RTL):
    # 0028 0627 05D0 0029 0020 0031 002D 0032
    (
        "p-00003",
        "(اא) 1-2",
        "Rtl",
        [],
    ),
    # Exact character sequence of BidiCharacterTest.txt line 49 (pd = RTL):
    # 05D0 05D1 05D2 0020 0028 0064 0065 0066 0020 0627 0628 062C 0029
    # 0020 0061 0062 0063
    (
        "p-00004",
        "אבג (def ابج) abc",
        "Rtl",
        [],
    ),
    (
        "p-00005",
        "קטע עברית לבדיקה: מספרים 123 וגם English בתוך הטקסט.",
        "Rtl",
        [],
    ),
    (
        "p-00006",
        "日本語のテキストは横書きで表示され、Latin や数字 123 と混在します。"
        "折り返しは単語の境界に限定されません。",
        "Ltr",
        [("123", BOLD)],
    ),
    (
        "p-00007",
        "The words café and naïve remain decomposed in logical order.",
        "Ltr",
        [],
    ),
    (
        "p-00008",
        "The efficient staff finished the difficult briefing in the office.",
        "Ltr",
        [("efficient", BOLD)],
    ),
    (
        "p-00009",
        "Normal text carries a bold emphasis and an italic aside before it "
        "continues normally.",
        "Ltr",
        [("bold emphasis", BOLD), ("italic aside", ITALIC)],
    ),
    (
        "p-00010",
        "The reader opens a long paragraph that will wrap across several "
        "lines at the narrow content width. Selection should be able to "
        "start inside the first wrapped line and end after the last one. "
        "Nothing in this paragraph depends on the renderer that will draw "
        "it later. The words stay in logical order from the first byte to "
        "the last. Wrapped line count is a layout observation for the UI "
        "tasks, not a property of this text. Copying this paragraph must "
        "reproduce these sentences exactly as stored.",
        "Ltr",
        [("several lines", ITALIC)],
    ),
    (
        "p-00011",
        "Glyph probes: musical clef (U+1D11E) 𝄞, smiling face (U+1F642) 🙂; "
        "both are non-gating for the baseline font set.",
        "Ltr",
        [],
    ),
    (
        "p-00012",
        "Digit probes: Arabic-Indic ١-1 beside ASCII 6-6, mixed with Latin.",
        "Ltr",
        [("١-1", BOLD)],
    ),
]

# Workload prelude order (document order of curated items).
ORDER = [
    "h-0001",
    "p-00001",
    "p-00002",
    "p-00003",
    "p-00004",
    "p-00005",
    "p-00006",
    "p-00007",
    "p-00008",
    "img-0001",
    "h-0002",
    "p-00009",
    "p-00010",
    "p-00011",
    "p-00012",
]

# ---------------------------------------------------------------------------
# Selection/copy reference cases.
# Endpoint coordinates: zero-based UTF-8 byte offsets into the item's
# logical text; ranges are start-inclusive/end-exclusive. Endpoint modes:
#   int 0        -> byte offset 0
#   "end"        -> byte offset == text length (valid end-of-text offset)
#   "<needle>"   -> byte offset of the first byte of the needle
# First-char expectations (recorded for endpoint-integrity validation) are
# computed here, from the curated text, not by the Rust logic under test.
# ---------------------------------------------------------------------------

CASES = [
    (
        "collapsed",
        "Zero-length selection: anchor and focus are the same byte offset; the copied text is empty.",
        ("p-00010", "depends"),
        ("p-00010", "depends"),
    ),
    (
        "style-boundary",
        "Within one paragraph: starts inside the bold run, ends before the trailing plain tail; plain text between the styled runs is included.",
        ("p-00009", "bold"),
        ("p-00009", "before it"),
    ),
    (
        "long-within-paragraph",
        "Long range inside one paragraph; at the 480 DIP content width it is intended to span more wrapped lines than one screen row shows.",
        ("p-00010", "Selection should"),
        ("p-00010", "not a property"),
    ),
    (
        "adjacent-paragraphs",
        "Two consecutive paragraphs joined by exactly one LF.",
        ("p-00005", 0),
        ("p-00006", "end"),
    ),
    (
        "cross-paragraph-partial",
        "Selection starts mid-paragraph and ends mid-paragraph across the paragraph boundary.",
        ("p-00005", "מספרים"),
        ("p-00006", "混在します"),
    ),
    (
        "cross-image-heading",
        "Range crosses an intervening image item (contributes no text, no separator) and a heading (contributes its text, joined with LF).",
        ("p-00008", 0),
        ("p-00009", "end"),
    ),
    (
        "across-viewport",
        "Twelve paragraphs plus the intervening heading at the shared layout recipe exceed the 600 DIP viewport, so this range requires scrolling; the intervening image contributes nothing.",
        ("p-00001", 0),
        ("p-00012", "end"),
    ),
    (
        "reversed",
        "Same content as adjacent-paragraphs with anchor and focus swapped; the copied text is identical.",
        ("p-00006", "end"),
        ("p-00005", 0),
    ),
    (
        "rtl-mixed",
        "Whole Arabic paragraph with Latin and digits embedded; stored in logical order.",
        ("p-00002", 0),
        ("p-00002", "end"),
    ),
    (
        "cjk",
        "Whole Japanese paragraph with Latin and digits embedded.",
        ("p-00006", 0),
        ("p-00006", "end"),
    ),
    (
        "combining",
        "Range covers complete base-plus-combining-mark sequences; endpoints must not bisect a mark from its base.",
        ("p-00007", "café"),
        ("p-00007", "remain"),
    ),
    (
        "supplementary",
        "Range covers an astral-plane scalar (U+1D11E) so byte offsets and scalar indexing stay distinct.",
        ("p-00011", "clef"),
        ("p-00011", ", smiling"),
    ),
]


def text_of(item_id):
    for _hid, htext, _level in HEADINGS:
        if _hid == item_id:
            return htext
    for pid, ptext, _dir, _runs in PARAGRAPHS:
        if pid == item_id:
            return ptext
    raise KeyError(item_id)


def bytes_of(item_id):
    return text_of(item_id).encode("utf-8")


def resolve_endpoint(item_id, mode):
    if mode == 0:
        return 0
    if mode == "end":
        return len(bytes_of(item_id))
    pos = bytes_of(item_id).find(mode.encode("utf-8"))
    if pos < 0:
        raise AssertionError(f"{mode!r} not found in {item_id}")
    return pos


def first_char_at(item_id, byte_offset):
    b = bytes_of(item_id)
    assert 0 <= byte_offset <= len(b), (item_id, byte_offset)
    assert b[:byte_offset].decode("utf-8") is not None
    if byte_offset == len(b):
        return None
    assert (b[byte_offset] & 0xC0) != 0x80, "not a char boundary"
    for width in (1, 2, 3, 4):
        try:
            return b[byte_offset : byte_offset + width].decode("utf-8")
        except UnicodeDecodeError:
            continue
    raise AssertionError("unreachable")


def expected_text(anchor, focus):
    (ai, amode), (fi, fmode) = anchor, focus
    a_off, f_off = resolve_endpoint(ai, amode), resolve_endpoint(fi, fmode)
    ai_pos, fi_pos = ORDER.index(ai), ORDER.index(fi)

    def piece(item_id, start, end):
        if item_id == "img-0001":
            return None  # image contributes no text and no separator
        return bytes_of(item_id)[start:end].decode("utf-8")

    parts = []
    if ai_pos == fi_pos:
        lo, hi = min(a_off, f_off), max(a_off, f_off)
        parts.append(piece(ai, lo, hi))
    else:
        lo_pos, hi_pos = min(ai_pos, fi_pos), max(ai_pos, fi_pos)
        lo_off = a_off if ai_pos == lo_pos else f_off
        hi_off = f_off if fi_pos == hi_pos else a_off
        between = ORDER[lo_pos : hi_pos + 1]
        for j, item_id in enumerate(between):
            if item_id == "img-0001":
                continue
            t = bytes_of(item_id)
            if j == 0:
                t = t[lo_off:]
            if j == len(between) - 1:
                t = t[:hi_off]
            # An item contributing no characters adds no text and no
            # separator (same clarified boundary rule as the Rust helper).
            if t:
                parts.append(t.decode("utf-8"))
    return "\n".join(parts)


def rust_str(s):
    assert '"#' not in s
    return 'r#"' + s + '"#'


def rust_char(ch):
    if ch is None:
        return "None"
    if ch.isprintable():
        return f"Some('{ch}')"
    return f"Some('\\u{{{ord(ch):04X}}}')"


def sha256_file(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


def main():
    required = [
        FIXTURE / "assets" / "fonts" / "NotoSans-Regular.ttf",
        FIXTURE / "assets" / "fonts" / "NotoSans-Bold.ttf",
        FIXTURE / "assets" / "fonts" / "NotoSans-Italic.ttf",
        FIXTURE / "assets" / "fonts" / "NotoSansArabic-Regular.ttf",
        FIXTURE / "assets" / "fonts" / "NotoSansHebrew-Regular.ttf",
        FIXTURE / "assets" / "fonts" / "NotoSansJP[wght].ttf",
        FIXTURE / "assets" / "images" / "reader-sample.png",
        FIXTURE / "licenses" / "OFL-NotoSans.txt",
        FIXTURE / "licenses" / "OFL-NotoSansArabic.txt",
        FIXTURE / "licenses" / "OFL-NotoSansHebrew.txt",
        FIXTURE / "licenses" / "OFL-NotoSansJP.txt",
        FIXTURE / "licenses" / "Unicode-License.txt",
        FIXTURE / "references" / "BidiCharacterTest-excerpt.txt",
        FIXTURE / "references" / "bidi-reference.md",
        FIXTURE / "tools" / "generate_sample_png.py",
    ]
    missing = [p for p in required if not p.exists()]
    if missing:
        sys.exit("missing files: " + ", ".join(str(m) for m in missing))

    # ---- curated.rs --------------------------------------------------------
    style_consts = []
    par_entries = []
    for pid, text, direction, runs in PARAGRAPHS:
        if not runs:
            runs_expr = "&[]"
        else:
            parts = []
            for needle, style in runs:
                hay = bytes_of(pid)
                start = hay.find(needle.encode("utf-8"))
                assert start >= 0, (pid, needle)
                end = start + len(needle.encode("utf-8"))
                assert hay[:start].decode("utf-8"), (pid, "start not boundary")
                assert hay[end:].decode("utf-8") or end == len(hay), (pid, "end not boundary")
                const = f"{pid.upper().replace('-', '_')}_{style.upper()}_RUN"
                style_consts.append(
                    f"pub const {const}: StyleRun = StyleRun {{ start_byte: {start}, "
                    f"end_byte: {end}, style: InlineStyle::{style} }};"
                )
                parts.append(const)
            runs_expr = "&[" + ", ".join(parts) + "]"
        par_entries.append(
            "        Paragraph {\n"
            f"            id: \"{pid}\",\n"
            f"            text: {rust_str(text)},\n"
            f"            base_direction: BaseDirection::{direction},\n"
            f"            style_runs: {runs_expr},\n"
            "        },"
        )
    head_entries = [
        f"        Heading {{ id: \"{hid}\", text: {rust_str(htext)}, level: {level} }},"
        for hid, htext, level in HEADINGS
    ]
    curated = (
        "//! Curated prelude content for the reader-workload fixture.\n"
        "//!\n"
        "//! GENERATED by `fixtures/reader-workload/tools/build_fixture_data.py`\n"
        "//! (authoring tool, not part of the Rust library, its tests, or any\n"
        "//! normal generation/verification flow). Do not edit this file by\n"
        "//! hand: change the generator, rerun it, and let the fixture\n"
        "//! revision and manifest move together.\n"
        "//!\n"
        "//! Text is stored in logical Unicode order; Arabic/Hebrew content is\n"
        "//! never visually pre-reversed. Byte offsets are UTF-8 offsets\n"
        "//! computed once by the generator and verified by fixture tests.\n"
        "\n"
        "use reader_document::{BaseDirection, InlineStyle, StyleRun};\n"
        "use PreludeItem::{Heading, Image, Paragraph};\n"
        "\n"
        "/// Fixture revision stamped on the manifest and every workload.\n"
        f"pub const FIXTURE_REVISION: &str = \"{FIXTURE_REVISION}\";\n"
        "\n"
        "/// Portable, fixture-root-relative path of the single workload image.\n"
        f"pub const IMAGE_ASSET_PATH: &str = \"{IMAGE_ASSET_PATH}\";\n"
        "\n"
        "/// Intrinsic pixel dimensions of the generated PNG.\n"
        f"pub const IMAGE_INTRINSIC: (u32, u32) = ({IMAGE_INTRINSIC[0]}, {IMAGE_INTRINSIC[1]});\n"
        "\n"
        "#[derive(Debug, Clone, Copy, PartialEq, Eq)]\n"
        "pub enum PreludeItem {\n"
        "    Heading { id: &'static str, text: &'static str, level: u8 },\n"
        "    Paragraph {\n"
        "        id: &'static str,\n"
        "        text: &'static str,\n"
        "        base_direction: BaseDirection,\n"
        "        style_runs: &'static [StyleRun],\n"
        "    },\n"
        "    Image { id: &'static str, asset_path: &'static str },\n"
        "}\n"
        "\n"
        + "\n".join(style_consts)
        + "\n\n"
        "/// Ordered curated items: 2 headings, 12 paragraphs, 1 image block.\n"
        "pub const PRELUDE: &[PreludeItem] = &[\n"
        + "\n".join(head_entries[:1])
        + "\n"
        + "\n".join(par_entries[:8])
        + "\n        Image { id: \"img-0001\", asset_path: IMAGE_ASSET_PATH },\n"
        + head_entries[1]
        + "\n"
        + "\n".join(par_entries[8:])
        + "\n];\n"
    )
    (CRATE_SRC / "curated.rs").write_bytes(curated.encode("utf-8"))

    # ---- selection_cases.rs ------------------------------------------------
    case_entries = []
    for name, notes, anchor, focus in CASES:
        a_off, f_off = resolve_endpoint(*anchor), resolve_endpoint(*focus)
        a_char, f_char = first_char_at(anchor[0], a_off), first_char_at(focus[0], f_off)
        case_entries.append(
            "        SelectionCase {\n"
            f"            name: \"{name}\",\n"
            f"            notes: {rust_str(notes)},\n"
            f"            anchor: Endpoint {{ item_id: \"{anchor[0]}\".to_string(), byte_offset: {a_off} }},\n"
            f"            anchor_first_char: {rust_char(a_char)},\n"
            f"            focus: Endpoint {{ item_id: \"{focus[0]}\".to_string(), byte_offset: {f_off} }},\n"
            f"            focus_first_char: {rust_char(f_char)},\n"
            "        },"
        )
    selection = (
        "//! Named selection/copy reference cases for the reader-workload fixture.\n"
        "//!\n"
        "//! GENERATED by `fixtures/reader-workload/tools/build_fixture_data.py`.\n"
        "//! Endpoints are stable text-item IDs plus zero-based UTF-8 byte\n"
        "//! offsets; ranges are start-inclusive/end-exclusive.\n"
        "//! `*_first_char: Option<char>` records the Unicode scalar expected\n"
        "//! to start at each endpoint (`None` = endpoint at end-of-text) so\n"
        "//! fixture tests detect text drift at the exact recorded coordinate.\n"
        "\n"
        "use crate::reference::SelectionCase;\n"
        "use reader_document::Endpoint;\n"
        "\n"
        "/// The curated selection/copy reference cases, in fixture order.\n"
        "pub fn cases() -> Vec<SelectionCase> {\n"
        "    vec![\n"
        + "\n".join(case_entries)
        + "\n    ]\n}\n"
    )
    (CRATE_SRC / "selection_cases.rs").write_bytes(selection.encode("utf-8"))

    # ---- golden files -------------------------------------------------------
    gold_dir = FIXTURE / "references" / "expected-copy"
    gold_dir.mkdir(parents=True, exist_ok=True)
    for old in gold_dir.glob("*.txt"):
        old.unlink()
    for name, _notes, anchor, focus in CASES:
        text = expected_text(anchor, focus)
        assert "\r" not in text
        (gold_dir / f"{name}.txt").write_bytes(text.encode("utf-8"))

    # ---- selection-cases.md -------------------------------------------------
    rows = []
    for name, notes, anchor, focus in CASES:
        rows.append(
            f"| `{name}` | {anchor[0]} @ {resolve_endpoint(*anchor)} | "
            f"{focus[0]} @ {resolve_endpoint(*focus)} | {notes} |"
        )
    cases_md = (
        "# Selection/copy reference cases (reader-workload fixture)\n"
        "\n"
        f"Fixture revision: `{FIXTURE_REVISION}`.\n"
        "\n"
        "Endpoints are stable text-item IDs plus zero-based UTF-8 byte\n"
        "offsets; ranges are start-inclusive/end-exclusive. Offsets are valid\n"
        "character boundaries and never bisect a base-plus-combining-mark\n"
        "sequence. These conventions are fixture-only, not a production\n"
        "locator contract.\n"
        "\n"
        "Plain-text copy semantics: preserve logical character order and\n"
        "original normalization; concatenate style runs without extra\n"
        "separators; join selected text items (including headings) with one\n"
        "LF; add no terminal LF unless present in the selected source text;\n"
        "image items contribute no text and no additional separators;\n"
        "reversing anchor/focus yields the same text; a collapsed range\n"
        "yields the empty string. Canonical references are LF-only; a future\n"
        "Windows clipboard transport may normalize LF to CRLF on write\n"
        "without changing these files.\n"
        "\n"
        "Expected copied strings are stored as checked-in UTF-8 golden files\n"
        "in `expected-copy/<case>.txt`, produced by this authoring script\n"
        "directly from the semantics above — not by the Rust extraction\n"
        "logic under test.\n"
        "\n"
        "| Case | Anchor (item @ byte) | Focus (item @ byte) | Intent |\n"
        "|---|---|---|---|\n"
        + "\n".join(rows)
        + "\n"
    )
    (FIXTURE / "references" / "selection-cases.md").write_bytes(
        cases_md.encode("utf-8")
    )

    # ---- licenses/PROJECT-AUTHORED-NOTICE.txt -------------------------------
    # Explicit LF: this notice is a manifest-hashed canonical-byte file.
    (FIXTURE / "licenses" / "PROJECT-AUTHORED-NOTICE.txt").write_bytes(
        (
            "Project-authored fixture material for initiative I-001 / W3 (T-003).\n"
            "No product license has been chosen yet; this file identifies authorship\n"
            "without inventing or claiming a product license. Third-party assets are\n"
            "licensed separately; see each manifest record's license-path.\n"
        ).encode("utf-8")
    )

    # ---- manifest.txt -------------------------------------------------------
    fonts = [
        (
            "assets/fonts/NotoSans-Regular.ttf",
            "2.015; ttfautohint (v1.8.4.7-5d5b); static hinted TTF",
            f"{NOTOFONTS_URL}/NotoSans/hinted/ttf/NotoSans-Regular.ttf (notofonts/notofonts.github.io @ {NOTOFONTS_REF})",
            "OFL-1.1",
            "licenses/OFL-NotoSans.txt",
            "latin-regular (primary Latin text; body default)",
        ),
        (
            "assets/fonts/NotoSans-Bold.ttf",
            "2.015; ttfautohint (v1.8.4.7-5d5b); static hinted TTF",
            f"{NOTOFONTS_URL}/NotoSans/hinted/ttf/NotoSans-Bold.ttf (notofonts/notofonts.github.io @ {NOTOFONTS_REF})",
            "OFL-1.1",
            "licenses/OFL-NotoSans.txt",
            "latin-bold (bold inline style; static weight 700)",
        ),
        (
            "assets/fonts/NotoSans-Italic.ttf",
            "2.015; ttfautohint (v1.8.4.7-5d5b); static hinted TTF",
            f"{NOTOFONTS_URL}/NotoSans/hinted/ttf/NotoSans-Italic.ttf (notofonts/notofonts.github.io @ {NOTOFONTS_REF})",
            "OFL-1.1",
            "licenses/OFL-NotoSans.txt",
            "latin-italic (italic inline style)",
        ),
        (
            "assets/fonts/NotoSansArabic-Regular.ttf",
            "2.013; ttfautohint (v1.8.4.16-eb64); static hinted TTF",
            f"{NOTOFONTS_URL}/NotoSansArabic/hinted/ttf/NotoSansArabic-Regular.ttf (notofonts/notofonts.github.io @ {NOTOFONTS_REF})",
            "OFL-1.1",
            "licenses/OFL-NotoSansArabic.txt",
            "arabic (Arabic script paragraphs and inline phrases)",
        ),
        (
            "assets/fonts/NotoSansHebrew-Regular.ttf",
            "3.001; ttfautohint (v1.8.4.7-5d5b); static hinted TTF",
            f"{NOTOFONTS_URL}/NotoSansHebrew/hinted/ttf/NotoSansHebrew-Regular.ttf (notofonts/notofonts.github.io @ {NOTOFONTS_REF})",
            "OFL-1.1",
            "licenses/OFL-NotoSansHebrew.txt",
            "hebrew (Hebrew script paragraphs and inline phrases)",
        ),
        (
            "assets/fonts/NotoSansJP[wght].ttf",
            "2.004-H2; variable TTF; axis wght intrinsic default 100 (Thin name-table face); fixture baseline requests instance wght=400",
            f"{JP_URL}/NotoSansJP%5Bwght%5D.ttf (google/fonts @ {GOOGLE_FONTS_REF})",
            "OFL-1.1",
            "licenses/OFL-NotoSansJP.txt",
            "japanese (Japanese/CJK paragraphs and inline phrases)",
        ),
    ]
    records = []
    for path, version, provenance, license_id, license_path, role in fonts:
        records.append(
            {
                "path": path,
                "sha256": sha256_file(FIXTURE / path),
                "version": version,
                "provenance": provenance,
                "license": license_id,
                "license-path": license_path,
                "role": role,
            }
        )
    records.append(
        {
            "path": "assets/images/reader-sample.png",
            "sha256": sha256_file(FIXTURE / "assets" / "images" / "reader-sample.png"),
            "version": f"generated, fixture revision {FIXTURE_REVISION}",
            "provenance": (
                "project-authored; deterministically generated by "
                "tools/generate_sample_png.py (Python stdlib zlib, no network); "
                "intrinsic 480x320 RGB 8-bit"
            ),
            "license": "project-authored (no product license decision yet)",
            "license-path": "licenses/PROJECT-AUTHORED-NOTICE.txt",
            "role": "workload image (display size 240x160 DIP)",
            "dimensions": "480x320",
        }
    )
    unicode_files = {
        "licenses/Unicode-License.txt",
        "references/BidiCharacterTest-excerpt.txt",
    }
    for path, license_id, role in [
        ("licenses/OFL-NotoSans.txt", "OFL-1.1 license text", "license notice for Noto Sans (from notofonts/latin-greek-cyrillic @ " + LGC_REF + ")"),
        ("licenses/OFL-NotoSansArabic.txt", "OFL-1.1 license text", "license notice for Noto Sans Arabic (from notofonts/arabic @ " + ARABIC_REF + ")"),
        ("licenses/OFL-NotoSansHebrew.txt", "OFL-1.1 license text", "license notice for Noto Sans Hebrew (from notofonts/hebrew @ " + HEBREW_REF + ")"),
        ("licenses/OFL-NotoSansJP.txt", "OFL-1.1 license text", "license notice for Noto Sans JP (from google/fonts @ " + GOOGLE_FONTS_REF + ")"),
        ("licenses/Unicode-License.txt", "Unicode License v3", "license notice for the Unicode BiDi test data excerpt"),
        ("references/BidiCharacterTest-excerpt.txt", "Unicode License (data files)", "pinned BiDi conformance reference excerpt (Unicode 17.0.0 UCD, " + UNICODE_UCD + "; original lines 47, 48, 49, 142, 144, 230, 231, 250, 251, 252, 253)"),
        ("tools/generate_sample_png.py", "project-authored (no product license decision yet)", "PNG generator provenance record"),
        ("tools/build_fixture_data.py", "project-authored (no product license decision yet)", "fixture authoring generator (this script)"),
        ("references/selection-cases.md", "project-authored (no product license decision yet)", "selection/copy reference case table"),
        ("references/bidi-reference.md", "project-authored (no product license decision yet)", "visual/BiDi correctness reference table (pinned Unicode sources)"),
        ("README.md", "project-authored (no product license decision yet)", "fixture package documentation"),
        ("licenses/PROJECT-AUTHORED-NOTICE.txt", "project-authored (no product license decision yet)", "authorship notice referenced by every project-authored manifest record"),
    ]:
        if path == "licenses/OFL-NotoSans.txt":
            source, origin = (
                f"https://github.com/notofonts/latin-greek-cyrillic/blob/{LGC_REF}/OFL.txt",
                f"third-party; copied unmodified from notofonts/latin-greek-cyrillic @ {LGC_REF}",
            )
            license_path = path
        elif path == "licenses/OFL-NotoSansArabic.txt":
            source, origin = (
                f"https://github.com/notofonts/arabic/blob/{ARABIC_REF}/OFL.txt",
                f"third-party; copied unmodified from notofonts/arabic @ {ARABIC_REF}",
            )
            license_path = path
        elif path == "licenses/OFL-NotoSansHebrew.txt":
            source, origin = (
                f"https://github.com/notofonts/hebrew/blob/{HEBREW_REF}/OFL.txt",
                f"third-party; copied unmodified from notofonts/hebrew @ {HEBREW_REF}",
            )
            license_path = path
        elif path == "licenses/OFL-NotoSansJP.txt":
            source, origin = (
                f"https://github.com/google/fonts/blob/{GOOGLE_FONTS_REF}/ofl/notosansjp/OFL.txt",
                f"third-party; copied unmodified from google/fonts @ {GOOGLE_FONTS_REF} (ofl/notosansjp/OFL.txt)",
            )
            license_path = path
        elif path == "licenses/Unicode-License.txt":
            source = "https://www.unicode.org/license.txt"
            origin = "third-party; copied unmodified from unicode.org (Unicode License V3)"
            license_path = "licenses/Unicode-License.txt"
        elif path == "references/BidiCharacterTest-excerpt.txt":
            source = UNICODE_UCD
            origin = (
                "third-party; verbatim excerpt of Unicode 17.0.0 UCD "
                "BidiCharacterTest.txt (dated 2025-07-30), attributed per its header"
            )
            license_path = "licenses/Unicode-License.txt"
        else:
            source = (
                "project-authored: fixtures/reader-workload/tools/ (and the "
                "hand-authored docs in this package)"
            )
            origin = "project-authored for initiative I-001 / W3 (T-003)"
            license_path = "licenses/PROJECT-AUTHORED-NOTICE.txt"
        records.append(
            {
                "path": path,
                "sha256": sha256_file(FIXTURE / path),
                "version": f"fixture revision {FIXTURE_REVISION}",
                "provenance": origin,
                "source": source,
                "license": license_id,
                "license-path": license_path,
                "role": role,
            }
        )
    manifest_lines = [
        "# reader-workload asset manifest.",
        "#",
        "# GENERATED by fixtures/reader-workload/tools/build_fixture_data.py.",
        "# Each record starts at its `path:` line; blank lines separate records.",
        "# Paths are relative to this fixture root (portable, never absolute).",
        f"fixture-revision: {FIXTURE_REVISION}",
        "",
    ]
    for r in records:
        for key in (
            "path",
            "sha256",
            "version",
            "provenance",
            "source",
            "license",
            "license-path",
            "role",
            "dimensions",
        ):
            if key in r:
                manifest_lines.append(f"{key}: {r[key]}")
        manifest_lines.append("")
    (FIXTURE / "manifest.txt").write_bytes("\n".join(manifest_lines).encode("utf-8"))

    print(
        "curated.rs, selection_cases.rs, manifest.txt, selection-cases.md,",
        f"and {len(CASES)} golden files written for fixture revision {FIXTURE_REVISION}.",
    )


if __name__ == "__main__":
    main()
