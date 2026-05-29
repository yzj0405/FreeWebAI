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

/// 隐藏/移除主窗口内的内容 WebView（兼容旧接口）
#[tauri::command]
pub async fn hide_content_webview(app: AppHandle) -> Result<(), String> {
    let window = app.get_window("main").ok_or("主窗口未找到")?;
    let webviews = window.webviews();
    for w in &webviews {
        if w.label() == "content-webview" || w.label().starts_with("webview-") {
            let _ = w.close();
        }
    }
    Ok(())
}

/// 同步主题到所有内容 WebView（注入 JS 设置 data-theme / dark class / color-scheme）
#[tauri::command]
pub async fn set_webview_theme(app: AppHandle, theme: String) -> Result<(), String> {
    let window = app.get_window("main").ok_or("主窗口未找到")?;
    let webviews = window.webviews();
    
    // 确定是否为深色
    let is_dark = match theme.as_str() {
        "dark" => "true",
        "system" => {
            return Ok(());
        }
        _ => "false",
    };
    
    // 同步主题到所有模型 WebView
    for w in &webviews {
        if w.label() == "content-webview" || w.label().starts_with("webview-") {
            let script = format!(
                r#"
(function() {{
  var isDark = {is_dark};
  var root = document.documentElement;
  var theme = isDark ? 'dark' : 'light';
  
  root.setAttribute('data-theme', theme);
  root.classList.toggle('dark', isDark);
  root.classList.toggle('light', !isDark);
  root.style.colorScheme = isDark ? 'dark' : 'light';
  root.dispatchEvent(new CustomEvent('theme-change', {{ detail: {{ theme: theme }} }}));
}})();
"#
            );
            let _ = w.eval(&script);
        }
    }
    Ok(())
}

/// 调整所有可见内容 WebView 的位置和大小（不重建）
#[tauri::command]
pub async fn resize_content_webview(
    app: AppHandle,
    x: f64,
    y: f64,
    width: f64,
    height: f64,
) -> Result<(), String> {
    let window = app.get_window("main").ok_or("主窗口未找到")?;
    let webviews = window.webviews();
    
    for w in &webviews {
        // 调整所有模型 WebView 和旧的 content-webview
        if w.label() == "content-webview" || w.label().starts_with("webview-") {
            w.set_position(tauri::LogicalPosition::new(x, y))
                .map_err(|e| format!("调整位置失败: {}", e))?;
            w.set_size(tauri::LogicalSize::new(width, height))
                .map_err(|e| format!("调整大小失败: {}", e))?;
        }
    }
    Ok(())
}

/// 在主窗口内加载内容 WebView（右侧显示外部网页）
/// 注意：此命令会销毁旧的 content-webview 并创建新的
/// 建议使用 get_or_create_model_webview 替代以支持缓存
#[tauri::command]
pub async fn load_content_webview(
    app: AppHandle,
    url: String,
    x: f64,
    y: f64,
    width: f64,
    height: f64,
    theme: String,
) -> Result<(), String> {
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

    // 克隆 window 句柄给 on_page_load 回调
    let window_handle = window.clone();

    // 生成主题注入脚本
    let init_script = generate_theme_init_script(&theme);

    // 创建新的 WebView builder
    let mut webview_builder = tauri::webview::WebviewBuilder::new(
        "content-webview",
        tauri::WebviewUrl::External(webview_url),
    ).on_page_load(move |_webview, payload| {
        use tauri::webview::PageLoadEvent;
        if payload.event() == PageLoadEvent::Finished {
            let _ = window_handle.emit("webview:page-loaded", "");
        }
    });

    // 注入主题初始化脚本（在页面加载前注册）
    if !init_script.is_empty() {
        webview_builder = webview_builder.initialization_script(&init_script);
    }

    // 作为子视图添加到主窗口
    window.add_child(
        webview_builder,
        tauri::LogicalPosition::new(x, y),
        tauri::LogicalSize::new(width, height),
    ).map_err(|e| format!("创建 WebView 失败: {}", e))?;

    Ok(())
}

/// 获取或创建指定模型的 WebView（支持缓存，避免重复加载）
/// 核心缓存逻辑：每个模型维护独立的 WebView 实例
/// 返回值：true 表示新创建的 WebView（需要 loading），false 表示已缓存（无需 loading）
#[tauri::command]
pub async fn get_or_create_model_webview(
    app: AppHandle,
    model_id: String,
    url: String,
    x: f64,
    y: f64,
    width: f64,
    height: f64,
    theme: String,
) -> Result<bool, String> {
    // 获取主窗口的 Window 对象
    let window = app.get_window("main").ok_or("主窗口未找到")?;
    
    let label = format!("webview-{}", model_id);
    
    // ① 检查是否已存在该模型的 WebView（缓存命中）
    let webviews = window.webviews();
    for w in &webviews {
        if w.label() == label {
            // 已存在：调整位置和大小 + 显示在最上层
            w.set_position(tauri::LogicalPosition::new(x, y))
                .map_err(|e| format!("调整位置失败: {}", e))?;
            w.set_size(tauri::LogicalSize::new(width, height))
                .map_err(|e| format!("调整大小失败: {}", e))?;
            
            // 确保可见（如果之前被隐藏了）
            let _ = w.show();
            
            // 返回 false 表示这是缓存的 WebView（无需 loading 动画）
            return Ok(false);
        }
    }
    
    // ② 不存在：创建新 WebView（首次加载）
    let webview_url = url::Url::parse(&url).map_err(|e| format!("URL 解析失败: {}", e))?;
    
    // 克隆 window 句柄给 on_page_load 回调
    let window_handle = window.clone();
    
    // 生成主题注入脚本
    let init_script = generate_theme_init_script(&theme);
    
    // 创建新的 WebView builder
    let mut webview_builder = tauri::webview::WebviewBuilder::new(
        &label,
        tauri::WebviewUrl::External(webview_url),
    ).on_page_load(move |_webview, payload| {
        use tauri::webview::PageLoadEvent;
        if payload.event() == PageLoadEvent::Finished {
            let _ = window_handle.emit("webview:page-loaded", "");
        }
    });
    
    // 注入主题初始化脚本
    if !init_script.is_empty() {
        webview_builder = webview_builder.initialization_script(&init_script);
    }
    
    // 作为子视图添加到主窗口
    window.add_child(
        webview_builder,
        tauri::LogicalPosition::new(x, y),
        tauri::LogicalSize::new(width, height),
    ).map_err(|e| format!("创建 WebView 失败: {}", e))?;
    
    // 返回 true 表示这是新创建的 WebView（需要 loading 动画）
    Ok(true)
}

/// 切换到指定模型（隐藏其他所有模型 WebView，只显示目标模型）
#[tauri::command]
pub async fn switch_to_model(
    app: AppHandle,
    model_id: String,
) -> Result<(), String> {
    let window = app.get_window("main").ok_or("主窗口未找到")?;
    
    let target_label = format!("webview-{}", model_id);
    
    // 遍历所有 WebView
    let webviews = window.webviews();
    for w in &webviews {
        // 只处理模型 WebView（标签以 "webview-" 开头）
        if w.label().starts_with("webview-") {
            if w.label() == target_label {
                // 目标 WebView：确保显示并置于最上层
                let _ = w.show();
                let _ = w.set_focus();
            } else {
                // 其他 WebView：隐藏
                let _ = w.hide();
            }
        }
    }
    
    Ok(())
}

/// 调整指定模型 WebView 的位置和大小（不重建）
#[tauri::command]
pub async fn resize_model_webview(
    app: AppHandle,
    model_id: String,
    x: f64,
    y: f64,
    width: f64,
    height: f64,
) -> Result<(), String> {
    let window = app.get_window("main").ok_or("主窗口未找到")?;
    
    let label = format!("webview-{}", model_id);
    let webviews = window.webviews();
    
    for w in &webviews {
        if w.label() == label {
            w.set_position(tauri::LogicalPosition::new(x, y))
                .map_err(|e| format!("调整位置失败: {}", e))?;
            w.set_size(tauri::LogicalSize::new(width, height))
                .map_err(|e| format!("调整大小失败: {}", e))?;
            return Ok(());
        }
    }
    
    // WebView 不存在，静默忽略
    Ok(())
}

/// 隐藏/移除指定模型的 WebView
#[tauri::command]
pub async fn hide_model_webview(
    app: AppHandle,
    model_id: String,
) -> Result<(), String> {
    let window = app.get_window("main").ok_or("主窗口未找到")?;
    
    let label = format!("webview-{}", model_id);
    let webviews = window.webviews();
    
    for w in &webviews {
        if w.label() == label {
            let _ = w.close();
            return Ok(());
        }
    }
    
    Ok(())
}

/// 隐藏所有模型 WebView（不销毁，仅隐藏）- 用于打开设置弹窗时避免遮挡
#[tauri::command]
pub async fn hide_all_model_webviews(app: AppHandle) -> Result<(), String> {
    let window = app.get_window("main").ok_or("主窗口未找到")?;
    
    let webviews = window.webviews();
    for w in &webviews {
        // 只隐藏模型 WebView（标签以 "webview-" 开头）
        if w.label().starts_with("webview-") {
            let _ = w.hide();
        }
    }
    
    Ok(())
}

/// 清除所有缓存的模型 WebView（释放内存）
#[tauri::command]
pub async fn clear_all_model_webviews(app: AppHandle) -> Result<(), String> {
    let window = app.get_window("main").ok_or("主窗口未找到")?;
    
    let webviews = window.webviews();
    for w in &webviews {
        // 只清理模型 WebView（保留其他 WebView 如 content-webview）
        if w.label().starts_with("webview-") {
            let _ = w.close();
        }
    }
    
    Ok(())
}

/// 生成主题初始化脚本
fn generate_theme_init_script(theme: &str) -> String {
    match theme {
        "dark" => r#"
(function() {
  var isDark = true;
  var _mm = window.matchMedia.bind(window);
  window.matchMedia = function(q) {
    if (q === '(prefers-color-scheme: dark)' || q === '(prefers-color-scheme: light)') {
      return { matches: isDark, media: q, onchange: null,
        addListener: function(){}, removeListener: function(){},
        addEventListener: function(t,f){ if(t==='change')this._f=f; },
        removeEventListener: function(){ this._f=null; },
        dispatchEvent: function(){ return true; } };
    }
    return _mm(q);
  };
  document.addEventListener('DOMContentLoaded', function() {
    var r = document.documentElement;
    r.setAttribute('data-theme', 'dark');
    r.classList.add('dark'); r.classList.remove('light');
    r.style.colorScheme = 'dark';
  });
})();
"#.to_string(),
        "light" => r#"
(function() {
  var isDark = false;
  var _mm = window.matchMedia.bind(window);
  window.matchMedia = function(q) {
    if (q === '(prefers-color-scheme: dark)' || q === '(prefers-color-scheme: light)') {
      return { matches: isDark, media: q, onchange: null,
        addListener: function(){}, removeListener: function(){},
        addEventListener: function(t,f){ if(t==='change')this._f=f; },
        removeEventListener: function(){ this._f=null; },
        dispatchEvent: function(){ return true; } };
    }
    return _mm(q);
  };
  document.addEventListener('DOMContentLoaded', function() {
    var r = document.documentElement;
    r.setAttribute('data-theme', 'light');
    r.classList.add('light'); r.classList.remove('dark');
    r.style.colorScheme = 'light';
  });
})();
"#.to_string(),
        _ => String::new(), // system: 不干预
    }
}
