//! 更新日志获取：从 GitHub Releases API 读取应用发布记录，
//! 仅保留应用发布 tag（`vX.Y.Z`），过滤辅助发布流。

use serde::{Deserialize, Serialize};

const GITHUB_RELEASES_API: &str = "https://api.github.com/repos/liuzheng110/yy_desk/releases?per_page=50";

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ChangelogItem {
    pub title: String,
    #[serde(default)]
    pub desc: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChangelogSection {
    #[serde(default)]
    pub r#type: String,
    #[serde(default)]
    pub title: String,
    #[serde(default)]
    pub items: Vec<ChangelogItem>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChangelogRelease {
    pub tag: String,
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub date: String,
    #[serde(default)]
    pub sections: Vec<ChangelogSection>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ChangelogData {
    #[serde(default)]
    pub updated_at: String,
    #[serde(default)]
    pub releases: Vec<ChangelogRelease>,
}

/// GitHub Releases API 返回的原始条目（只取用得到的字段）。
#[derive(Debug, Deserialize)]
struct GithubRelease {
    tag_name: String,
    #[serde(default)]
    name: Option<String>,
    #[serde(default)]
    published_at: Option<String>,
    #[serde(default)]
    body: Option<String>,
}

fn is_app_release_tag(tag: &str) -> bool {
    let Some(version) = tag.strip_prefix('v') else {
        return false;
    };
    let mut parts = version.splitn(3, '.');
    let Some(major) = parts.next() else {
        return false;
    };
    let Some(minor) = parts.next() else {
        return false;
    };
    let Some(patch_and_suffix) = parts.next() else {
        return false;
    };
    if major.is_empty()
        || minor.is_empty()
        || !major.bytes().all(|byte| byte.is_ascii_digit())
        || !minor.bytes().all(|byte| byte.is_ascii_digit())
    {
        return false;
    }

    let patch_len = patch_and_suffix.bytes().take_while(u8::is_ascii_digit).count();
    if patch_len == 0 {
        return false;
    }
    let suffix = &patch_and_suffix[patch_len..];
    suffix.is_empty()
        || ((suffix.starts_with('.') || suffix.starts_with('-'))
            && suffix.len() > 1
            && suffix[1..]
                .bytes()
                .all(|byte| byte.is_ascii_alphanumeric() || byte == b'.' || byte == b'-'))
}

pub async fn fetch_changelog() -> Result<ChangelogData, String> {
    let client = build_changelog_http_client()?;

    let releases = client
        .get(GITHUB_RELEASES_API)
        .header(reqwest::header::USER_AGENT, "myapp-changelog")
        .header(reqwest::header::ACCEPT, "application/vnd.github+json")
        .send()
        .await
        .and_then(|r| r.error_for_status())
        .map_err(|e| format!("Failed to fetch changelog: {e}"))?
        .json::<Vec<GithubRelease>>()
        .await
        .map_err(|e| format!("Failed to parse changelog: {e}"))?;

    // GitHub 上任何一条 release 都可能不是应用发布，统一过滤。
    let releases = releases
        .into_iter()
        .filter(|release| is_app_release_tag(release.tag_name.trim()))
        .map(|release| ChangelogRelease {
            tag: release.tag_name,
            name: release.name.unwrap_or_default(),
            date: release
                .published_at
                .map(|value| value.chars().take(10).collect())
                .unwrap_or_default(),
            sections: release
                .body
                .filter(|body| !body.trim().is_empty())
                .map(|body| {
                    vec![ChangelogSection {
                        r#type: "markdown".to_string(),
                        title: String::new(),
                        items: vec![ChangelogItem {
                            title: String::new(),
                            desc: body,
                        }],
                    }]
                })
                .unwrap_or_default(),
        })
        .collect();

    Ok(ChangelogData {
        updated_at: String::new(),
        releases,
    })
}

fn build_changelog_http_client() -> Result<reqwest::Client, String> {
    let mut builder = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(15))
        .user_agent("myapp-changelog");

    if let Some(proxy_url) = crate::update::system_proxy_url() {
        let proxy = reqwest::Proxy::all(&proxy_url).map_err(|e| format!("Invalid system proxy URL: {e}"))?;
        builder = builder.proxy(proxy);
    }

    builder
        .build()
        .map_err(|e| format!("Failed to create HTTP client: {e}"))
}

#[cfg(test)]
mod tests {
    use super::is_app_release_tag;

    #[test]
    fn recognizes_only_app_release_tags() {
        assert!(is_app_release_tag("v0.1.3"));
        assert!(is_app_release_tag("v1.2.3-hotfix.1"));
        assert!(!is_app_release_tag("packages-v0.4.42"));
        assert!(!is_app_release_tag("agents-v0.2.64"));
        assert!(!is_app_release_tag("v0.1"));
        assert!(!is_app_release_tag("v0.1.x"));
    }
}
