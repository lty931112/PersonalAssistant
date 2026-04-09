//! 配置设置

use serde::{Deserialize, Serialize};
use pa_core::CoreError;
use pa_memory::MemoryConfig;
use crate::loader::ConfigLoader;

/// 主配置结构
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Settings {
    /// Gateway 配置
    pub gateway: GatewaySettings,
    /// LLM 配置
    pub llm: LlmSettings,
    /// 记忆配置
    pub memory: MemorySettings,
    /// Agent 配置
    pub agent: AgentSettings,
    /// 工具配置
    pub tools: ToolSettings,
}

/// Gateway 配置
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GatewaySettings {
    /// 监听地址
    pub bind: String,
    /// 端口
    pub port: u16,
    /// 认证令牌
    pub auth_token: Option<String>,
    /// 是否启用 Tailscale
    pub tailscale_enabled: bool,
}

impl Default for GatewaySettings {
    fn default() -> Self {
        Self {
            bind: "127.0.0.1".into(),
            port: 18789,
            auth_token: None,
            tailscale_enabled: false,
        }
    }
}

/// LLM 配置
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LlmSettings {
    /// 提供商
    pub provider: String,
    /// 模型名称
    pub model: String,
    /// API 密钥
    pub api_key: String,
    /// API 基础 URL
    pub base_url: Option<String>,
    /// 最大 token 数
    pub max_tokens: u32,
    /// 备用模型
    pub fallback_model: Option<String>,
}

impl Default for LlmSettings {
    fn default() -> Self {
        Self {
            provider: "anthropic".into(),
            model: "claude-sonnet-4-20250514".into(),
            api_key: String::new(),
            base_url: None,
            max_tokens: 8192,
            fallback_model: None,
        }
    }
}

/// 记忆配置
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MemorySettings {
    /// 是否启用记忆
    pub enabled: bool,
    /// 向量搜索 K 值
    pub vector_search_k: usize,
    /// 关键词阈值
    pub keyword_threshold: f64,
    /// 最终 top-k
    pub top_k_final: usize,
    /// 最大遍历深度
    pub max_traversal_hops: usize,
}

impl Default for MemorySettings {
    fn default() -> Self {
        Self {
            enabled: true,
            vector_search_k: 20,
            keyword_threshold: 0.3,
            top_k_final: 5,
            max_traversal_hops: 3,
        }
    }
}

impl From<MemorySettings> for MemoryConfig {
    fn from(s: MemorySettings) -> Self {
        MemoryConfig {
            vector_search_k: s.vector_search_k,
            keyword_threshold: s.keyword_threshold,
            top_k_final: s.top_k_final,
            max_traversal_hops: s.max_traversal_hops,
            similarity_threshold: 0.7,
            enable_slow_integration: true,
            duplicate_threshold: 0.85,
            prune_frequency_threshold: 2,
        }
    }
}

/// Agent 配置
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentSettings {
    /// 默认最大轮数
    pub default_max_turns: u32,
    /// 工具结果预算
    pub tool_result_budget: usize,
    /// 预算上限（美元）
    pub max_budget_usd: Option<f64>,
}

impl Default for AgentSettings {
    fn default() -> Self {
        Self {
            default_max_turns: 10,
            tool_result_budget: 50000,
            max_budget_usd: None,
        }
    }
}

/// 工具配置
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolSettings {
    /// 启用的工具列表
    pub enabled: Vec<String>,
    /// 禁用的工具列表
    pub disabled: Vec<String>,
    /// 权限模式
    pub permission_mode: String,
}

impl Default for ToolSettings {
    fn default() -> Self {
        Self {
            enabled: vec!["*".into()],
            disabled: Vec::new(),
            permission_mode: "default".into(),
        }
    }
}

impl Settings {
    /// 从文件加载配置
    pub fn load(path: &str) -> Result<Self, CoreError> {
        ConfigLoader::load(path)
    }

    /// 加载配置或使用默认值
    pub fn load_or_default() -> Result<Self, CoreError> {
        ConfigLoader::load_or_default()
    }

    /// 保存配置到文件
    pub fn save(&self, path: &str) -> Result<(), CoreError> {
        ConfigLoader::save(self, path)
    }
}
