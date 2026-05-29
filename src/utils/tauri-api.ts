import { invoke } from '@tauri-apps/api/core'
import { getCurrentWindow } from '@tauri-apps/api/window'

export interface Model {
  id: string
  name: string
  url: string
  version: string
  visible: boolean
}

export interface AppConfig {
  theme: string
  serverUrl: string
  sidebarCollapsed: boolean
  models: Model[]
}

export const tauriAPI = {
  // 配置管理
  getConfig: () => invoke<AppConfig>('get_config'),
  saveConfig: (config: AppConfig) => invoke('save_config', { config }),
  syncFromCloud: (serverUrl: string) => invoke<AppConfig>('sync_from_cloud', { serverUrl }),

  // 模型检测
  checkModelUrl: (url: string) => invoke<boolean>('check_model_url', { url }),

  // 窗口管理
  openModelWindow: (model: Model) => invoke('open_model_window', { model }),
  toggleSidebar: (collapsed: boolean) => invoke('toggle_sidebar', { collapsed }),

  // 内容 WebView 管理（在主窗口内嵌显示网页）
  loadContentWebview: (url: string, x: number, y: number, width: number, height: number, theme: string) =>
    invoke('load_content_webview', { url, x, y, width, height, theme }),
  resizeContentWebview: (x: number, y: number, width: number, height: number) =>
    invoke('resize_content_webview', { x, y, width, height }),
  hideContentWebview: () => invoke('hide_content_webview'),
  setWebviewTheme: (theme: string) => invoke('set_webview_theme', { theme }),

  // 多 WebView 缓存支持（新增）
  // 获取或创建指定模型的 WebView，返回 true 表示新创建（需要 loading），false 表示已缓存
  getOrCreateModelWebview: (
    modelId: string,
    url: string,
    x: number,
    y: number,
    width: number,
    height: number,
    theme: string
  ) => invoke<boolean>('get_or_create_model_webview', {
    modelId, url, x, y, width, height, theme
  }),

  // 切换到指定模型（隐藏其他 WebView，只显示目标模型）
  switchToModel: (modelId: string) =>
    invoke('switch_to_model', { modelId }),

  // 调整指定模型 WebView 的位置和大小
  resizeModelWebview: (
    modelId: string,
    x: number,
    y: number,
    width: number,
    height: number
  ) => invoke('resize_model_webview', {
    modelId, x, y, width, height
  }),

  // 隐藏/移除指定模型的 WebView
  hideModelWebview: (modelId: string) =>
    invoke('hide_model_webview', { modelId }),

  // 隐藏所有模型 WebView（不销毁，仅隐藏）- 用于打开设置弹窗时
  hideAllModelWebviews: () =>
    invoke('hide_all_model_webviews'),

  // 清除所有缓存的模型 WebView（释放内存）
  clearAllModelWebviews: () =>
    invoke('clear_all_model_webviews'),

  // 主窗口控制
  minimizeWindow: () => {
    const win = getCurrentWindow()
    return win.minimize()
  },

  toggleMaximize: async () => {
    const win = getCurrentWindow()
    const isMax = await win.isMaximized()
    if (isMax) {
      return win.unmaximize()
    } else {
      return win.maximize()
    }
  },

  closeWindow: () => {
    const win = getCurrentWindow()
    return win.close()
  },
}
