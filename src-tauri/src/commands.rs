use tauri::{AppHandle, Manager, Emitter};
use crate::config::{self, AppConfig, Model};
use crate::ReqwestClient;
use serde::{Serialize, Deserialize};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;
use std::collections::VecDeque;
use std::sync::Mutex as StdMutex;
use tokio::sync::{Mutex as TokioMutex, oneshot};

/// 所有 HTTP 请求的超时时间（秒）
const REQUEST_TIMEOUT_SECS: u64 = 30;
/// WebView 页面加载超时时间（秒）
const PAGE_LOAD_TIMEOUT_SECS: u64 = 30;
/// WebView 缓存上限
const MAX_CACHED_WEBVIEWS: usize = 5;

/// LRU 缓存队列：队首为最近使用，队尾为最久未使用
static LRU_CACHE: StdMutex<VecDeque<String>> = StdMutex::new(VecDeque::new());
/// WebView 创建互斥锁，防止并发创建导致标签冲突
static WEBVIEW_CREATE_MUTEX: TokioMutex<()> = TokioMutex::const_new(());

// ==================== LRU 缓存辅助函数 ====================

/// 将模型移到 LRU 队首（最近使用）
fn lru_touch(model_id: &str) {
    if let Ok(mut lru) = LRU_CACHE.lock() {
        lru.retain(|id| id != model_id);
        lru.push_front(model_id.to_string());
    }
}

/// 从 LRU 队列移除指定模型
fn lru_remove(model_id: &str) {
    if let Ok(mut lru) = LRU_CACHE.lock() {
        lru.retain(|id| id != model_id);
    }
}

/// 从 LRU 队尾取出最久未使用的模型 ID
fn lru_pop_back() -> Option<String> {
    LRU_CACHE.lock().ok()?.pop_back()
}

/// 获取当前 LRU 缓存数量
fn lru_len() -> usize {
    LRU_CACHE.lock().map(|l| l.len()).unwrap_or(0)
}

/// 清空 LRU 队列
fn lru_clear() {
    if let Ok(mut lru) = LRU_CACHE.lock() {
        lru.clear();
    }
}

/// 在主线程上执行闭包并返回结果（通过 oneshot 通道同步）
/// Tauri 2.x 中，窗口/WebView 状态变更 API 必须在主线程执行
async fn run_on_main_thread<F, T>(app: &AppHandle, f: F) -> Result<T, String>
where
    F: FnOnce() -> Result<T, String> + Send + 'static,
    T: Send + 'static,
{
    let (tx, rx) = oneshot::channel();
    app.run_on_main_thread(move || {
        let result = f();
        let _ = tx.send(result);
    }).map_err(|e| format!("调度主线程任务失败: {}", e))?;
    rx.await.map_err(|_| "主线程通道已关闭".to_string())?
}

/// 获取配置
#[tauri::command]
pub async fn get_config(app: AppHandle) -> Result<AppConfig, String> {
    let config = config::read_config(&app).await;
    Ok(config)
}

/// 保存配置
#[tauri::command]
pub async fn save_config(app: AppHandle, config: AppConfig) -> Result<(), String> {
    config::write_config(&app, &config).await?;
    
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
    if let Some(window) = app.get_webview_window(&window_label) {
        // 窗口已存在，聚焦到该窗口（窗口状态变更必须在主线程）
        run_on_main_thread(&app, move || {
            let _ = window.show();
            let _ = window.set_focus();
            Ok::<(), String>(())
        }).await?;
        return Ok(());
    }

    // 解析 URL
    let url = url::Url::parse(&model.url).map_err(|e| e.to_string())?;
    let model_name = model.name.clone();
    let app_clone = app.clone();

    // 创建新窗口（必须在主线程执行）
    run_on_main_thread(&app, move || {
        let _window = tauri::WebviewWindowBuilder::new(&app_clone, &window_label, tauri::WebviewUrl::External(url))
            .title(&model_name)
            .inner_size(1200.0, 800.0)
            .min_inner_size(800.0, 600.0)
            .build()
            .map_err(|e| e.to_string())?;
        Ok::<(), String>(())
    }).await?;

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
    let mut config = config::read_config(&app).await;
    config.sidebar_collapsed = collapsed;
    config::write_config(&app, &config).await?;
    Ok(())
}

/// 隐藏/移除主窗口内的旧 content-webview（兼容旧接口，不再误杀缓存的模型 WebView）
#[tauri::command]
pub async fn hide_content_webview(app: AppHandle) -> Result<(), String> {
    let window = app.get_window("main").ok_or("主窗口未找到")?;

    run_on_main_thread(&app, move || {
        let webviews = window.webviews();
        for w in &webviews {
            if w.label() == "content-webview" {
                let _ = w.close();
            }
        }
        Ok::<(), String>(())
    }).await?;

    Ok(())
}

/// 同步主题到所有内容 WebView（注入 JS 设置 data-theme / dark class / color-scheme）
#[tauri::command]
pub async fn set_webview_theme(app: AppHandle, theme: String) -> Result<(), String> {
    let window = app.get_window("main").ok_or("主窗口未找到")?;
    
    // 确定是否为深色
    let is_dark = match theme.as_str() {
        "dark" => "true",
        "system" => {
            return Ok(());
        }
        _ => "false",
    };

    // 预生成主题脚本（所有 WebView 使用相同脚本）
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

    // WebView eval 必须在主线程执行
    run_on_main_thread(&app, move || {
        let webviews = window.webviews();
        for w in &webviews {
            if w.label() == "content-webview" || w.label().starts_with("webview-") {
                let _ = w.eval(&script);
            }
        }
        Ok::<(), String>(())
    }).await?;

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

    run_on_main_thread(&app, move || {
        let webviews = window.webviews();
        for w in &webviews {
            if w.label() == "content-webview" || w.label().starts_with("webview-") {
                w.set_position(tauri::LogicalPosition::new(x, y))
                    .map_err(|e| format!("调整位置失败: {}", e))?;
                w.set_size(tauri::LogicalSize::new(width, height))
                    .map_err(|e| format!("调整大小失败: {}", e))?;
            }
        }
        Ok::<(), String>(())
    }).await
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

    // 克隆 window 句柄供后续使用（必须在 move 之前）
    let window_handle = window.clone();
    let window_timeout = window.clone();

    // 解析 URL
    let webview_url = url::Url::parse(&url).map_err(|e| format!("URL 解析失败: {}", e))?;

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
    tokio::spawn(async move {
        tokio::time::sleep(Duration::from_secs(PAGE_LOAD_TIMEOUT_SECS)).await;
        if !loaded_clone.load(Ordering::Relaxed) {
            let _ = window_timeout.emit("webview:load-failed", "content-webview");
        }
    });

    // 创建新的 WebView builder（配置在任意线程完成）
    // on_page_load 回调在 WebView2 导航回调链内触发（主线程），只做原子标记，
    // 绝不调用 show()/emit() 等 Tauri API，避免在 WebView2 COM 回调链内
    // 触发 Win32 消息导致主线程消息泵阻塞（SendMessageW 广播死锁）。
    let loaded_flag = loaded.clone();
    let mut webview_builder = tauri::webview::WebviewBuilder::new(
        "content-webview",
        tauri::WebviewUrl::External(webview_url),
    )
    .background_color(bg_color)
    .on_page_load(move |_webview, payload| {
        use tauri::webview::PageLoadEvent;
        if payload.event() == PageLoadEvent::Finished {
            // 仅设置原子标记，不做任何 Tauri API 调用
            loaded_flag.store(true, Ordering::Relaxed);
        }
    });

    if !init_script.is_empty() {
        webview_builder = webview_builder.initialization_script(&init_script);
    }

    // 以下窗口操作拆分为独立的 run_on_main_thread 闭包，确保主线程消息泵
    // 在各闭包之间有呼吸空间，避免 SendMessageW（DPI/Win+D 广播）被阻塞导致系统级死锁

    // 1. 移除旧的 content-webview（独立闭包：遍历 + close）
    let window_close = window.clone();
    run_on_main_thread(&app, move || {
        let webviews = window_close.webviews();
        for w in &webviews {
            if w.label() == "content-webview" {
                let _ = w.close();
            }
        }
        Ok::<(), String>(())
    }).await?;

    // 2. 添加新的 WebView 子视图（独立闭包：add_child 涉及 WebView2 COM 初始化，较慢）
    run_on_main_thread(&app, move || {
        window.add_child(
            webview_builder,
            tauri::LogicalPosition::new(x, y),
            tauri::LogicalSize::new(width, height),
        ).map_err(|e| format!("创建 WebView 失败: {}", e))?;
        Ok::<(), String>(())
    }).await?;

    // 3. 立即隐藏 WebView，让 HTML loading overlay 可见（独立闭包：快速属性操作）
    let window_hide = window_handle.clone();
    run_on_main_thread(&app, move || {
        let all_webviews = window_hide.webviews();
        for w in &all_webviews {
            if w.label() == "content-webview" {
                let _ = w.hide();
                break;
            }
        }
        Ok::<(), String>(())
    }).await?;

    // 异步轮询 loaded_flag，页面加载完成后在主线程执行 show + emit
    // 这样避免在 on_page_load（WebView2 COM 回调链）内直接调用 Tauri API
    let poll_loaded = loaded.clone();
    let poll_app = app.clone();
    let poll_window = window_handle;
    tokio::spawn(async move {
        loop {
            tokio::time::sleep(Duration::from_millis(50)).await;
            if poll_loaded.load(Ordering::Relaxed) {
                let app_for_emit = poll_app.clone();
                let w = poll_window.clone();
                let _ = run_on_main_thread(&poll_app, move || {
                    let webviews = w.webviews();
                    for webview in &webviews {
                        if webview.label() == "content-webview" {
                            let _ = webview.show();
                            break;
                        }
                    }
                    if let Some(main_win) = app_for_emit.get_webview_window("main") {
                        let _ = main_win.emit("webview:page-loaded", "");
                    }
                    Ok::<(), String>(())
                }).await;
                break;
            }
        }
    });

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
    let window = app.get_window("main").ok_or("主窗口未找到")?;
    let label = format!("webview-{}", model_id);

    // ① 快速路径：在主线程检查 WebView 是否已存在
    let window_for_check = window.clone();
    let found = run_on_main_thread(&app, {
        let label = label.clone();
        let model_id = model_id.clone();
        move || -> Result<bool, String> {
            let webviews = window_for_check.webviews();
            for w in &webviews {
                if w.label() == label {
                    w.set_position(tauri::LogicalPosition::new(x, y))
                        .map_err(|e| format!("调整位置失败: {}", e))?;
                    w.set_size(tauri::LogicalSize::new(width, height))
                        .map_err(|e| format!("调整大小失败: {}", e))?;
                    let _ = w.show();
                    lru_touch(&model_id);
                    return Ok(true);
                }
            }
            Ok(false)
        }
    }).await?;

    if found {
        return Ok(false); // false = 已缓存
    }

    // ② 慢路径：先在锁外完成所有准备工作（URL 解析、WebView builder 构建、超时保护）
    let webview_url = url::Url::parse(&url).map_err(|e| format!("URL 解析失败: {}", e))?;
    let init_script = generate_theme_init_script(&theme);
    let bg_color = match theme.as_str() {
        "dark" => tauri::webview::Color(26, 26, 46, 255),
        "light" => tauri::webview::Color(255, 255, 255, 255),
        _ => tauri::webview::Color(26, 26, 46, 255),
    };

    // 超时保护
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

    // 创建 WebView builder（配置可在任意线程完成）
    // on_page_load 回调只做原子标记，避免在 WebView2 COM 回调链内阻塞主线程消息泵
    let webview_label = label.clone();
    let loaded_flag = loaded.clone();
    let mut webview_builder = tauri::webview::WebviewBuilder::new(
        &label,
        tauri::WebviewUrl::External(webview_url),
    )
    .background_color(bg_color)
    .on_page_load(move |_webview, payload| {
        use tauri::webview::PageLoadEvent;
        if payload.event() == PageLoadEvent::Finished {
            // 仅设置原子标记，不做任何 Tauri API 调用
            loaded_flag.store(true, Ordering::Relaxed);
        }
    });

    if !init_script.is_empty() {
        webview_builder = webview_builder.initialization_script(&init_script);
    }

    // ③ 获取互斥锁，仅在临界区内执行双重检查 + 创建，完成后立即释放
    // 关键：每个 run_on_main_thread 闭包只做最小粒度操作，确保主线程消息泵
    //       在各闭包之间有呼吸空间，避免 SendMessageW（DPI/Win+D 广播）被阻塞导致系统级死锁
    let created = {
        let _guard = WEBVIEW_CREATE_MUTEX.lock().await;

        // ③-a 锁内再次确认 WebView 不存在（快速闭包：仅遍历+属性设置）
        let window_check = window.clone();
        let label_dup = label.clone();
        let model_id_dup = model_id.clone();
        let found_in_lock = run_on_main_thread(&app, move || {
            let webviews = window_check.webviews();
            for w in &webviews {
                if w.label() == label_dup {
                    w.set_position(tauri::LogicalPosition::new(x, y))
                        .map_err(|e| format!("调整位置失败: {}", e))?;
                    w.set_size(tauri::LogicalSize::new(width, height))
                        .map_err(|e| format!("调整大小失败: {}", e))?;
                    let _ = w.show();
                    lru_touch(&model_id_dup);
                    return Ok(true);
                }
            }
            Ok(false)
        }).await?;

        if found_in_lock {
            false
        } else {
            // ③-b 创建新 WebView（独立闭包：add_child 涉及 WebView2 COM 初始化，较慢）
            // 此闭包结束后，主线程消息泵恢复 → 系统广播可被处理
            run_on_main_thread(&app, {
                let w = window.clone();
                move || {
                    w.add_child(
                        webview_builder,
                        tauri::LogicalPosition::new(x, y),
                        tauri::LogicalSize::new(width, height),
                    ).map_err(|e| format!("创建 WebView 失败: {}", e))?;
                    Ok::<(), String>(())
                }
            }).await?;

            // ③-c 立即隐藏 WebView（独立闭包：快速属性操作）
            run_on_main_thread(&app, {
                let w = window.clone();
                let lbl = webview_label.clone();
                move || {
                    let all = w.webviews();
                    for webview in &all {
                        if webview.label() == lbl {
                            let _ = webview.hide();
                            break;
                        }
                    }
                    Ok::<(), String>(())
                }
            }).await?;

            // ③-d LRU 操作（独立闭包：遍历 + close）
            run_on_main_thread(&app, {
                let w = window.clone();
                let mid = model_id.clone();
                move || {
                    lru_touch(&mid);
                    while lru_len() > MAX_CACHED_WEBVIEWS {
                        if let Some(evict_id) = lru_pop_back() {
                            let evict_label = format!("webview-{}", evict_id);
                            let all_wv = w.webviews();
                            for webview in &all_wv {
                                if webview.label() == evict_label {
                                    let _ = webview.close();
                                    break;
                                }
                            }
                        } else {
                            break;
                        }
                    }
                    Ok::<(), String>(())
                }
            }).await?;

            true
        }
    }; // _guard 在此处释放，锁不再持有

    // 如果是新创建的 WebView，异步轮询 loaded_flag，页面加载完成后在主线程执行 show + emit
    // 避免在 on_page_load（WebView2 COM 回调链）内直接调用 Tauri API 阻塞消息泵
    if created {
        let poll_loaded = loaded.clone();
        let poll_app = app.clone();
        let poll_label = label.clone();
        tokio::spawn(async move {
            loop {
                tokio::time::sleep(Duration::from_millis(50)).await;
                if poll_loaded.load(Ordering::Relaxed) {
                    let lbl = poll_label.clone();
                    let app_for_emit = poll_app.clone();
                    let _ = run_on_main_thread(&poll_app, move || {
                        if let Some(main_win) = app_for_emit.get_window("main") {
                            let webviews = main_win.webviews();
                            for w in &webviews {
                                if w.label() == lbl {
                                    let _ = w.show();
                                    break;
                                }
                            }
                            if let Some(emit_win) = app_for_emit.get_webview_window("main") {
                                let _ = emit_win.emit("webview:page-loaded", "");
                            }
                        }
                        Ok::<(), String>(())
                    }).await;
                    break;
                }
            }
        });
    }

    Ok(created)
}

/// 切换到指定模型（隐藏其他所有模型 WebView，只显示目标模型）
#[tauri::command]
pub async fn switch_to_model(
    app: AppHandle,
    model_id: String,
) -> Result<(), String> {
    let window = app.get_window("main").ok_or("主窗口未找到")?;
    
    let target_label = format!("webview-{}", model_id);

    // 更新 LRU 位置
    lru_touch(&model_id);

    // 窗口状态变更必须在主线程执行
    run_on_main_thread(&app, move || {
        let webviews = window.webviews();
        for w in &webviews {
            if w.label().starts_with("webview-") {
                if w.label() == target_label {
                    let _ = w.show();
                    let _ = w.set_focus();
                } else {
                    let _ = w.hide();
                }
            }
        }
        Ok::<(), String>(())
    }).await?;
    
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

    run_on_main_thread(&app, move || {
        let webviews = window.webviews();
        for w in &webviews {
            if w.label() == label {
                w.set_position(tauri::LogicalPosition::new(x, y))
                    .map_err(|e| format!("调整位置失败: {}", e))?;
                w.set_size(tauri::LogicalSize::new(width, height))
                    .map_err(|e| format!("调整大小失败: {}", e))?;
                break;
            }
        }
        Ok::<(), String>(())
    }).await
}

/// 隐藏/移除指定模型的 WebView
#[tauri::command]
pub async fn hide_model_webview(
    app: AppHandle,
    model_id: String,
) -> Result<(), String> {
    let window = app.get_window("main").ok_or("主窗口未找到")?;
    let label = format!("webview-{}", model_id);

    run_on_main_thread(&app, move || {
        let webviews = window.webviews();
        for w in &webviews {
            if w.label() == label {
                let _ = w.close();
                break;
            }
        }
        Ok::<(), String>(())
    }).await?;

    Ok(())
}

/// 隐藏所有模型 WebView（不销毁，仅隐藏）- 用于打开设置弹窗时避免遮挡
#[tauri::command]
pub async fn hide_all_model_webviews(app: AppHandle) -> Result<(), String> {
    let window = app.get_window("main").ok_or("主窗口未找到")?;

    run_on_main_thread(&app, move || {
        let webviews = window.webviews();
        for w in &webviews {
            if w.label().starts_with("webview-") {
                let _ = w.hide();
            }
        }
        Ok::<(), String>(())
    }).await?;

    Ok(())
}

/// 清除所有缓存的模型 WebView（释放内存）
#[tauri::command]
pub async fn clear_all_model_webviews(app: AppHandle) -> Result<(), String> {
    let window = app.get_window("main").ok_or("主窗口未找到")?;

    run_on_main_thread(&app, move || {
        let webviews = window.webviews();
        for w in &webviews {
            if w.label().starts_with("webview-") {
                let _ = w.close();
            }
        }
        Ok::<(), String>(())
    }).await?;

    // 清空 LRU 队列
    lru_clear();

    Ok(())
}

/// 移除指定模型的 WebView（销毁 + 从 LRU 移除）
/// 用于设置中删除模型时清理对应的缓存 WebView
#[tauri::command]
pub async fn remove_model_webview(app: AppHandle, model_id: String) -> Result<(), String> {
    let window = app.get_window("main").ok_or("主窗口未找到")?;
    let label = format!("webview-{}", model_id);

    run_on_main_thread(&app, move || {
        let webviews = window.webviews();
        for w in &webviews {
            if w.label() == label {
                let _ = w.close();
                break;
            }
        }
        Ok::<(), String>(())
    }).await?;

    // 从 LRU 移除
    lru_remove(&model_id);

    Ok(())
}

/// GitHub 仓库标识（用于构造 Releases URL）
const GITHUB_REPO: &str = "yzj0405/FreeWebAI";
/// Tauri 配置中的 productName（与 tauri.conf.json 保持一致）
const TAURI_PRODUCT_NAME: &str = "AI Hub Desktop";
/// 目标架构
const TAURI_ARCH: &str = "x64";

/// 根据 Tauri bundler 命名规则构造安装包文件名
/// NSIS: {ProductName}_{version}_{arch}-setup.exe
/// MSI:  {ProductName}_{version}_{arch}_en-US.msi
fn build_installer_filename(version: &str, ext: &str) -> String {
    let product = TAURI_PRODUCT_NAME.replace(' ', ".");
    match ext {
        ".msi" => format!("{}_{}_{}_en-US{}", product, version, TAURI_ARCH, ext),
        _ => format!("{}_{}_{}-setup{}", product, version, TAURI_ARCH, ext),
    }
}

/// 根据 Tauri bundler 命名规则构造下载 URL
fn build_download_url(tag: &str) -> String {
    let base = format!(
        "https://github.com/{}/releases/download/v{}",
        GITHUB_REPO, tag
    );
    // 返回 NSIS 安装包 URL（Windows 首选）
    let filename = build_installer_filename(tag, ".exe");
    format!("{}/{}", base, filename)
}

/// 从 GitHub Release 的 assets 列表中查找 .exe 安装包的下载 URL
fn extract_asset_url(release: &GitHubRelease) -> Option<String> {
    release.assets.iter()
        .find(|a| a.name.ends_with(".exe"))
        .map(|a| a.browser_download_url.clone())
}

/// 从 GitHub Releases 检查应用更新
/// 使用重定向方式获取版本号，避免 API 频率限制
/// 下载 URL 使用已知文件名模式构造直链，无需 API
#[tauri::command]
pub async fn check_for_update(app: AppHandle) -> Result<Option<UpdateInfo>, String> {
    let current_version = app.package_info().version.to_string();
    let client = app.state::<ReqwestClient>().0.clone();

    // ① 使用 /releases/latest 重定向提取最新版本号（无 API 频率限制）
    //    GitHub 会 302 重定向到 /releases/tag/v{version}
    let no_redirect_client = reqwest::Client::builder()
        .timeout(Duration::from_secs(REQUEST_TIMEOUT_SECS))
        .redirect(reqwest::redirect::Policy::none())
        .build()
        .map_err(|e| format!("创建 HTTP 客户端失败: {}", e))?;

    let response = no_redirect_client
        .get(format!("https://github.com/{}/releases/latest", GITHUB_REPO))
        .send()
        .await
        .map_err(|e| format!("检查更新失败（网络错误）: {}", e))?;

    let status = response.status();
    let tag = if status.is_redirection() {
        // 从 Location 头提取版本号
        // Location: https://github.com/yzj0405/FreeWebAI/releases/tag/v2.0.5
        response
            .headers()
            .get("location")
            .and_then(|v| v.to_str().ok())
            .and_then(|loc| loc.rsplit('/').next())
            .map(|t| t.trim_start_matches('v').to_string())
    } else {
        None
    };

    // 如果重定向方式失败，降级到 API（仅一次请求）
    let tag = match tag {
        Some(t) if !t.is_empty() => t,
        _ => {
            let api_response = client
                .get(format!(
                    "https://api.github.com/repos/{}/releases/latest",
                    GITHUB_REPO
                ))
                .header("Accept", "application/vnd.github+json")
                .header("X-GitHub-Api-Version", "2022-11-28")
                .send()
                .await
                .map_err(|e| format!("检查更新失败（网络错误）: {}", e))?;

            if !api_response.status().is_success() {
                return Err(format!(
                    "检查更新失败（GitHub 响应 {}），请稍后重试",
                    api_response.status()
                ));
            }

            let release: GitHubRelease = api_response
                .json()
                .await
                .map_err(|_| "解析 GitHub 版本信息失败".to_string())?;

            release.tag_name.trim_start_matches('v').to_string()
        }
    };

    if tag == current_version {
        return Ok(None);
    }

    // ② 构造下载 URL：先按 Tauri bundler 规则构造，再尝试用 API 获取真实 asset URL
    let constructed_url = build_download_url(&tag);

    // ③ 尝试从 Release API 获取更新日志和真实下载 URL（非关键，失败不影响更新检测）
    let (notes, download_url, sha256) = match client
        .get(format!(
            "https://api.github.com/repos/{}/releases/tags/v{}",
            GITHUB_REPO, tag
        ))
        .header("Accept", "application/vnd.github+json")
        .send()
        .await
    {
        Ok(resp) => {
            if let Ok(release) = resp.json::<GitHubRelease>().await {
                let notes = release.body.clone().unwrap_or_default();
                // 优先使用 API 返回的真实 asset URL
                let asset_url = extract_asset_url(&release).unwrap_or_else(|| constructed_url.clone());
                // 尝试提取 SHA256（如果有 digest 字段）
                let sha256 = release.assets.iter()
                    .find(|a| a.name.ends_with(".exe"))
                    .and_then(|a| a.digest.clone());
                (notes, asset_url, sha256)
            } else {
                (String::new(), constructed_url, None)
            }
        }
        Err(_) => (String::new(), constructed_url, None),
    };

    Ok(Some(UpdateInfo {
        latest: tag,
        url: download_url,
        notes,
        sha256,
    }))
}

/// 下载更新安装包到临时目录
/// 如果 auto_install 为 true，下载完成后自动启动安装程序并退出应用
/// 返回下载文件的本地路径
#[tauri::command]
pub async fn download_update(app: AppHandle, update_url: String, sha256: Option<String>, auto_install: Option<bool>) -> Result<String, String> {
    let client = app.state::<ReqwestClient>().0.clone();
    let response = client
        .get(&update_url)
        .timeout(Duration::from_secs(REQUEST_TIMEOUT_SECS * 10)) // 下载用更长超时（5分钟）
        .send()
        .await
        .map_err(|e| format!("下载失败: {}", e))?;

    if !response.status().is_success() {
        let status = response.status().as_u16();
        return Err(match status {
            404 => "下载失败：安装包文件不存在，请检查发布页面是否有新版本安装包".to_string(),
            403 => "下载失败：访问被拒绝（GitHub 速率限制），请稍后重试".to_string(),
            _ => format!("下载失败，服务器响应 ({})", response.status()),
        });
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
                // MSI 安装：打开安装向导
                Command::new("msiexec")
                    .args(["/i", &installer_path])
                    .spawn()
            } else {
                // NSIS 安装：打开安装向导（非静默模式）
                Command::new(&installer_path)
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
    #[serde(default)]
    digest: Option<String>,
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
