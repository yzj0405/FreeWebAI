<template>
  <el-dialog
    v-model="visible"
    title="设置"
    width="580px"
    :close-on-click-modal="true"
    :append-to-body="false"
    destroy-on-close
    class="settings-dialog"
  >
    <!-- 云端同步 -->
    <div class="block">
      <div class="block-title">主题设置</div>
      <el-radio-group v-model="localTheme" @change="onThemeChange" class="theme-group">
        <el-radio value="light">浅色</el-radio>
        <el-radio value="dark">深色</el-radio>
        <el-radio value="system">跟随系统</el-radio>
      </el-radio-group>
    </div>

    <!-- 云端同步 -->
    <div class="block">
      <div class="block-title">云端同步配置</div>
      <div class="url-row">
        <el-input v-model="localUrl" placeholder="服务器地址，如 https://example.com" clearable />
        <el-button type="primary" :loading="syncing" @click="doSync" class="sync-btn">
          <el-icon><Refresh /></el-icon>同步
        </el-button>
      </div>
    </div>

    <!-- 模型管理 -->
    <div class="block">
      <div class="block-title">模型管理（可拖拽排序、可编辑名称和地址）</div>
      <div v-if="list.length === 0" class="tip">暂无模型，请先配置云端同步地址</div>
      <div class="model-list">
        <div
          v-for="(m, idx) in list"
          :key="m.id"
          :class="['model-row', { 'drag-over': dragOverIdx === idx, 'drag-source': dragIdx === idx }]"
          draggable="true"
          @dragstart="onStart(idx, $event)"
          @dragover.prevent="onOver(idx)"
          @dragleave="onLeave"
          @dragend="onEnd"
          @drop.prevent="onDrop(idx)"
        >
          <span class="drag-handle" title="拖拽排序">≡</span>

          <el-checkbox
            :model-value="m.visible"
            @change="(v) => toggleVisible(m.id, v)"
            class="model-check"
          />

          <el-input
            :model-value="m.name"
            @update:model-value="(v) => { list[idx].name = v }"
            size="small"
            class="name-input"
            placeholder="名称"
          />

          <el-input
            :model-value="m.url"
            @update:model-value="(v) => { list[idx].url = v }"
            size="small"
            class="url-input"
            placeholder="地址"
          />

          <span v-if="m.version" class="ver">v{{ m.version }}</span>
        </div>
      </div>
    </div>

    <!-- 保存按钮 -->
    <div class="dialog-footer">
      <el-button type="primary" @click="save">
        <el-icon></el-icon>保存
      </el-button>
    </div>
  </el-dialog>
</template>

<script setup>
import { ref, computed, watch } from 'vue'
import { Check, Refresh } from '@element-plus/icons-vue'
import { ElMessage } from 'element-plus'

const { ipcRenderer } = window.electronAPI

const props = defineProps({
  modelValue: Boolean,
  models: { type: Array, default: () => [] },
  serverUrl: { type: String, default: '' },
  theme: { type: String, default: 'system' }
})
const emit = defineEmits(['update:modelValue', 'save', 'themeChange'])

const visible = computed({
  get: () => props.modelValue,
  set: v => emit('update:modelValue', v)
})

// ---- 本地可编辑副本 ----
const list = ref([])
const localUrl = ref('https://raw.githubusercontent.com/yzj0405/FreeWebAI/refs/heads/main/config/model_web.json')
const localTheme = ref('system')
const originalJson = ref('')  // 打开弹窗时的原始快照

// 弹窗打开时同步 props → list
watch(visible, (v) => {
  if (v) {
    // 深拷贝，避免直接修改 props
    list.value = props.models.map(m => ({ ...m }))
    localUrl.value = props.serverUrl
    // 记录原始数据快照
    localTheme.value = props.theme
    originalJson.value = JSON.stringify({ models: list.value, url: localUrl.value, theme: localTheme.value })
  }
})

// props 外部变化也同步（如云端同步后）
watch(() => props.models, (newModels) => {
  if (visible.value) {
    list.value = newModels.map(m => ({ ...m }))
  }
})

watch(() => props.serverUrl, (v) => {
  if (visible.value) localUrl.value = v
})

watch(() => props.theme, (v) => {
  if (visible.value) localTheme.value = v
})

// 弹窗关闭时保存（确保最后一次编辑不被丢失）
watch(visible, (v) => {
  if (!v) save()
})

// ---- 可见性切换（实时更新，不立即保存）----
function toggleVisible(id, val) {
  const idx = list.value.findIndex(m => m.id === id)
  if (idx >= 0) {
    list.value[idx].visible = val
  }
}

// ---- 保存 ----
function save() {
  // 深拷贝为纯对象
  const plainModels = JSON.parse(JSON.stringify(list.value))
  const currentJson = JSON.stringify({ models: plainModels, url: localUrl.value, theme: localTheme.value })

  // 无变更则跳过
  if (currentJson === originalJson.value) return

  ipcRenderer.send('config:save', {
    models: plainModels,
    serverUrl: localUrl.value,
    theme: localTheme.value
  })
  emit('save', { models: plainModels, url: localUrl.value, theme: localTheme.value })
  // 更新快照，避免重复提示
  originalJson.value = currentJson
  ElMessage.success('已保存')
}

// ---- 主题变更（立即生效，不等待保存）----
function onThemeChange(val) {
  emit('themeChange', val)
}

// ---- 云端同步 ----
const syncing = ref(false)

async function doSync() {
  if (!localUrl.value.trim()) return ElMessage.warning('请先输入服务器地址')
  syncing.value = true
  try {
    // 传入当前输入的 URL，确保使用的是最新地址
    const r = await ipcRenderer.invoke('config:sync', localUrl.value.trim())
    if (r.success) {
      ElMessage.success('同步成功')
      list.value = (r.data.models || []).map(m => ({ ...m }))
      localUrl.value = r.data.serverUrl || localUrl.value
      save()
    } else {
      ElMessage.error('同步失败: ' + r.error)
    }
  } catch (e) {
    ElMessage.error('请求异常: ' + e.message)
  } finally {
    syncing.value = false
  }
}

// ---- 拖拽排序 ----
const dragIdx = ref(-1)
const dragOverIdx = ref(-1)

function onStart(idx, e) {
  dragIdx.value = idx
  e.dataTransfer.effectAllowed = 'move'
  e.dataTransfer.setData('text/plain', String(idx))
}

function onOver(idx) {
  dragOverIdx.value = idx
}

function onLeave() {
  dragOverIdx.value = -1
}

function onEnd() {
  dragIdx.value = -1
  dragOverIdx.value = -1
}

function onDrop(idx) {
  if (dragIdx.value < 0 || dragIdx.value === idx) {
    dragIdx.value = -1
    dragOverIdx.value = -1
    return
  }
  const arr = [...list.value]
  const [moved] = arr.splice(dragIdx.value, 1)
  arr.splice(idx, 0, moved)
  list.value = arr
  dragIdx.value = -1
  dragOverIdx.value = -1
}
</script>

<style scoped>
.block { margin-bottom: 20px; }
.block:last-child { margin-bottom: 0; }

.block-title {
  font-size: 12px;
  font-weight: 600;
  color: var(--text-muted);
  text-transform: uppercase;
  letter-spacing: .5px;
  margin-bottom: 12px;
  padding-bottom: 8px;
  border-bottom: 1px solid var(--border-subtle);
}

.url-row { display: flex; gap: 10px; }
.url-row .el-input { flex: 1; }
.sync-btn { flex-shrink: 0; }

.tip {
  text-align: center;
  color: var(--text-muted);
  font-size: 13px;
  padding: 16px 0;
}

/* ---- 模型列表 ---- */
.model-list {
  display: flex;
  flex-direction: column;
  gap: 2px;
  max-height: 340px;
  overflow-y: auto;
  padding-right: 4px;
}

/* 滚动条样式 */
.model-list::-webkit-scrollbar {
  width: 5px;
}

.model-list::-webkit-scrollbar-track {
  background: transparent;
}

.model-list::-webkit-scrollbar-thumb {
  background: rgba(128,128,128,.3);
  border-radius: 3px;
}

.model-list::-webkit-scrollbar-thumb:hover {
  background: rgba(128,128,128,.5);
}

.model-row {
  display: flex;
  align-items: center;
  gap: 8px;
  padding: 6px 4px;
  border-radius: 6px;
  transition: background .15s, opacity .15s;
  cursor: default;
}

.model-row:hover {
  background: rgba(128,128,128,.08);
}

.model-row.drag-source {
  opacity: .4;
}

.model-row.drag-over {
  background: rgba(233,69,96,.12);
  outline: 1px dashed rgba(233,69,96,.4);
  outline-offset: -1px;
  border-radius: 6px;
}

/* 拖拽手柄 */
.drag-handle {
  flex-shrink: 0;
  width: 20px;
  text-align: center;
  font-size: 16px;
  color: rgba(128,128,128,.35);
  cursor: grab;
  user-select: none;
  transition: color .15s;
}
.drag-handle:hover {
  color: rgba(128,128,128,.6);
}
.model-row:active .drag-handle {
  cursor: grabbing;
}

/* checkbox */
.model-check {
  flex-shrink: 0;
  margin-right: 0;
}

/* 名称输入框 */
.name-input {
  width: 100px;
  flex-shrink: 0;
}

.url-input {
  flex: 1;
}

/* 底部按钮 */
.dialog-footer {
  display: flex;
  justify-content: flex-end;
  padding-top: 12px;
  border-top: 1px solid var(--border-subtle);
  margin-top: 4px;
}

/* 主题切换 */
.theme-group {
  display: flex;
  gap: 16px;
}

/* 版本号 */
.ver {
  flex-shrink: 0;
  font-size: 11px;
  color: var(--text-muted);
  background: rgba(128,128,128,.1);
  padding: 1px 6px;
  border-radius: 3px;
}
</style>

<style>
/* 全局样式覆盖 el-dialog，适配深/浅主题 */
.settings-dialog {
  --el-dialog-bg-color: var(--dialog-bg);
  --el-dialog-title-font-size: 14px;
}

.settings-dialog.el-dialog {
  background: var(--dialog-bg);
  border: 1px solid var(--border-subtle);
  border-radius: 8px;
}

.settings-dialog .el-dialog__header {
  background: var(--dialog-header-bg);
  margin: 0;
  padding: 14px 24px;
  border-bottom: 1px solid var(--border-color);
  border-radius: 8px 8px 0 0;
  display: flex;
  align-items: center;
  justify-content: space-between;
}

.settings-dialog .el-dialog__title {
  color: var(--text-primary) !important;
  font-size: 15px;
  font-weight: 600;
  letter-spacing: .5px;
  line-height: 1;
}

.settings-dialog .el-dialog__headerbtn {
  position: static;
  width: 32px;
  height: 32px;
  display: flex;
  align-items: center;
  justify-content: center;
  border-radius: 6px;
  transition: background .15s;
  flex-shrink: 0;
}

.settings-dialog .el-dialog__headerbtn:hover {
  background: rgba(128,128,128,.12);
}

.settings-dialog .el-dialog__headerbtn .el-dialog__close {
  color: var(--text-muted) !important;
  font-size: 18px;
  font-weight: 400;
  transition: color .15s;
}

.settings-dialog .el-dialog__headerbtn:hover .el-dialog__close {
  color: var(--text-primary) !important;
}

.settings-dialog .el-dialog__body {
  padding: 20px;
  color: var(--text-primary);
}

/* input 样式 */
.settings-dialog .name-input .el-input__wrapper,
.settings-dialog .url-input .el-input__wrapper {
  background: var(--input-bg);
  box-shadow: 0 0 0 1px var(--input-border) inset;
}
.settings-dialog .name-input .el-input__inner,
.settings-dialog .url-input .el-input__inner {
  color: var(--text-primary);
}
.settings-dialog .name-input .el-input__inner::placeholder,
.settings-dialog .url-input .el-input__inner::placeholder {
  color: var(--text-muted);
}
</style>
