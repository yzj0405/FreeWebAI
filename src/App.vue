<script setup lang="ts">
import { ref, computed, onMounted, onBeforeUnmount } from 'vue'
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
let switchTimeoutTimer: ReturnType<typeof setTimeout> | null = null
const SWITCH_TIMEOUT = 15000 // 15秒超时

// WebView 页面加载事件监听器
let unlistenPageLoaded: UnlistenFn | null = null

let sidebarTimer: ReturnType<typeof setTimeout> | null = null
let resizeTimer: ReturnType<typeof setTimeout> | null = null
let unlistenResize: (() => void) | null = null

// 新增：已缓存的模型 ID 集合（用于判断是否需要显示 loading）
const loadedModels = ref<Set<string>>(new Set())

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

// 核心改动：使用缓存机制处理模型点击
async function handleModelClick(model: Model) {
  if (activeModel.value?.id === model.id) return
  
  activeModel.value = model
  
  // 判断是否为首次加载该模型
  const isFirstLoad = !loadedModels.value.has(model.id)
  
  if (isFirstLoad) {
    // 首次加载：需要创建 WebView 并显示 loading
    await loadModelWithCache(model)
  } else {
    // 已缓存：先调整大小，再显示（无 loading 动画，瞬间完成）
    try {
      // 先获取容器当前位置，确保 WebView 大小正确
      await new Promise(resolve => setTimeout(resolve, 10))
      
      const container = document.getElementById('webview-container')
      if (container) {
        const rect = container.getBoundingClientRect()
        // 调整已缓存的 WebView 到正确位置和大小
        await tauriAPI.resizeModelWebview(
          model.id,
          rect.left,
          rect.top,
          rect.width,
          rect.height
        )
      }
      
      // 切换到目标模型（显示目标，隐藏其他）
      await tauriAPI.switchToModel(model.id)
      webviewLoaded.value = true
    } catch (error) {
      console.error('切换模型失败:', error)
      ElMessage.error('切换模型失败')
    }
  }
}

// 使用新缓存 API 加载模型
async function loadModelWithCache(model: Model) {
  // 清除之前的超时定时器
  if (switchTimeoutTimer) clearTimeout(switchTimeoutTimer)
  
  modelSwitchState.value = 'loading'
  modelSwitchError.value = ''
  webviewLoaded.value = false
  
  // 超时兜底
  switchTimeoutTimer = setTimeout(() => {
    if (modelSwitchState.value === 'loading') {
      modelSwitchState.value = 'error'
      modelSwitchError.value = '连接超时，请检查网络后重试'
    }
  }, SWITCH_TIMEOUT)
  
  try {
    // 等待 DOM 更新后获取容器位置
    await new Promise(resolve => setTimeout(resolve, 50))
    
    const container = document.getElementById('webview-container')
    if (!container) {
      if (switchTimeoutTimer) clearTimeout(switchTimeoutTimer)
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
    
    if (isNewCreated) {
      // 新创建的 WebView：标记为已加载，等待页面加载完成事件
      loadedModels.value.add(model.id)
      webviewLoaded.value = true
      // 保持 loading 状态，等待 Rust 侧 on_page_load → webview:page-loaded 事件
    } else {
      // 已缓存的 WebView（理论上不会走到这里，因为前面已经判断过 isFirstLoad）
      webviewLoaded.value = true
      modelSwitchState.value = 'idle'
    }
  } catch (error) {
    if (switchTimeoutTimer) clearTimeout(switchTimeoutTimer)
    console.error('加载 WebView 失败:', error)
    modelSwitchState.value = 'error'
    modelSwitchError.value = '网页加载失败: ' + (error as Error).message
  }
}

// 兼容旧接口已移除，统一使用 loadModelWithCache
// 保留此函数作为参考，实际请使用 loadModelWithCache
// async function loadModelInWebview(url: string) { ... }

function retryLoadModel() {
  if (activeModel.value) {
    // 使用新的缓存 API 重试
    loadModelWithCache(activeModel.value)
  }
}

function refreshCurrentModel() {
  if (activeModel.value) {
    // 刷新当前模型：先从缓存中移除，再重新加载
    const modelId = activeModel.value.id
    loadedModels.value.delete(modelId)
    loadModelWithCache(activeModel.value)
  }
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
  
  // 侧边栏动画完成后，重新调整 WebView 位置
  if (activeModel.value) {
    // 收起：0ms 延迟 + 300ms 动画 + 50ms 余量 = 350ms
    // 展开：300ms 延迟 + 300ms 动画 + 50ms 余量 = 650ms
    const delay = newCollapsed ? 350 : 650
    await new Promise(resolve => setTimeout(resolve, delay))
    resizeWebviewOnly()
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
      
      // 显示该模型的 WebView
      await tauriAPI.switchToModel(activeModel.value.id)
      webviewLoaded.value = true
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
  
  // 监听 WebView 页面真实加载完成事件（Rust on_page_load 回调）
  unlistenPageLoaded = await listen('webview:page-loaded', () => {
    if (switchTimeoutTimer) clearTimeout(switchTimeoutTimer)
    modelSwitchState.value = 'idle'
    // 主题由 initialization_script 在页面加载时自动应用，无需额外同步
  })
  
  // 监听窗口大小变化，自动调整 WebView
  // 注意：Tauri 2.x 使用 onResized 需要导入正确的类型
  const currentWindow = getCurrentWindow()
  unlistenResize = await currentWindow.onResized(() => {
    scheduleWebviewResize()
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
  if (switchTimeoutTimer) clearTimeout(switchTimeoutTimer)
  if (unlistenResize) unlistenResize()
  if (unlistenPageLoaded) unlistenPageLoaded()
})
</script>

<template>
  <div class="app-container" v-loading="loading">
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
              <span class="model-version">{{ model.version }}</span>
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
            <p class="switch-text">正在加载 {{ activeModel?.name }}...</p>
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
  gap: 4px;
}

.model-name {
  font-size: 14px;
  font-weight: 500;
  color: var(--text-color);
}

.model-version {
  font-size: 12px;
  color: #999;
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
