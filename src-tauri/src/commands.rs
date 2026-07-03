use tauri::{AppHandle, Manager, Emitter};
use crate::config::{self, AppConfig, Model};
use serde::{Serialize, Deserialize};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;

/// 所有 HTTP 请求的超时时间（秒）
const REQUEST_TIMEOUT_SECS: u64 = 30;
/// WebView 页面加载超时时间（秒）
const PAGE_LOAD_TIMEOUT_SECS: u64 = 30;

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

/// 隐藏/移除主窗口内的旧 content-webview（兼容旧接口，不再误杀缓存的模型 WebView）
#[tauri::command]
pub async fn hide_content_webview(app: AppHandle) -> Result<(), String> {
    let window = app.get_window("main").ok_or("主窗口未找到")?;
    let webviews = window.webviews();
    for w in &webviews {
        if w.label() == "content-webview" {
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

    // 根据主题动态计算 WebView 背景色
    let bg_color = match theme.as_str() {
        "dark" => tauri::webview::Color(26, 26, 46, 255),
        "light" => tauri::webview::Color(255, 255, 255, 255),
        _ => tauri::webview::Color(26, 26, 46, 255),
    };

    // 加载超时保护
    let loaded = Arc::new(AtomicBool::new(false));
    let loaded_clone = loaded.clone();
    let window_timeout = window.clone();
    tokio::spawn(async move {
        tokio::time::sleep(Duration::from_secs(PAGE_LOAD_TIMEOUT_SECS)).await;
        if !loaded_clone.load(Ordering::Relaxed) {
            let _ = window_timeout.emit("webview:load-failed", "content-webview");
        }
    });

    // 创建新的 WebView builder
    let loaded_flag = loaded.clone();
    let mut webview_builder = tauri::webview::WebviewBuilder::new(
        "content-webview",
        tauri::WebviewUrl::External(webview_url),
    )
    .background_color(bg_color)
    .on_page_load(move |webview, payload| {
        use tauri::webview::PageLoadEvent;
        if payload.event() == PageLoadEvent::Finished {
            loaded_flag.store(true, Ordering::Relaxed);
            let _ = webview.show();
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

    // 立即隐藏 WebView，让 HTML loading overlay 可见
    let all_webviews = window.webviews();
    for w in &all_webviews {
        if w.label() == "content-webview" {
            let _ = w.hide();
            break;
        }
    }

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

    // 根据主题动态计算 WebView 背景色
    let bg_color = match theme.as_str() {
        "dark" => tauri::webview::Color(26, 26, 46, 255),     // #1a1a2e
        "light" => tauri::webview::Color(255, 255, 255, 255), // #ffffff
        _ => tauri::webview::Color(26, 26, 46, 255),          // system: 默认深色
    };

    // 加载超时保护：如果 page-loaded 事件在超时时间内未触发，通知前端加载失败
    let loaded = Arc::new(AtomicBool::new(false));
    let loaded_clone = loaded.clone();
    let window_timeout = window.clone();
    let timeout_model_id = model_id.clone();
    tokio::spawn(async move {
        tokio::time::sleep(Duration::from_secs(PAGE_LOAD_TIMEOUT_SECS)).await;
        if !loaded_clone.load(Ordering::Relaxed) {
            let _ = window_timeout.emit("webview:load-failed", &timeout_model_id);
        }
    });

    // 创建新的 WebView builder
    // 关键设计：WebView 创建后立即隐藏，page-loaded 时再显示
    // 这样 HTML loading overlay 始终可见，不会被原生层覆盖
    let webview_label = label.clone();
    let loaded_flag = loaded.clone();
    let mut webview_builder = tauri::webview::WebviewBuilder::new(
        &label,
        tauri::WebviewUrl::External(webview_url),
    )
    .background_color(bg_color)
    .on_page_load(move |webview, payload| {
        use tauri::webview::PageLoadEvent;
        if payload.event() == PageLoadEvent::Finished {
            // 页面加载完成，显示 WebView（替换 loading overlay）
            loaded_flag.store(true, Ordering::Relaxed);
            let _ = webview.show();
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
    
    // 立即隐藏 WebView，让 HTML loading overlay 完全可见
    let all_webviews = window.webviews();
    for w in &all_webviews {
        if w.label() == webview_label {
            let _ = w.hide();
            break;
        }
    }
    
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

/// 从 GitHub Releases 自动检查应用更新
/// 调用 GitHub API 获取最新 Release，与当前版本比对
/// 无需配置服务器地址，完全自动
#[tauri::command]
pub async fn check_for_update(app: AppHandle) -> Result<Option<UpdateInfo>, String> {
    let current_version = app.package_info().version.to_string();
    
    let client = reqwest::Client::builder()
        .user_agent("AI-Hub-Desktop")
        .timeout(Duration::from_secs(REQUEST_TIMEOUT_SECS))
        .build()
        .map_err(|e| format!("创建 HTTP 客户端失败: {}", e))?;

    let response = client
        .get("https://api.github.com/repos/yzj0405/FreeWebAI/releases/latest")
        .header("Accept", "application/vnd.github+json")
        .header("X-GitHub-Api-Version", "2022-11-28")
        .send()
        .await
        .map_err(|e| format!("检查更新失败（网络错误）: {}", e))?;
    
    if !response.status().is_success() {
        // 403 可能是 API 频率限制（未认证每小时 60 次）
        if response.status().as_u16() == 403 {
            return Err("GitHub API 频率限制，请稍后重试".to_string());
        }
        return Err(format!("GitHub 响应错误 ({})", response.status()));
    }
    
    let release: GitHubRelease = response
        .json()
        .await
        .map_err(|_| "解析 GitHub 版本信息失败".to_string())?;
    
    // tag_name 去掉 'v' 前缀，如 "v2.1.0" → "2.1.0"
    let tag = release.tag_name.trim_start_matches('v').to_string();
    
    if tag == current_version {
        return Ok(None);
    }
    
    // 按优先级查找安装包：.exe > .msi
    let asset = release.assets.iter().find(|a| {
        let name = a.name.to_lowercase();
        name.ends_with(".exe")
    }).or_else(|| {
        release.assets.iter().find(|a| {
            a.name.to_lowercase().ends_with(".msi")
        })
    });
    
    match asset {
        Some(asset) => Ok(Some(UpdateInfo {
            latest: tag,
            url: asset.browser_download_url.clone(),
            notes: release.body.unwrap_or_default(),
            sha256: None, // GitHub Releases API 不提供 SHA256
        })),
        None => Err("新版本存在，但未找到 Windows 安装包（.exe 或 .msi），请手动下载".to_string()),
    }
}

/// 下载更新安装包到临时目录
/// 如果 auto_install 为 true，下载完成后自动启动安装程序并退出应用
/// 返回下载文件的本地路径
#[tauri::command]
pub async fn download_update(app: AppHandle, update_url: String, sha256: Option<String>, auto_install: Option<bool>) -> Result<String, String> {
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(REQUEST_TIMEOUT_SECS * 10)) // 下载用更长超时（5分钟）
        .build()
        .map_err(|e| format!("创建 HTTP 客户端失败: {}", e))?;
    let response = client
        .get(&update_url)
        .send()
        .await
        .map_err(|e| format!("下载失败: {}", e))?;

    if !response.status().is_success() {
        return Err(format!("下载失败，服务器响应 ({})", response.status()));
    }

    let bytes = response
        .bytes()
        .await
        .map_err(|e| format!("读取下载内容失败: {}", e))?;

    // SHA256 校验（如果提供了）
    if let Some(expected_hash) = &sha256 {
        use sha2::{Sha256, Digest};
        let mut hasher = Sha256::new();
        hasher.update(&bytes);
        let actual_hash = format!("{:x}", hasher.finalize());
        if !actual_hash.eq_ignore_ascii_case(expected_hash) {
            return Err(format!(
                "文件校验失败：下载的文件可能已损坏\n期望: {}\n实际: {}",
                expected_hash, actual_hash
            ));
        }
    }

    // 决定文件扩展名和类型说明
    #[cfg(target_os = "windows")]
    let (ext, _type_name) = (".exe", "Windows 安装程序");
    #[cfg(target_os = "macos")]
    let (ext, _type_name) = (".dmg", "macOS 磁盘映像");
    #[cfg(target_os = "linux")]
    let (ext, _type_name) = (".deb", "Debian 安装包");

    let file_name = format!("ai_hub_update_v{}{}", app.package_info().version, ext);
    let tmp_path = std::env::temp_dir().join(&file_name);

    tokio::fs::write(&tmp_path, &bytes)
        .await
        .map_err(|e| format!("保存安装包失败: {}", e))?;

    // 自动安装模式：启动安装程序后退出应用
    if auto_install.unwrap_or(false) {
        let installer_path = tmp_path.to_string_lossy().to_string();

        // 根据文件类型选择静默安装参数
        let is_msi = installer_path.to_lowercase().ends_with(".msi");

        #[cfg(target_os = "windows")]
        {
            use std::process::Command;
            let result = if is_msi {
                // MSI 静默安装：msiexec /i <path> /quiet /norestart
                Command::new("msiexec")
                    .args(["/i", &installer_path, "/quiet", "/norestart"])
                    .spawn()
            } else {
                // NSIS 静默安装：/S 区分大小写
                Command::new(&installer_path)
                    .arg("/S")
                    .spawn()
            };

            match result {
                Ok(_) => {
                    // 安装程序已启动，退出当前应用以释放文件锁
                    // 使用 std::process::exit 避免 Tokio runtime 清理阻塞
                    std::process::exit(0);
                }
                Err(e) => {
                    return Err(format!("启动安装程序失败: {}", e));
                }
            }
        }

        #[cfg(not(target_os = "windows"))]
        {
            // 非 Windows 平台暂不支持自动安装，返回路径由前端处理
            return Ok(installer_path);
        }
    }

    Ok(tmp_path.to_string_lossy().to_string())
}

/// 更新信息结构体（供前端展示和下载使用）
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UpdateInfo {
    pub latest: String,
    pub url: String,
    pub notes: String,
    #[serde(default)]
    pub sha256: Option<String>,
}

/// GitHub Release API 响应（只解析需要的字段）
#[derive(Debug, Deserialize)]
struct GitHubRelease {
    tag_name: String,
    body: Option<String>,
    assets: Vec<GitHubAsset>,
}

#[derive(Debug, Deserialize)]
struct GitHubAsset {
    name: String,
    #[serde(rename = "browser_download_url")]
    browser_download_url: String,
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
