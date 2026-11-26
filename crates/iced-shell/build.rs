fn main() {
    println!("cargo:rerun-if-changed=simPl.manifest");
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("windows")
        && std::env::var("CARGO_CFG_TARGET_ENV").as_deref() == Ok("msvc")
    {
        let manifest = std::path::PathBuf::from(
            std::env::var_os("CARGO_MANIFEST_DIR").expect("CARGO_MANIFEST_DIR is set by Cargo"),
        )
        .join("simPl.manifest");
        println!("cargo:rustc-link-arg-bin=iced-shell=/MANIFEST:EMBED");
        println!(
            "cargo:rustc-link-arg-bin=iced-shell=/MANIFESTINPUT:{}",
            manifest.display()
        );
    }
}
