//! 自动更新：检查最新版本、GitHub 发布元数据与发布说明。
//!
//! - 从 GitHub Releases 拉取 `latest.json`（由 CI 的 tauri-action 生成并上传）
//! - 拉取 GitHub release 元数据作为发布说明兜底
//! - 系统代理自动跟随

pub use app_platform::version::{is_newer_version, normalize_version, parse_version};

use serde::{Deserialize, Serialize};

const LATEST_JSON_GITHUB_PATH: &str = "https://github.com/liuzheng110/yy_desk/releases/latest/download/latest.json";
const GITHUB_RELEASE_API_PREFIX: &str = "https://api.github.com/repos/liuzheng110/yy_desk/releases/tags/v";
const RELEASE_URL_PREFIX: &str = "https://github.com/liuzheng110/yy_desk/releases/tag/v";

#[derive(Debug, Deserialize)]
pub struct TauriRelease {
    pub version: String,
    #[serde(default)]
    pub notes: Option<String>,
    #[serde(skip)]
    pub github: Option<GithubReleaseMetadata>,
}

#[derive(Debug, Deserialize)]
pub struct GithubReleaseMetadata {
    pub name: Option<String>,
    pub html_url: Option<String>,
    pub body: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct UpdateInfo {
    pub current_version: String,
    pub latest_version: String,
    pub update_available: bool,
    pub portable_mode: bool,
    pub manual_update_only: bool,
    pub release_name: String,
    pub release_url: String,
    pub release_notes: String,
}

pub async fn fetch_latest_release() -> Result<TauriRelease, String> {
    let client = build_update_http_client()?;

    let resp = client
        .get(LATEST_JSON_GITHUB_PATH)
        .header(reqwest::header::USER_AGENT, "myapp-update-checker")
        .header(reqwest::header::ACCEPT_ENCODING, "identity")
        .send()
        .await
        .and_then(|response| response.error_for_status())
        .map_err(|error| format!("Failed to check updates: {error}"))?;

    let mut release = resp
        .json::<TauriRelease>()
        .await
        .map_err(|e| format!("Failed to parse update response: {e}"))?;
    if let Ok(github) = fetch_github_release_metadata(&client, &release.version).await {
        release.github = Some(github);
    }
    Ok(release)
}

fn build_update_http_client() -> Result<reqwest::Client, String> {
    let mut builder = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(10))
        .user_agent("myapp-update-checker");

    if let Some(proxy_url) = system_proxy_url() {
        let proxy = reqwest::Proxy::all(&proxy_url).map_err(|e| format!("Invalid system proxy URL: {e}"))?;
        builder = builder.proxy(proxy);
    }

    builder
        .build()
        .map_err(|e| format!("Failed to create HTTP client: {e}"))
}

pub use app_platform::proxy::system_proxy_url;

async fn fetch_github_release_metadata(
    client: &reqwest::Client,
    version: &str,
) -> Result<GithubReleaseMetadata, String> {
    let url = format!("{GITHUB_RELEASE_API_PREFIX}{}", normalize_version(version));
    client
        .get(url)
        .header(reqwest::header::USER_AGENT, "myapp-update-checker")
        .send()
        .await
        .and_then(|r| r.error_for_status())
        .map_err(|e| format!("{e}"))?
        .json::<GithubReleaseMetadata>()
        .await
        .map_err(|e| format!("Failed to parse GitHub release response: {e}"))
}

pub fn build_update_info(release: TauriRelease, current_version: &str) -> UpdateInfo {
    let latest_version = normalize_version(&release.version);
    let github = release.github.as_ref();
    let release_notes = canonical_release_notes(release.notes.as_deref())
        .or_else(|| {
            github
                .and_then(|metadata| non_empty(metadata.body.as_deref()))
                .map(ToOwned::to_owned)
        })
        .unwrap_or_default();
    let release_name = github
        .and_then(|metadata| non_empty(metadata.name.as_deref()))
        .map(ToOwned::to_owned)
        .unwrap_or_else(|| format!("MyApp v{latest_version}"));
    let release_url = github
        .and_then(|metadata| non_empty(metadata.html_url.as_deref()))
        .map(ToOwned::to_owned)
        .unwrap_or_else(|| format!("{RELEASE_URL_PREFIX}{latest_version}"));

    UpdateInfo {
        update_available: is_newer_version(&latest_version, current_version),
        portable_mode: false,
        manual_update_only: false,
        current_version: current_version.to_string(),
        release_name,
        release_url,
        release_notes,
        latest_version,
    }
}

fn canonical_release_notes(notes: Option<&str>) -> Option<String> {
    let notes = non_empty(notes)?;
    // Tauri 生成的 updater notes 只是传输兜底，不是策划过的发布说明。
    if notes.starts_with("## What's Changed") || notes == "See the assets below to download and install." {
        return None;
    }
    Some(notes.to_owned())
}

fn non_empty(value: Option<&str>) -> Option<&str> {
    value.and_then(|value| {
        let trimmed = value.trim();
        if trimmed.is_empty() {
            None
        } else {
            Some(value)
        }
    })
}

#[cfg(test)]
mod tests {
    use super::{build_update_info, GithubReleaseMetadata, TauriRelease};

    #[test]
    fn update_info_prefers_github_release_metadata() {
        let release = TauriRelease {
            version: "0.1.3".to_string(),
            notes: Some("See the assets below to download and install.".to_string()),
            github: Some(GithubReleaseMetadata {
                name: Some("MyApp v0.1.3".to_string()),
                html_url: Some("https://github.com/liuzheng110/yy_desk/releases/tag/v0.1.3".to_string()),
                body: Some("### 新功能\n\n真实发布说明".to_string()),
            }),
        };

        let info = build_update_info(release, "0.1.2");

        assert_eq!(info.release_name, "MyApp v0.1.3");
        assert_eq!(
            info.release_url,
            "https://github.com/liuzheng110/yy_desk/releases/tag/v0.1.3"
        );
        assert_eq!(info.release_notes, "### 新功能\n\n真实发布说明");
        assert!(!info.portable_mode);
    }

    #[test]
    fn update_info_falls_back_to_release_url_prefix() {
        let release = TauriRelease {
            version: "v0.1.3".to_string(),
            notes: Some("### 新功能\n\n真实发布说明".to_string()),
            github: None,
        };

        let info = build_update_info(release, "0.1.2");

        assert_eq!(
            info.release_url,
            "https://github.com/liuzheng110/yy_desk/releases/tag/v0.1.3"
        );
        assert_eq!(info.release_notes, "### 新功能\n\n真实发布说明");
    }

    #[test]
    fn update_info_ignores_generated_notes_when_curated_notes_are_unavailable() {
        let release = TauriRelease {
            version: "0.1.3".to_string(),
            notes: Some("## What's Changed\n* generated item".to_string()),
            github: None,
        };

        let info = build_update_info(release, "0.1.2");

        assert_eq!(info.release_notes, "");
    }
}
