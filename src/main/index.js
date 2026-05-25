const { app, BrowserWindow, WebContentsView, ipcMain, nativeTheme } = require('electron')
const path = require('path')
const { readConfig, writeConfig, syncFromCloud } = require('./configManager')

let mainWindow = null
const views = new Map()
let currentViewId = null

const TITLEBAR_HEIGHT = 60
const SIDEBAR_WIDTH = 200

const CHROME_UA = 'Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/124.0.0.0 Safari/537.36'

// 将 HTML 字符串转为可靠的 data: URL（base64 编码，避免中文/特殊字符导致加载失败）
function htmlToDataUrl(html) {
  return 'data:text/html;charset=utf-8;base64,' + Buffer.from(html, 'utf-8').toString('base64')
}

function buildErrorPage(icon, title, subtitle, retryUrl, retryText) {
  return htmlToDataUrl(`<!DOCTYPE html>
<html><head><meta charset="UTF-8">
<style>
body{margin:0;display:flex;align-items:center;justify-content:center;height:100vh;background:#0f0f23;color:rgba(255,255,255,.85);font-family:sans-serif;flex-direction:column}
.icon{font-size:48px;margin-bottom:16px}
.msg{font-size:15px;margin-bottom:6px}
.sub{font-size:12px;color:rgba(255,255,255,.35);max-width:80%;text-align:center;word-break:break-all;line-height:1.5;margin-bottom:24px}
.btn{padding:8px 24px;border:1px solid rgba(255,255,255,.2);border-radius:6px;background:transparent;color:rgba(255,255,255,.6);cursor:pointer;font-size:13px;text-decoration:none}
.btn:hover{background:rgba(255,255,255,.1)}
</style></head><body>
<div class="icon">${icon}</div>
<div class="msg">${title}</div>
<div class="sub">${subtitle}</div>
<a class="btn" href="${retryUrl}">${retryText}</a>
</body></html>`)
}

function isDev() {
  return process.env.NODE_ENV === 'development' || !app.isPackaged
}

function getRendererUrl() {
  if (isDev()) {
    return 'http://localhost:5173'
  }
  return `file://${path.join(__dirname, '../../dist/renderer/index.html')}`
}

function calculateViewBounds() {
  if (!mainWindow) return null
  const [width, height] = mainWindow.getContentSize()
  return {
    x: SIDEBAR_WIDTH,
    y: TITLEBAR_HEIGHT,
    width: width - SIDEBAR_WIDTH,
    height: height - TITLEBAR_HEIGHT
  }
}

function createWindow() {
  // preload 脚本路径
  const preloadPath = isDev() 
    ? path.join(__dirname, 'preload.js')
    : path.join(__dirname, '../preload.js')

  mainWindow = new BrowserWindow({
    width: 1400,
    height: 900,
    minWidth: 800,
    minHeight: 600,
    frame: false,
    titleBarStyle: 'hidden',
    webPreferences: {
      preload: preloadPath,
      nodeIntegration: false,
      contextIsolation: true
    }
  })

  mainWindow.loadURL(getRendererUrl())

  // 窗口大小变化时调整当前视图
  mainWindow.on('resize', () => {
    if (currentViewId && views.has(currentViewId)) {
      const view = views.get(currentViewId)
      const bounds = calculateViewBounds()
      if (bounds) {
        view.setBounds(bounds)
      }
    }
  })

  registerIpcHandlers()

  // 启动时同步主题到原生层（影响 WebContentsView 的 prefers-color-scheme）
  const config = readConfig()
  nativeTheme.themeSource = config.theme || 'system'
}

function switchView(id, url) {
  // 隐藏当前视图（不销毁，保持挂起）
  if (currentViewId && currentViewId !== id && views.has(currentViewId)) {
    const currentView = views.get(currentViewId)
    mainWindow.contentView.removeChildView(currentView)
  }

  // 如果该模型视图不存在，创建新视图
  if (!views.has(id)) {
    const view = new WebContentsView({
      webPreferences: {
        partition: 'persist:' + id,
        nodeIntegration: false,
        contextIsolation: true
      }
    })

    // 深色背景，避免加载时白屏闪烁
    view.setBackgroundColor('#0f0f23')

    // 伪装 User-Agent
    view.webContents.setUserAgent(CHROME_UA)

    // 跟踪加载状态
    view._loaded = false
    view._addedToContentView = false

    // 加载超时定时器
    const timer = setTimeout(() => {
      if (!view._loaded) {
        view._loaded = true
        const errorUrl = buildErrorPage('⏱️', '连接超时', '页面加载超过 15 秒', encodeURI(url), '重新加载')
        view.webContents.loadURL(errorUrl).catch(() => {})
        // 超时后展示视图（仅首次）
        if (currentViewId === id && !view._addedToContentView) {
          mainWindow.contentView.addChildView(view)
          view._addedToContentView = true
        }
        mainWindow.webContents.send('view:state', { state: 'error', id, error: '连接超时' })
      }
    }, 15000)

    // 加载成功
    view.webContents.on('did-finish-load', () => {
      if (!view._loaded) {
        view._loaded = true
        clearTimeout(timer)
        // 首次加载完成，展示视图（后续 reloadUrl 不重复添加）
        if (currentViewId === id && !view._addedToContentView) {
          mainWindow.contentView.addChildView(view)
          view._addedToContentView = true
        }
        mainWindow.webContents.send('view:state', { state: 'loaded', id })
      }
    })

    // 加载失败时显示友好错误页
    view.webContents.on('did-fail-load', (_event, errorCode, errorDesc, validatedURL) => {
      if (errorCode === -3) return // 用户主动中止，忽略
      if (view._loaded) return
      view._loaded = true
      clearTimeout(timer)
      console.error(`加载 ${id} 失败: ${errorDesc} (${errorCode})`)
      const displayUrl = validatedURL || url
      const msg = errorDesc === 'ERR_NAME_NOT_RESOLVED'
        ? '无法解析域名，请检查网络或 URL 是否正确'
        : errorDesc === 'ERR_CONNECTION_REFUSED'
        ? '连接被拒绝，目标服务器不可达'
        : `加载失败: ${errorDesc}`
      const errorUrl = buildErrorPage('😔', '连接失败', `${msg}<br>${displayUrl}`, encodeURI(displayUrl), '重新加载')
      view.webContents.loadURL(errorUrl).catch(() => {})
      // 失败后展示视图（仅首次）
      if (currentViewId === id && !view._addedToContentView) {
        mainWindow.contentView.addChildView(view)
        view._addedToContentView = true
      }
      mainWindow.webContents.send('view:state', { state: 'error', id, error: msg })
    })

    views.set(id, view)

    // 开始加载目标 URL
    view.webContents.loadURL(url).catch(err => {
      console.error(`加载 ${id} 失败:`, err.message)
    })
  }

  // 计算 bounds
  const view = views.get(id)
  const bounds = calculateViewBounds()
  if (bounds) {
    view.setBounds(bounds)
  }
  currentViewId = id

  // 已加载的视图直接展示；首次加载则等 did-finish-load 后再添加
  if (view._loaded) {
    mainWindow.contentView.addChildView(view)
    view._addedToContentView = true
    mainWindow.webContents.send('view:state', { state: 'loaded', id })
  }
}

function registerIpcHandlers() {
  // 获取配置
  ipcMain.handle('config:get', () => {
    return { success: true, data: readConfig() }
  })

  // 保存配置
  ipcMain.on('config:save', (_event, config) => {
    const result = writeConfig(config)
    if (result.success) {
      // 通知渲染进程配置已更新
      mainWindow.webContents.send('config:updated')
    }
  })

  // 云端同步（接受渲染进程传入的 URL，优先于本地配置）
  ipcMain.handle('config:sync', async (_event, serverUrl) => {
    const url = serverUrl || readConfig().serverUrl
    const result = await syncFromCloud(url)
    if (result.success) {
      // 同步成功后广播更新
      mainWindow.webContents.send('config:updated')
    }
    return result
  })

  // 窗口控制
  ipcMain.on('window:minimize', () => {
    mainWindow.minimize()
  })

  ipcMain.on('window:maximize', () => {
    if (mainWindow.isMaximized()) {
      mainWindow.unmaximize()
    } else {
      mainWindow.maximize()
    }
  })

  ipcMain.on('window:close', () => {
    mainWindow.close()
  })

  // 切换视图
  ipcMain.handle('view:switch', (_event, { id, url }) => {
    try {
      switchView(id, url)
      return { success: true }
    } catch (err) {
      return { success: false, error: err.message }
    }
  })

  // 隐藏当前视图（打开设置时）
  ipcMain.on('view:hide', () => {
    if (currentViewId && views.has(currentViewId)) {
      const view = views.get(currentViewId)
      mainWindow.contentView.removeChildView(view)
    }
  })

  // 恢复当前视图（关闭设置时）
  ipcMain.on('view:show', () => {
    if (currentViewId && views.has(currentViewId)) {
      const view = views.get(currentViewId)
      const bounds = calculateViewBounds()
      if (bounds) {
        view.setBounds(bounds)
      }
      mainWindow.contentView.addChildView(view)
    }
  })

  // 刷新当前视图
  ipcMain.on('view:reload', () => {
    if (currentViewId && views.has(currentViewId)) {
      const view = views.get(currentViewId)
      view._loaded = false
      mainWindow.webContents.send('view:state', { state: 'loading', id: currentViewId })
      view.webContents.reload()
    }
  })

  // 重新加载当前视图的 URL（URL 变更时）
  ipcMain.on('view:reloadUrl', (_event, url) => {
    if (currentViewId && views.has(currentViewId)) {
      const view = views.get(currentViewId)
      // 重置加载标记，让 did-finish-load / did-fail-load 处理器重新工作
      view._loaded = false
      mainWindow.webContents.send('view:state', { state: 'loading', id: currentViewId })
      view.webContents.loadURL(url).catch(err => {
        console.error('重新加载 URL 失败:', err.message)
      })
    }
  })

  // 主题切换：同步到原生层，影响所有 WebContentsView 的 prefers-color-scheme
  ipcMain.on('theme:set', (_event, theme) => {
    nativeTheme.themeSource = theme
  })
}

app.whenReady().then(createWindow)

app.on('window-all-closed', () => {
  if (process.platform !== 'darwin') {
    app.quit()
  }
})

app.on('activate', () => {
  if (BrowserWindow.getAllWindows().length === 0) {
    createWindow()
  }
})
