//! LLM 提供商插件 trait

use async_trait::async_trait;
use pa_core::{CoreError, Message, ToolDefinition};
use crate::plugin::Plugin;

/// LLM 配置
#[derive(Debug, Clone)]
pub struct LlmConfig {
    pub model: String,
    pub api_key: String,
    pub base_url: Option<String>,
    pub max_tokens: u32,
}

/// LLM 提供商插件 trait
#[async_trait]
pub trait LlmProviderPlugin: Plugin {
    /// 完成请求
    async fn complete(
        &self,
        messages: &[Message],
        tools: &[ToolDefinition],
        system: &str,
    ) -> Result<pa_llm::LlmResponse, CoreError>;

    /// 流式完成
    async fn stream(
        &self,
        messages: &[Message],
        tools: &[ToolDefinition],
        system: &str,
    ) -> Result<tokio::sync::mpsc::Receiver<pa_llm::LlmStreamEvent>, CoreError>;
}
