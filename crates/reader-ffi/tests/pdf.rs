use reader_ffi::{PdfLocation, open_pdf_document};

fn sample_pdf() -> Vec<u8> {
    let mut objects = vec![
        "<</Type/Catalog/Pages 2 0 R>>".to_owned(),
        "<</Type/Pages/Kids[4 0 R 6 0 R]/Count 2>>".to_owned(),
        "<</Type/Font/Subtype/Type1/BaseFont/Helvetica>>".to_owned(),
    ];
    for (i, text) in ["Lighthouse keeper", "Watched the sea"].iter().enumerate() {
        let content = format!("BT /F1 18 Tf 20 150 Td ({text}) Tj ET");
        objects.push(format!("<</Type/Page/Parent 2 0 R/MediaBox[0 0 300 200]/Contents {} 0 R/Resources<</Font<</F1 3 0 R>>>>>>", 5 + i * 2));
        objects.push(format!(
            "<</Length {}>>\nstream\n{content}\nendstream",
            content.len()
        ));
    }
    let mut pdf = String::from("%PDF-1.4\n");
    let mut offsets = Vec::new();
    for (index, body) in objects.iter().enumerate() {
        offsets.push(pdf.len());
        pdf.push_str(&format!("{} 0 obj\n{body}\nendobj\n", index + 1));
    }
    let xref = pdf.len();
    pdf.push_str(&format!(
        "xref\n0 {}\n0000000000 65535 f \n",
        objects.len() + 1
    ));
    for offset in offsets {
        pdf.push_str(&format!("{offset:010} 00000 n \n"));
    }
    pdf.push_str(&format!(
        "trailer\n<</Root 1 0 R/Size {}>>\nstartxref\n{xref}\n%%EOF\n",
        objects.len() + 1
    ));
    pdf.into_bytes()
}

#[test]
fn document_rasters_glyph_copy_limits_and_desktop_positions() {
    let library = if cfg!(target_os = "android") {
        "libpdfium.so"
    } else {
        "pdfium.dll"
    };
    if !std::env::current_exe()
        .unwrap()
        .parent()
        .unwrap()
        .join(library)
        .exists()
    {
        eprintln!("skipped: {library} is not beside the test executable");
        return;
    }
    let root = std::env::temp_dir().join(format!("simpl-pdf-ffi-{}", std::process::id()));
    std::fs::create_dir_all(&root).unwrap();
    reader_ffi::initialize(
        root.join("data").to_string_lossy().into_owned(),
        root.join("cache").to_string_lossy().into_owned(),
        "en".into(),
    )
    .unwrap();
    let path = root.join("two-pages.pdf");
    std::fs::write(&path, sample_pdf()).unwrap();
    let imported = reader_ffi::import_library_book(path.to_string_lossy().into_owned()).unwrap();
    let document = open_pdf_document(imported.path.clone()).unwrap();
    let info = document.info().unwrap();
    assert_eq!(info.pages.len(), 2);
    assert!(info.can_copy);
    assert!(info.restored.fit_width);
    let low = document.render(1, 300).unwrap();
    let high = document.render(1, 1200).unwrap();
    assert_eq!((low.width, low.height), (300, 200));
    assert_eq!((high.width, high.height), (1200, 800));
    assert_eq!(high.rgba.len(), (high.width * high.height * 4) as usize);
    assert!(low.rgba.chunks_exact(4).any(|p| p[0] < 100));
    let capped = document.render(2, u32::MAX).unwrap();
    assert!(u64::from(capped.width) * u64::from(capped.height) <= 4_000_000);
    assert!(document.render(0, 300).is_err());
    assert!(document.render(3, 300).is_err());
    assert!(document.render(1, 0).is_err());
    let text = document.text(1, 600).unwrap();
    assert!(text.text.contains("Lighthouse keeper"));
    assert_eq!(text.glyphs.len(), text.text.chars().count());
    assert!(
        text.glyphs
            .iter()
            .filter_map(|g| g.bounds.as_ref())
            .all(|r| r.left >= 0.0 && r.bottom <= 1.0)
    );
    assert_eq!(document.copy(1, 0, 9).unwrap(), "Lighthouse");
    assert_eq!(document.copy(1, 9, 0).unwrap(), "Lighthouse");
    assert!(document.copy(1, u32::MAX, u32::MAX).is_err());
    document
        .save_location(PdfLocation {
            page: 2,
            within: 0.375,
            horizontal: 0.2,
            zoom: 1.75,
            fit_width: false,
        })
        .unwrap();
    let saved = reader_document::position::load_pdf(std::path::Path::new(&imported.path))
        .unwrap()
        .unwrap();
    assert_eq!(saved.page, 1);
    assert_eq!(saved.zoom, reader_document::position::PdfZoom::Scale(1.75));
    let reopened = open_pdf_document(imported.path.clone())
        .unwrap()
        .info()
        .unwrap()
        .restored;
    assert_eq!(reopened.page, 2);
    assert_eq!(reopened.within, 0.375);
    assert_eq!(reopened.horizontal, 0.2);
    assert_eq!(reopened.zoom, 1.75);
    assert!(!reopened.fit_width);
    assert!(
        document
            .save_location(PdfLocation {
                zoom: f32::NAN,
                ..reopened.clone()
            })
            .is_err()
    );
    assert!(
        document
            .save_location(PdfLocation {
                within: -0.1,
                ..reopened.clone()
            })
            .is_err()
    );
    document
        .save_location(PdfLocation {
            fit_width: true,
            ..reopened
        })
        .unwrap();
    assert!(document.info().unwrap().restored.fit_width);
    assert_eq!(reader_ffi::load_library().unwrap().books[0].current, 2);
    let restricted_path = root.join("copy-restricted.pdf");
    std::fs::write(
        &restricted_path,
        include_bytes!("fixtures/copy-restricted.pdf"),
    )
    .unwrap();
    let restricted = open_pdf_document(restricted_path.to_string_lossy().into_owned()).unwrap();
    assert!(!restricted.info().unwrap().can_copy);
    assert!(!restricted.render(1, 300).unwrap().rgba.is_empty());
    let hidden = restricted.text(1, 600).unwrap();
    assert!(hidden.text.is_empty() && hidden.glyphs.is_empty());
    assert!(
        restricted
            .copy(1, 0, 1)
            .unwrap_err()
            .to_string()
            .contains("permissions")
    );
    drop(restricted);
    drop(document);
    std::fs::remove_dir_all(root).unwrap();
}
