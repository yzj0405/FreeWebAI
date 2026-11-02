---
comet_change: fix-runtime-stability
role: technical-design
canonical_spec: openspec
---

# Design Doc: fix-runtime-stability

## 1. 概述

FreeWebAI Desktop 在长时间使用后卡死。本 Design Doc 细化 5 项修复的详细实现设计，覆盖数据结构、API 签名、错误处理、边界条件和测试策略。

## 2. 组件设计

### 2.1 全局共享 HTTP Client

**文件**：`lib.rs`、`commands.rs`、`config.rs`

**新增结构体**：
```rust
// lib.rs 中新增
pub struct ReqwestClient(pub reqwest::Client);
```

**初始化**（`lib.rs` setup）：
```rust
.setup(|app| {
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(30))
        .build()
        .expect("Failed to create HTTP client");
    app.manage(ReqwestClient(client));

    config::init_config(app.handle())?;
    Ok(())
})
```

**命令中获取**：
```rust
let client = app.state::<ReqwestClient>().0.clone();
```

**config.rs 中获取**：`sync_from_cloud` 改为接收 `&reqwest::Client` 参数：
```rust
pub async fn sync_from_cloud(
    app: &AppHandle,
    server_url: String,
    client: &reqwest::Client,  // 新增参数
) -> Result<AppConfig, String>
```

**影响的命令**：
- `sync_from_cloud`：从 `app.state` 获取 Client
- `check_for_update`：从 `app.state` 获取 Client
- `download_update`：从 `app.state` 获取 Client
- `check_model_url`：保留独立创建（5s 超时，不同于全局 30s）

### 2.2 配置文件异步 I/O

**文件**：`config.rs`、`commands.rs`

**改造函数**：

| 函数 | 改造前 | 改造后 |
|------|--------|--------|
| `read_config` | `fn read_config(app) -> AppConfig` | `async fn read_config(app) -> AppConfig` |
| `write_config` | `fn write_config(app, config) -> Result` | `async fn write_config(app, config) -> Result` |
| `get_config_path` | 不变 | 不变（纯路径计算） |

**改造后实现**：
```rust
pub async fn read_config(app: &AppHandle) -> AppConfig {
    let config_path = get_config_path(app);
    let content = tokio::fs::read_to_string(&config_path)
        .await
        .unwrap_or_else(|_| "{}".to_string());
    serde_json::from_str(&content).unwrap_or_default()
}

pub async fn write_config(app: &AppHandle, config: &AppConfig) -> Result<(), String> {
    let config_path = get_config_path(app);
    let json = serde_json::to_string_pretty(config)
        .map_err(|e| format!("序列化配置失败: {}", e))?;
    tokio::fs::write(&config_path, json)
        .await
        .map_err(|e| format!("写入配置文件失败: {}", e))?;
    Ok(())
}
```

**调用链更新**：
```
get_config     → config::read_config().await
save_config    → config::write_config().await
toggle_sidebar → config::read_config().await + config::write_config().await
init_config    → config::read_config().await（setup 中用 tokio::block_on 或改为异步初始化）
sync_from_cloud → config::read_config().await
```

**特殊处理**：`init_config` 在 `lib.rs` 的 `setup()` 中调用，`setup` 不是 async 上下文。解决方案：
```rust
.setup(|app| {
    // setup 中使用 tokio::runtime::Handle::current().block_on()
    // 或将 init_config 改为同步的 ensure_config_exists，只创建目录和默认文件
    config::ensure_config_exists(app.handle())?;
    Ok(())
})
```

将 `init_config` 拆分为：
- `ensure_config_exists`（同步）：仅检查目录和文件是否存在，不存在则创建
- `read_config`（异步）：读取并解析配置

### 2.3 LRU 缓存淘汰

**文件**：`commands.rs`

**全局状态**：
```rust
use std::collections::VecDeque;
use std::sync::Mutex;
use once_cell::sync::Lazy;  // 或使用 std::sync::LazyLock (Rust 1.80+)

static LRU_CACHE: Lazy<Mutex<VecDeque<String>>> = Lazy::new(|| Mutex::new(VecDeque::new()));
const MAX_CACHED_WEBVIEWS: usize = 5;
```

**辅助函数**：
```rust
fn lru_touch(model_id: &str) {
    if let Ok(mut lru) = LRU_CACHE.lock() {
        lru.retain(|id| id != model_id);
        lru.push_front(model_id.to_string());
    }
}

fn lru_remove(model_id: &str) {
    if let Ok(mut lru) = LRU_CACHE.lock() {
        lru.retain(|id| id != model_id);
    }
}

fn lru_pop_back() -> Option<String> {
    LRU_CACHE.lock().ok()?.pop_back()
}
```

**淘汰逻辑**（在 `get_or_create_model_webview` 中）：
```rust
// 创建新 WebView 后
lru_touch(&model_id);

// 淘汰超限的 WebView
while {
    let len = LRU_CACHE.lock().map(|l| l.len()).unwrap_or(0);
    len > MAX_CACHED_WEBVIEWS
} {
    if let Some(evict_id) = lru_pop_back() {
        let evict_label = format!("webview-{}", evict_id);
        for w in &window.webviews() {
            if w.label() == evict_label {
                let _ = w.close();
                break;
            }
        }
    } else {
        break;
    }
}
```

**`switch_to_model` 中更新 LRU**：
```rust
lru_touch(&model_id);
```

**`clear_all_model_webviews` 中清空 LRU**：
```rust
if let Ok(mut lru) = LRU_CACHE.lock() {
    lru.clear();
}
```

**`hide_model_webview`（销毁模式）中清理 LRU**：
```rust
lru_remove(&model_id);
```

### 2.4 WebView 创建并发保护

**文件**：`commands.rs`

```rust
use tokio::sync::Mutex as TokioMutex;

static WEBVIEW_CREATE_MUTEX: TokioMutex<()> = TokioMutex::const_new(());
```

**在 `get_or_create_model_webview` 中**：
```rust
// 快速路径：WebView 已存在（无锁检查）
for w in &window.webviews() {
    if w.label() == label {
        lru_touch(&model_id);
        resize_webview(&window, &label, x, y, width, height)?;
        return Ok(false);
    }
}

// 慢路径：需要创建
let _guard = WEBVIEW_CREATE_MUTEX.lock().await;

// 双重检查：锁内再次确认
for w in &window.webviews() {
    if w.label() == label {
        lru_touch(&model_id);
        resize_webview(&window, &label, x, y, width, height)?;
        return Ok(false);  // 另一个任务已创建
    }
}

// 确认不存在，创建 WebView...
```

### 2.5 删除模型时清理 WebView

**Rust 命令**（`commands.rs`）：
```rust
#[tauri::command]
pub async fn remove_model_webview(app: AppHandle, model_id: String) -> Result<(), String> {
    let window = app.get_webview_window("main").ok_or("主窗口未找到")?;
    let label = format!("webview-{}", model_id);

    // 销毁 WebView
    for w in &window.webviews() {
        if w.label() == label {
            let _ = w.close();
            break;
        }
    }

    // 从 LRU 移除
    lru_remove(&model_id);

    Ok(())
}
```

**注册**（`lib.rs`）：
```rust
commands::remove_model_webview,
```

**前端 API**（`tauri-api.ts`）：
```typescript
removeModelWebview: (modelId: string) =>
    invoke('remove_model_webview', { modelId }),
```

**前端联动**（`App.vue`）：
```typescript
// 在 onMounted 的 config:updated 监听中
const newConfig = await tauriAPI.getConfig()
const currentModelIds = new Set(newConfig.models.map(m => m.id))
for (const cachedId of loadedModels.value) {
    if (!currentModelIds.has(cachedId)) {
        await tauriAPI.removeModelWebview(cachedId)
        loadedModels.value.delete(cachedId)
        if (activeModel.value?.id === cachedId) {
            activeModel.value = null
        }
    }
}
```

## 3. 错误处理

| 场景 | 处理 |
|------|------|
| LRU 锁中毒（`Mutex::lock` 失败） | 跳过 LRU 操作，不影响主流程 |
| WebView 关闭失败（`w.close()` 返回错误） | 忽略，WebView 可能已被销毁 |
| `tokio::fs::read_to_string` 文件不存在 | 返回默认配置 `AppConfig::default()` |
| `tokio::fs::write` 权限不足 | 返回错误字符串，前端显示 ElMessage.error |
| 淘汰的 WebView 是当前活跃模型 | 显示欢迎页面，用户重新点击即可 |

## 4. 依赖关系与实施顺序

```
┌─────────────────────────────────────────────────────┐
│ 实施顺序（拓扑排序）                                  │
├─────────────────────────────────────────────────────┤
│ 1. 全局 HTTP Client (lib.rs + commands.rs + config) │
│    ↓                                                │
│ 2. 配置异步 I/O (config.rs → commands.rs)           │
│    ↓                                                │
│ 3. LRU 缓存淘汰 (commands.rs)  ← 可与 1 并行       │
│    ↓                                                │
│ 4. WebView 并发保护 (commands.rs)                   │
│    ↓                                                │
│ 5. 删除模型清理 (commands.rs + lib + ts + vue)      │
│    ↓                                                │
│ 6. 编译验证                                          │
└─────────────────────────────────────────────────────┘
```

## 5. 测试策略

| 测试类型 | 方法 | 验证点 |
|----------|------|--------|
| 编译验证 | `cargo check` + `pnpm run build` | 无编译错误 |
| 功能验证 | `pnpm tauri dev` 手动操作 | 切换模型正常、删除模型后 WebView 清理、设置面板更新正常 |
| 内存验证 | 任务管理器观察 | 连续切换 10+ 模型后内存 < 1.5GB |
| 超时验证 | 断网后点击"检查更新" | 30s 内返回错误提示 |
| 并发验证 | 快速连续点击不同模型 | 无 WebView 标签冲突、无界面混乱 |
