//! UniFFI surface for the Android app. Kotlin bindings are generated from the
//! compiled library (`uniffi-bindgen generate --library`), so every exported
//! item is declared here with proc-macros; there is no UDL file.

uniffi::setup_scaffolding!();

/// Identifies the native core the app loaded.
#[derive(Clone, Debug, PartialEq, Eq, uniffi::Record)]
pub struct BuildInfo {
    /// Workspace crate version of the core.
    pub core_version: String,
    /// Operating system the library was compiled for (`android` on devices).
    pub os: String,
    /// CPU architecture the library was compiled for (`aarch64`, `x86_64`).
    pub arch: String,
    /// Whether the library carries debug assertions (Cargo dev profile).
    pub debug: bool,
}

#[uniffi::export]
pub fn build_info() -> BuildInfo {
    BuildInfo {
        core_version: env!("CARGO_PKG_VERSION").to_owned(),
        os: std::env::consts::OS.to_owned(),
        arch: std::env::consts::ARCH.to_owned(),
        debug: cfg!(debug_assertions),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn build_info_describes_this_build() {
        let info = build_info();
        assert_eq!(info.core_version, env!("CARGO_PKG_VERSION"));
        assert_eq!(info.os, std::env::consts::OS);
        assert_eq!(info.arch, std::env::consts::ARCH);
        assert_eq!(info.debug, cfg!(debug_assertions));
    }
}
