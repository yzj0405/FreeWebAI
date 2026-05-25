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

/**
 * 从云端同步模型列表
 * 目标数据格式与 model_web.json 一致: { models: [{ id, name, url, version, visible }] }
 * @param {string} serverUrl - 服务器地址，如 https://example.com
 */
async function syncFromCloud(serverUrl) {
  if (!serverUrl) {
    return { success: false, error: '服务器地址未配置' }
  }

  // 规范化 URL（去掉尾部斜杠）
  const baseUrl = serverUrl.replace(/\/+$/, '')
  // 如果已经是文件地址（如完整 GitHub raw URL），直接用；否则拼接 /model_web.json
  const fetchUrl = baseUrl.endsWith('.json') ? baseUrl : `${baseUrl}/model_web.json`

  try {
    const controller = new AbortController()
    const timeoutId = setTimeout(() => controller.abort(), 10000)

    const response = await fetch(fetchUrl, {
      signal: controller.signal,
      headers: { 'Accept': 'application/json' }
    })
    clearTimeout(timeoutId)

    if (!response.ok) {
      if (response.status === 404) {
        return { success: false, error: `未找到配置文件 (${fetchUrl})，请确认服务器上存在 model_web.json` }
      }
      return { success: false, error: `服务器响应错误 (${response.status}): ${response.statusText}` }
    }

    const contentType = response.headers.get('content-type') || ''
    if (!contentType.includes('application/json') && !contentType.includes('text/plain')) {
      return { success: false, error: `响应类型异常: ${contentType}` }
    }

    let serverData
    try {
      serverData = await response.json()
    } catch {
      return { success: false, error: '数据格式错误，无法解析 JSON' }
    }

    // 验证数据结构
    if (!serverData || typeof serverData !== 'object') {
      return { success: false, error: '数据格式错误: 期望 JSON 对象' }
    }
    if (!Array.isArray(serverData.models)) {
      return { success: false, error: '数据格式错误: 缺少 models 数组' }
    }

    const serverModels = serverData.models

    // 验证每个模型至少包含必要字段
    for (let i = 0; i < serverModels.length; i++) {
      const m = serverModels[i]
      if (!m.id || !m.name || !m.url) {
        return { success: false, error: `数据格式错误: 第 ${i + 1} 个模型缺少 id/name/url` }
      }
      // 补充默认字段
      if (m.visible === undefined) m.visible = true
      if (!m.version) m.version = ''
    }

    const localConfig = readConfig()
    const localModels = localConfig.models || []

    // 建立本地模型 Map（以 id 为键）
    const localMap = new Map()
    for (const m of localModels) {
      localMap.set(m.id, m)
    }

    // 以服务器数据为底进行合并
    const mergedModels = []

    for (const serverModel of serverModels) {
      const localModel = localMap.get(serverModel.id)

      if (localModel) {
        // 本地存在：保留 visible，覆盖其他字段
        mergedModels.push({
          ...serverModel,
          visible: localModel.visible !== undefined ? localModel.visible : serverModel.visible
        })
      } else {
        // 本地不存在：作为增量新增
        mergedModels.push({ ...serverModel })
      }
    }

    // 服务器已删除的模型，本地同步剔除

    const mergedConfig = {
      ...localConfig,
      serverUrl: serverUrl,
      models: mergedModels
    }

    const writeResult = writeConfig(mergedConfig)
    if (!writeResult.success) {
      return { success: false, error: `保存配置失败: ${writeResult.error}` }
    }

    return { success: true, data: mergedConfig }
  } catch (err) {
    if (err.name === 'AbortError') {
      return { success: false, error: '同步超时（10 秒）' }
    }
    return { success: false, error: `网络请求失败: ${err.message}` }
  }
}

module.exports = {
  getConfigPath,
  readConfig,
  writeConfig,
  syncFromCloud
}
