//! MyApp 桌面壳（Tauri 2）。
//!
//! 从 dbx 的 src-tauri 裁剪而来。保留的框架机制：
//! - 自动更新（updater 插件 + update 命令组）
//! - 启动恢复（startup_recovery / webview2_recovery / window_state_guard）
//! - 数据目录三模式 + 迁移门槛（data_dir / migration_gate）
//! - 单实例 + 深链接 + 托盘 + 关闭行为状态机
//! - 设置存储（app-core storage）
//! - 开机自启动（commands/autostart.rs，Windows 注册表 RUN 键）
//! - 应用图标（2026-10-10 v2：主体放大至画布 95%，嵌入 regenerated icon set）
//! - UI v3（2026-10-10：细标题栏、右侧按钮并排、左侧导航 + 设置功能页）
//! - 图标 v3（2026-10-10：主体放大至 97%，多帧 icon.ico 16-256）
//! - 启动管理 v2（2026-10-18：支持端口探测状态 + URL 打开页面的 web 服务类启动项）
//!
//! 数据库业务命令已全部移除；你的业务命令放在 `commands/business/`。

mod commands;
mod data_dir;
#[cfg(target_os = "macos")]
mod macos_app_delegate;
#[cfg(target_os = "macos")]
mod macos_escape_guard;
mod migration_gate;
#[cfg(any(target_os = "windows", test))]
mod startup_recovery;
#[cfg(all(not(target_os = "windows"), not(test)))]
#[path = "startup_recovery_noop.rs"]
mod startup_recovery;
#[cfg(any(target_os = "windows", test))]
mod webview2_recovery;
mod window_state_guard;

use app_core::storage::{DesktopIconTheme, DesktopSettings, Storage};
use commands::state::AppState;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Instant;
use tauri::webview::PageLoadEvent;
use tauri::RunEvent;
use tauri::{Emitter, Manager};

const DESKTOP_TRAY_ID: &str = "main-tray";
const APP_CLOSE_REQUESTED_EVENT: &str = "myapp-app-close-requested";

pub struct CloseBehaviorState {
    confirmed_exit: AtomicBool,
    frontend_ready: AtomicBool,
}

impl CloseBehaviorState {
    fn new() -> Self {
        Self {
            confirmed_exit: AtomicBool::new(false),
            frontend_ready: AtomicBool::new(false),
        }
    }

    pub(crate) fn allow_next_exit(&self) {
        self.confirmed_exit.store(true, Ordering::Relaxed);
    }

    fn take_confirmed_exit(&self) -> bool {
        self.confirmed_exit.swap(false, Ordering::Relaxed)
    }

    pub(crate) fn set_frontend_ready(&self, ready: bool) {
        self.frontend_ready.store(ready, Ordering::Release);
    }

    fn is_frontend_ready(&self) -> bool {
        self.frontend_ready.load(Ordering::Acquire)
    }
}

/// UI 语言由前端 i18n 推送；原生菜单跟随，未设置时回退系统 locale。
pub struct AppLocaleState {
    locale: std::sync::Mutex<Option<String>>,
}

impl AppLocaleState {
    fn new() -> Self {
        Self {
            locale: std::sync::Mutex::new(None),
        }
    }

    pub(crate) fn set(&self, locale: String) {
        *self.locale.lock().unwrap_or_else(|poisoned| poisoned.into_inner()) = Some(locale);
    }

    fn get(&self) -> String {
        self.locale
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .clone()
            .unwrap_or_else(|| sys_locale::get_locale().unwrap_or_default())
    }
}

pub(crate) fn apply_debug_log_level(debug_logging_enabled: bool) {
    log::set_max_level(if debug_logging_enabled {
        log::LevelFilter::Debug
    } else {
        log::LevelFilter::Off
    });
}

fn should_hide_window_on_close(target_os: &str) -> bool {
    matches!(target_os, "macos" | "windows")
}

fn should_confirm_app_exit_request(target_os: &str, exit_code: Option<i32>, confirmed_exit: bool) -> bool {
    should_hide_window_on_close(target_os) && exit_code != Some(tauri::RESTART_EXIT_CODE) && !confirmed_exit
}

fn should_hide_window_before_exit(target_os: &str) -> bool {
    target_os == "macos"
}

fn should_setup_desktop_tray(target_os: &str, show_tray_icon: bool, linux_appindicator_available: bool) -> bool {
    show_tray_icon
        && (matches!(target_os, "macos" | "windows") || (target_os == "linux" && linux_appindicator_available))
}

fn should_enable_single_instance(debug_build: bool) -> bool {
    !debug_build
}

fn startup_data_dir_mode(mode: &data_dir::DataDirMode) -> &'static str {
    match mode {
        data_dir::DataDirMode::Default => "default",
        data_dir::DataDirMode::EnvOverride => "env_override",
    }
}

#[cfg(target_os = "linux")]
fn linux_appindicator_available() -> bool {
    const APPINDICATOR_LIBRARIES: &[&str] = &["libayatana-appindicator3.so.1", "libappindicator3.so.1"];

    APPINDICATOR_LIBRARIES
        .iter()
        .any(|library| unsafe { libloading::Library::new(library).is_ok() })
}

#[cfg(not(target_os = "linux"))]
fn linux_appindicator_available() -> bool {
    false
}

fn append_startup_probe(message: impl AsRef<str>) {
    startup_recovery::record(message);
}

pub(crate) fn clear_startup_probe_after_frontend_ready(main_window_visible: bool) {
    startup_recovery::mark_frontend_ready(main_window_visible);
}

fn should_fallback_to_native_quit(target: &str, frontend_ready: bool) -> bool {
    target == "quit" && !frontend_ready
}

fn native_window_decorations_override(target_os: &str) -> Option<bool> {
    match target_os {
        "windows" | "linux" => Some(false),
        _ => None,
    }
}

fn show_main_window<R: tauri::Runtime>(app: &tauri::AppHandle<R>) -> bool {
    let Some(window) = app.get_webview_window("main") else {
        eprintln!("[WINDOW] show_main_window: no \"main\" webview window to reveal");
        return false;
    };
    if let Err(err) = window.show() {
        eprintln!("[WINDOW] show_main_window: show() failed: {err}");
    }
    if let Err(err) = window.unminimize() {
        eprintln!("[WINDOW] show_main_window: unminimize() failed: {err}");
    }
    if let Err(err) = window.set_focus() {
        eprintln!("[WINDOW] show_main_window: set_focus() failed: {err}");
    }
    match window.is_visible() {
        Ok(visible) => visible,
        Err(err) => {
            eprintln!("[WINDOW] show_main_window: is_visible() failed: {err}");
            false
        }
    }
}

fn prepare_main_window_for_display<R: tauri::Runtime>(app: &tauri::AppHandle<R>) {
    if let Some(window) = app.get_webview_window("main") {
        let _ = window.set_title(app_platform::identity::APP_NAME);
    }
    if let Some(decorations) = native_window_decorations_override(std::env::consts::OS) {
        if let Some(window) = app.get_webview_window("main") {
            let _ = window.set_decorations(decorations);
        }
    }
    window_state_guard::enforce_main_window_bounds(app);
}

fn clear_main_webview_focus<R: tauri::Runtime>(app: &tauri::AppHandle<R>) {
    if let Some(window) = app.get_webview_window("main") {
        let _ = window.eval(
            r#"
            (() => {
              const active = document.activeElement;
              if (active instanceof HTMLElement) active.blur();
              if (document.body) {
                if (!document.body.hasAttribute("tabindex")) {
                  document.body.setAttribute("tabindex", "-1");
                }
                document.body.focus({ preventScroll: true });
              }
            })();
            "#,
        );
    }
}

pub(crate) fn hide_main_window_for_close<R: tauri::Runtime>(app: &tauri::AppHandle<R>, window: &tauri::Window<R>) {
    clear_main_webview_focus(app);
    let _ = window.hide();
}

pub(crate) fn request_app_close<R: tauri::Runtime>(app: &tauri::AppHandle<R>, target: &str) {
    let frontend_ready = app
        .try_state::<CloseBehaviorState>()
        .is_some_and(|state| state.is_frontend_ready());
    if should_fallback_to_native_quit(target, frontend_ready) {
        // WebView2 运行时缺失等场景会导致前端监听器永远无法加载；
        // 只有显式的托盘 Quit 兜底才绕过确认弹窗。
        if let Some(state) = app.try_state::<CloseBehaviorState>() {
            state.allow_next_exit();
        }
        app.exit(0);
        return;
    }
    show_main_window(app);
    let _ = app.emit(APP_CLOSE_REQUESTED_EVENT, target);
}

fn open_deep_links(app: &tauri::AppHandle, links: Vec<String>) {
    if links.is_empty() {
        return;
    }
    let should_emit = app
        .try_state::<commands::deep_link::DeepLinkOpenState>()
        .is_none_or(|state| state.route_links(links.clone()));
    if should_emit {
        let _ = app.emit("myapp-open-deep-links", links);
    }
    show_main_window(app);
}

pub(crate) fn refresh_native_menus(_app: &tauri::AppHandle) -> tauri::Result<()> {
    // 非 macOS 无原生菜单；macOS 菜单在 build_app_menu 中构建。
    Ok(())
}

fn build_tray_menu<R: tauri::Runtime, M: Manager<R>>(manager: &M) -> tauri::Result<tauri::menu::Menu<R>> {
    use tauri::menu::{MenuBuilder, MenuItem};

    let show = MenuItem::with_id(
        manager,
        "show-main",
        format!("显示 {}", app_platform::identity::APP_NAME),
        true,
        None::<&str>,
    )?;
    let quit = MenuItem::with_id(manager, "quit-app", "退出", true, None::<&str>)?;
    MenuBuilder::new(manager).items(&[&show, &quit]).build()
}

fn setup_desktop_tray<R: tauri::Runtime, M: Manager<R>>(
    manager: &M,
    icon_theme: DesktopIconTheme,
) -> tauri::Result<()> {
    use tauri::menu::MenuEvent;
    use tauri::tray::TrayIconBuilder;

    let menu = build_tray_menu(manager)?;
    let show_id = "show-main";
    let quit_id = "quit-app";

    let mut builder = TrayIconBuilder::with_id(DESKTOP_TRAY_ID)
        .tooltip("MyApp")
        .menu(&menu)
        .on_menu_event(move |app, event: MenuEvent| match event.id().as_ref() {
            id if id == show_id => {
                show_main_window(app);
            }
            id if id == quit_id => {
                request_app_close(app, "quit");
            }
            _ => {}
        });

    if icon_theme == DesktopIconTheme::Black {
        builder = builder.icon(app_icon_black());
    } else {
        builder = builder.icon(app_icon_default());
    }

    builder.build(manager)?;
    Ok(())
}

fn app_icon_default() -> tauri::image::Image<'static> {
    tauri::include_image!("icons/icon.png")
}

fn app_icon_black() -> tauri::image::Image<'static> {
    tauri::include_image!("icons/icon-black.png")
}

pub(crate) fn apply_desktop_settings(app: &tauri::AppHandle, settings: &DesktopSettings) -> tauri::Result<()> {
    apply_debug_log_level(settings.debug_logging_enabled);
    if should_setup_desktop_tray(
        std::env::consts::OS,
        settings.show_tray_icon,
        linux_appindicator_available(),
    ) {
        if app.tray_by_id(DESKTOP_TRAY_ID).is_none() {
            setup_desktop_tray(app, settings.icon_theme)?;
        }
    }
    Ok(())
}

pub fn run() {
    // 元数据/命令链嵌套非常大的异步 future，放不进 tokio 默认 2 MiB worker 栈；
    // 与 dbx 一致，使用 16 MiB 宽栈 runtime 并设为全局。
    let runtime = app_core::worker_runtime::worker_runtime().expect("Failed to build tokio runtime");
    let runtime_handle = runtime.handle().clone();
    let _runtime = Box::leak(Box::new(runtime));
    tauri::async_runtime::set(runtime_handle);

    startup_recovery::initialize();
    rustls::crypto::ring::default_provider()
        .install_default()
        .expect("Failed to install rustls crypto provider");
    append_startup_probe("runtime prerequisites configured");

    let startup_begin = Instant::now();

    let builder = tauri::Builder::default()
        .plugin(tauri_plugin_deep_link::init())
        .plugin(tauri_plugin_clipboard_manager::init())
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_fs::init());

    let builder = if should_enable_single_instance(cfg!(debug_assertions)) {
        builder.plugin(tauri_plugin_single_instance::init(|app, args, cwd| {
            let links = commands::deep_link::deep_links_from_args(args.clone());
            open_deep_links(app, links);

            let paths = commands::external_files::file_paths_from_args(args.clone(), std::path::Path::new(&cwd));
            if !paths.is_empty() {
                if let Some(state) = app.try_state::<commands::external_files::ExternalFileOpenState>() {
                    state.push(paths.clone());
                }
                let _ = app.emit("myapp-open-files", paths);
            }
            // 二次启动把参数交给已有实例后退出；如果无法唤醒主窗口，
            // 用户点击后会毫无反馈，日志里必须能查到原因。
            if !show_main_window(app) {
                eprintln!(
                    "[WINDOW] single-instance handoff could not reveal the main window; {}",
                    main_window_probe_state(app)
                );
            }
        }))
    } else {
        builder
    };

    let builder = builder
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_shell::init())
        .plugin(tauri_plugin_updater::Builder::new().build())
        .plugin(tauri_plugin_process::init())
        .plugin(tauri_plugin_window_state::Builder::default().build());

    builder
        .manage(CloseBehaviorState::new())
        .manage(AppLocaleState::new())
        .manage(commands::deep_link::DeepLinkOpenState::new())
        .manage(commands::external_files::new_external_file_open_state())
        .manage(commands::update::PendingUpdateState::default())
        .on_page_load(|webview, payload| {
            if payload.event() == PageLoadEvent::Started {
                if let Some(state) = webview.app_handle().try_state::<CloseBehaviorState>() {
                    state.set_frontend_ready(false);
                }
            }
        })
        .setup(move |app| {
            let setup_start = Instant::now();
            append_startup_probe(format!("setup entered after {:?}", startup_begin.elapsed()));

            prepare_main_window_for_display(app.handle());
            show_main_window(app.handle());
            append_startup_probe("early main window show requested");

            // 数据目录解析（默认 / 环境变量 / 便携模式）+ 回退导入
            append_startup_probe("resolving app data dir");
            let default_data_dir = app
                .path()
                .app_data_dir()
                .map_err(|e| e.to_string())
                .expect("Failed to resolve app data dir");
            let data_dir_resolution = data_dir::resolve_data_dir_with_mode(default_data_dir);
            let data_dir = data_dir_resolution.data_dir.clone();
            std::fs::create_dir_all(&data_dir).expect("Failed to create data dir");
            append_startup_probe(format!(
                "data dir ready mode={}",
                startup_data_dir_mode(&data_dir_resolution.mode)
            ));
            match app_core::storage::maybe_import_user_data_db(&data_dir, None) {
                Ok(result) => eprintln!("[STARTUP] data db fallback import: {result:?}"),
                Err(err) => eprintln!("[STARTUP] data db fallback import failed: {err}"),
            }
            let db_path = data_dir.join("app.db");

            append_startup_probe("opening storage file=app.db");
            let storage = tauri::async_runtime::block_on(async {
                let s = Storage::open_unmigrated(&db_path)
                    .await
                    .expect("Failed to open storage");
                append_startup_probe("storage opened");
                s
            });
            let migration_ready = tauri::async_runtime::block_on(storage.inspect_data_migration())
                .map(|status| status.is_ready())
                .unwrap_or(false);
            let migration_gate = Arc::new(migration_gate::MigrationGate::new(migration_ready));
            app.manage(migration_gate.clone());
            app.manage(Arc::new(AppState {
                storage: Arc::new(storage),
            }));

            let desktop_settings =
                tauri::async_runtime::block_on(app.state::<Arc<AppState>>().storage.load_desktop_settings())
                    .unwrap_or_default();
            app.handle().plugin(
                tauri_plugin_log::Builder::default()
                    .timezone_strategy(tauri_plugin_log::TimezoneStrategy::UseLocal)
                    .format(|out, message, record| {
                        out.finish(format_args!(
                            "[{}][{}][{}] {}",
                            chrono::Local::now().format("%Y-%m-%d][%H:%M:%S%.3f"),
                            record.level(),
                            record.target(),
                            message
                        ));
                    })
                    .level(log::LevelFilter::Debug)
                    .build(),
            )?;
            apply_debug_log_level(desktop_settings.debug_logging_enabled);
            append_startup_probe("storage ready");

            // MCP 网关：聚合外部 MCP 服务器，供 Codex 等客户端发现与调用。
            // tools/list 走持久化工具缓存（外部 MCP 离线也能列出工具），
            // tools/call 实时转发到对应外部 MCP（后启动也能直接用）。
            append_startup_probe("starting mcp gateway");
            let mcp_cfg_path = data_dir.join("mcp-servers.json");
            let mcp_cache_path = data_dir.join("mcp-tools-cache.json");
            match app_mcp::gateway::GatewayHandle::load(&mcp_cfg_path, &mcp_cache_path) {
                Ok(gateway) => {
                    let port = tauri::async_runtime::block_on(gateway.port());
                    app.manage(gateway.clone());
                    // 自动预取：启动即连接外部 MCP 扫描工具写入缓存（失败退避重试），
                    // 这样 Codex 无需手动刷新即可发现全部工具。
                    if tauri::async_runtime::block_on(gateway.auto_warm_start()) {
                        let gw = gateway.clone();
                        tauri::async_runtime::spawn(async move {
                            gw.run_warm_start().await;
                        });
                    }
                    let gateway_for_server = gateway;
                    tauri::async_runtime::spawn(async move {
                        match app_mcp::server_http::spawn_server(std::sync::Arc::new(gateway_for_server), port).await {
                            Ok(_) => eprintln!("[STARTUP] MCP gateway listening on 127.0.0.1:{port}/mcp"),
                            Err(e) => eprintln!("[STARTUP] MCP gateway failed to start: {e:#}"),
                        }
                    });
                }
                Err(e) => eprintln!("[STARTUP] MCP gateway load failed: {e:#}"),
            }

            // 启动恢复看门狗：WebView2 渲染进程崩溃后自动恢复
            #[cfg(any(target_os = "windows", test))]
            webview2_recovery::install(app.handle());
            startup_recovery::start_watchdog(app.handle());

            #[cfg(target_os = "macos")]
            macos_app_delegate::install_dock_quit_handler(app.handle());
            #[cfg(target_os = "macos")]
            macos_escape_guard::install_escape_fullscreen_guard();

            if should_setup_desktop_tray(
                std::env::consts::OS,
                desktop_settings.show_tray_icon,
                linux_appindicator_available(),
            ) {
                setup_desktop_tray(app, desktop_settings.icon_theme)?;
            }

            append_startup_probe(format!("setup complete in {:?}", setup_start.elapsed()));
            Ok(())
        })
        .on_window_event(|window, event| {
            if let tauri::WindowEvent::CloseRequested { api, .. } = event {
                if !should_hide_window_on_close(std::env::consts::OS) {
                    return;
                }
                let app = window.app_handle();
                if app.try_state::<CloseBehaviorState>().is_none() {
                    api.prevent_close();
                    hide_main_window_for_close(app, window);
                    return;
                }
                api.prevent_close();
                request_app_close(app, "settings");
            }
        })
        .invoke_handler(migration_gate::guard_handler(route_external_commands(
            tauri::generate_handler![
                // 设置
                commands::app_settings::load_desktop_settings,
                commands::app_settings::save_desktop_settings,
                commands::app_settings::load_app_appearance_settings,
                commands::app_settings::update_app_appearance_settings,
                commands::app_settings::set_app_locale,
                commands::app_settings::load_editor_settings,
                commands::app_settings::save_editor_settings,
                commands::app_settings::load_open_tabs_state,
                commands::app_settings::save_open_tabs_state,
                commands::app_settings::save_detached_tab_handoff,
                commands::app_settings::load_detached_tab_handoff,
                commands::app_settings::list_detached_tab_handoffs,
                commands::app_settings::delete_detached_tab_handoff,
                commands::app_settings::approve_detached_window_close,
                commands::app_settings::load_native_debug_logs,
                // 关闭行为
                commands::app_settings::complete_app_close,
                commands::app_settings::mark_frontend_ready,
                commands::app_settings::request_app_close_from_window_controls,
                // 开机自启动
                commands::autostart::get_autostart_enabled,
                commands::autostart::set_autostart_enabled,
                // 启动管理
                commands::launcher::list_launch_items,
                commands::launcher::launch_item,
                commands::launcher::stop_launch_item,
                commands::launcher::list_launch_statuses,
                commands::ports::scan_local_ports,
                commands::ports::stop_port_processes,
                // 窗口
                commands::window_controls::set_macos_traffic_light_position,
                // 更新
                commands::update::check_for_updates,
                commands::update::fetch_changelog,
                commands::update::get_system_proxy_url,
                commands::update::cancel_update_download,
                commands::update::get_downloaded_update,
                commands::update::discard_downloaded_update,
                commands::update::download_update,
                commands::update::install_downloaded_update,
                // 深链接与文件
                commands::deep_link::parse_deep_link,
                commands::external_files::take_pending_files,
                commands::fs_open::reveal_path_in_file_manager,
                // 系统信息
                commands::support_info::get_app_support_info,
                commands::diagnostics::get_process_memory_info,
                commands::system_fonts::list_system_fonts,
                commands::keychain::read_keychain_password,
                commands::keychain::read_keychain_passwords,
                // 应用信息
                commands::app_info::get_app_version,
                commands::app_info::get_platform,
                commands::app_info::show_main_window,
                // MCP 网关
                commands::mcp::mcp_status,
                commands::mcp::mcp_reload_config,
                commands::mcp::mcp_refresh_tools,
                commands::mcp::mcp_kill_server,
                commands::mcp::mcp_revive_server,
                commands::mcp::mcp_get_config,
                commands::mcp::mcp_save_config,
                // 业务示例命令（脚手架，替换成你自己的）
                commands::business::business_ping,
                commands::business::business_echo,
            ],
        )))
        .build(tauri::generate_context!())
        .inspect(|_app| {
            append_startup_probe(format!("tauri application built after {:?}", startup_begin.elapsed()));
        })
        .expect("error while building MyApp")
        .run(|app_handle, event| {
            if let RunEvent::ExitRequested { code, api, .. } = &event {
                let confirmed_exit = app_handle
                    .try_state::<CloseBehaviorState>()
                    .map(|state| state.take_confirmed_exit())
                    .unwrap_or(false);
                if should_confirm_app_exit_request(std::env::consts::OS, *code, confirmed_exit) {
                    api.prevent_exit();
                    request_app_close(app_handle, "quit");
                } else {
                    // 重启退出与无前端原生 Quit 兜底绕过 complete_app_close，
                    // 这里同样先隐藏窗口，让 WindowServer 在进程退出前移除它。
                    if should_hide_window_before_exit(std::env::consts::OS) {
                        if let Some(window) = app_handle.get_webview_window("main") {
                            let _ = window.hide();
                        }
                    }
                }
            }
        });
}

/// 转发命令：把外部路由入口与业务命令区分开（保留 dbx 的结构）。
fn route_external_commands(
    handler: impl Fn(tauri::ipc::Invoke) -> bool + Send + Sync + 'static,
) -> impl Fn(tauri::ipc::Invoke) -> bool + Send + Sync + 'static {
    handler
}

fn main_window_probe_state<R: tauri::Runtime>(app: &tauri::AppHandle<R>) -> String {
    let Some(window) = app.get_webview_window("main") else {
        return "main_window=missing".to_string();
    };
    format!(
        "main_window visible={:?} minimized={:?} maximized={:?} fullscreen={:?}",
        window.is_visible(),
        window.is_minimized(),
        window.is_maximized(),
        window.is_fullscreen()
    )
}
