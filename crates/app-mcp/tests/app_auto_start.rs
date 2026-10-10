use std::{path::{Path, PathBuf}, time::Duration};
use app_mcp::{app_launcher::AppLauncher, config::AppAutoStartConfig, GatewayHandle};
use serde_json::{json, Value};
use tokio::{net::TcpStream, time::{sleep, timeout}};

fn app_config(dir: &Path, mode: &str) -> Value {
    json!({
        "command": "python",
        "args": [Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/mock_desktop_app.py"),
            dir.join("port"), dir.join("starts"), dir.join("stop"), mode],
        "portFile": dir.join("port"),
        "portField": if mode == "json" { Some("port") } else { None },
        "startupTimeoutMs": 5000,
    })
}

fn make_gateway(dir: &Path, auto_start: Value) -> GatewayHandle {
    let cfg = dir.join("config.json");
    std::fs::write(&cfg, json!({"servers": [{
        "name": "app", "transport": "stdio", "command": "python",
        "args": [Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/mock_stdio_mcp.py")],
        "autoStart": auto_start,
    }]}).to_string()).unwrap();
    GatewayHandle::load(&cfg, &dir.join("cache.json")).unwrap()
}

async fn call(gateway: &GatewayHandle) {
    let result = gateway.call_tool_by_server("app", "echo", json!({"text":"ready"})).await.unwrap();
    assert_eq!(result["content"][0]["text"], "echo:ready");
}

async fn stop_app(dir: &Path, mode: &str) {
    let contents = std::fs::read_to_string(dir.join("port")).unwrap();
    let port = if mode == "json" {
        serde_json::from_str::<Value>(&contents).unwrap()["port"].as_u64().unwrap() as u16
    } else { contents.trim().parse::<u16>().unwrap() };
    std::fs::write(dir.join("stop"), "stop").unwrap();
    timeout(Duration::from_secs(5), async {
        while TcpStream::connect(("127.0.0.1", port)).await.is_ok() {
            sleep(Duration::from_millis(100)).await;
        }
    }).await.unwrap();
    // Let the exited launcher become observable through Child::try_wait.
    sleep(Duration::from_millis(150)).await;
}

// Ensure fixtures exit even if an assertion fails, before tempdir cleanup.
struct StopOnDrop(PathBuf);
impl Drop for StopOnDrop {
    fn drop(&mut self) {
        let _ = std::fs::write(&self.0, "stop");
        // Give the fixture time to observe the marker before TempDir removes it.
        std::thread::sleep(Duration::from_millis(400));
    }
}

#[tokio::test]
async fn lazy_start_concurrent_calls_refresh_restart_and_app_lifetime() {
    for mode in ["text", "json"] {
        let dir = tempfile::tempdir().unwrap();
        let _stop = StopOnDrop(dir.path().join("stop"));
        // Invalid or stale discovery must not be mistaken for a running app.
        std::fs::write(dir.path().join("port"), "stale").unwrap();
        let gateway = make_gateway(dir.path(), app_config(dir.path(), mode));
        assert!(gateway.list_all_tools().await.is_empty());
        assert!(!dir.path().join("starts").exists(), "cached discovery must not launch apps");
        gateway.kill_server("app").await.unwrap();
        assert!(gateway.call_tool_by_server("app", "echo", json!({})).await.is_err());
        assert!(gateway.refresh_tools().await.iter().all(|r| !r.ok));
        assert!(!dir.path().join("starts").exists(), "kill switch must block auto-start");
        gateway.revive_server("app").await.unwrap();
        let mut tasks = tokio::task::JoinSet::new();
        for _ in 0..4 {
            let gw = gateway.clone();
            tasks.spawn(async move { call(&gw).await; });
        }
        while let Some(result) = tasks.join_next().await { result.unwrap(); }
        assert_eq!(std::fs::read_to_string(dir.path().join("starts")).unwrap().lines().count(), 1);
        call(&gateway).await;
        stop_app(dir.path(), mode).await;
        std::fs::remove_file(dir.path().join("stop")).unwrap();
        let reports = gateway.refresh_tools().await;
        assert!(reports.iter().all(|r| r.ok), "manual refresh: {reports:?}");
        assert_eq!(std::fs::read_to_string(dir.path().join("starts")).unwrap().lines().count(), 2);
        call(&gateway).await;
        drop(gateway);
        let config: AppAutoStartConfig = serde_json::from_value(app_config(dir.path(), mode)).unwrap();
        // The app survives gateway drop; a new launcher must use the live port.
        AppLauncher::default().ensure_ready(Some(&config)).await.unwrap();
        assert_eq!(std::fs::read_to_string(dir.path().join("starts")).unwrap().lines().count(), 2);
        stop_app(dir.path(), mode).await;
    }
}

#[tokio::test]
async fn disabled_invalid_executable_and_bounded_timeout() {
    let dir = tempfile::tempdir().unwrap();
    let _stop = StopOnDrop(dir.path().join("stop"));
    let launcher = AppLauncher::default();
    let mut cfg: AppAutoStartConfig = serde_json::from_value(app_config(dir.path(), "never-ready")).unwrap();
    cfg.enabled = false;
    launcher.ensure_ready(Some(&cfg)).await.unwrap();
    assert!(!dir.path().join("starts").exists());
    cfg.enabled = true;
    cfg.command = dir.path().join("nonexistent-app").to_string_lossy().into_owned();
    assert!(launcher.ensure_ready(Some(&cfg)).await.unwrap_err().to_string().contains("自动启动应用失败"));
    cfg.command = "python".into();
    cfg.startup_timeout_ms = 600;
    for _ in 0..2 {
        let error = launcher.ensure_ready(Some(&cfg)).await.unwrap_err();
        assert!(error.to_string().contains("超时"), "{error:#}");
    }
    assert_eq!(std::fs::read_to_string(dir.path().join("starts")).unwrap().lines().count(), 1,
        "timed-out but live app must not be spawned repeatedly");
}
