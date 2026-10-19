use std::{env, fs, path::PathBuf};

fn main() {
    // This workspace builds one desktop application. Its public Tauri config is
    // the single source of identity for every Rust crate, including early startup.
    let config_path = PathBuf::from(env::var_os("CARGO_MANIFEST_DIR").unwrap()).join("../../src-tauri/tauri.conf.json");
    println!("cargo:rerun-if-changed={}", config_path.display());
    let config: serde_json::Value = serde_json::from_slice(&fs::read(config_path).unwrap()).unwrap();
    let string = |pointer: &str| {
        config
            .pointer(pointer)
            .and_then(serde_json::Value::as_str)
            .filter(|value| !value.is_empty())
            .unwrap_or_else(|| panic!("Missing {pointer} in tauri.conf.json"))
    };
    let endpoint = string("/plugins/updater/endpoints/0");
    let repo = endpoint
        .strip_prefix("https://github.com/")
        .and_then(|value| value.strip_suffix("/releases/latest/download/latest.json"))
        .expect("This release backend requires a GitHub latest.json endpoint");
    let parts: Vec<_> = repo.split('/').collect();
    assert!(
        parts.len() == 2
            && parts
                .iter()
                .all(|part| !part.is_empty() && part.chars().all(|c| c.is_ascii_alphanumeric() || "-_.".contains(c))),
        "Invalid GitHub repository"
    );
    assert_eq!(
        config
            .pointer("/plugins/updater/endpoints")
            .unwrap()
            .as_array()
            .unwrap()
            .len(),
        1,
        "Configure one canonical update endpoint"
    );
    assert_eq!(
        config
            .pointer("/plugins/deep-link/desktop/schemes")
            .unwrap()
            .as_array()
            .unwrap()
            .len(),
        1,
        "Configure one application scheme"
    );
    let mut generated = String::from("// Generated from tauri.conf.json. Do not edit.\n");
    for (name, value) in [
        ("APP_NAME", string("/productName").to_owned()),
        ("APP_ID", string("/identifier").to_owned()),
        (
            "APP_SCHEME",
            format!("{}://", string("/plugins/deep-link/desktop/schemes/0")),
        ),
        ("UPDATE_ENDPOINT", endpoint.to_owned()),
        ("GITHUB_REPOSITORY", repo.to_owned()),
        (
            "GITHUB_RELEASE_DOWNLOAD_PREFIX",
            format!("https://github.com/{repo}/releases/download/"),
        ),
        (
            "RELEASE_URL_PREFIX",
            format!("https://github.com/{repo}/releases/tag/v"),
        ),
        (
            "GITHUB_RELEASE_API_PREFIX",
            format!("https://api.github.com/repos/{repo}/releases/tags/v"),
        ),
        (
            "GITHUB_RELEASES_API",
            format!("https://api.github.com/repos/{repo}/releases?per_page=50"),
        ),
    ] {
        generated.push_str(&format!("pub const {name}: &str = {value:?};\n"));
    }
    fs::write(
        PathBuf::from(env::var_os("OUT_DIR").unwrap()).join("app_identity.rs"),
        generated,
    )
    .unwrap();
}
