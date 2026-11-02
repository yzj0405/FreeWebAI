// Learn more about Tauri commands at https://tauri.app/develop/calling-rust/
mod config;
mod commands;

use std::time::Duration;
use tauri::Manager;

/// 全局共享的 HTTP 客户端包装结构体
pub struct ReqwestClient(pub reqwest::Client);

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_shell::init())
        .invoke_handler(tauri::generate_handler![
            commands::get_config,
            commands::save_config,
            commands::sync_from_cloud,
            commands::check_model_url,
            commands::open_model_window,
            commands::minimize_window,
            commands::toggle_maximize,
            commands::close_window,
            commands::toggle_sidebar,
            commands::hide_content_webview,
            commands::resize_content_webview,
            commands::load_content_webview,
            commands::set_webview_theme,
            // 新增：多 WebView 缓存支持
            commands::get_or_create_model_webview,
            commands::switch_to_model,
            commands::restore_webview_focus,
            commands::resize_model_webview,
            commands::hide_model_webview,
            commands::hide_all_model_webviews,
            commands::clear_all_model_webviews,
            commands::remove_model_webview,
            // 应用更新
            commands::check_for_update,
            commands::download_update,
        ])
        .setup(|app| {
            // 创建全局共享的 HTTP 客户端（30s 超时）
            let client = reqwest::Client::builder()
                .timeout(Duration::from_secs(30))
                .build()
                .expect("Failed to create HTTP client");
            app.manage(ReqwestClient(client));

            // 初始化配置
            config::init_config(app.handle())?;

            // 注意：不要在此处（或任何窗口事件回调中）调用 webview.set_focus()。
            // WebView2 的焦点调用是同步跨进程 IPC，在 Focused 窗口事件（正在处理
            // WM_ACTIVATE）中调用会与 WebView2 的激活流程互相等待，直接卡死主线程——
            // 表现为窗口无法激活（alt+tab 切不过去）、所有主线程任务超时、页面无法加载。
            // 切回窗口后需点击一次页面获得键盘焦点，属可接受的正常行为。

            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
