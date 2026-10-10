// Prevents additional console window on Windows in release, DO NOT REMOVE!!
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
    let args: Vec<String> = std::env::args().collect();
    // stdio MCP 代理入口：Codex 用 `MyApp.exe --mcp-proxy` 作为 stdio MCP
    // 启动命令，代理把 Codex 的 JSON-RPC 消息转发到本地 MCP 网关
    // （MyApp 主进程），并保证主进程（网关）已就绪。
    if args.iter().any(|a| a == "--mcp-proxy") {
        let code = match app_mcp::proxy_stdio::run_proxy(app_mcp::DEFAULT_MCP_PORT) {
            Ok(_) => 0,
            Err(e) => {
                eprintln!("[myapp-mcp-proxy] fatal: {e:#}");
                1
            }
        };
        std::process::exit(code);
    }
    myapp_lib::run();
}
