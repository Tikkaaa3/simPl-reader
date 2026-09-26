use std::ffi::OsStr;

use reader_workload::{BaseDirection, InlineStyle, StyleRun};

/// Whether the process explicitly requested the disposable reader experiment.
#[must_use]
pub fn reader_poc_requested(arguments: impl IntoIterator<Item = impl AsRef<OsStr>>) -> bool {
    arguments
        .into_iter()
        .any(|argument| argument.as_ref() == OsStr::new("--reader-poc"))
}

/// Whether the process explicitly requested the same reader with 10k body rows.
#[must_use]
pub fn reader_large_requested(arguments: impl IntoIterator<Item = impl AsRef<OsStr>>) -> bool {
    arguments
        .into_iter()
        .any(|argument| argument.as_ref() == OsStr::new("--reader-poc-large"))
}

/// Whether the process explicitly requested the bounded native BiDi diagnostic.
#[must_use]
pub fn bidi_diagnostic_requested(arguments: impl IntoIterator<Item = impl AsRef<OsStr>>) -> bool {
    arguments
        .into_iter()
        .any(|argument| argument.as_ref() == OsStr::new("--bidi-diagnostic"))
}

/// The diagnostic instrumentation requires an exact process-local opt-in.
#[must_use]
pub fn bidi_diagnostic_enabled(value: Option<&OsStr>) -> bool {
    value == Some(OsStr::new("1"))
}

/// The shared comparison width currently selected for the reader view.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum WidthScenario {
    /// 800 DIP content width.
    Wide,
    /// 480 DIP content width.
    Narrow,
}

impl WidthScenario {
    /// Width from the shared workload recipe.
    #[must_use]
    pub fn width_dip(self) -> u32 {
        let widths = reader_workload::LAYOUT_RECIPE.content_widths_dip;
        match self {
            Self::Wide => widths[0],
            Self::Narrow => widths[1],
        }
    }

    /// Switch to the other shared comparison width.
    #[must_use]
    pub const fn toggled(self) -> Self {
        match self {
            Self::Wide => Self::Narrow,
            Self::Narrow => Self::Wide,
        }
    }
}

/// Controlled fixture face selected for one logical character run.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum FontRole {
    /// Noto Sans regular Latin face.
    LatinRegular,
    /// Noto Sans bold Latin face.
    LatinBold,
    /// Noto Sans italic Latin face.
    LatinItalic,
    /// Supplied regular Arabic face (no synthetic style).
    Arabic,
    /// Supplied regular Hebrew face (no synthetic style).
    Hebrew,
    /// Noto Sans JP variable face at the Iced weight corresponding to 400.
    Japanese,
}

impl FontRole {
    /// Iced-native family/weight/style selection. Font bytes are loaded from
    /// the checked fixture after the shell-first stage.
    #[must_use]
    pub const fn iced_font(self) -> iced::Font {
        use iced::font::{Family, Font, Style, Weight};
        match self {
            Self::LatinRegular => Font::with_name("Noto Sans"),
            Self::LatinBold => Font {
                weight: Weight::Bold,
                ..Font::with_name("Noto Sans")
            },
            Self::LatinItalic => Font {
                style: Style::Italic,
                ..Font::with_name("Noto Sans")
            },
            Self::Arabic => Font::with_name("Noto Sans Arabic"),
            Self::Hebrew => Font::with_name("Noto Sans Hebrew"),
            Self::Japanese => Font {
                family: Family::Name("Noto Sans JP"),
                // Iced maps Normal to cosmic-text weight 400. This requests
                // that variable-font instance but does not expose axis control.
                weight: Weight::Normal,
                ..Font::DEFAULT
            },
        }
    }
}

/// Font decision for a byte-aligned range in mapped logical text.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MappedRun {
    /// UTF-8 range in [`MappedParagraph::text`].
    pub bytes: std::ops::Range<usize>,
    /// Controlled fixture font requested for this run.
    pub role: FontRole,
}

/// Text and per-run native font requests for one workload paragraph.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MappedParagraph {
    /// Logical Unicode text, with a leading RLM for explicit RTL intent only.
    pub text: String,
    /// Character-aligned style/script runs covering the text.
    pub runs: Vec<MappedRun>,
}

/// Whether a diagnostic paragraph uses the current per-script/style font runs
/// or one uniform font request.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DiagnosticPresentation {
    /// Current mapped font-role runs.
    Styled,
    /// One regular Noto Sans run over the same input string.
    Uniform,
}

/// One condition in the bounded diagnostic matrix.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct BidiDiagnosticVariant {
    /// Presentation factor for this condition.
    pub presentation: DiagnosticPresentation,
    /// Whether the adapter's leading U+200F is present in the input.
    pub leading_rlm: bool,
}

impl BidiDiagnosticVariant {
    /// Stable short ID used by the opt-in driver and trace records.
    #[must_use]
    pub const fn label(self) -> &'static str {
        match (self.presentation, self.leading_rlm) {
            (DiagnosticPresentation::Styled, true) => "styled-rlm",
            (DiagnosticPresentation::Uniform, true) => "uniform-rlm",
            (DiagnosticPresentation::Uniform, false) => "uniform-no-rlm",
            (DiagnosticPresentation::Styled, false) => "styled-no-rlm",
        }
    }
}

/// Four conditions ordered as a Gray-code matrix so neighboring conditions
/// differ in only one factor.
#[must_use]
pub const fn bidi_diagnostic_variants() -> [BidiDiagnosticVariant; 4] {
    use DiagnosticPresentation::{Styled, Uniform};

    [
        BidiDiagnosticVariant {
            presentation: Styled,
            leading_rlm: true,
        },
        BidiDiagnosticVariant {
            presentation: Uniform,
            leading_rlm: true,
        },
        BidiDiagnosticVariant {
            presentation: Uniform,
            leading_rlm: false,
        },
        BidiDiagnosticVariant {
            presentation: Styled,
            leading_rlm: false,
        },
    ]
}

/// Maps logical text and fixture style ranges to Iced rich-text font runs.
///
/// Iced 0.14's public text API has no paragraph base-direction field. For RTL
/// workload items this prepends an RLM as a first-strong hint; logical text is
/// not reversed, and the actual native visual result remains an experiment.
pub fn map_paragraph(
    text: &str,
    base_direction: BaseDirection,
    style_runs: &[StyleRun],
) -> Result<MappedParagraph, String> {
    map_paragraph_with_rlm(
        text,
        base_direction,
        style_runs,
        base_direction == BaseDirection::Rtl,
    )
}

/// Applies one diagnostic condition while retaining the logical fixture text.
pub fn map_diagnostic_paragraph(
    text: &str,
    base_direction: BaseDirection,
    style_runs: &[StyleRun],
    variant: BidiDiagnosticVariant,
) -> Result<MappedParagraph, String> {
    let mut mapped = map_paragraph_with_rlm(text, base_direction, style_runs, variant.leading_rlm)?;
    if variant.presentation == DiagnosticPresentation::Uniform && !mapped.text.is_empty() {
        mapped.runs = vec![MappedRun {
            bytes: 0..mapped.text.len(),
            role: FontRole::LatinRegular,
        }];
    }
    Ok(mapped)
}

fn map_paragraph_with_rlm(
    text: &str,
    base_direction: BaseDirection,
    style_runs: &[StyleRun],
    leading_rlm: bool,
) -> Result<MappedParagraph, String> {
    let mut previous_end = 0;
    for style_run in style_runs {
        if style_run.start_byte < previous_end
            || style_run.start_byte > style_run.end_byte
            || style_run.end_byte > text.len()
            || !text.is_char_boundary(style_run.start_byte)
            || !text.is_char_boundary(style_run.end_byte)
        {
            return Err(format!(
                "invalid UTF-8 style range {}..{} for {} bytes",
                style_run.start_byte,
                style_run.end_byte,
                text.len()
            ));
        }
        previous_end = style_run.end_byte;
    }

    let add_rlm = leading_rlm && base_direction == BaseDirection::Rtl;
    let mut mapped_text =
        String::with_capacity(text.len() + usize::from(add_rlm) * '\u{200f}'.len_utf8());
    if add_rlm {
        mapped_text.push('\u{200f}');
    }

    let mut runs: Vec<MappedRun> = Vec::new();
    for (source_start, character) in text.char_indices() {
        let source_end = source_start + character.len_utf8();
        let style = style_runs
            .iter()
            .find(|run| run.start_byte <= source_start && source_end <= run.end_byte)
            .map(|run| run.style);
        let role = font_role(character, style);
        let mapped_start = mapped_text.len();
        mapped_text.push(character);
        let mapped_end = mapped_text.len();

        if let Some(last) = runs.last_mut()
            && last.role == role
            && last.bytes.end == mapped_start
        {
            last.bytes.end = mapped_end;
        } else {
            runs.push(MappedRun {
                bytes: mapped_start..mapped_end,
                role,
            });
        }
    }

    if add_rlm {
        if let Some(first) = runs.first_mut() {
            first.bytes.start = 0;
        } else {
            runs.push(MappedRun {
                bytes: 0..mapped_text.len(),
                role: FontRole::LatinRegular,
            });
        }
    }

    Ok(MappedParagraph {
        text: mapped_text,
        runs,
    })
}

/// Source indices for one Cosmic Text glyph's UTF-8 cluster range.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DiagnosticGlyphSourceMap {
    /// Unicode scalar indices in the mapped string, including an added RLM.
    pub mapped_scalar_indices: Vec<usize>,
    /// Unicode scalar indices in the original logical string.
    pub logical_scalar_indices: Vec<usize>,
    /// UTF-8 byte ranges in the original logical string.
    pub logical_byte_ranges: Vec<std::ops::Range<usize>>,
    /// Whether this cluster includes the adapter-added leading RLM.
    pub includes_leading_rlm: bool,
}

/// Projects Cosmic Text's mapped-string byte range back to logical fixture
/// scalar and byte coordinates. Invalid or non-scalar-aligned ranges fail closed.
#[must_use]
pub fn project_diagnostic_glyph_source(
    mapped_text: &str,
    glyph_bytes: std::ops::Range<usize>,
    leading_rlm: bool,
) -> Option<DiagnosticGlyphSourceMap> {
    if glyph_bytes.start > glyph_bytes.end
        || glyph_bytes.end > mapped_text.len()
        || !mapped_text.is_char_boundary(glyph_bytes.start)
        || !mapped_text.is_char_boundary(glyph_bytes.end)
        || (leading_rlm && !mapped_text.starts_with('\u{200f}'))
    {
        return None;
    }

    let mut mapped_scalar_indices = Vec::new();
    let mut logical_scalar_indices = Vec::new();
    let mut logical_byte_ranges = Vec::new();
    let mut includes_leading_rlm = false;
    for (mapped_scalar_index, (byte_start, character)) in mapped_text.char_indices().enumerate() {
        let byte_end = byte_start + character.len_utf8();
        if glyph_bytes.start >= byte_end || byte_start >= glyph_bytes.end {
            continue;
        }
        mapped_scalar_indices.push(mapped_scalar_index);
        if leading_rlm && mapped_scalar_index == 0 {
            includes_leading_rlm = true;
            continue;
        }

        logical_scalar_indices.push(mapped_scalar_index.checked_sub(usize::from(leading_rlm))?);
        let logical_start =
            byte_start.checked_sub(usize::from(leading_rlm) * '\u{200f}'.len_utf8())?;
        logical_byte_ranges.push(logical_start..logical_start + character.len_utf8());
    }

    Some(DiagnosticGlyphSourceMap {
        mapped_scalar_indices,
        logical_scalar_indices,
        logical_byte_ranges,
        includes_leading_rlm,
    })
}

/// Disposition for one observable diagnostic segment.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DiagnosticDisposition {
    /// The observed value matches the declared expectation.
    Pass,
    /// The observed value differs from the declared expectation.
    Fail,
    /// The pinned seam does not expose enough information to decide.
    Inconclusive,
}

impl DiagnosticDisposition {
    /// Stable uppercase label for bounded trace records and evidence tables.
    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Self::Pass => "PASS",
            Self::Fail => "FAIL",
            Self::Inconclusive => "INCONCLUSIVE",
        }
    }
}

/// One visible Cosmic Text cluster and its projected logical scalar indices.
#[derive(Clone, Debug, PartialEq)]
pub struct PositionedSourceCluster {
    /// Vertical line coordinate in paragraph-local DIP.
    pub line_y: f32,
    /// Horizontal glyph origin in paragraph-local DIP.
    pub x: f32,
    /// Glyph hitbox width in paragraph-local DIP; zero-advance marks may share
    /// a source scalar with their base glyph.
    pub glyph_width: f32,
    /// Resolved Unicode BiDi level exposed by Cosmic Text for this glyph.
    pub level: u8,
    /// Logical source scalar indices; empty only for an inserted control such
    /// as the leading RLM.
    pub source_scalars: Vec<usize>,
}

/// Compares physical top-to-bottom/left-to-right cluster placement against an
/// expected logical source-index order. Multi-scalar clusters or missing source
/// coverage are inconclusive; a complete but permuted index sequence is a fail.
#[must_use]
pub fn classify_diagnostic_source_order(
    expected: &[usize],
    placements: &[PositionedSourceCluster],
) -> DiagnosticDisposition {
    let mut unique_clusters: Vec<PositionedSourceCluster> = Vec::new();
    for placement in placements {
        let source_scalar = match placement.source_scalars.as_slice() {
            [] => continue,
            [scalar] => *scalar,
            _ => return DiagnosticDisposition::Inconclusive,
        };
        if let Some(existing) = unique_clusters.iter_mut().find(|existing| {
            existing.line_y.to_bits() == placement.line_y.to_bits()
                && existing.source_scalars == [source_scalar]
        }) {
            // A scalar split across advancing glyphs has ambiguous visual order;
            // do not discard either placement to manufacture a PASS.
            if existing.glyph_width > 0.0 && placement.glyph_width > 0.0 {
                return DiagnosticDisposition::Inconclusive;
            }
            let replace = if placement.glyph_width > 0.0 {
                existing.glyph_width <= 0.0 || placement.x < existing.x
            } else {
                existing.glyph_width <= 0.0 && placement.x < existing.x
            };
            if replace {
                *existing = placement.clone();
            }
        } else {
            unique_clusters.push(placement.clone());
        }
    }
    unique_clusters.sort_by(|left, right| {
        left.line_y
            .total_cmp(&right.line_y)
            .then_with(|| left.x.total_cmp(&right.x))
    });
    let observed = unique_clusters
        .iter()
        .filter_map(|placement| placement.source_scalars.first().copied())
        .collect::<Vec<_>>();

    if observed == expected {
        return DiagnosticDisposition::Pass;
    }
    let mut expected_coverage = expected.to_vec();
    let mut observed_coverage = observed;
    expected_coverage.sort_unstable();
    observed_coverage.sort_unstable();
    if observed_coverage == expected_coverage {
        DiagnosticDisposition::Fail
    } else {
        DiagnosticDisposition::Inconclusive
    }
}

/// Compares Cosmic Text's per-glyph resolved levels to source-scalar levels
/// from the T-010 baseline. A glyph spanning scalars with differing expected
/// levels is not attributed to either one and remains inconclusive.
#[must_use]
pub fn classify_diagnostic_levels(
    expected: &[u8],
    placements: &[PositionedSourceCluster],
) -> DiagnosticDisposition {
    let mut seen = vec![false; expected.len()];
    for placement in placements {
        let Some(&first) = placement.source_scalars.first() else {
            continue;
        };
        let Some(&expected_level) = expected.get(first) else {
            return DiagnosticDisposition::Inconclusive;
        };
        for &scalar in &placement.source_scalars {
            let Some(&scalar_level) = expected.get(scalar) else {
                return DiagnosticDisposition::Inconclusive;
            };
            if scalar_level != expected_level {
                return DiagnosticDisposition::Inconclusive;
            }
            seen[scalar] = true;
        }
        if placement.level != expected_level {
            return DiagnosticDisposition::Fail;
        }
    }

    if seen.into_iter().all(|was_seen| was_seen) {
        DiagnosticDisposition::Pass
    } else {
        DiagnosticDisposition::Inconclusive
    }
}

fn font_role(character: char, style: Option<InlineStyle>) -> FontRole {
    if is_arabic(character) {
        return FontRole::Arabic;
    }
    if is_hebrew(character) {
        return FontRole::Hebrew;
    }
    if is_japanese_or_cjk(character) {
        return FontRole::Japanese;
    }
    match style {
        Some(InlineStyle::Bold) => FontRole::LatinBold,
        Some(InlineStyle::Italic) => FontRole::LatinItalic,
        None => FontRole::LatinRegular,
    }
}

fn is_arabic(character: char) -> bool {
    matches!(
        character as u32,
        0x0600..=0x06ff
            | 0x0750..=0x077f
            | 0x0870..=0x089f
            | 0x08a0..=0x08ff
            | 0xfb50..=0xfdff
            | 0xfe70..=0xfeff
    )
}

fn is_hebrew(character: char) -> bool {
    matches!(character as u32, 0x0590..=0x05ff | 0xfb1d..=0xfb4f)
}

/// A validated local font asset selected for the reader's native text path.
#[derive(Debug)]
pub struct FontAsset {
    /// Requested role for this face.
    pub role: FontRole,
    /// Manifest-relative source path.
    pub path: String,
    /// Verified bytes loaded after the shell-first stage.
    pub bytes: Vec<u8>,
}

/// Decoded local PNG pixels for Iced's native WGPU image widget.
#[derive(Debug)]
pub struct ImageAsset {
    /// Intrinsic pixel width.
    pub width: u32,
    /// Intrinsic pixel height.
    pub height: u32,
    /// Decoded row-major RGBA pixels.
    pub rgba: Vec<u8>,
}

/// The validated fixture package consumed by explicit reader mode.
#[derive(Debug)]
pub struct ReaderPackage {
    /// Resolved fixture root.
    pub fixture_root: std::path::PathBuf,
    /// Checked-in fixture revision.
    pub fixture_revision: String,
    /// Shared ordered Small workload.
    pub workload: reader_workload::Workload,
    /// The six pinned font files in policy order.
    pub fonts: Vec<FontAsset>,
    /// Decoded manifest-listed sample image.
    pub image: ImageAsset,
}

/// Resolve and validate the checked-in reader fixture from `start_dir`.
///
/// The manifest must match the compiled fixture package exactly. Every listed
/// asset/license is hash-checked before six controlled fonts and the PNG are
/// returned. No network or system-font installation is used.
pub fn load_reader_package_from(start_dir: &std::path::Path) -> Result<ReaderPackage, String> {
    load_reader_package_from_size(start_dir, reader_workload::WorkloadSize::Small)
}

/// Explicit choice of shared workload size; normal shell never calls this.
pub fn load_reader_package_from_size(
    start_dir: &std::path::Path,
    size: reader_workload::WorkloadSize,
) -> Result<ReaderPackage, String> {
    use sha2::{Digest, Sha256};
    use std::{cell::RefCell, collections::HashMap, path::Path};

    const FONT_ROLES: [(&str, FontRole); 6] = [
        ("latin-regular", FontRole::LatinRegular),
        ("latin-bold", FontRole::LatinBold),
        ("latin-italic", FontRole::LatinItalic),
        ("arabic", FontRole::Arabic),
        ("hebrew", FontRole::Hebrew),
        ("japanese", FontRole::Japanese),
    ];

    let fixture_root = reader_workload::manifest::find_fixture_root(start_dir)
        .map_err(|error| error.to_string())?;
    let manifest_path = fixture_root.join("manifest.txt");
    let manifest_source = std::fs::read_to_string(&manifest_path).map_err(|error| {
        format!(
            "cannot read fixture manifest {}: {error}",
            manifest_path.display()
        )
    })?;
    let fixture_manifest = reader_workload::manifest::parse_manifest(&manifest_source)
        .map_err(|error| format!("invalid fixture manifest: {error}"))?;
    if manifest_source != reader_workload::manifest::MANIFEST_SOURCE {
        return Err("fixture manifest differs from the committed workload manifest".to_string());
    }

    let workload = reader_workload::workload(size);
    let mut fonts = Vec::with_capacity(FONT_ROLES.len());
    for (role_name, role) in FONT_ROLES {
        let mut matches = fixture_manifest
            .records
            .iter()
            .filter(|record| record.role.starts_with(role_name));
        let record = matches
            .next()
            .ok_or_else(|| format!("manifest has no {role_name} font"))?;
        if matches.next().is_some() {
            return Err(format!("manifest has more than one {role_name} font"));
        }
        if !record.path.ends_with(".ttf") {
            return Err(format!("manifest {role_name} asset is not a TTF"));
        }
        fonts.push((role, record.path.clone()));
    }

    let image_paths = workload.items().iter().filter_map(|item| match item {
        reader_workload::Item::Image { asset_path, .. } => Some(asset_path.as_str()),
        reader_workload::Item::Heading { .. } | reader_workload::Item::Paragraph { .. } => None,
    });
    let mut image_paths = image_paths.peekable();
    let image_path = image_paths
        .next()
        .ok_or_else(|| "workload contains no image block".to_string())?
        .to_string();
    if image_paths.any(|path| path != image_path) {
        return Err("workload image blocks refer to different assets".to_string());
    }
    if !fixture_manifest.records.iter().any(|record| {
        record.path == image_path
            && record.role.starts_with("workload image")
            && record.dimensions == Some(reader_workload::IMAGE_INTRINSIC)
    }) {
        return Err(format!(
            "workload image {image_path:?} is not the manifest-listed fixture image"
        ));
    }

    let cached = RefCell::new(HashMap::<std::path::PathBuf, Vec<u8>>::new());
    let hash_file = |path: &Path| -> Result<String, String> {
        let bytes = std::fs::read(path)
            .map_err(|error| format!("cannot read {} for SHA-256: {error}", path.display()))?;
        let hash = format!("{:x}", Sha256::digest(&bytes));
        cached.borrow_mut().insert(path.to_path_buf(), bytes);
        Ok(hash)
    };
    reader_workload::manifest::validate_assets(&fixture_root, &fixture_manifest, &hash_file)
        .map_err(|error| format!("fixture asset validation failed: {error}"))?;
    let mut cached = cached.into_inner();

    let fonts = fonts
        .into_iter()
        .map(|(role, path)| {
            let full_path = fixture_root.join(&path);
            let bytes = cached.remove(&full_path).ok_or_else(|| {
                format!(
                    "validated font bytes were not retained: {}",
                    full_path.display()
                )
            })?;
            Ok(FontAsset { role, path, bytes })
        })
        .collect::<Result<Vec<_>, String>>()?;
    let full_image_path = fixture_root.join(&image_path);
    let image_bytes = cached.remove(&full_image_path).ok_or_else(|| {
        format!(
            "validated image bytes were not retained: {}",
            full_image_path.display()
        )
    })?;
    let decoded = image::load_from_memory(&image_bytes)
        .map_err(|error| {
            format!(
                "cannot decode fixture image {}: {error}",
                full_image_path.display()
            )
        })?
        .to_rgba8();
    let (width, height) = decoded.dimensions();
    if (width, height) != reader_workload::IMAGE_INTRINSIC {
        return Err(format!(
            "decoded fixture image dimensions are {width}x{height}, expected {}x{}",
            reader_workload::IMAGE_INTRINSIC.0,
            reader_workload::IMAGE_INTRINSIC.1
        ));
    }

    Ok(ReaderPackage {
        fixture_root,
        fixture_revision: workload.fixture_revision().to_string(),
        workload,
        fonts,
        image: ImageAsset {
            width,
            height,
            rgba: decoded.into_raw(),
        },
    })
}

fn is_japanese_or_cjk(character: char) -> bool {
    matches!(
        character as u32,
        0x3000..=0x303f
            | 0x3040..=0x30ff
            | 0x31f0..=0x31ff
            | 0x3400..=0x4dbf
            | 0x4e00..=0x9fff
            | 0xff00..=0xffef
    )
}

#[cfg(test)]
mod tests {
    use std::{
        ffi::OsString,
        path::{Path, PathBuf},
        sync::atomic::{AtomicU64, Ordering},
    };

    use reader_workload::{BaseDirection, InlineStyle, Item, StyleRun, WorkloadSize, workload};

    struct TestDirectory(PathBuf);

    impl TestDirectory {
        fn new() -> Self {
            static NEXT_ID: AtomicU64 = AtomicU64::new(0);
            loop {
                let path = std::env::temp_dir().join(format!(
                    "iced-reader-test-{}-{}",
                    std::process::id(),
                    NEXT_ID.fetch_add(1, Ordering::Relaxed)
                ));
                match std::fs::create_dir(&path) {
                    Ok(()) => return Self(path),
                    Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => continue,
                    Err(error) => panic!("create test directory {}: {error}", path.display()),
                }
            }
        }

        fn path(&self) -> &Path {
            &self.0
        }
    }

    impl Drop for TestDirectory {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    fn copy_fixture(destination: &Path) -> PathBuf {
        let source_root = reader_workload::manifest::fixture_root().unwrap();
        let fixture_root = destination.join("fixtures/reader-workload");
        std::fs::create_dir_all(&fixture_root).unwrap();
        std::fs::copy(
            source_root.join("manifest.txt"),
            fixture_root.join("manifest.txt"),
        )
        .unwrap();
        let manifest = reader_workload::manifest::manifest().unwrap();
        for record in manifest.records {
            for relative in [&record.path, &record.license_path] {
                let source = source_root.join(relative);
                let target = fixture_root.join(relative);
                if target.exists() {
                    continue;
                }
                std::fs::create_dir_all(target.parent().unwrap()).unwrap();
                std::fs::copy(source, target).unwrap();
            }
        }
        fixture_root
    }

    #[test]
    fn bidi_diagnostic_requires_exact_argument_and_environment_gate() {
        assert!(!super::bidi_diagnostic_requested([OsString::from(
            "--reader-poc"
        )]));
        assert!(super::bidi_diagnostic_requested([OsString::from(
            "--bidi-diagnostic"
        )]));
        assert!(!super::bidi_diagnostic_requested([OsString::from(
            "--bidi-diagnostic-extra"
        )]));
        assert!(!super::bidi_diagnostic_enabled(None));
        assert!(!super::bidi_diagnostic_enabled(Some(std::ffi::OsStr::new(
            "true"
        ))));
        assert!(super::bidi_diagnostic_enabled(Some(std::ffi::OsStr::new(
            "1"
        ))));
    }

    #[test]
    fn reader_mode_requires_the_exact_explicit_flag() {
        assert!(!super::reader_poc_requested([OsString::from("--other")]));
        assert!(super::reader_poc_requested([OsString::from(
            "--reader-poc"
        )]));
        assert!(!super::reader_poc_requested([OsString::from(
            "--reader-poc-extra"
        )]));
    }

    #[test]
    fn large_reader_mode_is_explicit_and_not_the_default_reader_flag() {
        assert!(!super::reader_large_requested([OsString::from(
            "--reader-poc"
        )]));
        assert!(!super::reader_large_requested([OsString::from(
            "--reader-poc-large-extra"
        )]));
        assert!(super::reader_large_requested([OsString::from(
            "--reader-poc-large"
        )]));
    }

    #[test]
    fn width_scenarios_use_the_shared_recipe_and_toggle() {
        assert_eq!(super::WidthScenario::Wide.width_dip(), 800);
        assert_eq!(
            super::WidthScenario::Wide.toggled(),
            super::WidthScenario::Narrow
        );
        assert_eq!(super::WidthScenario::Narrow.width_dip(), 480);
        assert_eq!(
            super::WidthScenario::Narrow.toggled(),
            super::WidthScenario::Wide
        );
    }

    #[test]
    fn maps_fixture_styles_and_scripts_without_changing_logical_utf8_text() {
        let work = workload(WorkloadSize::Small);
        let Item::Paragraph {
            text, style_runs, ..
        } = work.item_by_id("p-00012").unwrap()
        else {
            panic!("p-00012 is a paragraph");
        };
        let mapped = super::map_paragraph(text, BaseDirection::Ltr, style_runs).unwrap();
        assert_eq!(mapped.text, *text);
        assert!(
            mapped
                .runs
                .iter()
                .any(|run| { run.bytes == (27..29) && run.role == super::FontRole::Arabic })
        );
        assert!(
            mapped
                .runs
                .iter()
                .any(|run| { run.bytes == (29..31) && run.role == super::FontRole::LatinBold })
        );
    }

    #[test]
    fn rtl_intent_is_marked_without_reversing_logical_unicode_or_style_bytes() {
        let mapped = super::map_paragraph(
            "(اא) 1-2",
            BaseDirection::Rtl,
            &[StyleRun {
                start_byte: 5,
                end_byte: 8,
                style: InlineStyle::Bold,
            }],
        )
        .unwrap();
        assert_eq!(mapped.text, "\u{200f}(اא) 1-2");
        assert!(mapped.text.ends_with("(اא) 1-2"));
        assert!(mapped.runs.iter().any(|run| {
            run.role == super::FontRole::LatinBold && run.bytes.end <= mapped.text.len()
        }));
    }

    #[test]
    fn curated_bidi_cases_map_to_exact_leading_rlm_inputs() {
        let work = workload(WorkloadSize::Small);
        for id in ["p-00003", "p-00004"] {
            let Item::Paragraph {
                text,
                base_direction,
                style_runs,
                ..
            } = work.item_by_id(id).unwrap_or_else(|| panic!("{id} exists"))
            else {
                panic!("{id} is a paragraph");
            };
            assert_eq!(*base_direction, BaseDirection::Rtl, "{id} direction");

            let mapped = super::map_paragraph(text, *base_direction, style_runs)
                .unwrap_or_else(|error| panic!("{id} maps: {error}"));
            let exact_adapter_input = format!("\u{200f}{text}");
            assert_eq!(
                mapped.text.as_bytes(),
                exact_adapter_input.as_bytes(),
                "{id}"
            );
            assert!(mapped.text.is_char_boundary('\u{200f}'.len_utf8()));
            assert_eq!(&mapped.text['\u{200f}'.len_utf8()..], *text);
            for run in &mapped.runs {
                assert!(run.bytes.start <= run.bytes.end, "{id} run range");
                assert!(run.bytes.end <= mapped.text.len(), "{id} run bounds");
                assert!(
                    mapped.text.is_char_boundary(run.bytes.start),
                    "{id} run start"
                );
                assert!(mapped.text.is_char_boundary(run.bytes.end), "{id} run end");
            }
        }
    }

    #[test]
    fn invalid_utf8_style_boundaries_are_rejected_before_rich_text_mapping() {
        let error = super::map_paragraph(
            "é",
            BaseDirection::Ltr,
            &[StyleRun {
                start_byte: 0,
                end_byte: 1,
                style: InlineStyle::Bold,
            }],
        )
        .unwrap_err();
        assert!(error.contains("invalid UTF-8 style range"), "{error}");
    }

    #[test]
    fn pinned_font_roles_use_the_family_and_weight_supported_by_iced() {
        use iced::font::{Family, Style, Weight};

        assert_eq!(
            super::FontRole::LatinBold.iced_font().family,
            Family::Name("Noto Sans")
        );
        assert_eq!(super::FontRole::LatinBold.iced_font().weight, Weight::Bold);
        assert_eq!(
            super::FontRole::LatinItalic.iced_font().style,
            Style::Italic
        );
        assert_eq!(
            super::FontRole::Arabic.iced_font().family,
            Family::Name("Noto Sans Arabic")
        );
        assert_eq!(
            super::FontRole::Hebrew.iced_font().family,
            Family::Name("Noto Sans Hebrew")
        );
        assert_eq!(
            super::FontRole::Japanese.iced_font().family,
            Family::Name("Noto Sans JP")
        );
        assert_eq!(super::FontRole::Japanese.iced_font().weight, Weight::Normal);
    }

    #[test]
    fn bidi_diagnostic_matrix_preserves_text_and_changes_one_factor_at_a_time() {
        let work = workload(WorkloadSize::Small);
        let Item::Paragraph {
            text,
            base_direction,
            style_runs,
            ..
        } = work.item_by_id("p-00004").expect("p-00004 exists")
        else {
            panic!("p-00004 is a paragraph");
        };
        let variants = super::bidi_diagnostic_variants();
        assert_eq!(variants.len(), 4);
        assert_eq!(
            variants.map(|variant| (variant.presentation, variant.leading_rlm)),
            [
                (super::DiagnosticPresentation::Styled, true),
                (super::DiagnosticPresentation::Uniform, true),
                (super::DiagnosticPresentation::Uniform, false),
                (super::DiagnosticPresentation::Styled, false),
            ]
        );

        for variant in variants {
            let mapped =
                super::map_diagnostic_paragraph(text, *base_direction, style_runs, variant)
                    .expect("valid fixture variant");
            let marker_bytes = if variant.leading_rlm {
                '\u{200f}'.len_utf8()
            } else {
                0
            };
            assert_eq!(&mapped.text[marker_bytes..], text);
            assert_eq!(mapped.text.starts_with('\u{200f}'), variant.leading_rlm);
            match variant.presentation {
                super::DiagnosticPresentation::Styled => {
                    let shift = if variant.leading_rlm { 3 } else { 0 };
                    assert_eq!(
                        mapped.runs,
                        [
                            super::MappedRun {
                                bytes: 0..(6 + shift),
                                role: super::FontRole::Hebrew,
                            },
                            super::MappedRun {
                                bytes: (6 + shift)..(12 + shift),
                                role: super::FontRole::LatinRegular,
                            },
                            super::MappedRun {
                                bytes: (12 + shift)..(18 + shift),
                                role: super::FontRole::Arabic,
                            },
                            super::MappedRun {
                                bytes: (18 + shift)..(23 + shift),
                                role: super::FontRole::LatinRegular,
                            },
                        ]
                    );
                }
                super::DiagnosticPresentation::Uniform => {
                    assert_eq!(mapped.runs.len(), 1);
                    assert_eq!(mapped.runs[0].bytes, 0..mapped.text.len());
                    assert_eq!(mapped.runs[0].role, super::FontRole::LatinRegular);
                }
            }
        }
    }

    #[test]
    fn diagnostic_glyph_byte_ranges_project_utf8_scalars_and_added_rlm_to_source() {
        let mapped = "\u{200f}אבג (def ابج) abc";
        let marker = super::project_diagnostic_glyph_source(mapped, 0..3, true)
            .expect("RLM is a valid mapped scalar");
        assert_eq!(marker.mapped_scalar_indices, [0]);
        assert!(marker.logical_scalar_indices.is_empty());
        assert!(marker.logical_byte_ranges.is_empty());
        assert!(marker.includes_leading_rlm);

        let hebrew = super::project_diagnostic_glyph_source(mapped, 3..5, true)
            .expect("Hebrew cluster is a UTF-8 range");
        assert_eq!(hebrew.mapped_scalar_indices, [1]);
        assert_eq!(hebrew.logical_scalar_indices, [0]);
        assert_eq!(hebrew.logical_byte_ranges.len(), 1);
        assert_eq!(hebrew.logical_byte_ranges.first(), Some(&(0..2)));
        assert!(!hebrew.includes_leading_rlm);

        let arabic_start = mapped.find('ا').expect("Arabic alef exists");
        let arabic = super::project_diagnostic_glyph_source(
            mapped,
            arabic_start..arabic_start + 'ا'.len_utf8(),
            true,
        )
        .expect("Arabic scalar is a UTF-8 range");
        assert_eq!(arabic.mapped_scalar_indices, [10]);
        assert_eq!(arabic.logical_scalar_indices, [9]);
        assert_eq!(arabic.logical_byte_ranges.len(), 1);
        assert_eq!(arabic.logical_byte_ranges.first(), Some(&(12..14)));

        assert!(super::project_diagnostic_glyph_source(mapped, 4..5, true).is_none());
        assert!(super::project_diagnostic_glyph_source("אבג", 0..2, true).is_none());
    }

    #[test]
    fn diagnostic_order_classification_distinguishes_match_mismatch_and_clusters() {
        let expected = [14, 15, 16, 13, 12, 11, 10, 9, 8, 5, 6, 7, 4, 3, 2, 1, 0];
        let matching = expected
            .iter()
            .enumerate()
            .map(|(x, source)| super::PositionedSourceCluster {
                line_y: 0.0,
                x: x as f32,
                glyph_width: 1.0,
                level: 1,
                source_scalars: vec![*source],
            })
            .collect::<Vec<_>>();
        let mut matching = matching;
        matching.push(super::PositionedSourceCluster {
            line_y: 0.0,
            x: -1.0,
            glyph_width: 0.0,
            level: 1,
            source_scalars: Vec::new(),
        });
        assert_eq!(
            super::classify_diagnostic_source_order(&expected, &matching),
            super::DiagnosticDisposition::Pass
        );

        let mut duplicate_glyph = matching.clone();
        let mut mark = matching[7].clone();
        mark.x += 0.25;
        mark.glyph_width = 0.0;
        duplicate_glyph.push(mark);
        assert_eq!(
            super::classify_diagnostic_source_order(&expected, &duplicate_glyph),
            super::DiagnosticDisposition::Pass
        );

        let missing_wrapped_space = matching
            .iter()
            .filter(|placement| placement.source_scalars.as_slice() != [8])
            .cloned()
            .collect::<Vec<_>>();
        assert_eq!(
            super::classify_diagnostic_source_order(&expected, &missing_wrapped_space),
            super::DiagnosticDisposition::Inconclusive
        );

        let mut mismatch = matching.clone();
        let (left, right) = mismatch.split_at_mut(12);
        std::mem::swap(&mut left[4].source_scalars, &mut right[0].source_scalars);
        assert_eq!(
            super::classify_diagnostic_source_order(&expected, &mismatch),
            super::DiagnosticDisposition::Fail
        );

        let mut clustered = matching;
        clustered[7].source_scalars = vec![9, 10];
        assert_eq!(
            super::classify_diagnostic_source_order(&expected, &clustered),
            super::DiagnosticDisposition::Inconclusive
        );
    }

    #[test]
    fn diagnostic_order_rejects_duplicate_positive_advance_source_placements() {
        let placements = [
            super::PositionedSourceCluster {
                line_y: 0.0,
                x: 0.0,
                glyph_width: 1.0,
                level: 0,
                source_scalars: vec![0],
            },
            super::PositionedSourceCluster {
                line_y: 0.0,
                x: 1.0,
                glyph_width: 1.0,
                level: 0,
                source_scalars: vec![1],
            },
            super::PositionedSourceCluster {
                line_y: 0.0,
                x: 2.0,
                glyph_width: 1.0,
                level: 0,
                source_scalars: vec![0],
            },
        ];
        assert_eq!(
            super::classify_diagnostic_source_order(&[0, 1], &placements),
            super::DiagnosticDisposition::Inconclusive
        );
    }

    #[test]
    fn diagnostic_level_classification_is_separate_from_source_order() {
        let expected = [1, 1, 1, 1, 1, 2, 2, 2, 1, 1, 1, 1, 1, 1, 2, 2, 2];
        let placements = expected
            .iter()
            .enumerate()
            .map(|(x, level)| super::PositionedSourceCluster {
                line_y: 0.0,
                x: x as f32,
                glyph_width: 1.0,
                source_scalars: vec![x],
                level: *level,
            })
            .collect::<Vec<_>>();
        assert_eq!(
            super::classify_diagnostic_levels(&expected, &placements),
            super::DiagnosticDisposition::Pass
        );

        let mut mismatch = placements.clone();
        mismatch[5].level = 1;
        assert_eq!(
            super::classify_diagnostic_levels(&expected, &mismatch),
            super::DiagnosticDisposition::Fail
        );
        let incomplete = placements.into_iter().skip(1).collect::<Vec<_>>();
        assert_eq!(
            super::classify_diagnostic_levels(&expected, &incomplete),
            super::DiagnosticDisposition::Inconclusive
        );
    }

    #[test]
    fn package_load_validates_and_returns_shared_small_workload_and_local_assets() {
        let package = super::load_reader_package_from(Path::new(env!("CARGO_MANIFEST_DIR")))
            .expect("committed local fixture is valid");
        assert_eq!(package.fixture_revision, reader_workload::FIXTURE_REVISION);
        assert_eq!(package.workload.body_paragraph_count(), 1_000);
        assert_eq!(package.workload.total_item_count(), 1_051);
        assert_eq!(
            package
                .fonts
                .iter()
                .map(|font| (font.role, font.path.as_str()))
                .collect::<Vec<_>>(),
            [
                (
                    super::FontRole::LatinRegular,
                    "assets/fonts/NotoSans-Regular.ttf"
                ),
                (super::FontRole::LatinBold, "assets/fonts/NotoSans-Bold.ttf"),
                (
                    super::FontRole::LatinItalic,
                    "assets/fonts/NotoSans-Italic.ttf"
                ),
                (
                    super::FontRole::Arabic,
                    "assets/fonts/NotoSansArabic-Regular.ttf"
                ),
                (
                    super::FontRole::Hebrew,
                    "assets/fonts/NotoSansHebrew-Regular.ttf"
                ),
                (
                    super::FontRole::Japanese,
                    "assets/fonts/NotoSansJP[wght].ttf"
                ),
            ]
        );
        assert_eq!((package.image.width, package.image.height), (480, 320));
        let colors = package
            .image
            .rgba
            .chunks_exact(4)
            .map(|pixel| [pixel[0], pixel[1], pixel[2], pixel[3]])
            .collect::<std::collections::HashSet<_>>();
        assert!(
            colors.len() > 2,
            "fixture image must not be a placeholder fill"
        );
    }

    #[test]
    fn large_package_keeps_the_same_validated_assets_and_small_prefix() {
        let root = Path::new(env!("CARGO_MANIFEST_DIR"));
        let small = super::load_reader_package_from(root).unwrap();
        let large = super::load_reader_package_from_size(root, WorkloadSize::Large).unwrap();
        assert_eq!(large.workload.body_paragraph_count(), 10_000);
        assert_eq!(large.workload.total_item_count(), 10_501);
        assert_eq!(large.fixture_revision, small.fixture_revision);
        assert_eq!(large.fonts.len(), small.fonts.len());
        assert_eq!(large.image.rgba, small.image.rgba);
        assert_eq!(
            &large.workload.items()[..small.workload.total_item_count()],
            small.workload.items()
        );
    }

    #[test]
    fn package_load_reports_missing_root_and_checksum_corruption() {
        let missing = TestDirectory::new();
        let error = super::load_reader_package_from(missing.path()).unwrap_err();
        assert!(error.contains("no ancestor"), "{error}");

        let corrupt = TestDirectory::new();
        let root = copy_fixture(corrupt.path());
        let font = root.join("assets/fonts/NotoSans-Regular.ttf");
        let mut bytes = std::fs::read(&font).unwrap();
        bytes[0] ^= 0xff;
        std::fs::write(font, bytes).unwrap();
        let error = super::load_reader_package_from(corrupt.path()).unwrap_err();
        assert!(error.contains("checksum mismatch"), "{error}");

        let missing_image = TestDirectory::new();
        let root = copy_fixture(missing_image.path());
        std::fs::remove_file(root.join("assets/images/reader-sample.png")).unwrap();
        let error = super::load_reader_package_from(missing_image.path()).unwrap_err();
        assert!(error.contains("missing manifest asset"), "{error}");

        let corrupt_image = TestDirectory::new();
        let root = copy_fixture(corrupt_image.path());
        let image = root.join("assets/images/reader-sample.png");
        let mut bytes = std::fs::read(&image).unwrap();
        let middle = bytes.len() / 2;
        bytes[middle] ^= 0xff;
        std::fs::write(image, bytes).unwrap();
        let error = super::load_reader_package_from(corrupt_image.path()).unwrap_err();
        assert!(error.contains("checksum mismatch"), "{error}");
    }

    #[test]
    fn package_load_rejects_a_manifest_from_another_fixture_revision() {
        let temporary = TestDirectory::new();
        let root = temporary.path().join("fixtures/reader-workload");
        std::fs::create_dir_all(&root).unwrap();
        let source = std::fs::read_to_string(
            reader_workload::manifest::fixture_root()
                .unwrap()
                .join("manifest.txt"),
        )
        .unwrap();
        let changed = source.replacen(
            &format!("fixture-revision: {}", reader_workload::FIXTURE_REVISION),
            "fixture-revision: unsupported-revision",
            1,
        );
        std::fs::write(root.join("manifest.txt"), changed).unwrap();
        let error = super::load_reader_package_from(temporary.path()).unwrap_err();
        assert!(error.contains("invalid fixture manifest"), "{error}");
    }
}
