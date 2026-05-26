use tauri::{AppHandle, Manager, Emitter};
use crate::config::{self, AppConfig, Model};

/// 获取配置
#[tauri::command]
pub async fn get_config(app: AppHandle) -> Result<AppConfig, String> {
    let config = config::read_config(&app);
    Ok(config)
}

/// 保存配置
#[tauri::command]
pub async fn save_config(app: AppHandle, config: AppConfig) -> Result<(), String> {
    config::write_config(&app, &config)?;
    
    // 通知前端配置已更新
    if let Some(window) = app.get_webview_window("main") {
        let _ = window.emit("config:updated", ());
    }
    
    Ok(())
}

/// 从云端同步配置
#[tauri::command]
pub async fn sync_from_cloud(app: AppHandle, server_url: String) -> Result<AppConfig, String> {
    let merged_config = config::sync_from_cloud(&app, server_url).await?;
    
    // 通知前端配置已更新
    if let Some(window) = app.get_webview_window("main") {
        let _ = window.emit("config:updated", ());
    }
    
    Ok(merged_config)
}

/// 检查模型 URL 是否可访问
#[tauri::command]
pub async fn check_model_url(url: String) -> Result<bool, String> {
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(5))
        .build()
        .map_err(|e| e.to_string())?;

    match client.head(&url).send().await {
        Ok(response) => {
            let status = response.status();
            Ok(status.is_success() || status.is_redirection())
        }
        Err(_) => Ok(false),
    }
}

/// 打开模型窗口
#[tauri::command]
pub async fn open_model_window(app: AppHandle, model: Model) -> Result<(), String> {
    let window_label = format!("model_{}", model.id);

    // 检查窗口是否已存在
    if app.get_webview_window(&window_label).is_some() {
        // 窗口已存在，聚焦到该窗口
        if let Some(window) = app.get_webview_window(&window_label) {
            let _ = window.show();
            let _ = window.set_focus();
        }
        return Ok(());
    }

    // 创建新窗口
    let _window = tauri::WebviewWindowBuilder::new(&app, &window_label, tauri::WebviewUrl::External(url::Url::parse(&model.url).map_err(|e| e.to_string())?))
        .title(&model.name)
        .inner_size(1200.0, 800.0)
        .min_inner_size(800.0, 600.0)
        .build()
        .map_err(|e| e.to_string())?;

    // 设置 User-Agent（伪装为标准 Chrome）
    // 注意：Tauri 2.x 中 User-Agent 设置可能因平台而异
    
    Ok(())
}

/// 最小化窗口
#[tauri::command]
pub fn minimize_window(window: tauri::Window) {
    let _ = window.minimize();
}

/// 切换最大化
#[tauri::command]
pub fn toggle_maximize(window: tauri::Window) {
    if let Ok(is_maximized) = window.is_maximized() {
        if is_maximized {
            let _ = window.unmaximize();
        } else {
            let _ = window.maximize();
        }
    }
}

/// 关闭窗口
#[tauri::command]
pub fn close_window(window: tauri::Window) {
    let _ = window.close();
}

/// 切换侧边栏折叠状态
#[tauri::command]
pub async fn toggle_sidebar(app: AppHandle, collapsed: bool) -> Result<(), String> {
    let mut config = config::read_config(&app);
    config.sidebar_collapsed = collapsed;
    config::write_config(&app, &config)?;
    Ok(())
}

/// 在主窗口内加载内容 WebView（右侧显示外部网页）
#[tauri::command]
pub async fn load_content_webview(
    app: AppHandle,
    url: String,
    x: f64,
    y: f64,
    width: f64,
    height: f64,
) -> Result<(), String> {
    // 获取主窗口的 Window 对象
    let window = app.get_window("main").ok_or("主窗口未找到")?;
    
    // 遍历已有的 webview，移除 content-webview
    let webviews = window.webviews();
    for w in &webviews {
        if w.label() == "content-webview" {
            let _ = w.close();
        }
    }
    
    // 解析 URL
    let webview_url = url::Url::parse(&url).map_err(|e| format!("URL 解析失败: {}", e))?;
    
    // 创建新的 WebView builder
    let webview_builder = tauri::webview::WebviewBuilder::new(
        "content-webview",
        tauri::WebviewUrl::External(webview_url),
    );
    
    // 作为子视图添加到主窗口
    window.add_child(
        webview_builder,
        tauri::LogicalPosition::new(x, y),
        tauri::LogicalSize::new(width, height),
    ).map_err(|e| format!("创建 WebView 失败: {}", e))?;
    
    Ok(())
}
