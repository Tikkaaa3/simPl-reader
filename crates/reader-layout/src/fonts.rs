//! The bundled reading fonts, embedded once. The desktop registers these same
//! byte slices (with its chrome fonts) and the Android core loads them before
//! measuring, so both shape book text with identical faces. The text system
//! skips a slice it already loaded (by address), so loading twice is harmless.

macro_rules! font {
    ($name:ident, $file:literal) => {
        pub static $name: &[u8] = include_bytes!(concat!("../../../assets/fonts/", $file));
    };
}

font!(GEIST, "Geist-Variable-Latin.ttf");
font!(LITERATA_REGULAR, "Literata-Regular.ttf");
font!(LITERATA_MEDIUM, "Literata-Medium.ttf");
font!(LITERATA_BOLD, "Literata-Bold.ttf");
font!(LITERATA_ITALIC, "Literata-Italic.ttf");
font!(LITERATA_BOLD_ITALIC, "Literata-BoldItalic.ttf");
font!(SPECTRAL_REGULAR, "Spectral-Regular.ttf");
font!(SPECTRAL_MEDIUM, "Spectral-Medium.ttf");
font!(SPECTRAL_BOLD, "Spectral-Bold.ttf");
font!(SPECTRAL_ITALIC, "Spectral-Italic.ttf");
font!(SPECTRAL_BOLD_ITALIC, "Spectral-BoldItalic.ttf");
font!(FIRA_SANS_REGULAR, "FiraSans-Regular.ttf");
font!(FIRA_SANS_MEDIUM, "FiraSans-Medium.ttf");
font!(FIRA_SANS_BOLD, "FiraSans-Bold.ttf");
font!(FIRA_SANS_ITALIC, "FiraSans-Italic.ttf");
font!(FIRA_SANS_BOLD_ITALIC, "FiraSans-BoldItalic.ttf");

/// Family of the layout's default font (messages such as "Image unavailable").
pub const SANS: iced_core::Font = iced_core::Font::with_name("Geist");

/// Every face book text can be laid out with, in the desktop's loading order.
pub fn reading() -> [&'static [u8]; 16] {
    [
        GEIST,
        LITERATA_REGULAR,
        LITERATA_MEDIUM,
        LITERATA_BOLD,
        LITERATA_ITALIC,
        LITERATA_BOLD_ITALIC,
        SPECTRAL_REGULAR,
        SPECTRAL_MEDIUM,
        SPECTRAL_BOLD,
        SPECTRAL_ITALIC,
        SPECTRAL_BOLD_ITALIC,
        FIRA_SANS_REGULAR,
        FIRA_SANS_MEDIUM,
        FIRA_SANS_BOLD,
        FIRA_SANS_ITALIC,
        FIRA_SANS_BOLD_ITALIC,
    ]
}

/// Register the reading fonts with the shared text system (once per process).
pub fn load() {
    static LOADED: std::sync::Once = std::sync::Once::new();
    LOADED.call_once(|| {
        let mut system = iced_renderer::graphics::text::font_system()
            .write()
            .expect("text system");
        for bytes in reading() {
            system.load_font(std::borrow::Cow::Borrowed(bytes));
        }
    });
}
