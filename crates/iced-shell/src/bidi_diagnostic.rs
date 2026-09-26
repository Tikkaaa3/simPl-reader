//! Opt-in Iced/Cosmic Text layout trace for the pinned BiDi fixture controls.
//!
//! This deliberately reconstructs the same public Iced Graphics paragraph
//! input as `iced_widget::Rich`; the widget keeps its live paragraph in private
//! state, so this is not presented as direct access to the widget's instance.

use std::{
    fs::{File, OpenOptions},
    io::{self, Write},
    path::Path,
};

use crate::reader::{
    self, BidiDiagnosticVariant, DiagnosticDisposition, DiagnosticPresentation,
    PositionedSourceCluster,
};
use reader_workload::{BaseDirection, Item, Workload};

const TRACE_HEADER: &str = "iced-cosmic-bidi-trace/v1\n";
const MAX_TRACE_BYTES: usize = 256 * 1024;
const MAX_RECORD_BYTES: usize = 32 * 1024;
const MAX_GLYPHS_PER_CONDITION: usize = 256;
const BODY_TEXT: iced::Color = iced::Color::from_rgb8(0xd7, 0xdc, 0xe2);
const EXPECTED_P00003_LEVELS: [u8; 8] = [1, 1, 1, 1, 1, 2, 2, 2];
const EXPECTED_P00003_ORDER: [usize; 8] = [5, 6, 7, 4, 3, 2, 1, 0];
const EXPECTED_P00004_LEVELS: [u8; 17] = [1, 1, 1, 1, 1, 2, 2, 2, 1, 1, 1, 1, 1, 1, 2, 2, 2];
const EXPECTED_P00004_ORDER: [usize; 17] =
    [14, 15, 16, 13, 12, 11, 10, 9, 8, 5, 6, 7, 4, 3, 2, 1, 0];

/// Bounded append-only writer created only for explicit diagnostic mode.
#[derive(Debug)]
pub struct TraceWriter {
    file: File,
    written: usize,
}

/// Dispositions derived from one native paragraph layout trace.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TraceSummary {
    /// Top-to-bottom/left-to-right source-scalar placement vs the declared order.
    pub source_order: DiagnosticDisposition,
    /// Exposed glyph levels vs the T-010 source-scalar baseline, if applicable.
    pub levels: Option<DiagnosticDisposition>,
    /// `p-00004` bracket positions, excluding glyph-mirroring shape.
    pub p00004_bracket_positions: Option<DiagnosticDisposition>,
}

impl TraceWriter {
    /// Creates a new trace file. Existing files are never overwritten.
    pub fn create(path: impl AsRef<Path>) -> io::Result<Self> {
        let mut file = OpenOptions::new().write(true).create_new(true).open(path)?;
        file.write_all(TRACE_HEADER.as_bytes())?;
        file.flush()?;
        Ok(Self {
            file,
            written: TRACE_HEADER.len(),
        })
    }

    /// Records one exact fixture condition using the public pinned Iced
    /// Graphics `Paragraph::with_spans` implementation and bounded raw output.
    pub fn record(
        &mut self,
        workload: &Workload,
        case_id: &str,
        variant: BidiDiagnosticVariant,
        width_dip: u32,
    ) -> Result<TraceSummary, String> {
        let Item::Paragraph {
            text,
            base_direction,
            style_runs,
            ..
        } = workload
            .item_by_id(case_id)
            .ok_or_else(|| format!("unknown BiDi diagnostic case {case_id:?}"))?
        else {
            return Err(format!(
                "BiDi diagnostic case {case_id:?} is not a paragraph"
            ));
        };
        if !matches!(case_id, "p-00001" | "p-00003" | "p-00004") {
            return Err(format!("unsupported BiDi diagnostic case {case_id:?}"));
        }

        let mapped = reader::map_diagnostic_paragraph(text, *base_direction, style_runs, variant)?;
        let spans = mapped
            .runs
            .iter()
            .map(|run| {
                iced::widget::text::Span::new(mapped.text[run.bytes.clone()].to_string())
                    .font(run.role.iced_font())
                    .size(reader_workload::LAYOUT_RECIPE.body_font_size_dip)
                    .line_height(iced::widget::text::LineHeight::Absolute(iced::Pixels(
                        reader_workload::LAYOUT_RECIPE.body_line_height_dip,
                    )))
                    .color(BODY_TEXT)
            })
            .collect::<Vec<iced::widget::text::Span<'static>>>();

        // `Rich::layout` in iced_widget 0.14.2 calls this exact public
        // paragraph constructor with these shaping/alignment/wrapping inputs.
        // The live Rich state remains private; this trace is a paired
        // reconstruction, not a getter for that instance.
        use iced::advanced::text::Paragraph as _;
        let paragraph =
            iced::advanced::graphics::text::Paragraph::with_spans(iced::advanced::Text {
                content: spans.as_slice(),
                bounds: iced::Size::new(width_dip as f32, 4_096.0),
                size: iced::Pixels(reader_workload::LAYOUT_RECIPE.body_font_size_dip),
                line_height: iced::advanced::text::LineHeight::Absolute(iced::Pixels(
                    reader_workload::LAYOUT_RECIPE.body_line_height_dip,
                )),
                font: iced::Font::with_name("Noto Sans"),
                align_x: iced::advanced::text::Alignment::Default,
                align_y: iced::alignment::Vertical::Top,
                shaping: iced::advanced::text::Shaping::Advanced,
                wrapping: iced::advanced::text::Wrapping::Word,
            });

        let has_leading_rlm =
            *base_direction == BaseDirection::Rtl && mapped.text.starts_with('\u{200f}');
        let (expected_levels, expected_order) = expectations(case_id, text);
        let oracle_label = match (case_id, has_leading_rlm) {
            ("p-00003" | "p-00004", true) => {
                "T-010 exact leading-RLM automatic-direction result projected to original Unicode-17 indices"
            }
            ("p-00003" | "p-00004", false) => {
                "Unicode-17 original explicit-RTL oracle; Iced base-direction argument unsupported"
            }
            ("p-00001", _) => "fixture logical LTR source order; no RTL excerpt oracle",
            _ => unreachable!("validated diagnostic case"),
        };
        let mut record = String::new();
        use std::fmt::Write as _;
        writeln!(
            record,
            "condition case={case_id} width_dip={width_dip} variant={} presentation={} requested_rlm={} actual_rlm={} logical={:?} mapped={:?} fixture_direction={:?} direction_api=unsupported oracle={oracle_label:?}",
            variant.label(),
            presentation_label(variant.presentation),
            variant.leading_rlm,
            has_leading_rlm,
            text,
            mapped.text,
            base_direction,
        )
        .map_err(|error| error.to_string())?;
        writeln!(
            record,
            "oracle levels_by_logical_scalar={:?} expected_left_to_right_source_indices={:?}",
            expected_levels.as_deref(),
            expected_order,
        )
        .map_err(|error| error.to_string())?;
        writeln!(
            record,
            "render spans={} default_font=Noto Sans shaping=Advanced align=Default wrap=Word width_dip={width_dip} height_dip=4096 line_height_dip={} requested_roles={}",
            spans.len(),
            reader_workload::LAYOUT_RECIPE.body_line_height_dip,
            mapped
                .runs
                .iter()
                .map(|run| format!("{:?}@{:?}", run.role, run.bytes))
                .collect::<Vec<_>>()
                .join(","),
        )
        .map_err(|error| error.to_string())?;
        for (span_index, run) in mapped.runs.iter().enumerate() {
            writeln!(
                record,
                "span index={span_index} bytes={}..{} role={:?} text={:?}",
                run.bytes.start,
                run.bytes.end,
                run.role,
                &mapped.text[run.bytes.clone()],
            )
            .map_err(|error| error.to_string())?;
        }

        let mut placements = Vec::new();
        let mut index_mapping_complete = true;
        let mut glyph_count = 0;
        for (run_index, run) in paragraph.buffer().layout_runs().enumerate() {
            writeln!(
                record,
                "run index={run_index} line_i={} line_top_dip={:.3} line_y_dip={:.3} line_width_dip={:.3} rtl={}",
                run.line_i,
                run.line_top,
                run.line_y,
                run.line_w,
                run.rtl,
            )
            .map_err(|error| error.to_string())?;
            for glyph in run.glyphs {
                glyph_count += 1;
                if glyph_count > MAX_GLYPHS_PER_CONDITION {
                    return Err(format!(
                        "glyph trace exceeded {MAX_GLYPHS_PER_CONDITION} entries for {case_id}"
                    ));
                }
                let projected = reader::project_diagnostic_glyph_source(
                    &mapped.text,
                    glyph.start..glyph.end,
                    has_leading_rlm,
                );
                let (mapped_scalars, source_scalars, source_ranges, includes_rlm) =
                    if let Some(projected) = projected {
                        (
                            projected.mapped_scalar_indices,
                            projected.logical_scalar_indices,
                            projected
                                .logical_byte_ranges
                                .iter()
                                .map(|range| format!("{}..{}", range.start, range.end))
                                .collect::<Vec<_>>()
                                .join(","),
                            projected.includes_leading_rlm,
                        )
                    } else {
                        index_mapping_complete = false;
                        (Vec::new(), Vec::new(), "unavailable".to_string(), false)
                    };
                let glyph_text = mapped
                    .text
                    .get(glyph.start..glyph.end)
                    .unwrap_or("<invalid-range>");
                let placed_x = glyph.x + glyph.font_size * glyph.x_offset;
                writeln!(
                    record,
                    "glyph run={run_index} line_i={} cluster_bytes={}..{} logical_bytes=[{source_ranges}] mapped_scalars={mapped_scalars:?} logical_scalars={source_scalars:?} includes_added_rlm={includes_rlm} mapped_text={glyph_text:?} level={} x_hitbox_dip={:.3} placed_x_dip={placed_x:.3} y_dip={:.3} w_dip={:.3} x_offset={:.3} glyph_id={} opaque_font_id={:?}",
                    run.line_i,
                    glyph.start,
                    glyph.end,
                    glyph.level.number(),
                    glyph.x,
                    glyph.y,
                    glyph.w,
                    glyph.x_offset,
                    glyph.glyph_id,
                    glyph.font_id,
                )
                .map_err(|error| error.to_string())?;
                placements.push(PositionedSourceCluster {
                    line_y: run.line_top,
                    x: placed_x,
                    glyph_width: glyph.w,
                    level: glyph.level.number(),
                    source_scalars,
                });
            }
        }

        let source_order = if index_mapping_complete {
            reader::classify_diagnostic_source_order(&expected_order, &placements)
        } else {
            DiagnosticDisposition::Inconclusive
        };
        let levels = expected_levels.as_deref().map(|expected| {
            if index_mapping_complete {
                reader::classify_diagnostic_levels(expected, &placements)
            } else {
                DiagnosticDisposition::Inconclusive
            }
        });
        let p00004_bracket_positions = (case_id == "p-00004").then(|| {
            if index_mapping_complete {
                classify_p00004_bracket_positions(&placements)
            } else {
                DiagnosticDisposition::Inconclusive
            }
        });
        writeln!(
            record,
            "disposition source_order={} levels={} bracket_positions={} live_widget_instance=INCONCLUSIVE",
            source_order.label(),
            levels.map_or("NOT_APPLICABLE", DiagnosticDisposition::label),
            p00004_bracket_positions.map_or("NOT_APPLICABLE", DiagnosticDisposition::label),
        )
        .map_err(|error| error.to_string())?;

        if record.len() > MAX_RECORD_BYTES {
            return Err(format!(
                "trace record for {case_id} exceeded {MAX_RECORD_BYTES} bytes"
            ));
        }
        if self.written.saturating_add(record.len()) > MAX_TRACE_BYTES {
            return Err(format!("trace exceeded its {MAX_TRACE_BYTES}-byte limit"));
        }
        self.file
            .write_all(record.as_bytes())
            .and_then(|()| self.file.flush())
            .map_err(|error| format!("cannot write bounded BiDi trace: {error}"))?;
        self.written += record.len();

        Ok(TraceSummary {
            source_order,
            levels,
            p00004_bracket_positions,
        })
    }
}

fn presentation_label(presentation: DiagnosticPresentation) -> &'static str {
    match presentation {
        DiagnosticPresentation::Styled => "styled-multispan",
        DiagnosticPresentation::Uniform => "uniform-single-span-Noto-Sans",
    }
}

fn expectations(case_id: &str, text: &str) -> (Option<Vec<u8>>, Vec<usize>) {
    match case_id {
        "p-00003" => (
            Some(EXPECTED_P00003_LEVELS.to_vec()),
            EXPECTED_P00003_ORDER.to_vec(),
        ),
        "p-00004" => (
            Some(EXPECTED_P00004_LEVELS.to_vec()),
            EXPECTED_P00004_ORDER.to_vec(),
        ),
        "p-00001" => {
            let order = (0..text.chars().count()).collect::<Vec<_>>();
            // The LTR control uses its fixture logical sequence, not the RTL
            // BidiCharacterTest excerpt or a claimed Unicode conformance row.
            (None, order)
        }
        _ => unreachable!("validated diagnostic case"),
    }
}

fn classify_p00004_bracket_positions(
    placements: &[PositionedSourceCluster],
) -> DiagnosticDisposition {
    fn unique_x(placements: &[PositionedSourceCluster], source_index: usize) -> Option<(f32, f32)> {
        let mut matching = placements
            .iter()
            .filter(|placement| placement.source_scalars.as_slice() == [source_index]);
        let first = matching.next()?;
        if matching.next().is_some() {
            return None;
        }
        Some((first.line_y, first.x))
    }

    let Some((close_line, close_x)) = unique_x(placements, 12) else {
        return DiagnosticDisposition::Inconclusive;
    };
    let Some((open_line, open_x)) = unique_x(placements, 4) else {
        return DiagnosticDisposition::Inconclusive;
    };
    let get_group = |indices: &[usize]| -> Option<Vec<f32>> {
        let mut xs = Vec::new();
        for &index in indices {
            let mut found = false;
            for placement in placements
                .iter()
                .filter(|placement| placement.source_scalars.contains(&index))
            {
                if placement.line_y != close_line || placement.line_y != open_line {
                    return None;
                }
                found = true;
                xs.push(placement.x);
            }
            if !found {
                return None;
            }
        }
        Some(xs)
    };
    let Some(arabic) = get_group(&[9, 10, 11]) else {
        return DiagnosticDisposition::Inconclusive;
    };
    let Some(def) = get_group(&[5, 6, 7]) else {
        return DiagnosticDisposition::Inconclusive;
    };
    let Some(space) = unique_x(placements, 3) else {
        return DiagnosticDisposition::Inconclusive;
    };
    let Some(hebrew) = get_group(&[0, 1, 2]) else {
        return DiagnosticDisposition::Inconclusive;
    };
    if space.0 != close_line || space.0 != open_line {
        return DiagnosticDisposition::Inconclusive;
    }

    let close_before_arabic = close_x < arabic.into_iter().fold(f32::INFINITY, f32::min);
    let open_after_latin = def.into_iter().fold(f32::NEG_INFINITY, f32::max) < open_x;
    let open_before_hebrew =
        open_x < space.1 && space.1 < hebrew.into_iter().fold(f32::INFINITY, f32::min);
    if close_before_arabic && open_after_latin && open_before_hebrew {
        DiagnosticDisposition::Pass
    } else {
        DiagnosticDisposition::Fail
    }
}

#[cfg(test)]
mod tests {
    use super::{DiagnosticDisposition, classify_p00004_bracket_positions};
    use crate::reader::PositionedSourceCluster;

    fn expected_placements() -> Vec<PositionedSourceCluster> {
        [14, 15, 16, 13, 12, 11, 10, 9, 8, 5, 6, 7, 4, 3, 2, 1, 0]
            .into_iter()
            .enumerate()
            .map(|(x, source)| PositionedSourceCluster {
                line_y: 0.0,
                x: x as f32,
                glyph_width: 1.0,
                level: 1,
                source_scalars: vec![source],
            })
            .collect()
    }

    #[test]
    fn original_styled_rtl_layout_matches_pinned_source_order_at_both_widths() {
        let package = crate::reader::load_reader_package_from(std::path::Path::new(env!(
            "CARGO_MANIFEST_DIR"
        )))
        .expect("verified fixture");
        {
            let mut system = iced::advanced::graphics::text::font_system()
                .write()
                .expect("font system");
            for asset in package.fonts {
                system.load_font(std::borrow::Cow::Owned(asset.bytes));
            }
        }
        let trace_path =
            std::env::temp_dir().join(format!("iced-t013-{}-rtl-trace", std::process::id()));
        let mut trace = super::TraceWriter::create(&trace_path).expect("fresh trace");
        for case_id in ["p-00003", "p-00004"] {
            for width in [800, 480] {
                for variant in [0, 3] {
                    let summary = trace
                        .record(
                            &package.workload,
                            case_id,
                            crate::reader::bidi_diagnostic_variants()[variant],
                            width,
                        )
                        .expect("bounded public paragraph trace");
                    assert_eq!(
                        summary.levels,
                        Some(DiagnosticDisposition::Pass),
                        "{case_id} {width} variant {variant}"
                    );
                    assert_eq!(
                        summary.source_order,
                        DiagnosticDisposition::Pass,
                        "{case_id} {width} variant {variant}"
                    );
                    if case_id == "p-00004" {
                        assert_eq!(
                            summary.p00004_bracket_positions,
                            Some(DiagnosticDisposition::Pass),
                            "{width} variant {variant}"
                        );
                    }
                }
            }
        }
        let ltr = trace
            .record(
                &package.workload,
                "p-00001",
                crate::reader::bidi_diagnostic_variants()[0],
                800,
            )
            .expect("LTR control");
        assert_eq!(ltr.source_order, DiagnosticDisposition::Pass);
        drop(trace);
        std::fs::remove_file(trace_path).expect("clean trace");
    }

    // Exercises the patched shaping-group branch independently of the curated
    // single-line oracle: two advancing glyph groups inside one RTL word,
    // then the same word across an actual soft wrap, and an LTR embedding.
    fn laid_out_visible_source_lines(
        spans: &[(&str, crate::reader::FontRole)],
        width: f32,
    ) -> Vec<Vec<usize>> {
        use iced::advanced::text::Paragraph as _;
        let input = spans.iter().map(|(text, _)| *text).collect::<String>();
        let rich = spans
            .iter()
            .map(|(text, role)| iced::widget::text::Span::<()>::new(*text).font(role.iced_font()))
            .collect::<Vec<_>>();
        let paragraph =
            iced::advanced::graphics::text::Paragraph::with_spans(iced::advanced::Text {
                content: &rich,
                bounds: iced::Size::new(width, 256.0),
                size: iced::Pixels(18.0),
                line_height: iced::advanced::text::LineHeight::Absolute(iced::Pixels(27.0)),
                font: crate::reader::FontRole::LatinRegular.iced_font(),
                align_x: iced::advanced::text::Alignment::Default,
                align_y: iced::alignment::Vertical::Top,
                shaping: iced::advanced::text::Shaping::Advanced,
                wrapping: iced::advanced::text::Wrapping::Word,
            });
        let mut lines = std::collections::BTreeMap::<u32, Vec<(f32, usize)>>::new();
        for run in paragraph.buffer().layout_runs() {
            for glyph in run.glyphs {
                if &input[glyph.start..glyph.end] == "-" {
                    assert_eq!(
                        glyph.level.number(),
                        1,
                        "ASCII neutral must be on RTL shaping path"
                    );
                }
                let projected = crate::reader::project_diagnostic_glyph_source(
                    &input,
                    glyph.start..glyph.end,
                    true,
                )
                .expect("scalar-aligned glyph cluster");
                if glyph.w <= 0.0 {
                    continue;
                }
                match projected.logical_scalar_indices.as_slice() {
                    [scalar]
                        if !input[glyph.start..glyph.end]
                            .chars()
                            .all(char::is_whitespace) =>
                    {
                        lines
                            .entry(run.line_top.to_bits())
                            .or_default()
                            .push((glyph.x + glyph.font_size * glyph.x_offset, *scalar));
                    }
                    [] | [_] => {}
                    _ => panic!("multi-scalar/advancing ambiguous cluster: {projected:?}"),
                }
            }
        }
        lines
            .into_values()
            .map(|mut line| {
                line.sort_by(|a, b| a.0.total_cmp(&b.0));
                let scalars = line
                    .into_iter()
                    .map(|(_, scalar)| scalar)
                    .collect::<Vec<_>>();
                let unique = scalars.iter().collect::<std::collections::HashSet<_>>();
                assert_eq!(
                    unique.len(),
                    scalars.len(),
                    "duplicated positive-width source"
                );
                scalars
            })
            .collect()
    }

    #[test]
    fn styled_rtl_groups_wrap_without_reversing_group_order_or_ltr_embedding() {
        let package = crate::reader::load_reader_package_from(std::path::Path::new(env!(
            "CARGO_MANIFEST_DIR"
        )))
        .expect("verified font assets");
        let mut fonts = iced::advanced::graphics::text::font_system()
            .write()
            .expect("fonts");
        for asset in package.fonts {
            fonts.load_font(std::borrow::Cow::Owned(asset.bytes));
        }
        drop(fonts);
        use crate::reader::FontRole::{Arabic, Hebrew, LatinRegular};
        let single = [("\u{200f}ابج", Arabic), ("אבג", Hebrew)];
        assert_eq!(
            laid_out_visible_source_lines(&single, 800.0),
            [vec![5, 4, 3, 2, 1, 0]]
        );
        let ascii_neutral = [("\u{200f}אב", Hebrew), ("--", LatinRegular), ("גד", Hebrew)];
        assert_eq!(
            laid_out_visible_source_lines(&ascii_neutral, 800.0),
            [vec![5, 4, 3, 2, 1, 0]]
        );
        // An ASCII-only neutral word separated by spaces resolves at RTL
        // level 1 between Hebrew words, exercising ShapeWord's ASCII path.
        let ascii_word = [
            ("\u{200f}אב ", Hebrew),
            ("--", LatinRegular),
            (" גד", Hebrew),
        ];
        assert_eq!(
            laid_out_visible_source_lines(&ascii_word, 800.0),
            [vec![7, 6, 4, 3, 1, 0]]
        );
        let wrapped = [
            ("\u{200f}ابج", Arabic),
            ("אבג ", Hebrew),
            ("ابج", Arabic),
            ("אבג", Hebrew),
        ];
        assert_eq!(
            laid_out_visible_source_lines(&wrapped, 65.0),
            [vec![5, 4, 3, 2, 1, 0], vec![12, 11, 10, 9, 8, 7]]
        );
        let embedding = [
            ("\u{200f}אבג ", Hebrew),
            ("abc ", LatinRegular),
            ("אבג", Hebrew),
        ];
        assert_eq!(
            laid_out_visible_source_lines(&embedding, 800.0),
            [vec![10, 9, 8, 4, 5, 6, 2, 1, 0]]
        );
    }

    #[test]
    fn bracket_position_classification_is_independent_of_mirroring() {
        let mut matching = expected_placements();
        let mut arabic_mark = matching
            .iter()
            .find(|placement| placement.source_scalars == [11])
            .expect("Arabic source scalar")
            .clone();
        arabic_mark.x += 0.25;
        arabic_mark.glyph_width = 0.0;
        matching.push(arabic_mark);
        assert_eq!(
            classify_p00004_bracket_positions(&matching),
            DiagnosticDisposition::Pass
        );

        let mut misplaced = expected_placements();
        misplaced
            .iter_mut()
            .find(|placement| placement.source_scalars == [12])
            .expect("paired close bracket source scalar")
            .x = 10.0;
        assert_eq!(
            classify_p00004_bracket_positions(&misplaced),
            DiagnosticDisposition::Fail
        );

        let mut incomplete = expected_placements();
        incomplete.retain(|placement| placement.source_scalars != [12]);
        assert_eq!(
            classify_p00004_bracket_positions(&incomplete),
            DiagnosticDisposition::Inconclusive
        );
    }
}
