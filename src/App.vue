<script setup lang="ts">
import { ref, computed, onMounted, onBeforeUnmount } from 'vue'
import { Setting, Minus, FullScreen, Close, DArrowLeft, DArrowRight } from '@element-plus/icons-vue'
import { ElMessage } from 'element-plus'
import { tauriAPI, type Model, type AppConfig } from './utils/tauri-api'
import { getCurrentWindow } from '@tauri-apps/api/window'
import { applyTheme, type ThemeMode } from './utils/theme'
import Settings from './components/Settings.vue'

const config = ref<AppConfig | null>(null)
const settingsVisible = ref(false)
const isMaximized = ref(false)
const loading = ref(true)

// 当前选中的模型
const activeModel = ref<Model | null>(null)
const webviewLoaded = ref(false)
let sidebarTimer: ReturnType<typeof setTimeout> | null = null
let resizeTimer: ReturnType<typeof setTimeout> | null = null
let unlistenResize: (() => void) | null = null

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

function handleModelClick(model: Model) {
  if (activeModel.value?.id === model.id) return
  activeModel.value = model
  loadModelInWebview(model.url)
}

// 通过 Rust 命令加载网页到右侧 WebView（URL 变化时重建）
async function loadModelInWebview(url: string) {
  try {
    webviewLoaded.value = false
    
    // 等待 DOM 更新后获取容器位置
    await new Promise(resolve => setTimeout(resolve, 50))
    
    const container = document.getElementById('webview-container')
    if (!container) {
      ElMessage.error('容器未找到')
      return
    }
    
    const rect = container.getBoundingClientRect()
    
    await tauriAPI.loadContentWebview(url, rect.left, rect.top, rect.width, rect.height)
    webviewLoaded.value = true
  } catch (error) {
    console.error('加载 WebView 失败:', error)
    ElMessage.error('网页加载失败: ' + (error as Error).message)
  }
}

// 仅调整 WebView 位置/大小（不重建，无白屏闪烁）
async function resizeWebviewOnly() {
  try {
    const container = document.getElementById('webview-container')
    if (!container) return
    const rect = container.getBoundingClientRect()
    await tauriAPI.resizeContentWebview(rect.left, rect.top, rect.width, rect.height)
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
  
  // 侧边栏宽度变化后，重新调整 WebView 位置
  if (activeModel.value) {
    await new Promise(resolve => setTimeout(resolve, 350))
    resizeWebviewOnly()
  }
}

// 打开设置弹窗
async function openSettings() {
  // 隐藏原生 WebView，否则会遮挡 dialog
  if (activeModel.value) {
    await tauriAPI.hideContentWebview()
    webviewLoaded.value = false
  }
  settingsVisible.value = true
}

// 关闭设置弹窗
async function closeSettings() {
  settingsVisible.value = false
  // 恢复 WebView
  if (activeModel.value) {
    loadModelInWebview(activeModel.value.url)
  }
}

onMounted(async () => {
  await loadConfig()
  
  // 监听窗口大小变化，自动调整 WebView
  unlistenResize = await getCurrentWindow().onResized(() => {
    scheduleWebviewResize()
  })
})

async function handleSaveConfig(newConfig: AppConfig) {
  try {
    await tauriAPI.saveConfig(newConfig)
    config.value = newConfig
    applyTheme(newConfig.theme as ThemeMode)
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
  if (unlistenResize) unlistenResize()
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
        <el-button @click="openSettings" circle size="small">
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
      <aside class="sidebar" :class="{ collapsed: config?.sidebarCollapsed }">
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
}

.sidebar.collapsed {
  width: 60px;
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
</style>
