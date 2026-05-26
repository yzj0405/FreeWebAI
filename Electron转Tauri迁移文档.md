# AI Hub Desktop - Electron 转 Tauri 完整迁移文档

## 📋 目录

- [项目概述](#项目概述)
- [技术栈对比](#技术栈对比)
- [架构差异分析](#架构差异分析)
- [核心功能清单](#核心功能清单)
- [迁移实施步骤](#迁移实施步骤)
- [代码迁移对照表](#代码迁移对照表)
- [配置文件迁移](#配置文件迁移)
- [IPC 通信改造](#ipc-通信改造)
- [窗口与视图管理](#窗口与视图管理)
- [主题系统迁移](#主题系统迁移)
- [配置管理迁移](#配置管理迁移)
- [构建与打包](#构建与打包)
- [常见问题与解决方案](#常见问题与解决方案)
- [测试验证清单](#测试验证清单)

---

## 项目概述

**AI Hub Desktop** 是一个多模型网页端整合桌面客户端，核心功能包括：

1. **多模型切换**：集成 18+ 主流 AI 模型（ChatGPT、Gemini、Claude、DeepSeek 等）
2. **WebContentsView 嵌入**：使用 Electron 的 WebContentsView 在桌面内嵌网页
3. **自定义窗口**：无边框窗口 + 自定义标题栏
4. **主题切换**：浅色/深色/跟随系统三态主题
5. **配置管理**：本地 JSON 配置 + 云端同步
6. **模型管理**：增删改查、拖拽排序、自动检测可用性
7. **侧边栏折叠**：可收起/展开的模型列表

---

## 技术栈对比

| 维度 | Electron (当前) | Tauri (目标) |
|------|----------------|-------------|
| **主进程语言** | JavaScript (Node.js) | Rust |
| **渲染进程** | Vue 3 + Element Plus | Vue 3 + Element Plus (保持不变) |
| **构建工具** | Vite + electron-builder | Vite + tauri-cli |
| **打包体积** | ~150-200MB | ~10-20MB |
| **内存占用** | ~200-400MB | ~50-100MB |
| **安全性** | contextIsolation + preload | 默认安全隔离 |
| **窗口嵌入** | WebContentsView | WebView2 (Windows) / WebKit (macOS/Linux) |
| **IPC 通信** | ipcMain/ipcRenderer | Tauri Commands + Events |
| **文件系统** | Node.js fs 模块 | Rust std::fs + tauri::api::file |
| **网络请求** | Node.js fetch | Rust reqwest |
| **配置存储** | app.getPath('userData') | tauri::api::path::app_data_dir |

---

## 架构差异分析

### Electron 架构
```
┌─────────────────────────────────────┐
│         Main Process (Node.js)       │
│  ├─ index.js (窗口管理、IPC)         │
│  ├─ configManager.js (配置读写)      │
│  └─ preload.js (安全桥接)            │
└──────────────┬──────────────────────┘
               │ IPC (invoke/send/on)
┌──────────────▼──────────────────────┐
│      Renderer Process (Vue 3)        │
│  ├─ App.vue (主界面)                 │
│  ├─ Settings.vue (设置弹窗)          │
│  └─ WebContentsView (嵌入网页)       │
└─────────────────────────────────────┘
```

### Tauri 架构
```
┌─────────────────────────────────────┐
│         Rust Backend                 │
│  ├─ main.rs (应用入口)               │
│  ├─ lib.rs (Commands + Events)       │
│  ├─ config.rs (配置管理)             │
│  └─ window.rs (窗口管理)             │
└──────────────┬──────────────────────┘
               │ Tauri API
┌──────────────▼──────────────────────┐
│      WebView (Vue 3)                 │
│  ├─ App.vue (主界面)                 │
│  ├─ Settings.vue (设置弹窗)          │
│  └─ WebView (嵌入网页 - 受限)        │
└─────────────────────────────────────┘
```

### ⚠️ 核心差异

1. **WebContentsView vs WebView**
   - Electron: `WebContentsView` 支持完整的 Chromium 特性
   - Tauri: 使用系统 WebView (Windows: WebView2, macOS: WebKit)
   - **影响**: 某些 Chromium 特有 API 不可用

2. **多视图管理**
   - Electron: 可同时创建多个 `WebContentsView` 并切换
   - Tauri: 一个窗口只能有一个 WebView
   - **解决方案**: 使用多窗口或动态切换 URL

3. **User-Agent 伪装**
   - Electron: `view.webContents.setUserAgent()`
   - Tauri: 需在 WebView 初始化时设置，部分平台有限制

---

## 核心功能清单

### ✅ 可直接迁移的功能

| 功能 | Electron 实现 | Tauri 实现 | 难度 |
|------|--------------|-----------|------|
| 配置读写 | `fs.readFileSync/writeFileSync` | `std::fs` + `serde_json` | ⭐ |
| 云端同步 | `fetch()` | `reqwest` crate | ⭐ |
| 主题切换 | `nativeTheme.themeSource` | CSS 变量 + 系统主题检测 | ⭐⭐ |
| 窗口控制 | `minimize/maximize/close` | `tauri::Window` API | ⭐ |
| IPC 通信 | `ipcMain.invoke/on` | `#[tauri::command]` | ⭐⭐ |
| 模型管理 | 纯前端逻辑 | 保持不变 | ✅ |
| 拖拽排序 | HTML5 Drag API | 保持不变 | ✅ |

### ⚠️ 需要改造的功能

| 功能 | 挑战 | 解决方案 | 难度 |
|------|------|---------|------|
| **多模型视图切换** | Tauri 单 WebView 限制 | 方案 A: 动态切换 URL<br>方案 B: 多窗口 | ⭐⭐⭐⭐ |
| **User-Agent 伪装** | WebView 初始化后不可改 | 初始化时设置，或每窗口独立 | ⭐⭐⭐ |
| **加载状态监听** | `did-finish-load` 事件 | WebView `on_page_load` 事件 | ⭐⭐ |
| **错误页处理** | `did-fail-load` | WebView `on_navigation` 事件 | ⭐⭐ |
| **视图显示/隐藏** | `addChildView/removeChildView` | 设置 WebView 尺寸或导航到空白页 | ⭐⭐ |

### ❌ 需要重新设计的功能

| 功能 | 原因 | 新方案 |
|------|------|--------|
| **同时保持多个模型会话** | Tauri 单窗口限制 | 使用持久化 Cookie/LocalStorage 或标签页式多窗口 |
| **视图挂起/恢复** | WebView 无法真正挂起 | 导航到 `about:blank` 或隐藏 WebView |

---

## 迁移实施步骤

### Phase 1: 环境准备 (1-2 天)

#### 1.1 安装 Tauri 依赖

```bash
# Windows (需要 Visual Studio Build Tools)
# 安装 Visual Studio 2022，选择 "C++ 桌面开发"

# 安装 Rust
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh

# 安装 Tauri CLI
cargo install tauri-cli
```

#### 1.2 初始化 Tauri 项目

```bash
# 在现有项目根目录
npm install --save-dev @tauri-apps/cli

# 初始化 Tauri
npm run tauri init

# 配置提示:
# - distDir: ../dist/renderer
# - devPath: http://localhost:5173
# - beforeDevCommand: npm run build:renderer
# - beforeBuildCommand: npm run build:renderer
```

#### 1.3 修改 package.json

```json
{
  "scripts": {
    "tauri": "tauri",
    "dev": "concurrently \"vite\" \"wait-on http://localhost:5173 && tauri dev\"",
    "build:renderer": "vite build",
    "build": "npm run build:renderer && tauri build"
  },
  "devDependencies": {
    "@tauri-apps/cli": "^1.5.0"
  }
}
```

---

### Phase 2: 后端迁移 (3-5 天)

#### 2.1 项目结构

```
src-tauri/
├── Cargo.toml
├── tauri.conf.json
├── build.rs
├── src/
│   ├── main.rs           # 应用入口
│   ├── lib.rs            # Commands + Events
│   ├── config.rs         # 配置管理
│   ├── window.rs         # 窗口管理
│   ├── models.rs         # 模型数据结构
│   └── sync.rs           # 云端同步
└── icons/
    └── icon.ico
```

#### 2.2 Cargo.toml 配置

```toml
[package]
name = "ai-hub-desktop"
version = "1.0.0"
description = "AI Hub Desktop - 多大模型网页端整合桌面客户端"
authors = ["AI Hub"]
edition = "2021"

[build-dependencies]
tauri-build = { version = "1.5", features = [] }

[dependencies]
tauri = { version = "1.5", features = ["shell-open", "window-all"] }
serde = { version = "1.0", features = ["derive"] }
serde_json = "1.0"
reqwest = { version = "0.11", features = ["json"] }
tokio = { version = "1", features = ["full"] }
dirs = "5.0"

[features]
custom-protocol = ["tauri/custom-protocol"]
```

#### 2.3 tauri.conf.json 配置

```json
{
  "build": {
    "beforeDevCommand": "npm run dev:renderer",
    "beforeBuildCommand": "npm run build:renderer",
    "devPath": "http://localhost:5173",
    "distDir": "../dist/renderer"
  },
  "package": {
    "productName": "AI Hub Desktop",
    "version": "1.0.0"
  },
  "tauri": {
    "allowlist": {
      "all": false,
      "shell": {
        "open": true
      },
      "window": {
        "all": true
      },
      "fs": {
        "readFile": true,
        "writeFile": true,
        "scope": ["$APPDATA/*"]
      },
      "path": {
        "all": true
      }
    },
    "windows": [
      {
        "title": "AI Hub Desktop",
        "width": 1400,
        "height": 900,
        "minWidth": 800,
        "minHeight": 600,
        "decorations": false,
        "transparent": true
      }
    ],
    "security": {
      "csp": "default-src 'self'; style-src 'self' 'unsafe-inline'; script-src 'self'"
    },
    "bundle": {
      "active": true,
      "targets": "all",
      "identifier": "com.aihub.desktop",
      "icon": [
        "icons/32x32.png",
        "icons/128x128.png",
        "icons/128x128@2x.png",
        "icons/icon.icns",
        "icons/icon.ico"
      ],
      "windows": {
        "certificateThumbprint": null,
        "digestAlgorithm": "sha256",
        "timestampUrl": ""
      }
    }
  }
}
```

#### 2.4 主入口 main.rs

```rust
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod config;
mod sync;
mod commands;

fn main() {
    tauri::Builder::default()
        .invoke_handler(tauri::generate_handler![
            commands::get_config,
            commands::save_config,
            commands::sync_from_cloud,
            commands::check_model_url,
        ])
        .setup(|app| {
            // 初始化配置
            config::init_config(app)?;
            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
```

#### 2.5 配置管理 config.rs

```rust
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;
use tauri::api::path::app_data_dir;
use tauri::AppHandle;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppConfig {
    pub theme: String,
    #[serde(rename = "serverUrl")]
    pub server_url: String,
    pub models: Vec<Model>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Model {
    pub id: String,
    pub name: String,
    pub url: String,
    pub version: String,
    pub visible: bool,
}

pub fn get_config_path(app: &AppHandle) -> PathBuf {
    let app_data = app_data_dir(&app.config()).unwrap();
    app_data.join("model_config.json")
}

pub fn get_default_config() -> AppConfig {
    AppConfig {
        theme: "system".to_string(),
        server_url: String::new(),
        models: vec![
            // 这里放入默认模型列表，与 configManager.js 一致
            Model {
                id: "deepseek".to_string(),
                name: "DeepSeek".to_string(),
                url: "https://chat.deepseek.com/".to_string(),
                version: "V3 / R1".to_string(),
                visible: true,
            },
            // ... 其他模型
        ],
    }
}

pub fn read_config(app: &AppHandle) -> AppConfig {
    let config_path = get_config_path(app);
    if !config_path.exists() {
        return get_default_config();
    }
    
    match fs::read_to_string(&config_path) {
        Ok(content) => {
            match serde_json::from_str(&content) {
                Ok(config) => config,
                Err(_) => get_default_config(),
            }
        }
        Err(_) => get_default_config(),
    }
}

pub fn write_config(app: &AppHandle, config: &AppConfig) -> Result<(), String> {
    let config_path = get_config_path(app);
    if let Some(parent) = config_path.parent() {
        fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    
    let content = serde_json::to_string_pretty(config)
        .map_err(|e| e.to_string())?;
    
    fs::write(&config_path, content)
        .map_err(|e| e.to_string())?;
    
    Ok(())
}

pub fn init_config(app: &AppHandle) -> Result<(), Box<dyn std::error::Error>> {
    let _ = read_config(app);
    Ok(())
}
```

#### 2.6 云端同步 sync.rs

```rust
use reqwest;
use serde_json;
use crate::config::{AppConfig, read_config, write_config};
use tauri::AppHandle;

pub async fn sync_from_cloud(
    app: &AppHandle,
    server_url: String,
) -> Result<AppConfig, String> {
    if server_url.is_empty() {
        return Err("服务器地址未配置".to_string());
    }

    let base_url = server_url.trim_end_matches('/');
    let fetch_url = if base_url.ends_with(".json") {
        base_url.to_string()
    } else {
        format!("{}/model_web.json", base_url)
    };

    let client = reqwest::Client::new();
    let response = client
        .get(&fetch_url)
        .header("Accept", "application/json")
        .send()
        .await
        .map_err(|e| format!("网络请求失败: {}", e))?;

    if !response.status().is_success() {
        return Err(format!("服务器响应错误 ({})", response.status()));
    }

    let server_data: serde_json::Value = response
        .json()
        .await
        .map_err(|_| "数据格式错误，无法解析 JSON".to_string())?;

    let server_models = server_data["models"]
        .as_array()
        .ok_or("数据格式错误: 缺少 models 数组".to_string())?;

    let local_config = read_config(app);
    let local_models = &local_config.models;

    let mut local_map = std::collections::HashMap::new();
    for m in local_models {
        local_map.insert(&m.id, m);
    }

    let mut merged_models = Vec::new();

    for server_model in server_models {
        let id = server_model["id"]
            .as_str()
            .ok_or("模型缺少 id".to_string())?
            .to_string();
        let name = server_model["name"]
            .as_str()
            .ok_or("模型缺少 name".to_string())?
            .to_string();
        let url = server_model["url"]
            .as_str()
            .ok_or("模型缺少 url".to_string())?
            .to_string();
        let version = server_model["version"]
            .as_str()
            .unwrap_or("")
            .to_string();
        let visible = server_model["visible"]
            .as_bool()
            .unwrap_or(true);

        let merged_model = if let Some(local_model) = local_map.get(&id) {
            crate::config::Model {
                id,
                name,
                url,
                version,
                visible: local_model.visible,
            }
        } else {
            crate::config::Model {
                id,
                name,
                url,
                version,
                visible,
            }
        };

        merged_models.push(merged_model);
    }

    let merged_config = AppConfig {
        theme: local_config.theme,
        server_url: server_url.clone(),
        models: merged_models,
    };

    write_config(app, &merged_config)?;

    Ok(merged_config)
}
```

#### 2.7 IPC Commands commands.rs

```rust
use tauri::{AppHandle, Manager};
use crate::config::{read_config, write_config, AppConfig};
use crate::sync;

#[derive(serde::Serialize)]
pub struct CommandResult {
    pub success: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub data: Option<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

#[tauri::command]
pub async fn get_config(app: AppHandle) -> Result<CommandResult, String> {
    let config = read_config(&app);
    Ok(CommandResult {
        success: true,
        data: Some(serde_json::to_value(config).map_err(|e| e.to_string())?),
        error: None,
    })
}

#[tauri::command]
pub async fn save_config(
    app: AppHandle,
    config: AppConfig,
    window: tauri::Window,
) -> Result<(), String> {
    write_config(&app, &config)?;
    
    // 通知前端配置已更新
    window.emit_all("config:updated", ()).ok();
    
    Ok(())
}

#[tauri::command]
pub async fn sync_from_cloud(
    app: AppHandle,
    server_url: String,
    window: tauri::Window,
) -> Result<CommandResult, String> {
    match sync::sync_from_cloud(&app, server_url).await {
        Ok(config) => {
            window.emit_all("config:updated", ()).ok();
            Ok(CommandResult {
                success: true,
                data: Some(serde_json::to_value(config).map_err(|e| e.to_string())?),
                error: None,
            })
        }
        Err(e) => Ok(CommandResult {
            success: false,
            data: None,
            error: Some(e),
        }),
    }
}

#[tauri::command]
pub async fn check_model_url(url: String) -> Result<serde_json::Value, String> {
    let client = reqwest::Client::new();
    
    match client.head(&url).send().await {
        Ok(response) => {
            let accessible = response.status().is_success() 
                || response.status().as_u16() < 500;
            Ok(serde_json::json!({
                "accessible": accessible
            }))
        }
        Err(e) => Ok(serde_json::json!({
            "accessible": false,
            "error": e.to_string()
        })),
    }
}
```

---

### Phase 3: 前端改造 (2-3 天)

#### 3.1 安装 Tauri API

```bash
npm install @tauri-apps/api
```

#### 3.2 替换 IPC 通信层

创建 `src/renderer/src/utils/tauri-api.js`:

```javascript
import { invoke } from '@tauri-apps/api/tauri'
import { listen } from '@tauri-apps/api/event'

// 兼容原有 electronAPI 接口
export const electronAPI = {
  ipcRenderer: {
    invoke: async (channel, ...args) => {
      // 映射到 Tauri commands
      switch (channel) {
        case 'config:get':
          return await invoke('get_config')
        case 'config:sync':
          return await invoke('sync_from_cloud', { serverUrl: args[0] })
        case 'model:check':
          return await invoke('check_model_url', { url: args[0] })
        default:
          throw new Error(`Unknown channel: ${channel}`)
      }
    },
    send: async (channel, ...args) => {
      switch (channel) {
        case 'config:save':
          await invoke('save_config', { config: args[0] })
          break
        case 'window:minimize':
          const { getCurrent } = await import('@tauri-apps/api/window')
          getCurrent().minimize()
          break
        case 'window:maximize':
          const { getCurrent: getCurrent2 } = await import('@tauri-apps/api/window')
          const win = getCurrent2()
          const isMax = await win.isMaximized()
          if (isMax) {
            win.unmaximize()
          } else {
            win.maximize()
          }
          break
        case 'window:close':
          const { getCurrent: getCurrent3 } = await import('@tauri-apps/api/window')
          getCurrent3().close()
          break
        case 'view:switch':
          await invoke('switch_view', { id: args[0].id, url: args[0].url })
          break
        case 'view:hide':
          await invoke('hide_view')
          break
        case 'view:show':
          await invoke('show_view')
          break
        case 'view:reload':
          await invoke('reload_view')
          break
        case 'view:reloadUrl':
          await invoke('reload_view_url', { url: args[0] })
          break
        case 'theme:set':
          await invoke('set_theme', { theme: args[0] })
          break
        case 'sidebar:toggle':
          await invoke('toggle_sidebar', { collapsed: args[0] })
          break
        default:
          console.warn(`Unknown channel: ${channel}`)
      }
    },
    on: (channel, func) => {
      listen(channel, (event) => {
        func(event.payload)
      })
    },
    removeListener: () => {
      // Tauri 的 listen 返回 unsubscribe 函数，需要自行管理
      console.warn('removeListener not fully supported in Tauri')
    }
  }
}

// 导出供全局使用
window.electronAPI = electronAPI
```

修改 `src/renderer/src/main.js`:

```javascript
import { createApp } from 'vue'
import ElementPlus from 'element-plus'
import 'element-plus/dist/index.css'
import * as ElementPlusIconsVue from '@element-plus/icons-vue'
import App from './App.vue'
import './assets/styles.css'
import './utils/tauri-api'  // 添加这行

const app = createApp(App)

for (const [key, component] of Object.entries(ElementPlusIconsVue)) {
  app.component(key, component)
}

app.use(ElementPlus)
app.mount('#app')
```

#### 3.3 App.vue 改造

主要变化：
1. 移除 WebContentsView 相关逻辑（由后端管理）
2. 使用 iframe 或 Tauri WebView 事件监听

由于 Tauri 限制，推荐使用以下方案：

**方案 A: 使用 iframe (简单但受限)**

```vue
<template>
  <div class="app-container">
    <!-- 顶栏和侧边栏保持不变 -->
    
    <!-- 内容区域 -->
    <div class="content-area" :class="{ expanded: sidebarCollapsed }">
      <!-- 加载中覆盖层 -->
      <div v-if="viewState === 'loading'" class="view-overlay">
        <div class="view-spinner"></div>
        <span class="view-status-text">正在加载...</span>
      </div>

      <!-- WebView 容器 -->
      <iframe
        v-if="activeModelUrl"
        :src="activeModelUrl"
        class="webview-container"
        @load="onWebViewLoad"
        @error="onWebViewError"
      ></iframe>

      <!-- 无模型选中 -->
      <div v-if="!activeModelId && viewState === 'idle'" class="welcome-screen">
        <div class="welcome-title">欢迎使用 AI Hub Desktop</div>
        <div class="welcome-desc">请从左侧选择一个模型开始使用</div>
      </div>
    </div>
  </div>
</template>

<script setup>
import { ref, computed, onMounted, watch } from 'vue'
import { Setting, Minus, Close, FullScreen, CopyDocument, Refresh, DArrowLeft, DArrowRight } from '@element-plus/icons-vue'
import { ElMessage } from 'element-plus'
import Settings from './components/Settings.vue'

// 使用 Tauri API
const { ipcRenderer } = window.electronAPI

const allModels = ref([])
const serverUrl = ref('')
const activeModelId = ref(null)
const activeModelUrl = ref('')  // 新增：当前模型 URL
const settingsVisible = ref(false)
const isMaximized = ref(false)
const theme = ref('system')
const viewState = ref('idle')
const viewError = ref('')
const sidebarCollapsed = ref(false)

function switchModel(model) {
  if (activeModelId.value === model.id) return

  viewState.value = 'loading'
  viewError.value = ''
  activeModelUrl.value = model.url  // 直接设置 iframe src
  activeModelId.value = model.id
}

function onWebViewLoad() {
  viewState.value = 'loaded'
}

function onWebViewError() {
  viewState.value = 'error'
  viewError.value = '加载失败'
}

// ... 其他逻辑保持不变
</script>
```

**方案 B: 使用 Tauri 原生 WebView (推荐)**

需要在后端管理 WebView 状态，前端通过事件监听。

---

### Phase 4: 窗口与视图管理 (2-3 天)

#### 4.1 窗口管理 window.rs

```rust
use tauri::{AppHandle, Manager, Window};

#[tauri::command]
pub fn minimize_window(window: Window) {
    window.minimize().ok();
}

#[tauri::command]
pub fn toggle_maximize(window: Window) {
    if let Ok(is_maximized) = window.is_maximized() {
        if is_maximized {
            window.unmaximize().ok();
        } else {
            window.maximize().ok();
        }
    }
}

#[tauri::command]
pub fn close_window(window: Window) {
    window.close().ok();
}
```

#### 4.2 视图切换方案

由于 Tauri 限制，推荐以下两种方案：

**方案 A: 单窗口动态 URL 切换**

```rust
use tauri::{AppHandle, Manager, Window};
use std::collections::HashMap;
use std::sync::Mutex;

pub struct ViewState {
    pub current_id: Option<String>,
    pub current_url: Option<String>,
    pub sidebar_collapsed: bool,
}

pub fn switch_view(app: &AppHandle, id: String, url: String) -> Result<(), String> {
    let window = app.get_window("main").ok_or("Main window not found")?;
    
    // 导航到新 URL
    window.eval(&format!("window.location.href = '{}';", url))
        .map_err(|e| e.to_string())?;
    
    // 更新状态
    let mut state = app.state::<Mutex<ViewState>>();
    let mut view_state = state.lock().unwrap();
    view_state.current_id = Some(id);
    view_state.current_url = Some(url);
    
    Ok(())
}
```

**方案 B: 多窗口方案 (更复杂但体验更好)**

```rust
use tauri::{AppHandle, Manager, WindowBuilder, Url};

pub fn create_model_window(
    app: &AppHandle,
    id: String,
    url: String,
) -> Result<(), String> {
    let window_label = format!("model_{}", id);
    
    // 检查窗口是否已存在
    if app.get_window(&window_label).is_some() {
        return Ok(());
    }
    
    WindowBuilder::new(app, &window_label)
        .title(format!("Model - {}", id))
        .url(Url::parse(&url).map_err(|e| e.to_string())?)
        .build()
        .map_err(|e| e.to_string())?;
    
    Ok(())
}
```

---

### Phase 5: 主题系统 (1 天)

#### 5.1 主题管理 theme.rs

```rust
use tauri::{AppHandle, Manager};

#[tauri::command]
pub fn set_theme(app: &AppHandle, theme: String) {
    // 发送事件到前端
    let window = app.get_window("main").unwrap();
    window.emit("theme:changed", &theme).ok();
}
```

#### 5.2 前端主题适配

在 `App.vue` 中监听主题变化：

```javascript
import { listen } from '@tauri-apps/api/event'

onMounted(async () => {
  await listen('theme:changed', (event) => {
    applyTheme(event.payload)
  })
})
```

---

### Phase 6: 构建与打包 (1-2 天)

#### 6.1 生成图标

```bash
# 准备 1024x1024 PNG 图标
# 放置在 src-tauri/icons/icon.png

# 自动生成所有尺寸
npm run tauri icon
```

#### 6.2 开发模式

```bash
npm run dev
```

#### 6.3 生产构建

```bash
npm run build
```

产物位置：`src-tauri/target/release/bundle/`

#### 6.4 跨平台构建

```bash
# Windows
npm run tauri build -- --target x86_64-pc-windows-msvc

# macOS
npm run tauri build -- --target x86_64-apple-darwin
npm run tauri build -- --target aarch64-apple-darwin

# Linux
npm run tauri build -- --target x86_64-unknown-linux-gnu
```

---

## 代码迁移对照表

### Electron → Tauri API 映射

| Electron API | Tauri API | 文件 |
|-------------|----------|------|
| `ipcMain.handle()` | `#[tauri::command]` | commands.rs |
| `ipcMain.on()` | `#[tauri::command]` + `window.emit()` | commands.rs |
| `mainWindow.webContents.send()` | `window.emit()` | commands.rs |
| `BrowserWindow` | `tauri::Window` | window.rs |
| `app.getPath('userData')` | `app_data_dir()` | config.rs |
| `fs.readFileSync()` | `std::fs::read_to_string()` | config.rs |
| `fs.writeFileSync()` | `std::fs::write()` | config.rs |
| `nativeTheme.themeSource` | 前端 CSS + 系统主题检测 | theme.rs |
| `WebContentsView` | `tauri::Window` 或 iframe | 需重构 |
| `view.webContents.loadURL()` | `window.eval()` 或导航 | window.rs |
| `view.webContents.setUserAgent()` | WebView 初始化时设置 | tauri.conf.json |
| `view.setBounds()` | `window.set_size()` | window.rs |
| `fetch()` (Node.js) | `reqwest` crate | sync.rs |

### Vue 组件变化

| 组件 | Electron | Tauri | 变化 |
|------|---------|-------|------|
| App.vue | 使用 WebContentsView | iframe 或后端管理 | ⚠️ 需改造 |
| Settings.vue | 通过 electronAPI 通信 | 通过 tauri-api 通信 | ✅ 微调 |
| main.js | 引入 electronAPI | 引入 tauri-api | ✅ 微调 |

---

## 配置文件迁移

### package.json 变化

```diff
{
  "scripts": {
-   "dev": "cross-env ... concurrently \"vite\" \"wait-on ... && electron .\"",
-   "build": "cross-env ... vite build && electron-builder",
+   "dev": "concurrently \"vite\" \"wait-on ... && tauri dev\"",
+   "build:renderer": "vite build",
+   "build": "npm run build:renderer && tauri build",
    "preview": "vite preview"
  },
  "devDependencies": {
-   "electron": "^30.0.0",
-   "electron-builder": "^24.13.0",
+   "@tauri-apps/cli": "^1.5.0",
    "vite": "^5.2.0",
    "@vitejs/plugin-vue": "^5.0.0",
    "concurrently": "^8.2.0",
    "wait-on": "^7.2.0",
    "cross-env": "^7.0.3"
  }
- "build": { ... electron-builder 配置 ... }
}
```

### vite.config.js 变化

```diff
export default defineConfig({
  base: './',
  plugins: [vue()],
  resolve: {
    alias: {
      '@': path.resolve(__dirname, 'src/renderer/src')
    }
  },
  build: {
-   outDir: 'dist/renderer'
+   outDir: '../dist/renderer'  // 相对于 src/renderer 目录
  },
  server: {
    port: 5173,
    strictPort: true
  }
})
```

---

## IPC 通信改造

### 请求-响应模式

**Electron:**
```javascript
// 前端
const result = await ipcRenderer.invoke('config:get')

// 后端
ipcMain.handle('config:get', () => {
  return { success: true, data: readConfig() }
})
```

**Tauri:**
```javascript
// 前端
import { invoke } from '@tauri-apps/api/tauri'
const result = await invoke('get_config')

// 后端
#[tauri::command]
pub async fn get_config(app: AppHandle) -> Result<CommandResult, String> {
  let config = read_config(&app);
  Ok(CommandResult { success: true, data: Some(serde_json::to_value(config)?), error: None })
}
```

### 事件监听模式

**Electron:**
```javascript
// 后端发送
mainWindow.webContents.send('config:updated')

// 前端监听
ipcRenderer.on('config:updated', () => {
  loadConfig()
})
```

**Tauri:**
```rust
// 后端发送
window.emit_all("config:updated", ()).ok();
```

```javascript
// 前端监听
import { listen } from '@tauri-apps/api/event'
await listen('config:updated', () => {
  loadConfig()
})
```

---

## 窗口与视图管理

### 自定义标题栏

Tauri 配置 `tauri.conf.json`:

```json
{
  "tauri": {
    "windows": [
      {
        "decorations": false,
        "transparent": true
      }
    ]
  }
}
```

前端保持不变，使用 `-webkit-app-region: drag`。

### 视图切换策略

由于 Tauri 的 WebView 限制，推荐以下方案：

**推荐方案: iframe 嵌入**

优点：
- 实现简单
- 保持原有逻辑
- 支持多会话（不同 iframe）

缺点：
- 部分网站可能拒绝 iframe 嵌入 (X-Frame-Options)
- 性能略低于原生 WebView

**备选方案: 多窗口**

优点：
- 完整 WebView 功能
- 支持 User-Agent 设置

缺点：
- 窗口管理复杂
- 用户体验不一致

---

## 主题系统迁移

### Electron 方案
```javascript
nativeTheme.themeSource = 'dark' // 'light' | 'dark' | 'system'
```

### Tauri 方案

纯前端实现：

```javascript
function applyTheme(theme) {
  if (theme === 'system') {
    const isDark = window.matchMedia('(prefers-color-scheme: dark)').matches
    document.documentElement.setAttribute('data-theme', isDark ? 'dark' : 'light')
  } else {
    document.documentElement.setAttribute('data-theme', theme)
  }
}

// 监听系统主题变化
window.matchMedia('(prefers-color-scheme: dark)').addEventListener('change', (e) => {
  if (currentTheme === 'system') {
    document.documentElement.setAttribute('data-theme', e.matches ? 'dark' : 'light')
  }
})
```

**注意**: Tauri 的 WebView 会自动跟随系统主题，无需额外处理。

---

## 配置管理迁移

### 路径变化

**Electron:**
```javascript
const path = require('path')
const { app } = require('electron')
const CONFIG_FILE = path.join(app.getPath('userData'), 'model_config.json')
```

**Tauri:**
```rust
use tauri::api::path::app_data_dir;

fn get_config_path(app: &AppHandle) -> PathBuf {
    let app_data = app_data_dir(&app.config()).unwrap();
    app_data.join("model_config.json")
}
```

### 数据序列化

**Electron:**
```javascript
JSON.stringify(config, null, 2)
JSON.parse(raw)
```

**Tauri:**
```rust
use serde_json;

serde_json::to_string_pretty(&config)?
serde_json::from_str(&content)?
```

---

## 构建与打包

### 开发流程对比

| 步骤 | Electron | Tauri |
|------|---------|-------|
| 安装依赖 | `npm install` | `npm install` + Rust 环境 |
| 开发模式 | `npm run dev` | `npm run dev` |
| 生产构建 | `npm run build` | `npm run build` |
| 输出目录 | `dist-electron/` | `src-tauri/target/release/bundle/` |
| 产物大小 | ~150-200MB | ~10-20MB |

### Tauri 构建产物

```
src-tauri/target/release/bundle/
├── msi/
│   └── AI Hub Desktop_1.0.0_x64_en-US.msi
├── nsis/
│   └── AI Hub Desktop_1.0.0_x64-setup.exe
├── deb/
│   └── ai-hub-desktop_1.0.0_amd64.deb
└── appimage/
    └── ai-hub-desktop_1.0.0_amd64.AppImage
```

### 图标要求

Tauri 需要多种尺寸的图标：

```
src-tauri/icons/
├── 32x32.png
├── 128x128.png
├── 128x128@2x.png
├── icon.icns          # macOS
├── icon.ico           # Windows
└── icon.png           # 1024x1024 源文件
```

自动生成：
```bash
npm run tauri icon  # 需要 src-tauri/icons/icon.png (1024x1024)
```

---

## 常见问题与解决方案

### 1. WebView 无法加载某些网站

**问题**: 部分网站设置 `X-Frame-Options: DENY`，无法在 iframe 中加载

**解决方案**:
- 方案 A: 使用 Tauri 原生 WebView (需重构为多窗口)
- 方案 B: 后端代理请求 (复杂，不推荐)
- 方案 C: 使用 `shell.open()` 在系统浏览器打开

### 2. User-Agent 无法修改

**问题**: Tauri WebView 初始化后无法修改 User-Agent

**解决方案**:
```json
// tauri.conf.json
{
  "tauri": {
    "windows": [
      {
        "userAgent": "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36..."
      }
    ]
  }
}
```

**限制**: 所有窗口共用同一 User-Agent

### 3. Cookie/会话不持久

**问题**: WebView 关闭后会话丢失

**解决方案**:
- Tauri 默认持久化 WebView 数据
- 确保 `tauri.conf.json` 中未设置 `incognito: true`

### 4. 跨域问题

**问题**: iframe 加载外部网站时跨域

**解决方案**:
- 使用 Tauri 后端代理请求
- 或接受跨域限制 (部分功能受限)

### 5. 主题切换不生效

**问题**: WebView 内部网页主题不跟随

**解决方案**:
- 通过 `window.eval()` 注入 CSS
- 或依赖 `prefers-color-scheme` 媒体查询

### 6. Rust 编译错误

**问题**: 依赖编译失败

**解决方案**:
```bash
# 更新 Rust
rustup update

# 清理缓存
cargo clean

# 重新构建
cargo build
```

---

## 测试验证清单

### 功能测试

- [ ] 窗口控制 (最小化、最大化、关闭)
- [ ] 自定义标题栏拖拽
- [ ] 模型列表加载
- [ ] 模型切换
- [ ] 网页加载状态 (loading/loaded/error)
- [ ] 网页刷新
- [ ] 侧边栏折叠/展开
- [ ] 设置弹窗打开/关闭
- [ ] 主题切换 (浅色/深色/跟随系统)
- [ ] 系统主题变化响应
- [ ] 配置保存
- [ ] 配置读取
- [ ] 云端同步
- [ ] 模型自动检测
- [ ] 模型新增/编辑/删除
- [ ] 模型拖拽排序
- [ ] 模型可见性切换
- [ ] URL 变更后重新加载

### 兼容性测试

- [ ] Windows 10/11
- [ ] macOS (如有)
- [ ] Linux (如有)
- [ ] 高分辨率屏幕
- [ ] 多显示器

### 性能测试

- [ ] 启动时间 (< 3 秒)
- [ ] 内存占用 (< 150MB)
- [ ] 模型切换延迟 (< 1 秒)
- [ ] 配置保存响应 (< 500ms)

### 安全测试

- [ ] CSP 策略生效
- [ ] 文件访问限制
- [ ] IPC 通信安全
- [ ] 用户数据加密 (可选)

---

## 迁移时间估算

| 阶段 | 预计时间 | 风险 |
|------|---------|------|
| Phase 1: 环境准备 | 1-2 天 | 低 |
| Phase 2: 后端迁移 | 3-5 天 | 中 (Rust 学习曲线) |
| Phase 3: 前端改造 | 2-3 天 | 中 (WebView 限制) |
| Phase 4: 窗口与视图 | 2-3 天 | 高 (架构变化) |
| Phase 5: 主题系统 | 1 天 | 低 |
| Phase 6: 构建打包 | 1-2 天 | 低 |
| 测试与调试 | 2-3 天 | 中 |
| **总计** | **12-19 天** | |

---

## 总结

### 迁移收益

✅ **体积减少 80%+** (200MB → 20MB)  
✅ **内存减少 60%+** (400MB → 100MB)  
✅ **启动速度提升 50%+**  
✅ **安全性提升** (默认沙箱隔离)  
✅ **跨平台一致性更好**

### 迁移挑战

⚠️ **WebContentsView 限制** (需重构视图管理)  
⚠️ **Rust 学习曲线** (后端语言变化)  
⚠️ **User-Agent 限制** (无法动态修改)  
⚠️ **部分网站不兼容 iframe**  

### 推荐策略

1. **分阶段迁移**: 先完成后端，再改造前端
2. **保留 Electron 分支**: 迁移期间保持双版本
3. **优先核心功能**: 先保证模型切换和配置管理
4. **充分测试**: 每个阶段完成后进行功能验证

---

## 参考资料

- [Tauri 官方文档](https://tauri.app/v1/guides/)
- [Tauri API 参考](https://tauri.app/v1/api/js/)
- [Rust Book](https://doc.rust-lang.org/book/)
- [Tauri vs Electron 对比](https://tauri.app/v1/guides/comparison/electron/)
- [Vue 3 文档](https://cn.vuejs.org/)
- [Element Plus 文档](https://element-plus.org/zh-CN/)

---

**文档版本**: v1.0  
**创建日期**: 2026-05-26  
**适用项目**: AI Hub Desktop v1.0.0
