//! 发布资产下载：统一走 GitHub Releases。
//! 单一发布源（GitHub），`race_download_urls` 可在需要时扩展为多镜像竞速。

pub const GITHUB_RELEASE_DOWNLOAD_PREFIX: &str = "https://github.com/liuzheng110/yy_desk/releases/download/";

use std::pin::Pin;

type ResponseFuture = Pin<Box<dyn std::future::Future<Output = Result<reqwest::Response, String>> + Send>>;

/// 并发请求多个候选 URL，返回第一个成功的响应。
pub async fn race_download_urls(
    client: &reqwest::Client,
    urls: &[String],
    user_agent: &str,
) -> Result<reqwest::Response, String> {
    use futures::future::select_ok;

    let mut futs: Vec<ResponseFuture> = Vec::with_capacity(urls.len());

    for url in urls {
        let client = client.clone();
        let url = url.clone();
        let ua = user_agent.to_string();
        futs.push(Box::pin(async move {
            client
                .get(url)
                .header(reqwest::header::USER_AGENT, ua)
                .header(reqwest::header::ACCEPT_ENCODING, "identity")
                .send()
                .await
                .and_then(|r| r.error_for_status())
                .map_err(|e| format!("{e}"))
        }) as ResponseFuture);
    }

    match select_ok(futs).await {
        Ok((resp, _)) => Ok(resp),
        Err(last_err) => Err(last_err),
    }
}

#[cfg(test)]
mod tests {
    use super::GITHUB_RELEASE_DOWNLOAD_PREFIX;

    #[test]
    fn github_release_prefix_points_at_this_repo() {
        assert_eq!(
            GITHUB_RELEASE_DOWNLOAD_PREFIX,
            "https://github.com/liuzheng110/yy_desk/releases/download/"
        );
    }
}
