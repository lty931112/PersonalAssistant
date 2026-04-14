//! 类型定义模块
//!
//! 定义 LLM 客户端相关的核心类型，包括配置、响应、流式事件等。

use async_trait::async_trait;
use pa_core::{
    ContentBlock, CoreError, Message, StopReason, ToolDefinition, UsageInfo,
};
use serde::{Deserialize, Serialize};
use std::fmt;
use tokio::sync::mpsc;

// ============================================================
// LLM 提供商
// ============================================================

/// LLM 提供商枚举
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum LlmProvider {
    /// OpenAI 兼容 API（也支持 Ollama 等本地模型）
    OpenAI,
    /// Anthropic Claude API
    Anthropic,
    /// 自定义提供商
    Custom { name: String },
}

impl fmt::Display for LlmProvider {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            LlmProvider::OpenAI => write!(f, "OpenAI"),
            LlmProvider::Anthropic => write!(f, "Anthropic"),
            LlmProvider::Custom { name } => write!(f, "Custom({})", name),
        }
    }
}

// ============================================================
// LLM 配置
// ============================================================

/// LLM 配置
#[derive(Debug, Clone)]
pub struct LlmConfig {
    /// LLM 提供商
    pub provider: LlmProvider,
    /// 模型名称
    pub model: String,
    /// API 密钥
    pub api_key: String,
    /// 自定义 API 基础 URL（可选）
    pub base_url: Option<String>,
    /// 最大输出 token 数
    pub max_tokens: u32,
    /// 温度参数 (0.0 - 2.0)
    pub temperature: f32,
    /// 备用模型名称（当主模型出错时切换）
    pub fallback_model: Option<String>,
    /// 是否在主模型被判定为不可用时，经探测后自动切换到 `fallback_model`
    pub fallback_switch_enabled: bool,
    /// 最大重试次数
    pub max_retries: u32,
    /// 是否启用流式响应
    pub stream: bool,
}

impl LlmConfig {
    /// 创建 OpenAI 兼容配置
    pub fn openai(model: impl Into<String>, api_key: impl Into<String>) -> Self {
        LlmConfig {
            provider: LlmProvider::OpenAI,
            model: model.into(),
            api_key: api_key.into(),
            base_url: None,
            max_tokens: 8192,
            temperature: 0.7,
            fallback_model: None,
            fallback_switch_enabled: false,
            max_retries: 3,
            stream: true,
        }
    }

    /// 创建 Anthropic 配置
    pub fn anthropic(model: impl Into<String>, api_key: impl Into<String>) -> Self {
        LlmConfig {
            provider: LlmProvider::Anthropic,
            model: model.into(),
            api_key: api_key.into(),
            base_url: None,
            max_tokens: 8192,
            temperature: 0.7,
            fallback_model: None,
            fallback_switch_enabled: false,
            max_retries: 3,
            stream: true,
        }
    }

    /// 设置自定义基础 URL
    pub fn with_base_url(mut self, url: impl Into<String>) -> Self {
        self.base_url = Some(url.into());
        self
    }

    /// 设置最大 token 数
    pub fn with_max_tokens(mut self, max_tokens: u32) -> Self {
        self.max_tokens = max_tokens;
        self
    }

    /// 设置温度参数
    pub fn with_temperature(mut self, temperature: f32) -> Self {
        self.temperature = temperature.clamp(0.0, 2.0);
        self
    }

    /// 设置备用模型
    pub fn with_fallback_model(mut self, model: impl Into<String>) -> Self {
        self.fallback_model = Some(model.into());
        self
    }

    /// 是否启用「主模型不可用 → 探测备用模型 → 切换」流程
    pub fn with_fallback_switch_enabled(mut self, enabled: bool) -> Self {
        self.fallback_switch_enabled = enabled;
        self
    }

    /// 设置最大重试次数
    pub fn with_max_retries(mut self, max_retries: u32) -> Self {
        self.max_retries = max_retries;
        self
    }
}

// ============================================================
// LLM 响应
// ============================================================

/// LLM 响应
#[derive(Debug, Clone)]
pub struct LlmResponse {
    /// 响应内容块列表
    pub content: Vec<ContentBlock>,
    /// 停止原因
    pub stop_reason: StopReason,
    /// Token 使用量
    pub usage: UsageInfo,
    /// 使用的模型名称
    pub model: String,
}

impl LlmResponse {
    /// 获取响应的纯文本内容
    pub fn text_content(&self) -> String {
        self.content
            .iter()
            .filter_map(|block| block.as_text())
            .collect::<Vec<_>>()
            .join("")
    }

    /// 是否包含工具使用
    pub fn has_tool_use(&self) -> bool {
        self.content.iter().any(|b| b.is_tool_use())
    }

    /// 获取所有工具使用块
    pub fn tool_uses(&self) -> Vec<&ContentBlock> {
        self.content.iter().filter(|b| b.is_tool_use()).collect()
    }
}

// ============================================================
// LLM 流式事件
// ============================================================

/// LLM 流式事件
#[derive(Debug, Clone)]
pub enum LlmStreamEvent {
    /// 文本增量
    Delta { text: String },
    /// 工具使用开始
    ToolUseStart { id: String, name: String },
    /// 工具使用输入增量
    ToolUseInputDelta { id: String, delta: String },
    /// 工具使用结束
    ToolUseEnd { id: String },
    /// 思考增量
    ThinkingDelta { delta: String },
    /// 使用量更新
    Usage { input_tokens: u32, output_tokens: u32 },
    /// 停止
    Stop { reason: StopReason },
    /// 错误
    Error(String),
}

// ============================================================
// LLM 客户端 trait
// ============================================================

/// LLM 客户端 trait
///
/// 定义了 LLM 客户端的统一接口，所有提供商实现都需要实现此 trait。
#[async_trait]
pub trait LlmClientTrait: Send + Sync {
    /// 非流式调用
    ///
    /// 发送消息列表给 LLM，等待完整响应返回。
    async fn complete(
        &self,
        messages: &[Message],
        tools: &[ToolDefinition],
        system: &str,
    ) -> Result<LlmResponse, CoreError>;

    /// 流式调用
    ///
    /// 发送消息列表给 LLM，通过 channel 返回流式事件。
    async fn stream(
        &self,
        messages: &[Message],
        tools: &[ToolDefinition],
        system: &str,
    ) -> Result<mpsc::Receiver<LlmStreamEvent>, CoreError>;

    /// 获取当前使用的模型名称
    fn model(&self) -> &str;

    /// 获取提供商名称
    fn provider(&self) -> &str;
}

// ============================================================
// LLM 客户端工厂
// ============================================================

/// LLM 客户端工厂
///
/// 根据配置创建对应的 LLM 客户端实例。
pub struct LlmClient;

impl LlmClient {
    /// 根据配置创建 LLM 客户端
    pub fn new(config: &LlmConfig) -> Result<Box<dyn LlmClientTrait>, CoreError> {
        match config.provider {
            LlmProvider::OpenAI => {
                let client = crate::openai::OpenAiClient::new(config)?;
                Ok(Box::new(client))
            }
            LlmProvider::Anthropic => {
                let client = crate::anthropic::AnthropicClient::new(config)?;
                Ok(Box::new(client))
            }
            LlmProvider::Custom { ref name } => {
                tracing::warn!(provider = %name, "自定义提供商将使用 OpenAI 兼容协议");
                let client = crate::openai::OpenAiClient::new(config)?;
                Ok(Box::new(client))
            }
        }
    }
}

// ============================================================
// LLM 错误类型
// ============================================================

/// LLM 特定错误
#[derive(Debug, thiserror::Error)]
pub enum LlmError {
    /// API 请求失败
    #[error("API 请求失败: {0}")]
    RequestFailed(String),

    /// API 返回错误状态码
    #[error("API 错误 [{status}]: {message}")]
    ApiError { status: u16, message: String },

    /// 响应解析失败
    #[error("响应解析失败: {0}")]
    ParseError(String),

    /// 流式响应中断
    #[error("流式响应中断: {0}")]
    StreamInterrupted(String),

    /// 重试次数耗尽
    #[error("重试次数耗尽: {0}")]
    RetriesExhausted(String),

    /// 不支持的操作
    #[error("不支持的操作: {0}")]
    Unsupported(String),
}

impl From<LlmError> for CoreError {
    fn from(err: LlmError) -> Self {
        match &err {
            LlmError::ApiError { status: 429, .. } => CoreError::RateLimit {
                retry_after: Some(1.0),
            },
            LlmError::ApiError { status: 529, message } => {
                CoreError::Overloaded(message.clone())
            }
            LlmError::ApiError { status: 401 | 403, message } => {
                CoreError::Authentication(message.clone())
            }
            _ => CoreError::LlmClient(err.to_string()),
        }
    }
}
