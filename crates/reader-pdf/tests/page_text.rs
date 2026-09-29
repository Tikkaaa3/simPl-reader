use std::{
    future::Future,
    sync::Arc,
    task::{Context, Poll, Wake, Waker},
};

struct Thread(std::thread::Thread);
impl Wake for Thread {
    fn wake(self: Arc<Self>) {
        self.0.unpark();
    }
}
fn complete<T>(future: impl Future<Output = T>) -> T {
    let waker = Waker::from(Arc::new(Thread(std::thread::current())));
    let mut context = Context::from_waker(&waker);
    let mut future = std::pin::pin!(future);
    loop {
        match future.as_mut().poll(&mut context) {
            Poll::Ready(value) => return value,
            Poll::Pending => std::thread::park(),
        }
    }
}

/// A one-page PDF with two lines of Helvetica text and a correct cross-reference table.
fn sample_pdf() -> Vec<u8> {
    let content =
        "BT /F1 18 Tf 20 150 Td (The old Lighthouse keeper) Tj 0 -30 Td (watched the sea) Tj ET";
    let objects = [
        "<</Type/Catalog/Pages 2 0 R>>".to_owned(),
        "<</Type/Pages/Kids[3 0 R]/Count 1>>".to_owned(),
        "<</Type/Page/Parent 2 0 R/MediaBox[0 0 300 200]/Contents 4 0 R/Resources<</Font<</F1 5 0 R>>>>>>".to_owned(),
        format!("<</Length {}>>\nstream\n{content}\nendstream", content.len()),
        "<</Type/Font/Subtype/Type1/BaseFont/Helvetica>>".to_owned(),
    ];
    let mut pdf = b"%PDF-1.4\n".to_vec();
    let mut offsets = Vec::new();
    for (index, body) in objects.iter().enumerate() {
        offsets.push(pdf.len());
        pdf.extend(format!("{} 0 obj\n{body}\nendobj\n", index + 1).bytes());
    }
    let xref = pdf.len();
    pdf.extend(format!("xref\n0 {}\n0000000000 65535 f \n", objects.len() + 1).bytes());
    for offset in offsets {
        pdf.extend(format!("{offset:010} 00000 n \n").bytes());
    }
    pdf.extend(
        format!(
            "trailer\n<</Root 1 0 R/Size {}>>\nstartxref\n{xref}\n%%EOF\n",
            objects.len() + 1
        )
        .bytes(),
    );
    pdf
}

#[test]
fn page_text_indices_are_the_glyph_indices_of_the_text_layer() {
    let dll = std::env::current_exe()
        .unwrap()
        .parent()
        .unwrap()
        .join("pdfium.dll");
    if !dll.exists() {
        eprintln!("skipped: pdfium.dll is not beside the test executable");
        return;
    }
    let path = std::env::temp_dir().join(format!("simpl-page-text-{}.pdf", std::process::id()));
    std::fs::write(&path, sample_pdf()).unwrap();
    let document = complete(reader_pdf::open(path.clone())).unwrap();
    let plain = complete(document.session.page_text(0)).unwrap();
    let layer = complete(document.session.text(0, 600)).unwrap();
    let _ = std::fs::remove_file(path);

    assert!(plain.contains("Lighthouse keeper"), "{plain:?}");
    assert!(plain.contains("watched the sea"), "{plain:?}");
    // Search reports character positions in `plain`; selection addresses glyphs.
    assert_eq!(plain, layer.text);
    assert_eq!(layer.glyphs.len(), plain.chars().count());
    let first = plain.chars().position(|c| c == 'L').unwrap();
    let last = first + "Lighthouse".len() - 1;
    let matched: String = plain.chars().skip(first).take(last - first + 1).collect();
    assert_eq!(matched, "Lighthouse");
    let boxed: Vec<_> = layer.glyphs[first..=last]
        .iter()
        .filter_map(|glyph| glyph.bounds)
        .collect();
    assert_eq!(boxed.len(), "Lighthouse".len());
    // The first line sits above the second one on the page (top-down coordinates).
    let second_line = plain.chars().position(|c| c == 'w').unwrap();
    let a = layer.glyphs[first].bounds.unwrap();
    let b = layer.glyphs[second_line].bounds.unwrap();
    assert!(a.top < b.top, "{a:?} {b:?}");
}
