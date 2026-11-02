## Context

FreeWebAI Desktop 是基于 Tauri 2.x 的桌面应用，主窗口内嵌多个 WebView 实现多 AI 模型并行展示。当前架构存在以下约束：

- Tauri 的 `#[tauri::command]` async 函数运行在 Tokio 线程池上，同步 I/O 会阻塞 worker thread
- 每个 WebView 为独立的 Chromium 渲染进程，占用 50-200MB 内存
- 前端通过 `invoke()` 调用 Rust 命令，命令返回前前端 `await` 会一直等待
- 项目使用 `reqwest` 进行 HTTP 通信，`tauri-plugin-shell` 执行系统命令

## Goals / Non-Goals

**Goals:**
- 消除所有可能导致应用永久卡死的代码路径
- 控制长时间运行的内存增长在合理范围内（< 1.5GB）
- 提升网络请求和文件 I/O 的效率

**Non-Goals:**
- 不重构 WebView 多模型缓存的整体架构
- 不修改前端 UI 框架或组件库
- 不引入新的外部依赖

## Decisions

### Decision 1: WebView 缓存采用 LRU 淘汰策略，上限 5 个

**选择**：维护一个 LRU 队列（Rust 侧 `VecDeque<String>`），每次访问模型时将其移到队首，缓存数超过 5 时销毁队尾的 WebView。

**替代方案**：
- A. 不设上限，依赖用户手动清理 → 内存不可控，已被证明会导致卡死
- B. 固定上限 3 个 → 过于激进，用户频繁切换时体验差
- C. 基于内存阈值淘汰 → 实现复杂，需要跨平台内存监控

**理由**：5 个上限平衡了内存（~500MB-1GB）和用户体验（大多数用户同时不会用超过 3-5 个模型）。LRU 策略简单高效，淘汰最久未使用的模型符合用户直觉。

**实现**：在 `commands.rs` 中引入 `static LRU_CACHE: Mutex<VecDeque<String>>`，`get_or_create_model_webview` 中维护顺序，`switch_to_model` 时更新 LRU 位置。

### Decision 2: 全局共享 `reqwest::Client` 通过 `app.manage()` 注入

**选择**：在 `lib.rs` 的 `setup()` 中创建 `reqwest::Client`（带 30s 超时），通过 `app.manage()` 注入为全局状态。所有命令通过 `app.state::<ReqwestClient>()` 获取。

**替代方案**：
- A. 每次命令调用时创建 Client → 当前方案，浪费资源
- B. 模块级 `lazy_static!` → 不符合 Tauri 的状态管理模式

**理由**：`app.manage()` 是 Tauri 推荐的全局状态注入方式，与 Tauri 的生命周期管理一致。单例 Client 可复用连接池、TLS 会话缓存、DNS 缓存。

### Decision 3: `get_config`/`save_config` 改用 `tokio::fs`

**选择**：将 `config.rs` 中的 `read_config` 和 `write_config` 改为 async 函数，使用 `tokio::fs::read_to_string` 和 `tokio::fs::write`。所有调用点相应改为 `.await`。

**替代方案**：
- A. 使用 `tokio::task::spawn_blocking` 包装同步调用 → 可行但代码更复杂
- B. 保持同步，接受偶尔的线程池阻塞 → 已被证明会导致卡死

**理由**：直接改用 `tokio::fs` 最简洁，与项目中 `download_update` 已使用的模式一致。

### Decision 4: WebView 创建使用 `tokio::sync::Mutex` 保护

**选择**：引入 `static WEBVIEW_MUTEX: tokio::sync::Mutex<()>`，在 `get_or_create_model_webview` 的创建路径中获取锁，确保同一时间只有一个 WebView 在创建。

**替代方案**：
- A. 无保护 → 可能创建重复 label 的 WebView，导致未定义行为
- B. `std::sync::Mutex` → 不能在 async 函数中 `.lock().await`

**理由**：WebView 创建是低频操作（仅首次加载模型时），锁竞争极小，不会成为性能瓶颈。

## Risks / Trade-offs

| 风险 | 影响 | 缓解措施 |
|------|------|----------|
| LRU 淘汰导致用户切回已淘汰模型时需要重新加载 | 用户体验略降 | 仅在缓存数超过 5 时淘汰，正常使用不会触发 |
| `tokio::fs` 在某些平台可能比 `std::fs` 慢 | 配置文件极小（<1KB），差异可忽略 | 无 |
| 全局 Mutex 可能在极端并发下成为瓶颈 | WebView 创建是低频操作，实际不会并发 | 使用 `tokio::sync::Mutex` 而非 `std::sync::Mutex` |
| `reqwest::Client` 单例在连接异常后可能需要重建 | 极少见 | reqwest 内部有连接重试机制 |
