use std::{
    future::Future,
    sync::Arc,
    task::{Context, Poll, Wake, Waker},
    time::Instant,
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
fn main() {
    for path in std::env::args_os().skip(1) {
        let path = std::path::PathBuf::from(path);
        let document = complete(reader_pdf::open(path.clone())).unwrap();
        let mut reference = None;
        for attempt in 0..2 {
            let start = Instant::now();
            let book = complete(document.session.book()).unwrap();
            println!(
                "{} attempt {}: {:?}, {} blocks, {} illustrations",
                document.title,
                attempt + 1,
                start.elapsed(),
                book.blocks.len(),
                book.illustrations.len()
            );
            let bytes = serde_json::to_value(&book).unwrap();
            if let Some(reference) = &reference {
                assert_eq!(reference, &bytes);
            }
            reference = Some(bytes);
        }
        drop(document);
        let document = complete(reader_pdf::open(path)).unwrap();
        let start = Instant::now();
        let book = complete(document.session.book()).unwrap();
        println!("{} reopened: {:?}", document.title, start.elapsed());
        assert_eq!(reference.unwrap(), serde_json::to_value(&book).unwrap());
    }
}
