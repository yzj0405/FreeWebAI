---
change: fix-runtime-stability
design-doc: docs/superpowers/specs/2026-07-02-fix-runtime-stability-design.md
base-ref: 2ff7518c5c93694ba856b728ed74ef4d000f5930
---

# 实施计划：fix-runtime-stability

## 概述

基于 Design Doc 的 5 项修复，按依赖顺序拆分为 6 组任务。

## 任务清单

### Task 1: 全局共享 HTTP Client
**文件**：`src-tauri/src/lib.rs`、`src-tauri/src/commands.rs`
**依赖**：无
**变更**：
1. 在 `lib.rs` 中新增 `pub struct ReqwestClient(pub reqwest::Client)`
2. 在 `setup()` 中创建带 30s 超时的 Client，`app.manage(ReqwestClient(client))`
3. 在 `commands.rs` 中修改 `sync_from_cloud`、`check_for_update`、`download_update` 从 `app.state::<ReqwestClient>()` 获取 Client
4. `check_model_url` 保留独立创建（5s 超时）

### Task 2: 配置文件异步 I/O
**文件**：`src-tauri/src/config.rs`、`src-tauri/src/commands.rs`
**依赖**：Task 1（config.rs 中 sync_from_cloud 需要 Client 参数）
**变更**：
1. 将 `read_config` 改为 `async fn`，使用 `tokio::fs::read_to_string`
2. 将 `write_config` 改为 `async fn`，使用 `tokio::fs::write`
3. 将 `init_config` 拆分为同步 `ensure_config_exists` + 异步初始化
4. 更新所有调用点添加 `.await`

### Task 3: LRU 缓存淘汰
**文件**：`src-tauri/src/commands.rs`
**依赖**：无（可与 Task 1 并行）
**变更**：
1. 引入 `static LRU_CACHE: Lazy<Mutex<VecDeque<String>>>` 和 `const MAX_CACHED_WEBVIEWS: usize = 5`
2. 实现 `lru_touch`、`lru_remove`、`lru_pop_back` 辅助函数
3. 在 `get_or_create_model_webview` 创建后执行 LRU 插入 + 淘汰
4. 在 `switch_to_model` 中调用 `lru_touch`
5. 在 `clear_all_model_webviews` 和 `hide_model_webview` 中清理 LRU

### Task 4: WebView 创建并发保护
**文件**：`src-tauri/src/commands.rs`
**依赖**：Task 3
**变更**：
1. 引入 `static WEBVIEW_CREATE_MUTEX: tokio::sync::Mutex<()>`
2. 在 `get_or_create_model_webview` 的创建路径中获取锁 + 双重检查

### Task 5: 删除模型时清理 WebView
**文件**：`src-tauri/src/commands.rs`、`src-tauri/src/lib.rs`、`src/utils/tauri-api.ts`、`src/App.vue`
**依赖**：Task 3 + Task 4
**变更**：
1. 新增 `remove_model_webview` 命令
2. 注册到 `lib.rs`
3. 添加前端 API
4. 在 `App.vue` 的 `config:updated` 监听中检测并清理被删模型

### Task 6: 编译验证
**依赖**：全部
**验证**：
1. `cargo check` 通过
2. `pnpm run build` 通过
3. `pnpm tauri dev` 启动正常

## 执行顺序

```
Task 1 ──→ Task 2 ──→ Task 5 ──→ Task 6
                    ↗
Task 3 ──→ Task 4 ─┘
```
