<script setup lang="ts">
import { ref, computed, onMounted, onBeforeUnmount, nextTick } from 'vue'
import { Setting, Minus, FullScreen, Close, DArrowLeft, DArrowRight, Loading, WarningFilled, Refresh } from '@element-plus/icons-vue'
import { ElMessage } from 'element-plus'
import { tauriAPI, type Model, type AppConfig } from './utils/tauri-api'
import { getCurrentWindow } from '@tauri-apps/api/window'
import { listen, type UnlistenFn } from '@tauri-apps/api/event'
import { applyTheme, type ThemeMode } from './utils/theme'
import Settings from './components/Settings.vue'

const config = ref<AppConfig | null>(null)
const settingsVisible = ref(false)
const isMaximized = ref(false)
const loading = ref(true)

// 当前选中的模型
const activeModel = ref<Model | null>(null)
const webviewLoaded = ref(false)

// 模型切换状态（loading 过渡 + 超时处理）
type SwitchState = 'idle' | 'loading' | 'error'
const modelSwitchState = ref<SwitchState>('idle')
const modelSwitchError = ref('')
// 15s 后仅提示"网速较慢"，不显示红色错误（慢网首载常见，页面稍后加载完成会自动覆盖）
const slowLoading = ref(false)
let switchTimeoutTimer: ReturnType<typeof setTimeout> | null = null
let slowTimer: ReturnType<typeof setTimeout> | null = null
// 15s：慢速提示；32s：兜底超时（Rust 端 30s 会先发出 load-failed 事件）
const SWITCH_SLOW_TIMEOUT = 15000
const SWITCH_TIMEOUT = 32000

// 最近一次请求加载的模型（首次加载失败时 activeModel 为空，"重试"按钮需要用到）
const lastRequestedModel = ref<Model | null>(null)

function clearSwitchTimers() {
  if (switchTimeoutTimer) {
    clearTimeout(switchTimeoutTimer)
    switchTimeoutTimer = null
  }
  if (slowTimer) {
    clearTimeout(slowTimer)
    slowTimer = null
  }
  slowLoading.value = false
}

function armSwitchTimers(version: number) {
  clearSwitchTimers()
  slowTimer = setTimeout(() => {
    if (loadVersion === version && modelSwitchState.value === 'loading') {
      slowLoading.value = true
    }
  }, SWITCH_SLOW_TIMEOUT)
  switchTimeoutTimer = setTimeout(() => {
    if (loadVersion === version && modelSwitchState.value === 'loading') {
      modelSwitchState.value = 'error'
      modelSwitchError.value = '连接超时，请检查网络后重试'
    }
  }, SWITCH_TIMEOUT)
}

// 从事件 payload（"webview-{id}" 或裸 id）提取模型 ID
function modelIdFromLabel(label: string): string {
  return label.startsWith('webview-') ? label.slice('webview-'.length) : label
}

// 焦点恢复：把键盘焦点交还当前模型 WebView，实现切回窗口/切换模型后直接续打输入。
// 必须延迟 200ms 调用：窗口激活消息（WM_ACTIVATE）处理阶段内调用 WebView2 焦点接口
// 会与其激活流程互相等待导致主线程死锁（已实测复现），激活完成后再调用是安全的。
// 页面会记住上次聚焦的输入框（document.activeElement），焦点一恢复即可继续输入。
let focusRestoreTimer: ReturnType<typeof setTimeout> | null = null

function scheduleFocusRestore() {
  if (!activeModel.value) return
  if (focusRestoreTimer) clearTimeout(focusRestoreTimer)
  focusRestoreTimer = setTimeout(() => {
    focusRestoreTimer = null
    if (!activeModel.value) return
    tauriAPI.restoreWebviewFocus().catch(() => {})
  }, 200)
}

// WebView 页面加载事件监听器
let unlistenPageLoaded: UnlistenFn | null = null
let unlistenLoadFailed: UnlistenFn | null = null
let unlistenFocus: (() => void) | null = null

let sidebarTimer: ReturnType<typeof setTimeout> | null = null
let resizeTimer: ReturnType<typeof setTimeout> | null = null
let unlistenResize: (() => void) | null = null

// 已缓存的模型 ID 集合（插入顺序 = 最近使用顺序，用于镜像 Rust 端 LRU 容量限制）
const loadedModels = ref<Map<string, boolean>>(new Map())
// 与 Rust 端 MAX_CACHED_WEBVIEWS 保持一致
const MAX_CACHED_WEBVIEWS = 5

// 标记模型为已加载（移到最新位置），超出容量时淘汰最旧记录，与 Rust 端 LRU 保持同步
function markModelLoaded(modelId: string) {
  loadedModels.value.delete(modelId)
  loadedModels.value.set(modelId, true)
  while (loadedModels.value.size > MAX_CACHED_WEBVIEWS) {
    const oldest = loadedModels.value.keys().next().value
    if (oldest === undefined) break
    loadedModels.value.delete(oldest)
  }
}

// P0-4: 操作版本号，用于取消过期的异步操作
let loadVersion = 0

// 模型加载锁：防止快速点击时的重复操作
let loadingModelId: string | null = null

const visibleModels = computed(() => 
  config.value?.models.filter(m => m.visible) || []
)

async function loadConfig() {
  try {
    config.value = await tauriAPI.getConfig()
    sidebarVisualCollapsed.value = config.value.sidebarCollapsed
    applyTheme(config.value.theme as ThemeMode)
  } catch (error) {
    ElMessage.error('加载配置失败')
    console.error(error)
  } finally {
    loading.value = false
  }
}

// 模型操作串行队列：同一时间只执行一个切换/刷新/重试流程。
// 快速点击时若两个流程并发执行，迟到的 RPC 会交错覆盖（例如旧流程的
// switchToModel 晚于新流程执行，导致显示的界面与点击不对应），
// 串行化 + 版本号检查保证过期流程完全跳过、新流程在旧流程彻底结束后才开始
let modelOpQueue: Promise<void> = Promise.resolve()

function enqueueModelOp(model: Model, run: (model: Model, version: number) => Promise<void>) {
  const version = ++loadVersion
  loadingModelId = model.id
  lastRequestedModel.value = model

  // 立即给出 loading 反馈（实际操作在队列中串行执行）
  modelSwitchState.value = 'loading'
  modelSwitchError.value = ''
  armSwitchTimers(version)

  const op = modelOpQueue.then(() => {
    if (loadVersion !== version) return // 已被更新的操作取代
    return run(model, version)
  })
  modelOpQueue = op.catch(() => {})
  return op.finally(() => {
    if (loadVersion === version && loadingModelId === model.id) loadingModelId = null
  })
}

// 核心改动：使用缓存机制处理模型点击
function handleModelClick(model: Model) {
  if (activeModel.value?.id === model.id) return
  // 防抖：如果正在加载同一个模型，忽略
  if (loadingModelId === model.id) return
  enqueueModelOp(model, doModelSwitch)
}

// 执行模型切换（在串行队列中调用）
async function doModelSwitch(model: Model, version: number) {
  try {
    const isFirstLoad = !loadedModels.value.has(model.id)
    const previousModelId = activeModel.value?.id

    if (isFirstLoad) {
      // 首次加载：先隐藏旧模型的 WebView（让 loading overlay 可见）
      if (previousModelId && loadedModels.value.has(previousModelId)) {
        try {
          await tauriAPI.switchToModel(model.id)
        } catch {
          // 忽略
        }
      }
      // P0-4: 检查版本号是否仍然有效
      if (loadVersion !== version) return
      // 创建新 WebView 并显示 loading
      await loadModelWithCache(model, version)
    } else {
      // 已缓存：先调整大小，再显示（无 loading 动画，瞬间完成）
      // 先获取容器当前位置，确保 WebView 大小正确
      await new Promise(resolve => setTimeout(resolve, 10))

      // P0-4: 检查版本号
      if (loadVersion !== version) return

      const container = document.getElementById('webview-container')
      if (container) {
        const rect = container.getBoundingClientRect()
        await tauriAPI.resizeModelWebview(
          model.id,
          rect.left,
          rect.top,
          rect.width,
          rect.height
        )
      }

      // P0-4: 再次检查版本号
      if (loadVersion !== version) return

      // 切换到目标模型（显示目标，隐藏其他）
      // 返回 false 表示 WebView 已被 Rust 端 LRU 淘汰，回退到重建流程，
      // 否则点击会静默无操作（表现为"点了没反应"）
      const exists = await tauriAPI.switchToModel(model.id)
      if (loadVersion !== version) return
      if (!exists) {
        loadedModels.value.delete(model.id)
        await loadModelWithCache(model, version)
        return
      }

      // 只有版本号仍然匹配时才更新状态
      if (loadVersion === version) {
        markModelLoaded(model.id)
        activeModel.value = model
        webviewLoaded.value = true
        modelSwitchState.value = 'idle'
        clearSwitchTimers()
        // 切换完成后恢复焦点：页面记住了上次的输入框位置，可直接续打
        scheduleFocusRestore()
      }
    }
  } catch (error) {
    console.error('切换模型失败:', error)
    if (loadVersion === version) {
      modelSwitchState.value = 'error'
      modelSwitchError.value = '切换模型失败: ' + (error as Error).message
    }
  }
}

// 使用新缓存 API 加载模型
async function loadModelWithCache(model: Model, version?: number) {
  // P1-4: 使用当前版本号，如果没有传入则使用最新
  const myVersion = version ?? loadVersion

  // 清除之前的超时定时器
  clearSwitchTimers()

  modelSwitchState.value = 'loading'
  modelSwitchError.value = ''
  webviewLoaded.value = false

  // 等待 Vue 渲染 loading overlay（确保 DOM 更新后再创建 WebView）
  await nextTick()
  // 额外延迟确保 loading 动画可见（Rust 创建 WebView 后会隐藏它，原生层覆盖 DOM）
  await new Promise(resolve => setTimeout(resolve, 200))

  // P0-4: 检查版本号
  if (loadVersion !== myVersion) return

  // 超时兜底（15s 慢速提示，32s 兜底超时；Rust 端 30s 会先发出 load-failed 事件）
  armSwitchTimers(myVersion)

  try {
    // 等待 DOM 更新后获取容器位置
    await new Promise(resolve => setTimeout(resolve, 50))

    // P0-4: 检查版本号
    if (loadVersion !== myVersion) return

    const container = document.getElementById('webview-container')
    if (!container) {
      clearSwitchTimers()
      modelSwitchState.value = 'error'
      modelSwitchError.value = '容器未找到'
      return
    }

    const rect = container.getBoundingClientRect()

    // 调用新的缓存 API，返回值表示是否为新创建的 WebView
    const isNewCreated = await tauriAPI.getOrCreateModelWebview(
      model.id,
      model.url,
      rect.left,
      rect.top,
      rect.width,
      rect.height,
      config.value?.theme || 'system'
    )

    // P0-4: 检查版本号
    if (loadVersion !== myVersion) return

    if (isNewCreated) {
      // 新创建的 WebView：标记为已加载，保持 loading 状态（等待 page-loaded 事件后进入 idle）
      markModelLoaded(model.id)
      webviewLoaded.value = true
    } else {
      // 已缓存的 WebView（Rust 快速路径已直接 show）
      markModelLoaded(model.id)
      webviewLoaded.value = true
      modelSwitchState.value = 'idle'
      clearSwitchTimers()
    }
    
    // 加载成功后更新 activeModel
    if (loadVersion === myVersion) {
      activeModel.value = model
    }
  } catch (error) {
    clearSwitchTimers()
    if (loadVersion !== myVersion) return // P0-4: 过期操作不再处理
    console.error('加载 WebView 失败:', error)
    modelSwitchState.value = 'error'
    modelSwitchError.value = '网页加载失败: ' + (error as Error).message
  }
}

// 兼容旧接口已移除，统一使用 loadModelWithCache
// 保留此函数作为参考，实际请使用 loadModelWithCache
// async function loadModelInWebview(url: string) { ... }

function retryLoadModel() {
  // 首次加载失败时 activeModel 为空，回退到最近请求的模型
  const model = activeModel.value ?? lastRequestedModel.value
  if (!model) return
  enqueueModelOp(model, (m, v) => loadModelWithCache(m, v))
}

// 刷新当前模型：销毁旧 WebView 后重新创建（走队列，避免与切换流程交错）
function refreshCurrentModel() {
  const model = activeModel.value
  if (!model) return
  enqueueModelOp(model, async (m, version) => {
    // 先销毁旧 WebView（确保 get_or_create_model_webview 走新建分支）
    loadedModels.value.delete(m.id)
    try {
      await tauriAPI.hideModelWebview(m.id)
    } catch {
      // 忽略销毁失败（WebView 可能已不存在）
    }
    // P0-4: 过期操作不再继续
    if (loadVersion !== version) return
    // 重新创建（会触发 loading 动画 + 页面重新加载）
    await loadModelWithCache(m, version)
  })
}

// 仅调整当前活动模型 WebView 的位置/大小（不重建，无白屏闪烁）
async function resizeWebviewOnly() {
  try {
    if (!activeModel.value) return
    
    const container = document.getElementById('webview-container')
    if (!container) return
    const rect = container.getBoundingClientRect()
    
    // 使用新的 resizeModelWebview API
    await tauriAPI.resizeModelWebview(
      activeModel.value.id,
      rect.left,
      rect.top,
      rect.width,
      rect.height
    )
  } catch (error) {
    console.error('调整 WebView 大小失败:', error)
  }
}

// 窗口大小变化时，防抖调整 WebView（只 resize，不重建）
function scheduleWebviewResize() {
  if (!activeModel.value) return
  if (resizeTimer) clearTimeout(resizeTimer)
  resizeTimer = setTimeout(() => {
    resizeWebviewOnly()
  }, 150)
}

async function toggleMaximize() {
  await tauriAPI.toggleMaximize()
  isMaximized.value = await getCurrentWindow().isMaximized()
  // 最大化/还原后 WebView 需要重新适配
  scheduleWebviewResize()
}

// 侧边栏视觉折叠状态（延迟切换，与 CSS 动画同步）
const sidebarVisualCollapsed = ref(config.value?.sidebarCollapsed ?? false)

// 侧边栏动画期间的 WebView 分步同步定时器
let resizeAnimTimer: ReturnType<typeof setInterval> | null = null

// 在 duration 毫秒内以约 30fps 分步调整 WebView 位置/大小，结束时精确对齐。
// 原生 WebView 层不参与 CSS 过渡，若只在动画结束后调整一次，
// 动画过程中会出现 WebView 与侧边栏重叠/留缝、画面跳动（不优雅的根源）
function animateWebviewResize(duration: number) {
  if (!activeModel.value) return
  if (resizeAnimTimer) clearInterval(resizeAnimTimer)
  const start = performance.now()
  resizeAnimTimer = setInterval(() => {
    resizeWebviewOnly()
    if (performance.now() - start >= duration) {
      if (resizeAnimTimer) clearInterval(resizeAnimTimer)
      resizeAnimTimer = null
      resizeWebviewOnly() // 最终精确对齐
    }
  }, 33)
}

async function toggleSidebar() {
  if (!config.value) return
  const newCollapsed = !config.value.sidebarCollapsed
  config.value.sidebarCollapsed = newCollapsed
  await tauriAPI.toggleSidebar(newCollapsed)

  // 清除之前的定时器
  if (sidebarTimer) clearTimeout(sidebarTimer)

  if (newCollapsed) {
    // 收起：内容立即切换为缩写
    sidebarVisualCollapsed.value = true
  } else {
    // 展开：延迟切换内容，等宽度动画完成后再显示全名
    sidebarTimer = setTimeout(() => {
      sidebarVisualCollapsed.value = false
    }, 300)
  }

  // 动画期间分步同步 WebView 位置，让原生层与侧边栏宽度过渡保持一致
  // 收起：300ms 动画 + 余量；展开：300ms 延迟切换文字 + 300ms 动画 + 余量
  if (activeModel.value) {
    animateWebviewResize(newCollapsed ? 380 : 680)
  }
}

// 打开设置弹窗
async function openSettings() {
  // 隐藏所有模型 WebView（不销毁，仅隐藏），否则会遮挡 dialog
  if (activeModel.value) {
    // 使用 switchToModel 传入空字符串来隐藏所有 WebView
    // 或者直接调用一个"隐藏所有"的命令
    await tauriAPI.hideAllModelWebviews()
    webviewLoaded.value = false
  }
  settingsVisible.value = true
}

// 关闭设置弹窗
async function closeSettings() {
  settingsVisible.value = false

  if (!config.value) return

  // 重新加载配置以获取最新模型列表
  await loadConfig()

  if (!config.value) return
  const newModelIds = new Set(config.value.models.map(m => m.id))

  // 检测被删除的模型，清理对应的缓存 WebView
  for (const cachedId of Array.from(loadedModels.value.keys())) {
    if (!newModelIds.has(cachedId)) {
      // 模型已被删除，清理 WebView
      try {
        await tauriAPI.removeModelWebview(cachedId)
      } catch {
        // 忽略清理失败
      }
      loadedModels.value.delete(cachedId)
      // 如果当前活跃模型被删除，清空
      if (activeModel.value?.id === cachedId) {
        activeModel.value = null
        webviewLoaded.value = false
      }
    }
  }

  if (!activeModel.value || !config.value) {
    return
  }
  
  // 检查当前活动模型的 WebView 是否还存在（未被清理）
  const isModelCached = loadedModels.value.has(activeModel.value.id)
  
  if (isModelCached) {
    // 模型已缓存：直接恢复显示并调整大小（无需重新加载）
    try {
      await new Promise(resolve => setTimeout(resolve, 10))
      
      const container = document.getElementById('webview-container')
      if (container) {
        const rect = container.getBoundingClientRect()
        // 先调整到正确位置和大小
        await tauriAPI.resizeModelWebview(
          activeModel.value.id,
          rect.left,
          rect.top,
          rect.width,
          rect.height
        )
      }
      
      // 显示该模型的 WebView（返回 false 表示已被 LRU 淘汰，回退到重建流程）
      const restored = await tauriAPI.switchToModel(activeModel.value.id)
      if (!restored) {
        loadedModels.value.delete(activeModel.value.id)
        await loadModelWithCache(activeModel.value)
        return
      }
      markModelLoaded(activeModel.value.id)
      webviewLoaded.value = true
      // 设置关闭后恢复焦点，可直接续打
      scheduleFocusRestore()
    } catch (error) {
      console.error('恢复模型失败:', error)
      // 如果恢复失败（WebView 可能已被意外销毁），尝试重新加载
      loadedModels.value.delete(activeModel.value.id)
      await loadModelWithCache(activeModel.value)
    }
  } else {
    // 模型未缓存：需要重新加载
    await loadModelWithCache(activeModel.value)
  }
}

onMounted(async () => {
  await loadConfig()
  
  // 监听 WebView 页面真实加载完成事件（Rust on_page_load 回调，payload 携带 "webview-{id}" 标识）
  // 只处理当前正在加载/当前激活模型的事件，过滤被放弃的旧加载任务发出的过期事件，
  // 避免过期事件把界面误置为错误态（红色错误图标闪现的根源之一）
  unlistenPageLoaded = await listen<string>('webview:page-loaded', (event) => {
    const modelId = modelIdFromLabel(event.payload || '')
    if (modelId !== loadingModelId && modelId !== activeModel.value?.id) return
    clearSwitchTimers()
    modelSwitchState.value = 'idle'
    // 页面加载完成，恢复焦点，可直接输入
    scheduleFocusRestore()
  })

  // 监听 WebView 页面加载失败事件（Rust 超时保护触发，payload 为 "webview-{id}"）
  // 仅在 loading 阶段且匹配当前在加载的模型时生效
  unlistenLoadFailed = await listen<string>('webview:load-failed', (event) => {
    console.error('WebView 加载超时:', event.payload)
    const modelId = modelIdFromLabel(event.payload || '')
    if (modelId !== loadingModelId || modelSwitchState.value !== 'loading') return
    clearSwitchTimers()
    modelSwitchState.value = 'error'
    modelSwitchError.value = '页面加载超时，请检查网络后重试'
  })
  
  // 监听窗口大小变化，自动调整 WebView
  // 注意：Tauri 2.x 使用 onResized 需要导入正确的类型
  const currentWindow = getCurrentWindow()
  unlistenResize = await currentWindow.onResized(() => {
    scheduleWebviewResize()
  })

  // 监听窗口焦点变化：从其他软件切回时，延迟恢复模型 WebView 的键盘焦点，
  // 实现切回后直接续打输入（延迟调用避开窗口激活消息处理阶段，防止死锁）
  unlistenFocus = await currentWindow.onFocusChanged(({ payload: focused }) => {
    if (focused) scheduleFocusRestore()
  })
})

async function handleSaveConfig(newConfig: AppConfig) {
  // 先同步更新 config，确保 closeSettings 重建 WebView 时使用新主题
  config.value = newConfig
  applyTheme(newConfig.theme as ThemeMode)
  
  try {
    await tauriAPI.saveConfig(newConfig)
    ElMessage.success('配置已保存')
  } catch (error) {
    ElMessage.error('保存配置失败')
    console.error(error)
  }
}

async function handleSyncCloud(serverUrl: string) {
  try {
    const newConfig = await tauriAPI.syncFromCloud(serverUrl)
    config.value = newConfig
    ElMessage.success('云端同步成功')
  } catch (error: any) {
    ElMessage.error(error || '云端同步失败')
    console.error(error)
  }
}

onBeforeUnmount(() => {
  if (sidebarTimer) clearTimeout(sidebarTimer)
  if (resizeTimer) clearTimeout(resizeTimer)
  if (resizeAnimTimer) clearInterval(resizeAnimTimer)
  if (focusRestoreTimer) clearTimeout(focusRestoreTimer)
  clearSwitchTimers()
  if (unlistenResize) unlistenResize()
  if (unlistenFocus) unlistenFocus()
  if (unlistenPageLoaded) unlistenPageLoaded()
  if (unlistenLoadFailed) unlistenLoadFailed()
})
</script>

<template>
  <div class="app-container">
    <!-- 自定义标题栏 -->
    <header class="titlebar">
      <div class="titlebar-left">
        <span class="app-title">AI Hub Desktop</span>
      </div>
      <div class="titlebar-right">
        <el-button @click="refreshCurrentModel" circle size="small" :disabled="!activeModel" title="刷新当前页面">
          <el-icon><Refresh /></el-icon>
        </el-button>
        <el-button @click="openSettings" circle size="small" title="设置">
          <el-icon><Setting /></el-icon>
        </el-button>
        <el-button @click="tauriAPI.minimizeWindow()" circle size="small">
          <el-icon><Minus /></el-icon>
        </el-button>
        <el-button @click="toggleMaximize" circle size="small">
          <el-icon><FullScreen /></el-icon>
        </el-button>
        <el-button @click="tauriAPI.closeWindow()" circle size="small">
          <el-icon><Close /></el-icon>
        </el-button>
      </div>
    </header>

    <!-- 主内容区 -->
    <div class="main-content">
      <!-- 左侧边栏 -->
      <aside class="sidebar" :class="{ collapsed: sidebarVisualCollapsed }">
        <div class="model-list">
          <div 
            v-for="model in visibleModels" 
            :key="model.id"
            class="model-item"
            :class="{ active: activeModel?.id === model.id }"
            :title="model.name"
            @click="handleModelClick(model)"
          >
            <span v-if="sidebarVisualCollapsed" class="model-abbr">{{ model.name.charAt(0) }}</span>
            <div v-else class="model-info">
              <span class="model-name">{{ model.name }}</span>
            </div>
          </div>
          
          <div v-if="visibleModels.length === 0" class="empty-models">
            <p>暂无可见模型</p>
            <p class="hint">请在设置中启用模型</p>
          </div>
        </div>
        
        <el-button 
          class="collapse-btn" 
          @click="toggleSidebar"
          circle
          size="small"
        >
          <el-icon v-if="config?.sidebarCollapsed"><DArrowRight /></el-icon>
          <el-icon v-else><DArrowLeft /></el-icon>
        </el-button>
      </aside>

      <!-- 右侧内容区 -->
      <main class="content-area">
        <!-- WebView 容器 -->
        <div id="webview-container"></div>
        
        <!-- 模型切换加载过渡层（仅在首次加载或刷新时显示） -->
        <div v-if="modelSwitchState === 'loading'" class="switch-overlay">
          <div class="switch-loading">
            <el-icon class="is-loading" :size="48"><Loading /></el-icon>
            <p class="switch-text">{{ slowLoading ? `网速较慢，仍在加载 ${activeModel?.name ?? ''} …` : `正在加载 ${activeModel?.name ?? ''}...` }}</p>
          </div>
        </div>
        
        <!-- 加载失败/超时层 -->
        <div v-else-if="modelSwitchState === 'error'" class="switch-overlay">
          <div class="switch-error">
            <el-icon :size="48"><WarningFilled /></el-icon>
            <p class="switch-text">{{ modelSwitchError }}</p>
            <el-button type="primary" @click="retryLoadModel">重试</el-button>
          </div>
        </div>
        
        <!-- 欢迎页面（未选择模型时显示） -->
        <div v-if="!activeModel" class="welcome-screen">
          <div class="welcome-content">
            <h1>欢迎使用 AI Hub Desktop</h1>
            <p>请从左侧选择一个 AI 模型开始使用</p>
            <div class="quick-actions">
              <el-button type="primary" @click="openSettings">
                <el-icon><Setting /></el-icon>
                打开设置
              </el-button>
            </div>
          </div>
        </div>
      </main>
    </div>

    <!-- 设置弹窗 -->
    <Settings 
      v-model="settingsVisible"
      :config="config"
      :sidebar-collapsed="config?.sidebarCollapsed ?? false"
      @save="handleSaveConfig"
      @sync="handleSyncCloud"
      @close="closeSettings"
    />
  </div>
</template>

<style scoped>
.app-container {
  width: 100%;
  height: 100%;
  display: flex;
  flex-direction: column;
  background-color: var(--bg-color);
}

/* 标题栏 */
.titlebar {
  height: 40px;
  display: flex;
  align-items: center;
  justify-content: space-between;
  padding: 0 12px;
  background-color: var(--header-bg);
  border-bottom: 1px solid var(--border-color);
}

.titlebar-left {
  display: flex;
  align-items: center;
  gap: 8px;
}

.app-title {
  font-size: 14px;
  font-weight: 600;
  color: var(--text-color);
}

.titlebar-right {
  display: flex;
  align-items: center;
  gap: 4px;
}

/* 主内容区 */
.main-content {
  flex: 1;
  display: flex;
  overflow: hidden;
}

/* 侧边栏 */
.sidebar {
  width: 240px;
  background-color: var(--sidebar-bg);
  border-right: 1px solid var(--border-color);
  display: flex;
  flex-direction: column;
  transition: width 0.3s ease;
  position: relative;
  overflow-x: hidden;
}

.sidebar.collapsed {
  width: 60px;
}

.sidebar.collapsed .model-list {
  padding: 6px 4px;
}

.sidebar.collapsed .model-item {
  padding: 6px 4px;
  margin-bottom: 2px;
  display: flex;
  justify-content: center;
}

.sidebar.collapsed .model-item.active {
  border-left: none;
  border-radius: 6px;
  background-color: var(--active-bg, rgba(64, 158, 255, 0.2));
}

.sidebar.collapsed .model-abbr {
  width: 32px;
  height: 32px;
  font-size: 14px;
}

.model-list {
  flex: 1;
  overflow-y: auto;
  padding: 8px;
}

.model-item {
  padding: 12px;
  margin-bottom: 4px;
  border-radius: 6px;
  cursor: pointer;
  transition: background-color 0.2s;
  overflow: hidden;
}

.model-item:hover {
  background-color: var(--hover-bg);
}

.model-item.active {
  background-color: var(--active-bg, rgba(64, 158, 255, 0.15));
  border-left: 3px solid var(--active-border, #409EFF);
}

.model-info {
  display: flex;
  flex-direction: column;
  justify-content: center;
  min-width: 0;
}

.model-name {
  font-size: 14px;
  font-weight: 500;
  color: var(--text-color);
  /* 宽度动画期间禁止换行，避免文字错位；超宽时省略号截断 */
  white-space: nowrap;
  overflow: hidden;
  text-overflow: ellipsis;
}

.model-abbr {
  display: flex;
  align-items: center;
  justify-content: center;
  width: 36px;
  height: 36px;
  font-size: 16px;
  font-weight: 600;
  color: var(--text-color);
  border-radius: 6px;
  background-color: var(--hover-bg);
  text-transform: uppercase;
}

.empty-models {
  text-align: center;
  padding: 40px 20px;
  color: #999;
}

.empty-models .hint {
  font-size: 12px;
  margin-top: 8px;
}

.collapse-btn {
  position: absolute;
  bottom: 12px;
  right: 12px;
}

/* 右侧内容区 */
.content-area {
  flex: 1;
  position: relative;
  overflow: hidden;
  background-color: var(--bg-color);
}

/* WebView 容器 */
#webview-container {
  width: 100%;
  height: 100%;
  position: absolute;
  top: 0;
  left: 0;
  background-color: var(--bg-color);
}

/* 欢迎页面 */
.welcome-screen {
  position: absolute;
  top: 0;
  left: 0;
  right: 0;
  bottom: 0;
  display: flex;
  align-items: center;
  justify-content: center;
  background-color: var(--bg-color);
}

.welcome-content {
  text-align: center;
  max-width: 600px;
  padding: 40px;
}

.welcome-content h1 {
  font-size: 32px;
  font-weight: 700;
  margin-bottom: 16px;
  color: var(--text-color);
}

.welcome-content p {
  font-size: 16px;
  color: #666;
  margin-bottom: 32px;
}

.quick-actions {
  display: flex;
  justify-content: center;
  gap: 12px;
}

/* 模型切换过渡层 */
.switch-overlay {
  position: absolute;
  top: 0;
  left: 0;
  right: 0;
  bottom: 0;
  display: flex;
  align-items: center;
  justify-content: center;
  background-color: var(--bg-color);
  z-index: 5;
}

.switch-loading,
.switch-error {
  text-align: center;
  display: flex;
  flex-direction: column;
  align-items: center;
  gap: 16px;
}

.switch-text {
  font-size: 15px;
  color: var(--text-color);
  margin: 0;
}

.switch-error .el-icon {
  color: #f56c6c;
}
</style>
