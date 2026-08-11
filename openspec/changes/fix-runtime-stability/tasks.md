## 1. 全局共享 HTTP 客户端

- [x] 1.1 在 `lib.rs` 中创建 `ReqwestClient` 包装结构体，`setup()` 中初始化带 30s 超时的 `reqwest::Client`，通过 `app.manage()` 注入
- [x] 1.2 修改 `commands.rs` 中所有 `reqwest::Client::new()` 调用点（`sync_from_cloud`、`check_for_update`、`download_update`）改为从 `app.state::<ReqwestClient>()` 获取
- [x] 1.3 修改 `config.rs` 中 `sync_from_cloud` 的 `reqwest::Client` 创建改为接收外部传入的 Client

## 2. 配置文件异步 I/O

- [x] 2.1 将 `config.rs` 中 `read_config` 改为 async 函数，使用 `tokio::fs::read_to_string`
- [x] 2.2 将 `config.rs` 中 `write_config` 改为 async 函数，使用 `tokio::fs::write`
- [x] 2.3 更新所有调用点（`get_config`、`save_config`、`toggle_sidebar`、`init_config`、`sync_from_cloud`）添加 `.await`

## 3. WebView 缓存 LRU 淘汰

- [x] 3.1 在 `commands.rs` 中引入 `static LRU_CACHE: std::sync::Mutex<VecDeque<String>>` 和 `const MAX_CACHED_WEBVIEWS: usize = 5`
- [x] 3.2 在 `get_or_create_model_webview` 创建新 WebView 前，检查缓存数是否超限，超限时从队尾取出最久未使用的 label 并 `w.close()` 销毁
- [x] 3.3 在 `switch_to_model` 中更新 LRU 位置（将目标模型移到队首）
- [x] 3.4 在 `clear_all_model_webviews` 中清空 LRU 队列

## 4. WebView 创建并发保护

- [x] 4.1 在 `commands.rs` 中引入 `static WEBVIEW_CREATE_MUTEX: tokio::sync::Mutex<()>`
- [x] 4.2 在 `get_or_create_model_webview` 的创建路径（缓存未命中时）获取锁后再做一次存在性检查（双重检查锁定）

## 5. 删除模型时清理 WebView

- [x] 5.1 在 `commands.rs` 中新增 `remove_model_webview(model_id)` 命令，销毁指定模型的 WebView 并从 LRU 队列移除
- [x] 5.2 在 `lib.rs` 中注册 `remove_model_webview` 命令
- [x] 5.3 在 `tauri-api.ts` 中添加 `removeModelWebview` API
- [x] 5.4 在 `App.vue` 中 `closeSettings` 时检测被删除的模型并调用 `removeModelWebview` 清理

## 6. 编译验证

- [x] 6.1 `cargo check` 通过
- [x] 6.2 `pnpm run build` 通过
- [ ] 6.3 `pnpm tauri dev` 启动正常，切换模型无卡死
