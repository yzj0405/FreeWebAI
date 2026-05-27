// Learn more about Tauri commands at https://tauri.app/develop/calling-rust/
mod config;
mod commands;

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
        ])
        .setup(|app| {
            // 初始化配置
            config::init_config(app.handle())?;
            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
