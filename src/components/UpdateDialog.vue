<script setup lang="ts">
import { ref, watch } from 'vue'
import { Refresh } from '@element-plus/icons-vue'
import { ElMessage, ElMessageBox } from 'element-plus'
import { tauriAPI, type UpdateInfo } from '../utils/tauri-api'

const props = defineProps<{
  modelValue: boolean
  // 顶栏启动检测已发现的新版本（可选，传入则打开即显示，无需再检测）
  initialUpdateInfo?: UpdateInfo | null
}>()

const emit = defineEmits<{
  'update:modelValue': [value: boolean]
}>()

// 应用更新相关状态（与设置弹窗中的更新逻辑保持一致）
const currentVersion = ref(__APP_VERSION__)
const isCheckingUpdate = ref(false)
const isDownloading = ref(false)
const updateInfo = ref<UpdateInfo | null>(null)
const updateCheckDone = ref(false)
const downloadProgress = ref(0)

// 打开弹窗时：优先展示顶栏检测到的结果，否则自动检查一次
watch(() => props.modelValue, (visible) => {
  if (!visible) return
  if (props.initialUpdateInfo && !updateCheckDone.value) {
    updateInfo.value = props.initialUpdateInfo
    updateCheckDone.value = true
    return
  }
  if (!updateCheckDone.value && !isCheckingUpdate.value) {
    handleCheckUpdate()
  }
})

function handleClose(value: boolean) {
  emit('update:modelValue', value)
}

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
    title="应用更新"
    width="520px"
    :close-on-click-modal="false"
    append-to-body
  >
    <div class="update-section">
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
  </el-dialog>
</template>

<style scoped>
.update-section {
  display: flex;
  flex-direction: column;
  gap: 12px;
}

.update-info-row {
  display: flex;
  align-items: center;
  gap: 6px;
}

.update-label {
  color: #666;
  font-size: 14px;
}

.update-value {
  font-size: 14px;
  font-weight: 600;
}

.update-actions {
  display: flex;
  gap: 8px;
  margin-top: 8px;
}

.update-notes {
  max-height: 160px;
  overflow-y: auto;
  white-space: pre-wrap;
  font-size: 13px;
  line-height: 1.5;
}
</style>
