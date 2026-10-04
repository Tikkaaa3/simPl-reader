//! Bounded offline word lookup using separately installed data packages.
use reader_core::word_translation::Lexicon;
pub use reader_core::word_translation::{
    Language, MAX_QUERY_BYTES, MAX_QUERY_WORDS, Settings, Translation, query, supported,
};
use serde::Deserialize;
use sha2::{Digest, Sha256};
use std::{
    io::{Cursor, Read},
    sync::{Arc, LazyLock, Mutex},
};
#[path = "dictionary_packages.rs"]
mod packages;
pub use packages::{
    LookupError, Package, PackageId, PackageState, Store, data_version, package, package_id,
    packages,
};
const MAX_DATA: u64 = 16 * 1024 * 1024;
#[derive(Deserialize)]
struct Manifest {
    version: u32,
    pairs: Vec<Pair>,
}
#[derive(Deserialize)]
struct Pair {
    source: String,
    target: String,
    provider: String,
    sha256: String,
}

#[cfg(test)]
mod tests {
    use super::*;

    struct Fixture {
        store: Store,
        path: std::path::PathBuf,
    }
    impl Drop for Fixture {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.path);
        }
    }
    fn fixture() -> Fixture {
        static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
        let path = std::env::temp_dir().join(format!(
            "simpl-dictionaries-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
        ));
        Fixture {
            store: Store::new(path.clone()),
            path,
        }
    }
    fn bytes(id: PackageId) -> Vec<u8> {
        std::fs::read(
            std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("../../assets/dictionaries/packs")
                .join(&package(id).unwrap().file),
        )
        .unwrap()
    }
    fn install(store: &Store, id: PackageId) {
        store
            .install(id, &bytes(id), &std::sync::atomic::AtomicBool::new(false))
            .unwrap();
    }

    #[test]
    #[ignore = "Release lookup timing: installed source fixtures in an owned temporary store"]
    fn profile_installed_dictionary_lookup() {
        let fixture = fixture();
        for (index, p) in packages().iter().enumerate() {
            let id = PackageId(index);
            install(&fixture.store, id);
            let word = match p.source {
                Language::English => "book",
                Language::Turkish => "kitap",
                Language::Spanish => "libro",
                Language::German => "Buch",
                Language::French => "livre",
                Language::Japanese => "本",
                Language::Chinese => "書",
                Language::Korean => "책",
            };
            let mut first = Vec::new();
            for _ in 0..5 {
                let reopened = Store::new(fixture.path.clone());
                let started = std::time::Instant::now();
                assert!(reopened.lookup(word, p.source, p.target).unwrap().is_some());
                first.push(started.elapsed().as_micros());
            }
            first.sort_unstable();
            let reopened = Store::new(fixture.path.clone());
            assert!(reopened.lookup(word, p.source, p.target).unwrap().is_some());
            let started = std::time::Instant::now();
            for _ in 0..1000 {
                assert!(
                    std::hint::black_box(reopened.lookup(word, p.source, p.target).unwrap())
                        .is_some()
                );
            }
            println!(
                "{} first_median_us={} warm_mean_us={:.2}",
                p.label(),
                first[2],
                started.elapsed().as_secs_f64() * 1000.0
            );
        }
    }

    #[test]
    fn normalization_preserves_language_and_bounds() {
        assert_eq!(
            query("  “BOOK,” ", Language::English).as_deref(),
            Some("book")
        );
        assert_eq!(query("IŞIK", Language::Turkish).as_deref(), Some("ışık"));
        assert_eq!(query("İYİ", Language::Turkish).as_deref(), Some("iyi"));
        assert_eq!(
            query("ＢＯＯＫ", Language::English).as_deref(),
            Some("book")
        );
        assert_eq!(query("Don't", Language::English).as_deref(), Some("don't"));
        assert_eq!(
            query("cafe\u{0301}", Language::French).as_deref(),
            Some("café")
        );
        assert!(query("one two three four five", Language::English).is_none());
        assert!(query(&"a".repeat(257), Language::English).is_none());
        assert!(query("...", Language::English).is_none());
    }

    #[test]
    fn downloadable_pairs_pass_integrity_and_return_real_words() {
        let fixture = fixture();
        let store = &fixture.store;
        for (i, p) in packages().iter().enumerate() {
            assert_eq!(package_id(p.source, p.target), Some(PackageId(i)));
            assert_eq!(
                p.file,
                format!(
                    "{}-{}-{}.zip",
                    p.source.code(),
                    p.target.code(),
                    data_version()
                )
            );
            assert!(p.bytes < 8 * 1024 * 1024 && p.data_bytes <= MAX_DATA);
            install(store, PackageId(i));
        }
        assert!(
            store
                .inventory()
                .iter()
                .all(|state| *state == PackageState::Installed)
        );
        for (source, word, target) in [
            (Language::English, "book", Language::Turkish),
            (Language::Turkish, "kitap", Language::English),
            (Language::Spanish, "libro", Language::English),
            (Language::German, "Buch", Language::English),
            (Language::French, "livre", Language::English),
            (Language::Japanese, "本", Language::English),
            (Language::Korean, "책", Language::English),
            (Language::Chinese, "书", Language::English),
            (Language::Chinese, "書", Language::English),
        ] {
            assert!(
                store.lookup(word, source, target).unwrap().is_some(),
                "{word}"
            );
        }
        for target in Language::English.targets() {
            assert!(
                store
                    .lookup("book", Language::English, target)
                    .unwrap()
                    .is_some(),
                "{target}"
            );
        }
    }

    #[test]
    fn fallbacks_are_labeled_and_missing_entries_are_not_invented() {
        let fixture = fixture();
        let store = &fixture.store;
        install(
            store,
            package_id(Language::English, Language::Turkish).unwrap(),
        );
        let run = store
            .lookup("ran", Language::English, Language::Turkish)
            .unwrap()
            .unwrap();
        assert_eq!(run.headword, "run");
        assert!(run.base_form);
        assert!(
            store
                .lookup("zzzzzzzzz", Language::English, Language::Turkish)
                .unwrap()
                .is_none()
        );
        assert!(
            store
                .lookup("book", Language::English, Language::Korean)
                .is_err()
        );
        assert_eq!(
            Settings {
                source: Language::Korean,
                target: Language::Turkish,
                ..Settings::default()
            }
            .validated()
            .target,
            Language::English
        );
    }

    #[test]
    fn missing_corrupt_cancelled_and_removed_packages_preserve_storage_contract() {
        let fixture = fixture();
        let store = &fixture.store;
        let id = package_id(Language::English, Language::Turkish).unwrap();
        let lookup = || store.lookup("book", Language::English, Language::Turkish);
        assert_eq!(
            lookup(),
            Err(LookupError::Unavailable {
                package: id,
                error: None
            })
        );
        assert!(
            !fixture.path.exists(),
            "a lookup must not create or download a file"
        );
        install(store, id);
        assert!(lookup().unwrap().is_some());
        let valid = bytes(id);
        let cancel = std::sync::atomic::AtomicBool::new(false);
        assert!(
            store
                .install(id, &valid[..valid.len() - 1], &cancel)
                .is_err()
        );
        let mut corrupt = valid.clone();
        corrupt[0] ^= 1;
        assert!(store.install(id, &corrupt, &cancel).is_err());
        assert!(
            store
                .install(id, &valid, &std::sync::atomic::AtomicBool::new(true))
                .is_err()
        );
        assert_eq!(
            std::fs::read(fixture.path.join(&package(id).unwrap().file)).unwrap(),
            valid
        );
        store.remove(id).unwrap();
        assert_eq!(
            lookup(),
            Err(LookupError::Unavailable {
                package: id,
                error: None
            }),
            "removal must invalidate the cached lexicon"
        );
        std::fs::write(fixture.path.join(&package(id).unwrap().file), corrupt).unwrap();
        assert_eq!(store.inventory()[id.0], PackageState::Invalid);
        assert!(matches!(
            lookup(),
            Err(LookupError::Unavailable { error: Some(_), .. })
        ));
        let file = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../assets/dictionaries/packs")
            .join(&package(id).unwrap().file);
        assert_eq!(store.import(&file, &cancel).unwrap(), id);
        assert!(lookup().unwrap().is_some());
        assert_eq!(
            std::fs::read_dir(&fixture.path).unwrap().count(),
            1,
            "no temporary files remain"
        );
    }

    #[test]
    fn invalid_indexes_are_rejected_before_lookup() {
        for text in [
            "",
            "word\tword\t\n",
            "word\tword\tmeaning\textra\n",
            "z\tz\tz\na\ta\ta\n",
            "a\ta\ta\na\ta\ta\n",
        ] {
            assert!(Lexicon::parse(text.into(), "test".into()).is_err());
        }
    }
}
