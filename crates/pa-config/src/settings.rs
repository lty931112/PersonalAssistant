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
    /// MCP 配置
    pub mcp: Option<McpSettings>,
    /// 飞书配置
    pub feishu: Option<FeishuSettings>,
    /// 任务配置
    pub task: TaskSettings,
    /// 告警配置
    pub alert: AlertSettings,
    /// 安全与权限收窄（工作区、外联、删除等）
    #[serde(default)]
    pub security: SecuritySettings,
    /// 可观测性与执行审计
    #[serde(default)]
    pub observability: ObservabilitySettings,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            gateway: GatewaySettings::default(),
            llm: LlmSettings::default(),
            memory: MemorySettings::default(),
            agent: AgentSettings::default(),
            tools: ToolSettings::default(),
            mcp: None,
            feishu: None,
            task: TaskSettings::default(),
            alert: AlertSettings::default(),
            security: SecuritySettings::default(),
            observability: ObservabilitySettings::default(),
        }
    }
}

/// 安全策略：工作区边界、外发 URL 白名单、删除风险等
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SecuritySettings {
    /// 是否强制文件类工具仅能访问工作区根目录之下
    pub enforce_workspace: bool,
    /// 工作区根路径列表（绝对或相对路径）；为空且 `enforce_workspace` 为 true 时，运行时使用进程当前工作目录作为唯一根
    pub workspace_roots: Vec<String>,
    /// `web_fetch` URL 前缀白名单；匹配前缀的请求可自动通过权限检查（仍会写入审计日志）
    pub web_fetch_allow_url_prefixes: Vec<String>,
    /// 为 true 时：不在白名单中的 `web_fetch` 一律走「需确认」路径（`Ask`）
    pub strict_web_fetch: bool,
}

impl Default for SecuritySettings {
    fn default() -> Self {
        Self {
            enforce_workspace: false,
            workspace_roots: Vec::new(),
            web_fetch_allow_url_prefixes: Vec::new(),
            strict_web_fetch: true,
        }
    }
}

/// 执行过程可观测性与持久化审计
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ObservabilitySettings {
    /// 是否写入审计日志（JSON Lines）
    pub audit_log_enabled: bool,
    /// 审计日志文件路径（追加写入）
    pub audit_log_path: String,
}

impl Default for ObservabilitySettings {
    fn default() -> Self {
        Self {
            audit_log_enabled: true,
            audit_log_path: ".pa/audit/execution.jsonl".into(),
        }
    }
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
            port: 19870,
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
    /// 主模型被严格判定为不可用时，是否允许在探测通过后切换到备用模型
    #[serde(default)]
    pub fallback_switch_enabled: bool,
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
            fallback_switch_enabled: false,
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

/// MCP 配置
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct McpSettings {
    /// MCP 配置文件路径
    pub config_path: Option<String>,
    /// 是否启用 MCP
    pub enabled: bool,
}

impl Default for McpSettings {
    fn default() -> Self {
        Self {
            config_path: Some("config/mcp.toml".into()),
            enabled: false,
        }
    }
}

/// 飞书配置
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FeishuSettings {
    /// 是否启用飞书
    pub enabled: bool,
    /// App ID
    pub app_id: String,
    /// App Secret
    pub app_secret: String,
    /// 验证 Token
    pub verification_token: String,
    /// 加密密钥（可选）
    pub encrypt_key: Option<String>,
    /// Webhook URL 路径
    pub webhook_path: String,
    /// 允许的用户列表（空=全部允许）
    pub allowed_users: Vec<String>,
}

impl Default for FeishuSettings {
    fn default() -> Self {
        Self {
            enabled: false,
            app_id: String::new(),
            app_secret: String::new(),
            verification_token: String::new(),
            encrypt_key: None,
            webhook_path: "/feishu/webhook".into(),
            allowed_users: Vec::new(),
        }
    }
}

/// 任务配置
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskSettings {
    /// SQLite 数据库路径
    pub db_path: String,
    /// 自动清理天数
    pub cleanup_days: u32,
    /// 最大并发任务数
    pub max_concurrent_tasks: u32,
}

impl Default for TaskSettings {
    fn default() -> Self {
        Self {
            db_path: ".pa/tasks.db".into(),
            cleanup_days: 30,
            max_concurrent_tasks: 10,
        }
    }
}

/// 告警配置
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AlertSettings {
    /// 是否启用告警
    pub enabled: bool,
    /// 告警渠道: "webhook" 或 "feishu"
    pub channel: String,
    /// Webhook URL（默认渠道）
    pub webhook_url: String,
    /// 飞书告警配置（可选，当 channel 为 "feishu" 时使用）
    pub feishu: Option<AlertFeishuSettings>,
    /// 告警冷却时间（秒），同一类型的告警在此时间内不重复发送
    pub cooldown_secs: u64,
}

impl Default for AlertSettings {
    fn default() -> Self {
        Self {
            enabled: true,
            channel: "webhook".to_string(),
            webhook_url: String::new(),
            feishu: None,
            cooldown_secs: 300,
        }
    }
}

/// 飞书告警配置
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AlertFeishuSettings {
    /// 告警通知的群聊 ID
    pub chat_id: String,
    /// App ID（如果未启用飞书通道，需要单独配置）
    pub app_id: Option<String>,
    /// App Secret
    pub app_secret: Option<String>,
}

impl Default for AlertFeishuSettings {
    fn default() -> Self {
        Self {
            chat_id: String::new(),
            app_id: None,
            app_secret: None,
        }
    }
}
