//! Where the reader keeps its data.
//!
//! Every core crate stores under `<base>/simPl/…`. On the desktop the base is
//! `%LOCALAPPDATA%`, read again on every call (process temp directory when it is
//! unset), and disposable caches share it. An app without that convention
//! (Android) calls [`configure`] once at startup with its private data and cache
//! directories; afterwards the environment is no longer consulted.

use std::path::{Path, PathBuf};
use std::sync::OnceLock;

#[derive(Debug, PartialEq, Eq)]
struct Roots {
    data: PathBuf,
    cache: PathBuf,
}

static CONFIGURED: OnceLock<Roots> = OnceLock::new();

/// Use `data` for user data and `cache` for disposable caches from now on.
/// Both must be absolute. Repeating the same roots is accepted; changing them
/// after the first call is an error because open stores would split.
pub fn configure(data: &Path, cache: &Path) -> Result<(), String> {
    if !data.is_absolute() || !cache.is_absolute() {
        return Err("Profile directories must be absolute paths".into());
    }
    let roots = Roots {
        data: data.to_path_buf(),
        cache: cache.to_path_buf(),
    };
    let current = CONFIGURED.get_or_init(|| roots);
    if current.data == data && current.cache == cache {
        Ok(())
    } else {
        Err(format!(
            "Profile already configured at {} (cache {})",
            current.data.display(),
            current.cache.display()
        ))
    }
}

/// Parent of the `simPl` data folder.
pub fn storage_base() -> PathBuf {
    if let Some(roots) = CONFIGURED.get() {
        return roots.data.clone();
    }
    std::env::var_os("LOCALAPPDATA")
        .filter(|value| !value.is_empty())
        .map(PathBuf::from)
        .unwrap_or_else(std::env::temp_dir)
}

/// Parent of the `simPl` cache folder, or `None` when disposable caches are
/// disabled (desktop without `LOCALAPPDATA`).
pub fn cache_base() -> Option<PathBuf> {
    if let Some(roots) = CONFIGURED.get() {
        return Some(roots.cache.clone());
    }
    std::env::var_os("LOCALAPPDATA").map(PathBuf::from)
}

#[cfg(test)]
mod tests {
    use super::*;

    // One test owns the process-wide configuration; the others would race it.
    #[test]
    fn configuration_is_absolute_fixed_and_replaces_the_environment() {
        let relative = Path::new("relative");
        let data = std::env::temp_dir().join("simpl-profile-data");
        let cache = std::env::temp_dir().join("simpl-profile-cache");
        assert!(configure(relative, &cache).is_err());
        assert!(CONFIGURED.get().is_none());

        configure(&data, &cache).unwrap();
        configure(&data, &cache).unwrap();
        assert!(configure(&cache, &data).is_err());
        assert_eq!(storage_base(), data);
        assert_eq!(cache_base(), Some(cache));
    }
}
