<template>
  <div class="app-container">
    <!-- 自定义顶栏 -->
    <div class="titlebar">
      <div class="titlebar-brand">
        <span class="titlebar-logo">AI</span>
        <span class="titlebar-text">Hub Desktop</span>
      </div>
      <div class="titlebar-controls">
        <el-button :icon="Refresh" circle @click="refreshView" class="control-btn" title="刷新当前页面" />
        <el-button :icon="Setting" circle @click="settingsVisible = true" class="control-btn" title="设置" />
        <el-button :icon="Minus" circle @click="minimize" class="control-btn" />
        <el-button :icon="isMaximized ? CopyDocument : FullScreen" circle @click="maximize" class="control-btn" />
        <el-button :icon="Close" circle @click="close" class="control-btn close-btn" />
      </div>
    </div>

    <!-- 左侧模型列表 -->
    <div class="sidebar">
      <div class="sidebar-header">模型列表</div>
      <div class="model-list">
        <div
          v-for="model in visibleModels"
          :key="model.id"
          :class="['model-item', { active: activeModelId === model.id }]"
          @click="switchModel(model)"
        >
          <span class="model-icon">{{ getModelIcon(model.name) }}</span>
          <span class="model-name">{{ model.name }}</span>
        </div>
        <div v-if="visibleModels.length === 0" class="empty-hint">
          暂无可用模型，请在设置中启用
        </div>
      </div>
    </div>

    <!-- 设置弹窗 -->
    <Settings
      v-model="settingsVisible"
      :models="allModels"
      :server-url="serverUrl"
      :theme="theme"
      @save="onSettingsSave"
      @theme-change="applyTheme"
    />

    <!-- 模型内容区域 -->
    <div class="content-area">
      <!-- 加载中覆盖层 -->
      <div v-if="viewState === 'loading'" class="view-overlay">
        <div class="view-spinner"></div>
        <span class="view-status-text">正在加载...</span>
      </div>

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
import { Setting, Minus, Close, FullScreen, CopyDocument, Refresh } from '@element-plus/icons-vue'
import { ElMessage } from 'element-plus'
import Settings from './components/Settings.vue'

// 使用 preload 脚本暴露的安全 API
const { ipcRenderer } = window.electronAPI

const allModels = ref([])
const serverUrl = ref('')
const activeModelId = ref(null)
const settingsVisible = ref(false)
const isMaximized = ref(false)
const theme = ref('system')
const viewState = ref('idle')  // 'idle' | 'loading' | 'loaded' | 'error'
const viewError = ref('')

// 主题应用
function applyTheme(t) {
  theme.value = t
  if (t === 'system') {
    const isDark = window.matchMedia('(prefers-color-scheme: dark)').matches
    document.documentElement.setAttribute('data-theme', isDark ? 'dark' : 'light')
  } else {
    document.documentElement.setAttribute('data-theme', t)
  }
  // 同步到原生层，影响 WebContentsView 内网页的 prefers-color-scheme
  ipcRenderer.send('theme:set', t)
}

// 监听系统主题变化
const systemThemeQuery = window.matchMedia('(prefers-color-scheme: dark)')
systemThemeQuery.addEventListener('change', (e) => {
  if (theme.value === 'system') {
    document.documentElement.setAttribute('data-theme', e.matches ? 'dark' : 'light')
  }
})

// 打开设置时隐藏 WebContentsView，关闭时恢复
watch(settingsVisible, (visible) => {
  if (visible) {
    ipcRenderer.send('view:hide')
  } else {
    ipcRenderer.send('view:show')
  }
})

const visibleModels = computed(() => {
  return allModels.value.filter(m => m.visible)
})

function getModelIcon(name) {
  const map = {
    'DeepSeek': 'D',
    'Gemini': 'G',
    '华为小艺': 'H',
    'ChatGPT': 'C',
    'Claude': 'K'
  }
  return map[name] || name.charAt(0).toUpperCase()
}

async function loadConfig() {
  const result = await ipcRenderer.invoke('config:get')
  if (result.success) {
    allModels.value = result.data.models || []
    serverUrl.value = result.data.serverUrl || ''
    applyTheme(result.data.theme || 'system')

    // 自动切换到第一个可见模型
    if (visibleModels.value.length > 0 && !activeModelId.value) {
      switchModel(visibleModels.value[0])
    }
  }
}

async function switchModel(model) {
  if (activeModelId.value === model.id) return

  viewState.value = 'loading'
  viewError.value = ''

  const result = await ipcRenderer.invoke('view:switch', { id: model.id, url: model.url })
  if (result.success) {
    activeModelId.value = model.id
  } else {
    viewState.value = 'error'
    viewError.value = result.error || '切换失败'
    ElMessage.error('切换模型失败: ' + result.error)
  }
}

function minimize() {
  ipcRenderer.send('window:minimize')
}

function maximize() {
  ipcRenderer.send('window:maximize')
}

function close() {
  ipcRenderer.send('window:close')
}

function onSettingsSave({ models, url, theme: newTheme }) {
  // 检测当前活跃模型的 URL 是否发生变化
  const oldActive = allModels.value.find(m => m.id === activeModelId.value)
  const newActive = models.find(m => m.id === activeModelId.value)

  allModels.value = models
  serverUrl.value = url

  // 主题变更立即生效
  if (newTheme && newTheme !== theme.value) {
    applyTheme(newTheme)
  }

  // 如果当前活跃模型的 URL 变了，重新加载
  if (oldActive && newActive && oldActive.url !== newActive.url) {
    ipcRenderer.send('view:reloadUrl', newActive.url)
  }
}

function refreshView() {
  if (activeModelId.value) {
    ipcRenderer.send('view:reload')
    ElMessage.success('已刷新')
  }
}

// 监听配置更新
onMounted(() => {
  loadConfig()

  ipcRenderer.on('config:updated', () => {
    loadConfig()
  })

  // 监听 WebContentsView 加载状态
  ipcRenderer.on('view:state', (data) => {
    if (data.id === activeModelId.value) {
      viewState.value = data.state
      if (data.error) viewError.value = data.error
    }
  })
})
</script>

<style scoped>
.app-container {
  display: flex;
  flex-direction: column;
  width: 100%;
  height: 100%;
  background: var(--bg-primary);
}

/* 顶栏样式 */
.titlebar {
  display: flex;
  align-items: center;
  justify-content: space-between;
  height: 60px;
  padding: 0 16px;
  background: var(--bg-titlebar);
  -webkit-app-region: drag;
  flex-shrink: 0;
  border-bottom: 1px solid var(--border-color);
}

.titlebar-brand {
  display: flex;
  align-items: center;
  gap: 8px;
}

.titlebar-logo {
  font-size: 18px;
  font-weight: 700;
  color: var(--accent);
  letter-spacing: 1px;
}

.titlebar-text {
  font-size: 14px;
  color: var(--text-primary);
  font-weight: 500;
}

.titlebar-controls {
  display: flex;
  align-items: center;
  gap: 8px;
  -webkit-app-region: no-drag;
}

.control-btn {
  background: transparent !important;
  border: none !important;
  color: var(--text-secondary) !important;
  padding: 8px !important;
  width: 32px !important;
  height: 32px !important;
  display: flex !important;
  align-items: center;
  justify-content: center;
}

.control-btn:hover {
  background: rgba(128, 128, 128, 0.15) !important;
  color: var(--text-primary) !important;
}

.close-btn:hover {
  background: var(--accent) !important;
  color: #fff !important;
}

/* 侧边栏样式 */
.sidebar {
  position: absolute;
  top: 60px;
  left: 0;
  bottom: 0;
  width: 200px;
  background: var(--bg-secondary);
  border-right: 1px solid var(--border-subtle);
  display: flex;
  flex-direction: column;
}

.sidebar-header {
  padding: 16px;
  font-size: 13px;
  color: var(--text-secondary);
  font-weight: 600;
  text-transform: uppercase;
  letter-spacing: 0.5px;
  border-bottom: 1px solid var(--border-subtle);
}

.model-list {
  flex: 1;
  overflow-y: auto;
  padding: 8px;
}

.model-item {
  display: flex;
  align-items: center;
  gap: 10px;
  padding: 12px;
  border-radius: 8px;
  cursor: pointer;
  transition: background 0.2s;
  color: var(--text-primary);
  margin-bottom: 4px;
}

.model-item:hover {
  background: rgba(128, 128, 128, 0.12);
}

.model-item.active {
  background: rgba(233, 69, 96, 0.15);
  color: var(--accent);
}

.model-icon {
  width: 28px;
  height: 28px;
  border-radius: 6px;
  background: rgba(128, 128, 128, 0.15);
  display: flex;
  align-items: center;
  justify-content: center;
  font-size: 13px;
  font-weight: 600;
}

.model-item.active .model-icon {
  background: rgba(233, 69, 96, 0.3);
}

.model-name {
  font-size: 14px;
  font-weight: 500;
}

.empty-hint {
  padding: 24px 16px;
  text-align: center;
  color: var(--text-muted);
  font-size: 13px;
}

/* 内容区域 */
.content-area {
  position: absolute;
  top: 60px;
  left: 200px;
  right: 0;
  bottom: 0;
  background: var(--bg-secondary);
}

.welcome-screen {
  display: flex;
  flex-direction: column;
  align-items: center;
  justify-content: center;
  height: 100%;
  color: var(--text-secondary);
}

.welcome-title {
  font-size: 24px;
  font-weight: 600;
  margin-bottom: 12px;
  color: var(--text-primary);
}

.welcome-desc {
  font-size: 14px;
  color: var(--text-secondary);
}

/* 加载覆盖层 */
.view-overlay {
  display: flex;
  flex-direction: column;
  align-items: center;
  justify-content: center;
  height: 100%;
  gap: 16px;
  color: var(--text-secondary);
}

.view-spinner {
  width: 36px;
  height: 36px;
  border: 3px solid rgba(128,128,128,.2);
  border-top-color: var(--accent);
  border-radius: 50%;
  animation: spin .8s linear infinite;
}

@keyframes spin {
  to { transform: rotate(360deg); }
}

.view-status-text {
  font-size: 14px;
  color: var(--text-secondary);
}
</style>
