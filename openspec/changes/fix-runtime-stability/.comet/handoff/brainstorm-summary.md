# Brainstorm Summary

- Change: fix-runtime-stability
- Date: 2026-07-02

## 确认的技术方案

5 项系统性修复，消除 FreeWebAI Desktop 所有已知卡死路径：

1. **LRU 缓存淘汰**：`static LRU_CACHE: Mutex<VecDeque<String>>`，上限 5 个 WebView，超出时 `pop_back()` + `w.close()` 销毁最久未使用的
2. **全局共享 HTTP Client**：`app.manage(ReqwestClient(client))` 注入单例，所有命令通过 `app.state::<ReqwestClient>()` 获取
3. **配置异步 I/O**：`read_config`/`write_config` 改为 async，使用 `tokio::fs::read_to_string`/`tokio::fs::write`
4. **WebView 创建并发保护**：`static WEBVIEW_CREATE_MUTEX: tokio::sync::Mutex<()>` + 双重检查锁定
5. **删除模型清理 WebView**：新增 `remove_model_webview` 命令，前端 `config:updated` 事件中检测并清理被删模型

## 关键取舍与风险

| 取舍 | 决策 |
|------|------|
| LRU 上限 5 vs 3 vs 不限 | 选择 5，平衡内存（~1GB）和用户体验 |
| tokio::fs vs spawn_blocking | 选择 tokio::fs，更简洁，配置文件极小无性能差异 |
| tokio::sync::Mutex vs std::sync::Mutex | 选择 tokio，可在 async 函数中 `.await` |
| 淘汰当前活跃模型 | 淘汰后显示欢迎页面，用户重新点击即重新加载 |

## 测试策略

- 编译验证：`cargo check` + `pnpm run build`
- 功能验证：`pnpm tauri dev` 手动测试切换模型、删除模型、长时间运行
- 压力测试：快速连续切换 10+ 个模型，观察内存和响应性

## Spec Patch

无。OpenSpec delta spec 已包含完整验收场景。
