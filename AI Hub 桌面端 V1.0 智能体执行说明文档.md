AI Hub 桌面端 V1.0 智能体执行说明文档

1. 🎯 核心开发目标

本阶段目标是基于 **Electron 30+**、**Vue 3** 和 **Element Plus** 构建一个超低内存占用的多大模型网页端整合桌面客户端。支持用户通过官方网页免 API 额度使用，并具备弹性的云端配置同步能力。

------

2. 🗂 目录树与关键架构设计

text

```
├── src
│   ├── main
│   │   ├── index.js          # 主进程入口：窗口控制、WebContentsView 生命周期管理
│   │   └── configManager.js  # 本地文件读取、网络同步、覆盖算法
│   └── renderer
│       └── src
│           ├── App.vue       # 主框架：无边框窗口顶栏、左侧菜单、视图挂载区
│           └── components
│               └── Settings.vue # 弹窗组件：模型勾选显隐、服务器同步配置
```

请谨慎使用此类代码。

核心技术选型原因

- **WebContentsView (取代 webview)**：直接由主进程调用底层的渲染机制，能够更细粒度地控制网络拦截、静音和挂起。通过控制其 `setBounds` 与主窗口的绑定，实现**原生级、零内存泄漏**的平滑切换。
- **无边框窗口 (Frame-less)**：为了在右上角最小化/关闭按钮左侧嵌入“设置”图标，软件必须采用 `frame: false`，由 Vue 渲染一套自定义的顶部控制栏（TitleBar）。

------

3. 🤖 智能体模块开发执行指令（分步实现）

🛑 步骤 1：本地配置与云端同步中心 (`configManager.js`)

**智能体任务**：编写主进程配置管理逻辑。本地文件路径使用 `app.getPath('userData')/model_config.json`。

1. 数据结构规范 (`model_config.json`)

json

```
{
  "serverUrl": "https://yourdomain.com",
  "models": [
    { "id": "deepseek", "name": "DeepSeek", "url": "https://deepseek.com", "visible": true, "version": 1 },
    { "id": "gemini", "name": "Gemini", "url": "https://google.com", "visible": true, "version": 1 },
    { "id": "xiaoyi", "name": "华为小艺", "url": "https://huawei.com", "visible": false, "version": 2 }
  ]
}
```

请谨慎使用此类代码。

2. 云端增量同步算法逻辑（核心要点）

- 当触发同步时，请求 `serverUrl` 获取服务器最新的模型数组。
- **以服务器为底、覆盖当前参数**：
  - 如果本地有该 `id`，保留本地的 `visible`（显隐）状态，但强行覆盖 `name`、`url` 和 `version`。
  - 如果服务器新增了本地没有的 `id`，则直接作为**增量**插入本地数组，默认 `visible: true`。
  - 如果服务器删除了某 `id`，本地对应的模型也予以剔除。

------

🛑 步骤 2：主进程视图操控与生命周期 (`main/index.js`)

**智能体任务**：管理 `BaseWindow` 以及多个 `WebContentsView` 的显示、隐藏与销毁。

1. 初始化窗口与无边框设置

javascript

```
const { app, BaseWindow, WebContentsView, ipcMain } = require('electron')

let mainWindow
let views = {} // 存储所有已实例化的 WebContentsView

function createWindow() {
  mainWindow = new BaseWindow({
    width: 1400,
    height: 900,
    frame: false, // 启用无边框，交由前端渲染顶栏
    titleBarStyle: 'hidden'
  })
  // 加载 Vue 前端渲染进程
  mainWindow.loadURL('http://localhost:5173') 
}
```

请谨慎使用此类代码。

2. WebContentsView 动态管理与切换逻辑

- **延迟加载**：不要在启动时一次性把所有大模型全跑起来。只有当用户在左侧列表**点击该模型时，再去 `new WebContentsView()` 实例化**并加载 `url`。
- **独立沙盒**：实例化时必须指定 `partition: 'persist:' + id`，隔离账户登录状态。
- **低压力切换 (Bounds 控制法)**：
  - 当前激活模型：调用 `view.setBounds({ x: 200, y: 60, width: 1200, height: 840 })` 并 `mainWindow.contentView.addChildView(view)`。
  - 切换到其他模型时：**不要销毁它**。执行 `mainWindow.contentView.removeChildView(view)`（使其在后台挂起，保留内存和登录态），或者将其 `setBounds` 移出屏幕可视区（如 `width: 0, height: 0`）。

------

🛑 步骤 3：前端高级 UI 与操作映射 (`App.vue` & `Settings.vue`)

**智能体任务**：使用 Element Plus 渲染高级交互，并通过 `ipcRenderer` 与主进程进行频繁通信。

1. 自定义顶栏布局（右上角精细化控制）

前端的顶栏（Height: 60px）使用 Flex 布局。右侧控制区排列如下：

- `[ 设置按钮 (el-button) ]` `[ 最小化 ➖ ]` `[ 最大化 🔲 ]` `[ 关闭 ❌ ]`
- 顶栏需设置 CSS 样式 `-webkit-app-region: drag;` 允许拖动，但按钮部分需显式设为 `no-drag`。
- 设置弹窗（模型管理与云同步）

- 弹窗内使用 `el-checkbox-group` 对所有从主进程读取过来的模型进行显隐勾选。
- 变更勾选后，立即触发 `ipcRenderer.send('save-config', data)` 写入本地文件。
- 增加一个 **“检查更新/云端同步”** 按钮。点击后，展示 `el-loading` 状态，发送指令给主进程请求服务器，完成后接收最新数据刷新渲染进程的 List。

------

4. 🔏 核心安全与防检测指令 (防被 AI 厂商拦截)

由于使用 `WebContentsView` 访问的是官方免登录/登录页，智能体在生成主进程代码时，必须对每个生成的 View 注入以下安全设置：

1. **伪装 User-Agent**：统一将 UserAgent 改为标准最新的桌面级 Chrome，防止被判定为自动化脚本或 Electron 爬虫：

   javascript

   ```
   view.webContents.setUserAgent('Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/124.0.0.0 Safari/537.36')
   ```

   请谨慎使用此类代码。

2. **关闭安全上下文警告**：关闭 `nodeIntegration`，开启 `contextIsolation`，严格保护外部网页无法读取用户的本地文件系统。

------