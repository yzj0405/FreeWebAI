## Why

FreeWebAI Desktop 在长时间使用过程中会不定时卡死（无响应、无法切换显示）。经深度分析，根因包括：HTTP 请求无超时可永久挂起、同步文件 I/O 阻塞 Tokio 线程池、WebView 实例只隐藏不销毁导致内存无限膨胀、WebView 加载失败无处理导致永久 loading、前端快速操作引发竞态条件。虽然本对话中已实施了部分 P0 修复（超时、tokio::fs、load-failed 事件、竞态保护），但仍有 5 个关键问题未解决，需要系统性修复以彻底消除卡死风险。

## What Changes

- **WebView 缓存上限 + LRU 淘汰**：限制同时缓存的 WebView 数量（建议 5 个），超出时自动销毁最久未使用的 WebView，控制内存在 1GB 以内
- **全局共享 `reqwest::Client`**：通过 `app.manage()` 注入单例 Client，所有 HTTP 命令复用连接池和 TLS 缓存，避免反复创建
- **删除模型时清理 WebView**：设置中删除模型时，同步销毁对应的缓存 WebView，防止孤立实例累积
- **`get_config`/`save_config` 改用 `tokio::fs`**：消除配置读写对 Tokio 线程池的阻塞
- **WebView 创建并发保护**：引入互斥锁，防止对同一模型的并发创建导致标签冲突

## Capabilities

### New Capabilities
- `webview-cache-lifecycle`: WebView 缓存的生命周期管理（上限、LRU 淘汰、模型删除时清理）
- `shared-http-client`: 全局共享 HTTP 客户端的注入与使用
- `async-config-io`: 配置文件的异步 I/O 读写

### Modified Capabilities

（无现有 spec 需要修改）

## Impact

- **Rust 后端**：`commands.rs`（WebView 管理、HTTP 客户端、命令处理）、`config.rs`（异步 I/O）、`lib.rs`（Client 注入）
- **Vue 前端**：`App.vue`（模型删除时清理 WebView）、`Settings.vue`（删除模型事件传递）
- **依赖**：无新增依赖，复用现有 `tokio`、`reqwest`
- **API**：无 Breaking Change，仅内部实现优化
