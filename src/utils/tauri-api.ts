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
