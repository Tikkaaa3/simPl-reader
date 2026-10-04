//! Cache roots differ on Android; the canonical on-disk schema does not.
use reader_layout::{atlas, book};
use std::sync::{Arc, atomic::AtomicBool};

#[test]
fn cache_uses_the_configured_cache_root_and_retains_the_v1_schema() {
    let scratch = std::env::temp_dir().join(format!("simpl-atlas-cache-{}", std::process::id()));
    let data = scratch.join("data");
    let cache = scratch.join("cache");
    reader_profile::configure(&data, &cache).unwrap();
    std::fs::create_dir_all(&scratch).unwrap();
    let path = scratch.join("book.html");
    std::fs::write(&path, "<h1>Harbour</h1><p>The boats came in.</p>").unwrap();
    let book = Arc::new(book::open(&path).unwrap());
    let cancel = AtomicBool::new(false);
    let first = atlas::build_with_cache(book.clone(), &cancel, true)
        .unwrap()
        .unwrap();
    let cache_path = cache
        .join("simPl/page-maps")
        .join(format!("v1-{}.json", book.fingerprint));
    assert!(cache_path.exists());
    assert!(!data.join("simPl/page-maps").exists());
    let bytes = std::fs::read(&cache_path).unwrap();
    let record: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(record["version"], 1);
    let second = atlas::build_with_cache(book.clone(), &cancel, true)
        .unwrap()
        .unwrap();
    assert_eq!(
        serde_json::to_vec(&first).unwrap(),
        serde_json::to_vec(&second).unwrap()
    );
    assert_eq!(std::fs::read(&cache_path).unwrap(), bytes);
    assert!(
        atlas::build_with_cache(book.clone(), &AtomicBool::new(true), true)
            .unwrap()
            .is_none()
    );
    std::fs::write(&cache_path, b"invalid cache").unwrap();
    assert!(atlas::cached(&book, 1).is_none());
    let rebuilt = atlas::build_with_cache(book, &cancel, true)
        .unwrap()
        .unwrap();
    assert_eq!(
        serde_json::to_vec(&first).unwrap(),
        serde_json::to_vec(&rebuilt).unwrap()
    );
    std::fs::remove_dir_all(scratch).unwrap();
}
