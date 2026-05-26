use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;
use tauri::{AppHandle, Manager};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppConfig {
    pub theme: String,
    #[serde(rename = "serverUrl", default)]
    pub server_url: String,
    #[serde(rename = "sidebarCollapsed", default = "default_sidebar_collapsed")]
    pub sidebar_collapsed: bool,
    pub models: Vec<Model>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Model {
    pub id: String,
    pub name: String,
    pub url: String,
    pub version: String,
    pub visible: bool,
}

fn default_sidebar_collapsed() -> bool {
    false
}

fn default_theme() -> String {
    "system".to_string()
}

fn default_server_url() -> String {
    String::new()
}

/// 获取配置文件路径
pub fn get_config_path(app: &AppHandle) -> PathBuf {
    let app_data = app.path().app_data_dir().expect("Failed to get app data dir");
    app_data.join("model_config.json")
}

/// 获取默认配置
pub fn get_default_config() -> AppConfig {
    AppConfig {
        theme: default_theme(),
        server_url: default_server_url(),
        sidebar_collapsed: default_sidebar_collapsed(),
        models: vec![
            Model {
                id: "deepseek".to_string(),
                name: "DeepSeek".to_string(),
                url: "https://chat.deepseek.com/".to_string(),
                version: "V3 / R1".to_string(),
                visible: true,
            },
            Model {
                id: "xiaoyi".to_string(),
                name: "小艺 (华为)".to_string(),
                url: "https://xiaoyi.huawei.com/chat/".to_string(),
                version: "deepseek".to_string(),
                visible: true,
            },
            Model {
                id: "xiaomi".to_string(),
                name: "MiMo (小米)".to_string(),
                url: "https://aistudio.xiaomimimo.com/#/".to_string(),
                version: "MiMo".to_string(),
                visible: true,
            },
            Model {
                id: "doubao".to_string(),
                name: "豆包 (字节跳动)".to_string(),
                url: "https://doubao.com".to_string(),
                version: "Doubao-pro".to_string(),
                visible: true,
            },
            Model {
                id: "qwen".to_string(),
                name: "通义千问 (阿里)".to_string(),
                url: "https://qwenchat.com".to_string(),
                version: "2.5 / Max".to_string(),
                visible: true,
            },
            Model {
                id: "kimi".to_string(),
                name: "Kimi Chat (月之暗面)".to_string(),
                url: "https://www.kimi.com/".to_string(),
                version: "K2.6".to_string(),
                visible: true,
            },
            Model {
                id: "yuanbao".to_string(),
                name: "腾讯元宝".to_string(),
                url: "https://yuanbao.tencent.com/chat/naQivTmsDa".to_string(),
                version: "混元 Hy3 / DeepSeek".to_string(),
                visible: true,
            },
            Model {
                id: "yiyan".to_string(),
                name: "文心一言 (百度)".to_string(),
                url: "https://yiyan.baidu.com/".to_string(),
                version: "文心 4.5".to_string(),
                visible: true,
            },
            Model {
                id: "xinghuo".to_string(),
                name: "讯飞星火 (科大讯飞)".to_string(),
                url: "https://xinghuo.xfyun.cn/desk".to_string(),
                version: "4.0 Ultra".to_string(),
                visible: true,
            },
            Model {
                id: "sensechat".to_string(),
                name: "商量 (商汤科技)".to_string(),
                url: "https://chat.sensetime.com/".to_string(),
                version: "SenseNova 5.5".to_string(),
                visible: true,
            },
            Model {
                id: "nvidia-build".to_string(),
                name: "NVIDIA Build (NIM)".to_string(),
                url: "https://build.nvidia.com/".to_string(),
                version: "Nemotron 4 / Llama3".to_string(),
                visible: true,
            },
            Model {
                id: "grok".to_string(),
                name: "Grok (xAI)".to_string(),
                url: "https://grok.com/".to_string(),
                version: "Grok 2 / 3".to_string(),
                visible: true,
            },
            Model {
                id: "chatgpt".to_string(),
                name: "ChatGPT (OpenAI)".to_string(),
                url: "https://chatgpt.com".to_string(),
                version: "4o-mini / o1-mini".to_string(),
                visible: true,
            },
            Model {
                id: "gemini".to_string(),
                name: "Google Gemini".to_string(),
                url: "https://google.com".to_string(),
                version: "2.0 Flash".to_string(),
                visible: true,
            },
            Model {
                id: "claude".to_string(),
                name: "Claude AI (Anthropic)".to_string(),
                url: "https://claude.ai".to_string(),
                version: "3.5 Sonnet".to_string(),
                visible: true,
            },
            Model {
                id: "copilot".to_string(),
                name: "Microsoft Copilot".to_string(),
                url: "https://microsoft.com".to_string(),
                version: "GPT-4o".to_string(),
                visible: true,
            },
            Model {
                id: "mistral".to_string(),
                name: "Mistral Le Chat".to_string(),
                url: "https://mistral.ai".to_string(),
                version: "Large 3 / Pixtral".to_string(),
                visible: true,
            },
            Model {
                id: "meta-ai".to_string(),
                name: "Meta AI".to_string(),
                url: "https://meta.ai".to_string(),
                version: "Llama 3.3".to_string(),
                visible: true,
            },
            Model {
                id: "perplexity".to_string(),
                name: "Perplexity AI".to_string(),
                url: "https://perplexity.ai".to_string(),
                version: "Default Search".to_string(),
                visible: true,
            },
        ],
    }
}

/// 读取配置文件
pub fn read_config(app: &AppHandle) -> AppConfig {
    let config_path = get_config_path(app);
    
    if !config_path.exists() {
        return get_default_config();
    }

    match fs::read_to_string(&config_path) {
        Ok(content) => {
            match serde_json::from_str(&content) {
                Ok(config) => config,
                Err(_) => get_default_config(),
            }
        }
        Err(_) => get_default_config(),
    }
}

/// 写入配置文件
pub fn write_config(app: &AppHandle, config: &AppConfig) -> Result<(), String> {
    let config_path = get_config_path(app);
    
    if let Some(parent) = config_path.parent() {
        fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }

    let content = serde_json::to_string_pretty(config)
        .map_err(|e| format!("序列化配置失败: {}", e))?;

    fs::write(&config_path, content)
        .map_err(|e| format!("写入配置文件失败: {}", e))?;

    Ok(())
}

/// 初始化配置
pub fn init_config(app: &AppHandle) -> Result<(), Box<dyn std::error::Error>> {
    let _ = read_config(app);
    Ok(())
}

/// 从云端同步配置
pub async fn sync_from_cloud(app: &AppHandle, server_url: String) -> Result<AppConfig, String> {
    if server_url.is_empty() {
        return Err("服务器地址未配置".to_string());
    }

    let base_url = server_url.trim_end_matches('/');
    let fetch_url = if base_url.ends_with(".json") {
        base_url.to_string()
    } else {
        format!("{}/model_web.json", base_url)
    };

    let client = reqwest::Client::new();
    let response = client
        .get(&fetch_url)
        .header("Accept", "application/json")
        .send()
        .await
        .map_err(|e| format!("网络请求失败: {}", e))?;

    if !response.status().is_success() {
        return Err(format!("服务器响应错误 ({})", response.status()));
    }

    let server_data: serde_json::Value = response
        .json()
        .await
        .map_err(|_| "数据格式错误，无法解析 JSON".to_string())?;

    let server_models = server_data["models"]
        .as_array()
        .ok_or("数据格式错误: 缺少 models 数组".to_string())?;

    let local_config = read_config(app);
    let local_models = &local_config.models;

    // 构建本地模型映射
    let mut local_map = std::collections::HashMap::new();
    for m in local_models {
        local_map.insert(&m.id, m);
    }

    let mut merged_models = Vec::new();

    // 遍历服务器模型，合并配置
    for server_model in server_models {
        let id = server_model["id"]
            .as_str()
            .ok_or("模型缺少 id".to_string())?
            .to_string();
        let name = server_model["name"]
            .as_str()
            .ok_or("模型缺少 name".to_string())?
            .to_string();
        let url = server_model["url"]
            .as_str()
            .ok_or("模型缺少 url".to_string())?
            .to_string();
        let version = server_model["version"]
            .as_str()
            .unwrap_or("")
            .to_string();
        let visible = server_model["visible"].as_bool().unwrap_or(true);

        let merged_model = if let Some(local_model) = local_map.get(&id) {
            // 本地存在，保留 visible 状态
            Model {
                id,
                name,
                url,
                version,
                visible: local_model.visible,
            }
        } else {
            // 本地不存在，作为新增
            Model {
                id,
                name,
                url,
                version,
                visible,
            }
        };

        merged_models.push(merged_model);
    }

    let merged_config = AppConfig {
        theme: local_config.theme,
        server_url: server_url.clone(),
        sidebar_collapsed: local_config.sidebar_collapsed,
        models: merged_models,
    };

    // 保存合并后的配置
    write_config(app, &merged_config)?;

    Ok(merged_config)
}
