const { app } = require('electron')
const fs = require('fs')
const path = require('path')

const CONFIG_FILE = path.join(app.getPath('userData'), 'model_config.json')

function getDefaults() {
  return {
    theme: 'system',
    serverUrl: '',
    models: [
      // === 英伟达及特殊公共模型平台 ===
      {
        id: 'nvidia-build',
        name: 'NVIDIA Build (NIM)',
        url: 'https://build.nvidia.com/',
        version: 'Nemotron 4 / Llama3',
        visible: true
      },
      {
        id: 'grok',
        name: 'Grok (xAI)',
        url: 'https://grok.com/',
        version: 'Grok 2 / 3',
        visible: true
      },

      // === 国际顶级独立模型及提供商 ===
      {
        id: 'deepseek',
        name: 'DeepSeek',
        url: 'https://chat.deepseek.com/',
        version: 'V3 / R1',
        visible: true
      },
      {
        id: 'chatgpt',
        name: 'ChatGPT (OpenAI)',
        url: 'https://chatgpt.com',
        version: '4o-mini / o1-mini',
        visible: true
      },
      {
        id: 'gemini',
        name: 'Google Gemini',
        url: 'https://google.com',
        version: '2.0 Flash',
        visible: true
      },
      {
        id: 'claude',
        name: 'Claude AI (Anthropic)',
        url: 'https://claude.ai',
        version: '3.5 Sonnet',
        visible: true
      },
      {
        id: 'copilot',
        name: 'Microsoft Copilot',
        url: 'https://microsoft.com',
        version: 'GPT-4o',
        visible: true
      },
      {
        id: 'mistral',
        name: 'Mistral Le Chat',
        url: 'https://mistral.ai',
        version: 'Large 3 / Pixtral',
        visible: true
      },
      {
        id: 'meta-ai',
        name: 'Meta AI',
        url: 'https://meta.ai',
        version: 'Llama 3.3',
        visible: true
      },
      {
        id: 'perplexity',
        name: 'Perplexity AI',
        url: 'https://perplexity.ai',
        version: 'Default Search',
        visible: true
      },

      // === 国内主流大厂及顶尖团队 ===
      {
        id: 'doubao',
        name: '豆包 (字节跳动)',
        url: 'https://doubao.com',
        version: 'Doubao-pro',
        visible: true
      },
      {
        id: 'xiaoyi',
        name: '小艺 (华为)',
        url: 'https://huawei.com',
        version: '盘古大模型',
        visible: true
      },
      {
        id: 'yuanbao',
        name: '腾讯元宝',
        url: 'https://tencent.com',
        version: '混元 Hy3 / DeepSeek',
        visible: true
      },
      {
        id: 'yiyan',
        name: '文心一言 (百度)',
        url: 'https://baidu.com',
        version: '文心 4.5',
        visible: true
      },
      {
        id: 'qwen',
        name: '通义千问 (阿里)',
        url: 'https://qwenchat.com',
        version: '2.5 / Max',
        visible: true
      },
      {
        id: 'kimi',
        name: 'Kimi Chat (月之暗面)',
        url: 'https://moonshot.cn',
        version: 'K2.6',
        visible: true
      },
      {
        id: 'xinghuo',
        name: '讯飞星火 (科大讯飞)',
        url: 'https://xfyun.cn',
        version: '4.0 Ultra',
        visible: true
      },
      {
        id: 'sensechat',
        name: '商量 (商汤科技)',
        url: 'https://sensetime.com',
        version: 'SenseNova 5.5',
        visible: true
      }
    ]

  }
}

function getConfigPath() {
  return CONFIG_FILE
}

function readConfig() {
  try {
    if (!fs.existsSync(CONFIG_FILE)) {
      return getDefaults()
    }
    const raw = fs.readFileSync(CONFIG_FILE, 'utf-8')
    return JSON.parse(raw)
  } catch (err) {
    console.error('读取配置文件失败:', err.message)
    return getDefaults()
  }
}

function writeConfig(config) {
  try {
    const dir = path.dirname(CONFIG_FILE)
    if (!fs.existsSync(dir)) {
      fs.mkdirSync(dir, { recursive: true })
    }
    fs.writeFileSync(CONFIG_FILE, JSON.stringify(config, null, 2), 'utf-8')
    return { success: true }
  } catch (err) {
    console.error('写入配置文件失败:', err.message)
    return { success: false, error: err.message }
  }
}

async function syncFromCloud(serverUrl) {
  if (!serverUrl) {
    return { success: false, error: '服务器地址未配置' }
  }

  try {
    const controller = new AbortController()
    const timeoutId = setTimeout(() => controller.abort(), 10000)

    const response = await fetch(`${serverUrl}/api/models`, {
      signal: controller.signal
    })
    clearTimeout(timeoutId)

    if (!response.ok) {
      return { success: false, error: `服务器响应错误: ${response.status}` }
    }

    const serverData = await response.json()
    const serverModels = serverData.models || []

    const localConfig = readConfig()
    const localModels = localConfig.models || []

    // 建立本地模型 Map（以 id 为键）
    const localMap = new Map()
    for (const m of localModels) {
      localMap.set(m.id, m)
    }

    // 以服务器为底进行合并
    const mergedModels = []
    const serverIds = new Set()

    for (const serverModel of serverModels) {
      serverIds.add(serverModel.id)
      const localModel = localMap.get(serverModel.id)

      if (localModel) {
        // 本地存在：保留 visible，覆盖其他字段
        mergedModels.push({
          ...serverModel,
          visible: localModel.visible !== undefined ? localModel.visible : true
        })
      } else {
        // 本地不存在：作为增量新增，默认 visible: true
        mergedModels.push({
          ...serverModel,
          visible: true
        })
      }
    }

    // 服务器删除的 id，本地也剔除（不在 mergedModels 中的即为剔除）

    const mergedConfig = {
      ...localConfig,
      serverUrl: serverData.serverUrl || localConfig.serverUrl,
      models: mergedModels
    }

    const writeResult = writeConfig(mergedConfig)
    if (!writeResult.success) {
      return { success: false, error: writeResult.error }
    }

    return { success: true, data: mergedConfig }
  } catch (err) {
    if (err.name === 'AbortError') {
      return { success: false, error: '同步超时' }
    }
    return { success: false, error: err.message }
  }
}

module.exports = {
  getConfigPath,
  readConfig,
  writeConfig,
  syncFromCloud
}
