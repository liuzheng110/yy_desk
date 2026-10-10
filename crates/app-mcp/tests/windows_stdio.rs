#![cfg(windows)]

use std::os::windows::io::{AsRawHandle, FromRawHandle, OwnedHandle};
use app_mcp::{client_stdio::StdioMcpClient, config::McpServerConfig};
use serde_json::{json, Value};
use windows_sys::Win32::{
    Foundation::WAIT_OBJECT_0,
    System::Threading::{OpenProcess, WaitForSingleObject, PROCESS_SYNCHRONIZE},
};

#[tokio::test]
async fn exe_and_cmd_run_without_console_and_cleanup_descendants() {
    let dir = tempfile::tempdir().unwrap();
    let script = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/mock_windows_stdio.py");
    let wrapper = dir.path().join("MCP wrapper with spaces.cmd");
    std::fs::write(&wrapper, format!("@echo off\r\npython \"{}\"\r\n", script.display())).unwrap();

    for (command, args) in [
        ("python".to_owned(), vec![script.to_string_lossy().into_owned()]),
        (wrapper.to_string_lossy().into_owned(), vec![]),
    ] {
        let cfg: McpServerConfig = serde_json::from_value(json!({
            "name": "probe", "transport": "stdio", "command": command, "args": args,
        })).unwrap();
        let mut client = StdioMcpClient::connect(&cfg).await.unwrap();
        assert_eq!(client.list_tools().await.unwrap().len(), 1);
        let result = client.call_tool("inspect", json!({})).await.unwrap();
        let info: Value = serde_json::from_str(result["content"][0]["text"].as_str().unwrap()).unwrap();
        assert_eq!(info["consoleWindow"], 0, "MCP process must have no console window");
        let pids = [client.child_id().unwrap(), info["pid"].as_u64().unwrap() as u32,
            info["descendantPid"].as_u64().unwrap() as u32];
        let handles: Vec<_> = pids.into_iter().map(|pid| {
            // SAFETY: only request synchronization; own the returned live handle.
            let raw = unsafe { OpenProcess(PROCESS_SYNCHRONIZE, 0, pid) };
            assert!(!raw.is_null(), "process {pid} must be alive before cleanup");
            unsafe { OwnedHandle::from_raw_handle(raw) }
        }).collect();
        drop(client);
        for handle in handles {
            // SAFETY: handle remains owned for the duration of the bounded wait.
            assert_eq!(unsafe { WaitForSingleObject(handle.as_raw_handle(), 5000) },
                WAIT_OBJECT_0, "MCP wrapper/server/descendant must exit on drop");
        }
    }
}
