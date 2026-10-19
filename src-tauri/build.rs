fn main() {
    let is_win7_target = std::env::var("CARGO_CFG_TARGET_VENDOR").as_deref() == Ok("win7");
    if is_win7_target && std::env::var_os("CARGO_FEATURE_CUSTOM_PROTOCOL").is_none() {
        panic!("Windows 7 release builds must enable the custom-protocol feature");
    }

    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("windows")
        && std::env::var("CARGO_CFG_TARGET_ENV").as_deref() == Ok("msvc")
    {
        // Tauri's resource is linked to application binaries, but lib test
        // executables also import TaskDialogIndirect (Common Controls v6).
        // Embed the same manifest through the linker for every executable.
        let manifest =
            std::path::PathBuf::from(std::env::var_os("CARGO_MANIFEST_DIR").unwrap()).join("windows/app.manifest.xml");
        println!("cargo:rerun-if-changed={}", manifest.display());
        println!("cargo:rustc-link-arg=/MANIFEST:EMBED");
        println!("cargo:rustc-link-arg=/MANIFESTINPUT:{}", manifest.display());
        tauri_build::try_build(
            tauri_build::Attributes::new()
                .windows_attributes(tauri_build::WindowsAttributes::new_without_app_manifest()),
        )
        .expect("Failed to build application resources");
    } else {
        tauri_build::build();
    }
}
