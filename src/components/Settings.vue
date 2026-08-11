<script setup lang="ts">
import { ref, computed, watch, nextTick } from 'vue'
import { Plus, Refresh, Check } from '@element-plus/icons-vue'
import { ElMessage, ElMessageBox } from 'element-plus'
import Sortable from 'sortablejs'
import { tauriAPI, type AppConfig, type Model, type UpdateInfo } from '../utils/tauri-api'
import { applyTheme, type ThemeMode } from '../utils/theme'

const props = defineProps<{
  modelValue: boolean
  config: AppConfig | null
  sidebarCollapsed: boolean
}>()

const emit = defineEmits<{
  'update:modelValue': [value: boolean]
  'save': [config: AppConfig]
  'sync': [serverUrl: string]
  'close': []
}>()

const localConfig = ref<AppConfig | null>(null)
const isSyncing = ref(false)
const isCheckingAll = ref(false)
const editDialogVisible = ref(false)
const editingModel = ref<Model | null>(null)
const isEditDialog = ref(false)
const tableRef = ref()
const sortableInstance = ref<Sortable | null>(null)

// 应用更新相关状态
const currentVersion = ref('')
const isCheckingUpdate = ref(false)
const isDownloading = ref(false)
const updateInfo = ref<UpdateInfo | null>(null)
const updateCheckDone = ref(false)
const downloadProgress = ref(0)

// 深拷贝配置
watch(() => props.config, (newConfig) => {
  if (newConfig) {
    localConfig.value = JSON.parse(JSON.stringify(newConfig))
  }
}, { deep: true, immediate: true })

// 检测是否有变更
const hasChanges = computed(() => {
  return JSON.stringify(localConfig.value) !== JSON.stringify(props.config)
})

// 关闭弹窗时自动保存
function handleClose() {
  if (localConfig.value && hasChanges.value) {
    emit('save', localConfig.value)
  }
  emit('update:modelValue', false)
  emit('close') // 通知父组件关闭 iframe
}

// 主题切换
function handleThemeChange(theme: ThemeMode) {
  if (!localConfig.value) return
  localConfig.value.theme = theme
  applyTheme(theme)
}

// 云端同步
async function handleSync() {
  if (!localConfig.value?.serverUrl) {
    ElMessage.warning('请先配置服务器地址')
    return
  }
  
  isSyncing.value = true
  try {
    emit('sync', localConfig.value.serverUrl)
  } finally {
    isSyncing.value = false
  }
}

// 新增模型
function addModel() {
  isEditDialog.value = false
  editingModel.value = {
    id: '',
    name: '',
    url: '',
    version: '',
    visible: true,
  }
  editDialogVisible.value = true
}

// 编辑模型
function editModel(model: Model) {
  isEditDialog.value = true
  editingModel.value = { ...model }
  editDialogVisible.value = true
}

// 保存模型编辑
function saveModelEdit() {
  if (!editingModel.value || !localConfig.value) return
  
  if (!editingModel.value.id || !editingModel.value.name || !editingModel.value.url) {
    ElMessage.warning('请填写完整信息')
    return
  }
  
  if (isEditDialog.value) {
    // 编辑模式：更新现有模型
    const index = localConfig.value.models.findIndex(m => m.id === editingModel.value!.id)
    if (index !== -1) {
      localConfig.value.models[index] = { ...editingModel.value }
    }
  } else {
    // 新增模式：检查 ID 是否重复
    const exists = localConfig.value.models.some(m => m.id === editingModel.value!.id)
    if (exists) {
      ElMessage.error('模型 ID 已存在')
      return
    }
    localConfig.value.models.push({ ...editingModel.value })
  }
  
  editDialogVisible.value = false
  ElMessage.success('保存成功')
}

// 删除模型
async function deleteModel(model: Model) {
  try {
    await ElMessageBox.confirm(
      `确定要删除模型 "${model.name}" 吗？`,
      '确认删除',
      {
        confirmButtonText: '确定',
        cancelButtonText: '取消',
        type: 'warning',
      }
    )
    
    if (!localConfig.value) return
    localConfig.value.models = localConfig.value.models.filter(m => m.id !== model.id)
    ElMessage.success('删除成功')
  } catch {
    // 用户取消
  }
}

// 拖拽排序 - 使用 SortableJS
function initSortable() {
  const tbody = tableRef.value?.$el?.querySelector('.el-table__body-wrapper tbody')
  if (!tbody) {
    console.warn('[Sortable] tbody not found')
    return
  }

  // 销毁旧实例
  if (sortableInstance.value) {
    sortableInstance.value.destroy()
    sortableInstance.value = null
  }

  sortableInstance.value = Sortable.create(tbody, {
    animation: 150,
    handle: '.drag-handle',
    forceFallback: true,
    ghostClass: 'sortable-ghost',
    chosenClass: 'sortable-chosen',
    dragClass: 'sortable-drag',
    onEnd: ({ newIndex, oldIndex }) => {
      if (oldIndex === undefined || newIndex === undefined || !localConfig.value) return
      const currRow = localConfig.value.models.splice(oldIndex, 1)[0]
      localConfig.value.models.splice(newIndex, 0, currRow)
    }
  })
}

// 弹窗打开动画结束后初始化拖拽
function onDialogOpened() {
  nextTick(() => {
    initSortable()
  })
}

// 检测模型可用性
async function checkModel(model: Model) {
  try {
    const accessible = await tauriAPI.checkModelUrl(model.url)
    if (accessible) {
      ElMessage.success(`${model.name} 可访问`)
    } else {
      ElMessage.warning(`${model.name} 无法访问`)
    }
  } catch {
    ElMessage.error(`检测 ${model.name} 失败`)
  }
}

// 一键检测所有模型
async function checkAllModels() {
  if (!localConfig.value || localConfig.value.models.length === 0) {
    ElMessage.warning('没有可检测的模型')
    return
  }

  isCheckingAll.value = true
  const accessibleModels: Model[] = []
  const inaccessibleModels: Model[] = []

  try {
    // 并行检测所有模型
    const checkPromises = localConfig.value.models.map(async (model) => {
      try {
        const accessible = await tauriAPI.checkModelUrl(model.url)
        if (accessible) {
          accessibleModels.push(model)
          model.visible = true // 自动勾选可访问的模型
        } else {
          model.visible = false // 自动取消勾选不可访问的模型
          inaccessibleModels.push(model)
        }
      } catch {
        model.visible = false // 检测异常也取消勾选
        inaccessibleModels.push(model)
      }
    })

    await Promise.all(checkPromises)

    // 重新排序：可访问的在前，不可访问的在后
    localConfig.value.models = [...accessibleModels, ...inaccessibleModels]

    ElMessage.success(
      `检测完成！${accessibleModels.length} 个模型可访问，${inaccessibleModels.length} 个无法访问`
    )
  } finally {
    isCheckingAll.value = false
  }
}

// 获取当前版本号（从 package.json 在构建时注入）
// 使用 Vite 的 import.meta 特性获取版本
currentVersion.value = __APP_VERSION__

// 检查应用更新
async function handleCheckUpdate() {
  isCheckingUpdate.value = true
  updateCheckDone.value = false
  updateInfo.value = null

  try {
    const result = await tauriAPI.checkForUpdate()
    updateCheckDone.value = true

    if (result) {
      updateInfo.value = result
      ElMessage.success(`发现新版本 v${result.latest}`)
    } else {
      ElMessage.success('当前已是最新版本')
    }
  } catch (error) {
    ElMessage.error('检查更新失败: ' + (error as string))
    updateCheckDone.value = true
  } finally {
    isCheckingUpdate.value = false
  }
}

// 下载并安装更新
async function handleDownloadUpdate() {
  if (!updateInfo.value) return

  try {
    await ElMessageBox.confirm(
      `即将下载 v${updateInfo.value.latest}，下载完成后将打开安装程序。\n\n应用会自动关闭，请手动完成安装后重新启动应用。\n\n更新日志：\n${updateInfo.value.notes || '无'}`,
      '下载并安装更新',
      {
        confirmButtonText: '立即更新',
        cancelButtonText: '取消',
        type: 'info',
      }
    )
  } catch {
    return // 用户取消
  }

  isDownloading.value = true
  downloadProgress.value = 0

  try {
    // 模拟下载进度（实际下载由 Rust 后端处理，无进度回调）
    const progressTimer = setInterval(() => {
      if (downloadProgress.value < 90) {
        downloadProgress.value += Math.random() * 15
      }
    }, 500)

    // autoInstall = true：Rust 下载完成后自动启动安装程序并退出应用
    await tauriAPI.downloadUpdate(
      updateInfo.value.url,
      updateInfo.value.sha256,
      true  // 自动安装
    )

    clearInterval(progressTimer)
    downloadProgress.value = 100

    // 如果执行到这里，说明自动安装未生效（非 Windows 平台），回退到手动打开
    ElMessage.success('下载完成，即将打开安装程序')
  } catch (error) {
    ElMessage.error('下载失败: ' + (error as string))
  } finally {
    isDownloading.value = false
    downloadProgress.value = 0
  }
}

// 打开 GitHub Releases 页面（手动下载）
function openReleasesPage() {
  import('@tauri-apps/plugin-opener').then(({ openUrl }) => {
    openUrl('https://github.com/yzj0405/FreeWebAI/releases/latest')
  }).catch(() => {
    window.open('https://github.com/yzj0405/FreeWebAI/releases/latest', '_blank')
  })
}
</script>

<template>
  <el-dialog 
    :model-value="modelValue"
    @update:model-value="handleClose"
    @opened="onDialogOpened"
    title="设置"
    width="900px"
    :close-on-click-modal="false"
    :style="{ transform: sidebarCollapsed ? 'translateX(30px)' : 'translateX(120px)' }"
  >
    <div v-if="localConfig" class="settings-container">
      <!-- 主题设置 -->
      <el-form-item label="主题">
        <el-radio-group :model-value="localConfig.theme" @change="handleThemeChange">
          <el-radio-button value="light">浅色</el-radio-button>
          <el-radio-button value="dark">深色</el-radio-button>
          <el-radio-button value="system">跟随系统</el-radio-button>
        </el-radio-group>
      </el-form-item>

      <!-- 应用更新 -->
      <div class="update-section">
        <div class="section-header">
          <h3>应用更新</h3>
        </div>
        <div class="update-info-row">
          <span class="update-label">当前版本：</span>
          <span class="update-value">v{{ currentVersion }}</span>
          <el-button
            size="small"
            :loading="isCheckingUpdate"
            @click="handleCheckUpdate"
            style="margin-left: auto"
          >
            <el-icon><Refresh /></el-icon>
            检查更新
          </el-button>
        </div>

        <!-- 发现新版本 -->
        <div v-if="updateInfo" class="update-available">
          <el-alert
            :title="`发现新版本 v${updateInfo.latest}`"
            type="success"
            :closable="false"
            show-icon
          >
            <template #default>
              <div class="update-notes" v-if="updateInfo.notes">{{ updateInfo.notes }}</div>
            </template>
          </el-alert>
          <div class="update-actions">
            <el-button
              type="primary"
              :loading="isDownloading"
              @click="handleDownloadUpdate"
            >
              <template v-if="isDownloading">
                下载中 {{ Math.round(downloadProgress) }}%
              </template>
              <template v-else>
                下载并安装
              </template>
            </el-button>
            <el-button @click="openReleasesPage">手动下载</el-button>
          </div>
          <el-progress
            v-if="isDownloading"
            :percentage="Math.round(downloadProgress)"
            :stroke-width="6"
            :show-text="false"
            style="margin-top: 8px"
          />
        </div>

        <!-- 已是最新版本 -->
        <div v-else-if="updateCheckDone && !updateInfo" class="update-latest">
          <el-alert title="当前已是最新版本" type="info" :closable="false" show-icon />
        </div>
      </div>

      <!-- 云端同步 -->
      <el-form-item label="服务器地址">
        <div style="display: flex; gap: 10px; width: 100%">
          <el-input 
            v-model="localConfig.serverUrl" 
            placeholder="https://raw.githubusercontent.com/yzj0405/FreeWebAI/refs/heads/v2.0/config/model_web.json"
            clearable
            style="flex: 1"
          />
          <el-button 
            :loading="isSyncing" 
            @click="handleSync"
          >
            <el-icon><Refresh /></el-icon>
            检查更新
          </el-button>
        </div>
      </el-form-item>

      <!-- 模型管理 -->
      <div class="model-management">
        <div class="section-header">
          <h3>模型管理</h3>
          <div class="header-actions">
            <el-button 
              type="success" 
              size="small" 
              @click="checkAllModels"
              :loading="isCheckingAll"
            >
              <el-icon><Check /></el-icon>
              一键检测
            </el-button>
            <el-button type="primary" size="small" @click="addModel">
              <el-icon><Plus /></el-icon>
              新增模型
            </el-button>
          </div>
        </div>

        <el-table 
          ref="tableRef"
          :data="localConfig.models" 
          border
          style="width: 100%"
          max-height="400"
          :row-key="(row: Model) => row.id"
        >
          <el-table-column label="拖拽" width="60" align="center">
            <template #default>
              <div class="drag-handle">
                ⋮⋮
              </div>
            </template>
          </el-table-column>
          
          <el-table-column label="可见" width="80" align="center">
            <template #default="{ row }">
              <el-checkbox v-model="row.visible" />
            </template>
          </el-table-column>
          
          <el-table-column prop="name" label="名称" min-width="150" />
          <el-table-column prop="url" label="URL" min-width="200" show-overflow-tooltip />
          <el-table-column prop="version" label="版本" width="120" />
          
          <el-table-column label="操作" width="220" align="center">
            <template #default="{ row }">
              <div class="action-buttons">
                <el-button size="small" @click="checkModel(row)">
                  <el-icon><Check /></el-icon>
                  检测
                </el-button>
                <el-button size="small" @click="editModel(row)">编辑</el-button>
                <el-button size="small" type="danger" @click="deleteModel(row)">删除</el-button>
              </div>
            </template>
          </el-table-column>
        </el-table>
      </div>
    </div>

    <template #footer>
      <div class="dialog-footer">
        <el-button @click="handleClose">关闭</el-button>
      </div>
    </template>
  </el-dialog>

  <!-- 编辑/新增模型弹窗 -->
  <el-dialog
    v-model="editDialogVisible"
    :title="isEditDialog ? '编辑模型' : '新增模型'"
    width="500px"
  >
    <el-form v-if="editingModel" :model="editingModel" label-width="80px">
      <el-form-item label="ID" v-if="!isEditDialog">
        <el-input v-model="editingModel.id" placeholder="例如: chatgpt" />
      </el-form-item>
      <el-form-item label="名称">
        <el-input v-model="editingModel.name" placeholder="例如: ChatGPT" />
      </el-form-item>
      <el-form-item label="URL">
        <el-input v-model="editingModel.url" placeholder="https://..." />
      </el-form-item>
      <el-form-item label="版本">
        <el-input v-model="editingModel.version" placeholder="例如: GPT-4" />
      </el-form-item>
      <el-form-item label="可见">
        <el-switch v-model="editingModel.visible" />
      </el-form-item>
    </el-form>
    
    <template #footer>
      <el-button @click="editDialogVisible = false">取消</el-button>
      <el-button type="primary" @click="saveModelEdit">确定</el-button>
    </template>
  </el-dialog>
</template>

<style scoped>
.settings-container {
  display: flex;
  flex-direction: column;
  gap: 20px;
}

.model-management {
  margin-top: 10px;
}

.section-header {
  display: flex;
  justify-content: space-between;
  align-items: center;
  margin-bottom: 12px;
}

.section-header h3 {
  font-size: 16px;
  font-weight: 600;
  margin: 0;
}

.header-actions {
  display: flex;
  gap: 8px;
  align-items: center;
}

.drag-handle {
  cursor: move;
  color: #999;
  font-size: 16px;
  user-select: none;
}

.drag-handle:hover {
  color: #666;
}

.dialog-footer {
  display: flex;
  justify-content: flex-end;
  align-items: center;
}

.unsaved-hint {
  font-size: 12px;
  color: #999;
}

.action-buttons {
  display: flex;
  gap: 6px;
  justify-content: center;
  flex-wrap: nowrap;
}

.action-buttons .el-button {
  margin: 0;
  white-space: nowrap;
}

/* 应用更新区域 */
.update-section {
  border: 1px solid var(--el-border-color-lighter);
  border-radius: 8px;
  padding: 16px;
}

.update-info-row {
  display: flex;
  align-items: center;
  gap: 8px;
  margin-top: 8px;
}

.update-label {
  font-size: 14px;
  color: var(--el-text-color-regular);
}

.update-value {
  font-size: 14px;
  font-weight: 600;
  color: var(--el-text-color-primary);
}

.update-available {
  margin-top: 12px;
}

.update-notes {
  font-size: 13px;
  color: var(--el-text-color-regular);
  white-space: pre-line;
  margin-top: 4px;
}

.update-actions {
  display: flex;
  gap: 10px;
  margin-top: 10px;
}

.update-latest {
  margin-top: 12px;
}

/* SortableJS 拖拽样式 */
.sortable-ghost {
  opacity: 0.4;
  background: var(--el-fill-color-light);
}

.sortable-chosen {
  background: var(--el-color-primary-light-9);
}

.sortable-drag {
  opacity: 0.8;
  background: var(--el-bg-color);
  box-shadow: 0 2px 12px 0 rgba(0, 0, 0, 0.1);
}
</style>
