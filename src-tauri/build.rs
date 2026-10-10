fn main() {
    let is_win7_target = std::env::var("CARGO_CFG_TARGET_VENDOR").as_deref() == Ok("win7");
    if is_win7_target && std::env::var_os("CARGO_FEATURE_CUSTOM_PROTOCOL").is_none() {
        panic!("Windows 7 release builds must enable the custom-protocol feature");
    }

    // Force rebuild to re-embed frontend assets and the Windows icon resource.
    // (touched 2026-10-10 v2: icon regenerated via `tauri icon`; re-embed new dist
    //  for close/quit fix — the previous exe still embedded the OLD frontend
    //  because only lib.rs was touched, build.rs did not rerun.
    //  v2: dist/index.html carries embed-marker-2026-10-10-dist-v2)
    tauri_build::build()
}
