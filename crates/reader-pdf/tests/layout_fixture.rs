use reader_pdf::book::BlockLayout;
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

#[test]
#[ignore = "Native PDF QA: generate fixtures/pdf-book/build_fixtures.py and place pdfium.dll beside the test executable"]
fn book_preserves_layout_evidence_from_real_pdf() {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../target/book-milestone3/fixtures/layout.pdf");
    let document = complete(reader_pdf::open(path)).unwrap();
    let conversion = complete(document.session.book()).unwrap();
    for (page, label) in ["i", "ii", "iii", "1"].into_iter().enumerate() {
        assert_eq!(
            conversion
                .page_labels
                .get(&(page as u32))
                .map(String::as_str),
            Some(label)
        );
    }
    assert!(conversion.blocks.iter().any(|b| b.sources[0].page == 0
        && b.text.contains("A BOOK OF MYTHS")
        && b.layout == BlockLayout::Centered
        && b.top_gap > 0.2));
    assert!(conversion.blocks.iter().any(|b| b.sources[0].page == 0
        && b.text.contains("A short illustrated edition")
        && b.layout == BlockLayout::Centered));
    let toc: Vec<_> = conversion
        .blocks
        .iter()
        .filter(|b| b.sources[0].page == 1 && matches!(b.layout, BlockLayout::Toc { .. }))
        .collect();
    assert_eq!(toc.len(), 3, "{toc:?}");
    assert!(
        toc.iter()
            .any(|entry| entry.text.starts_with("The first myth")
                && entry.links.iter().any(|link| link.href == "pdf-page:2"
                    && &entry.text[link.start..link.end] == "The first myth")),
        "{toc:?}"
    );
    let myths: Vec<_> = conversion
        .blocks
        .iter()
        .filter(|b| b.sources[0].page == 2 && b.text.starts_with("Myth #"))
        .collect();
    assert_eq!(myths.len(), 2, "{myths:?}");
    assert!(myths[0].styles.iter().any(|style| style.bold));
    assert!(myths[1].styles.iter().any(|style| style.italic));
    assert!(conversion.fallback_pages.contains(&3));
    assert!(
        conversion
            .blocks
            .iter()
            .any(|b| b.sources[0].page == 3 && conversion.illustrations.contains_key(&b.id))
    );
}
