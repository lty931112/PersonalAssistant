//! Anthropic Claude 客户端实现
//!
//! 支持 Anthropic Claude API 的 /v1/messages 端点，
//! 包括流式 SSE 响应处理和扩展思考（extended thinking）功能。

use async_trait::async_trait;
use futures::StreamExt;
use pa_core::{
    ContentBlock, CoreError, Message, MessageRole, StopReason, ToolDefinition, UsageInfo,
};
use reqwest::header::{HeaderMap, HeaderValue, AUTHORIZATION, CONTENT_TYPE};
use serde::{Deserialize, Serialize};
use tokio::sync::mpsc;
use tracing;

use crate::types::{LlmConfig, LlmError, LlmResponse, LlmStreamEvent, LlmClientTrait};

// ============================================================
// Anthropic API 请求/响应类型
// ============================================================

/// Anthropic 消息请求
#[derive(Debug, Serialize)]
struct MessagesRequest {
    model: String,
    max_tokens: u32,
    #[serde(skip_serializing_if = "Option::is_none")]
    temperature: Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    system: Option<String>,
    messages: Vec<AnthropicMessage>,
    stream: bool,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    tools: Vec<AnthropicTool>,
}

/// Anthropic 消息格式
#[derive(Debug, Clone, Serialize, Deserialize)]
struct AnthropicMessage {
    role: String,
    content: Vec<AnthropicContentBlock>,
}

/// Anthropic 内容块
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
enum AnthropicContentBlock {
    #[serde(rename = "text")]
    Text { text: String },
    #[serde(rename = "tool_use")]
    ToolUse {
        id: String,
        name: String,
        input: serde_json::Value,
    },
    #[serde(rename = "tool_result")]
    ToolResult {
        tool_use_id: String,
        content: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        is_error: Option<bool>,
    },
    #[serde(rename = "thinking")]
    Thinking { thinking: String },
}

/// Anthropic 工具定义
#[derive(Debug, Clone, Serialize)]
struct AnthropicTool {
    name: String,
    description: String,
    input_schema: serde_json::Value,
}

/// Anthropic 消息响应
#[derive(Debug, Deserialize)]
struct MessagesResponse {
    content: Vec<AnthropicContentBlock>,
    model: Option<String>,
    stop_reason: Option<String>,
    usage: AnthropicUsage,
    error: Option<AnthropicError>,
}

/// Anthropic 使用量
#[derive(Debug, Clone, Deserialize)]
struct AnthropicUsage {
    input_tokens: u32,
    output_tokens: u32,
    #[serde(default)]
    cache_read_input_tokens: u32,
    #[serde(default)]
    cache_creation_input_tokens: u32,
}

/// Anthropic 错误
#[derive(Debug, Deserialize)]
struct AnthropicError {
    message: String,
}

// ============================================================
// Anthropic 客户端
// ============================================================

/// Anthropic Claude 客户端
pub struct AnthropicClient {
    http_client: reqwest::Client,
    base_url: String,
    api_key: String,
    model: String,
    max_tokens: u32,
    temperature: f32,
    fallback_model: Option<String>,
    fallback_switch_enabled: bool,
    max_retries: u32,
    api_version: String,
}

impl AnthropicClient {
    /// 创建新的 Anthropic 客户端
    pub fn new(config: &LlmConfig) -> Result<Self, CoreError> {
        let base_url = config
            .base_url
            .clone()
            .unwrap_or_else(|| "https://api.anthropic.com".to_string());

        let base_url = base_url.trim_end_matches('/').to_string();

        let http_client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(300))
            .build()
            .map_err(|e| CoreError::Configuration(format!("HTTP 客户端创建失败: {}", e)))?;

        Ok(AnthropicClient {
            http_client,
            base_url,
            api_key: config.api_key.clone(),
            model: config.model.clone(),
            max_tokens: config.max_tokens,
            temperature: config.temperature,
            fallback_model: config.fallback_model.clone(),
            fallback_switch_enabled: config.fallback_switch_enabled,
            max_retries: config.max_retries,
            api_version: "2023-06-01".to_string(),
        })
    }

    /// 构建请求头
    fn build_headers(&self) -> Result<HeaderMap, CoreError> {
        let mut headers = HeaderMap::new();
        headers.insert(CONTENT_TYPE, HeaderValue::from_static("application/json"));
        headers.insert(
            "anthropic-version",
            HeaderValue::from_str(self.api_version.as_str())
                .map_err(|e| CoreError::Configuration(format!("API 版本格式错误: {}", e)))?,
        );
        headers.insert(
            AUTHORIZATION,
            HeaderValue::from_str(&format!("Bearer {}", self.api_key))
                .map_err(|e| CoreError::Configuration(format!("API 密钥格式错误: {}", e)))?,
        );
        Ok(headers)
    }

    /// 将内部消息格式转换为 Anthropic 格式
    fn convert_messages(messages: &[Message]) -> Vec<AnthropicMessage> {
        let mut anthropic_messages = Vec::new();
        let mut pending_tool_results: Vec<AnthropicContentBlock> = Vec::new();

        for msg in messages {
            match msg.role {
                MessageRole::System => continue,
                MessageRole::User => {
                    let mut content_blocks = Vec::new();
                    content_blocks.append(&mut pending_tool_results);

                    let text = msg.text_content();
                    if !text.is_empty() {
                        content_blocks.push(AnthropicContentBlock::Text { text });
                    }

                    for block in &msg.content {
                        if let ContentBlock::ToolResult {
                            tool_use_id,
                            content,
                            is_error,
                        } = block
                        {
                            content_blocks.push(AnthropicContentBlock::ToolResult {
                                tool_use_id: tool_use_id.clone(),
                                content: content.clone(),
                                is_error: Some(*is_error),
                            });
                        }
                    }

                    if !content_blocks.is_empty() {
                        anthropic_messages.push(AnthropicMessage {
                            role: "user".to_string(),
                            content: content_blocks,
                        });
                    }
                }
                MessageRole::Assistant => {
                    let mut content_blocks = Vec::new();
                    for block in &msg.content {
                        match block {
                            ContentBlock::Text { text } => {
                                if !text.is_empty() {
                                    content_blocks.push(AnthropicContentBlock::Text {
                                        text: text.clone(),
                                    });
                                }
                            }
                            ContentBlock::ToolUse { id, name, input } => {
                                content_blocks.push(AnthropicContentBlock::ToolUse {
                                    id: id.clone(),
                                    name: name.clone(),
                                    input: input.clone(),
                                });
                            }
                            ContentBlock::ToolResult {
                                tool_use_id,
                                content,
                                is_error,
                            } => {
                                pending_tool_results.push(AnthropicContentBlock::ToolResult {
                                    tool_use_id: tool_use_id.clone(),
                                    content: content.clone(),
                                    is_error: Some(*is_error),
                                });
                            }
                            ContentBlock::Thinking { thinking } => {
                                content_blocks.push(AnthropicContentBlock::Thinking {
                                    thinking: thinking.clone(),
                                });
                            }
                        }
                    }
                    if !content_blocks.is_empty() {
                        anthropic_messages.push(AnthropicMessage {
                            role: "assistant".to_string(),
                            content: content_blocks,
                        });
                    }
                }
            }
        }

        if !pending_tool_results.is_empty() {
            anthropic_messages.push(AnthropicMessage {
                role: "user".to_string(),
                content: pending_tool_results,
            });
        }

        anthropic_messages
    }

    /// 转换工具定义为 Anthropic 格式
    fn convert_tools(tools: &[ToolDefinition]) -> Vec<AnthropicTool> {
        tools
            .iter()
            .map(|tool| AnthropicTool {
                name: tool.name.clone(),
                description: tool.description.clone(),
                input_schema: tool.input_schema.clone(),
            })
            .collect()
    }

    /// 解析停止原因
    fn parse_stop_reason(reason: Option<&str>) -> StopReason {
        match reason {
            Some("end_turn") => StopReason::EndTurn,
            Some("max_tokens") => StopReason::MaxTokens,
            Some("tool_use") => StopReason::ToolUse,
            Some("stop_sequence") => StopReason::StopSequence,
            Some(other) => StopReason::Other(other.to_string()),
            None => StopReason::EndTurn,
        }
    }

    /// 解析 Anthropic 内容块为内部格式
    fn parse_content_blocks(blocks: &[AnthropicContentBlock]) -> Vec<ContentBlock> {
        blocks
            .iter()
            .map(|block| match block {
                AnthropicContentBlock::Text { text } => ContentBlock::text(text),
                AnthropicContentBlock::ToolUse { id, name, input } => {
                    ContentBlock::tool_use(id, name, input.clone())
                }
                AnthropicContentBlock::Thinking { thinking } => ContentBlock::thinking(thinking),
                AnthropicContentBlock::ToolResult { .. } => ContentBlock::text("[工具结果]"),
            })
            .collect()
    }

    fn anthropic_error_message(body: &str) -> String {
        #[derive(Deserialize)]
        struct ErrBody {
            error: Option<AnthropicError>,
        }
        serde_json::from_str::<ErrBody>(body)
            .ok()
            .and_then(|b| b.error.map(|e| e.message))
            .filter(|s| !s.is_empty())
            .unwrap_or_else(|| body.chars().take(4096).collect())
    }

    /// 用极小请求探测指定模型是否可用（与真实请求相同端点与鉴权）。
    async fn probe_model_availability(&self, model: &str) -> bool {
        let probe = MessagesRequest {
            model: model.to_string(),
            max_tokens: 1,
            temperature: Some(self.temperature),
            system: None,
            messages: vec![AnthropicMessage {
                role: "user".to_string(),
                content: vec![AnthropicContentBlock::Text { text: ".".into() }],
            }],
            stream: false,
            tools: vec![],
        };
        let url = format!("{}/v1/messages", self.base_url);
        let Ok(headers) = self.build_headers() else {
            return false;
        };
        let Ok(resp) = self
            .http_client
            .post(&url)
            .headers(headers)
            .json(&probe)
            .send()
            .await
        else {
            return false;
        };
        resp.status().is_success()
    }

    /// 若配置允许且错误满足严格判定，则探测并切换到备用模型。
    async fn try_fallback_after_primary_failure(
        &self,
        status: u16,
        message: &str,
        active_model: &mut String,
        switched_to_fallback: &mut bool,
        attempt: &mut u32,
    ) -> bool {
        if !self.fallback_switch_enabled {
            return false;
        }
        if *switched_to_fallback {
            return false;
        }
        if !crate::fallback::primary_model_unreachable_for_fallback(status, message) {
            return false;
        }
        let Some(ref fb) = self.fallback_model else {
            return false;
        };
        if fb.is_empty() || fb == active_model.as_str() {
            return false;
        }
        if !self.probe_model_availability(fb).await {
            tracing::warn!(
                fallback = %fb,
                "主模型不可用（严格判定），但备用模型探测未通过，保持原错误流程"
            );
            return false;
        }
        tracing::warn!(
            primary = %active_model,
            fallback = %fb,
            "主模型不可用（严格判定），备用模型探测成功，切换模型后重试"
        );
        *active_model = fb.clone();
        *switched_to_fallback = true;
        *attempt = 0;
        true
    }
}

#[async_trait]
impl LlmClientTrait for AnthropicClient {
    async fn complete(
        &self,
        messages: &[Message],
        tools: &[ToolDefinition],
        system: &str,
    ) -> Result<LlmResponse, CoreError> {
        let anthropic_messages = Self::convert_messages(messages);
        let anthropic_tools = Self::convert_tools(tools);

        let url = format!("{}/v1/messages", self.base_url);
        let headers = self.build_headers()?;

        let mut active_model = self.model.clone();
        let mut switched_to_fallback = false;
        let mut attempt: u32 = 0;

        loop {
            let request_body = MessagesRequest {
                model: active_model.clone(),
                max_tokens: self.max_tokens,
                temperature: Some(self.temperature),
                system: if system.is_empty() {
                    None
                } else {
                    Some(system.to_string())
                },
                messages: anthropic_messages.clone(),
                stream: false,
                tools: anthropic_tools.clone(),
            };

            tracing::debug!(url = %url, model = %active_model, "发送 Anthropic 非流式请求");

            let resp = self
                .http_client
                .post(&url)
                .headers(headers.clone())
                .json(&request_body)
                .send()
                .await
                .map_err(|e| LlmError::RequestFailed(e.to_string()))?;

            let status = resp.status();
            let status_u16 = status.as_u16();
            let body = resp
                .text()
                .await
                .map_err(|e| LlmError::RequestFailed(e.to_string()))?;

            if status.is_success() {
                let response: MessagesResponse = serde_json::from_str(&body).map_err(|e| {
                    LlmError::ParseError(format!("响应解析失败: {}", e))
                })?;

                let content = Self::parse_content_blocks(&response.content);
                let stop_reason = Self::parse_stop_reason(response.stop_reason.as_deref());

                let usage = UsageInfo {
                    input_tokens: response.usage.input_tokens,
                    output_tokens: response.usage.output_tokens,
                    cache_read_tokens: response.usage.cache_read_input_tokens,
                    cache_creation_tokens: response.usage.cache_creation_input_tokens,
                };

                let model = response
                    .model
                    .unwrap_or_else(|| active_model.clone());

                tracing::debug!(
                    stop_reason = %stop_reason,
                    input_tokens = usage.input_tokens,
                    output_tokens = usage.output_tokens,
                    "Anthropic 非流式请求完成"
                );

                return Ok(LlmResponse {
                    content,
                    stop_reason,
                    usage,
                    model,
                });
            }

            let err_text = Self::anthropic_error_message(&body);

            if let Ok(err_resp) = serde_json::from_str::<MessagesResponse>(&body) {
                if let Some(error) = err_resp.error {
                    let api_err = LlmError::ApiError {
                        status: status_u16,
                        message: error.message.clone(),
                    };
                    match status_u16 {
                        429 if attempt < self.max_retries => {
                            let wait_ms = 1000u64 * 2u64.pow(attempt);
                            tracing::warn!(attempt = attempt + 1, wait_ms, "Anthropic 速率限制");
                            tokio::time::sleep(std::time::Duration::from_millis(wait_ms)).await;
                            attempt += 1;
                            continue;
                        }
                        529 if attempt < self.max_retries => {
                            let wait_ms = 2000u64 * 2u64.pow(attempt);
                            tracing::warn!(attempt = attempt + 1, wait_ms, "Anthropic 服务过载");
                            tokio::time::sleep(std::time::Duration::from_millis(wait_ms)).await;
                            attempt += 1;
                            continue;
                        }
                        _ => {
                            if self
                                .try_fallback_after_primary_failure(
                                    status_u16,
                                    &error.message,
                                    &mut active_model,
                                    &mut switched_to_fallback,
                                    &mut attempt,
                                )
                                .await
                            {
                                continue;
                            }
                            return Err(api_err.into());
                        }
                    }
                }
            }

            if status_u16 == 429 && attempt < self.max_retries {
                let wait_ms = 1000u64 * 2u64.pow(attempt);
                tokio::time::sleep(std::time::Duration::from_millis(wait_ms)).await;
                attempt += 1;
                continue;
            }
            if status_u16 == 529 && attempt < self.max_retries {
                let wait_ms = 2000u64 * 2u64.pow(attempt);
                tokio::time::sleep(std::time::Duration::from_millis(wait_ms)).await;
                attempt += 1;
                continue;
            }

            if self
                .try_fallback_after_primary_failure(
                    status_u16,
                    &err_text,
                    &mut active_model,
                    &mut switched_to_fallback,
                    &mut attempt,
                )
                .await
            {
                continue;
            }

            return Err(
                LlmError::ApiError {
                    status: status_u16,
                    message: err_text,
                }
                .into(),
            );
        }
    }

    async fn stream(
        &self,
        messages: &[Message],
        tools: &[ToolDefinition],
        system: &str,
    ) -> Result<mpsc::Receiver<LlmStreamEvent>, CoreError> {
        let anthropic_messages = Self::convert_messages(messages);
        let anthropic_tools = Self::convert_tools(tools);
        let url = format!("{}/v1/messages", self.base_url);
        let headers = self.build_headers()?;

        let mut active_model = self.model.clone();
        let mut switched_to_fallback = false;
        let mut attempt: u32 = 0;

        loop {
            let request_body = MessagesRequest {
                model: active_model.clone(),
                max_tokens: self.max_tokens,
                temperature: Some(self.temperature),
                system: if system.is_empty() {
                    None
                } else {
                    Some(system.to_string())
                },
                messages: anthropic_messages.clone(),
                stream: true,
                tools: anthropic_tools.clone(),
            };

            tracing::debug!(url = %url, model = %active_model, "发送 Anthropic 流式请求");

            let resp = match self
                .http_client
                .post(&url)
                .headers(headers.clone())
                .json(&request_body)
                .send()
                .await
            {
                Ok(r) => r,
                Err(e) => return Err(LlmError::RequestFailed(e.to_string()).into()),
            };

            let status = resp.status();
            if status.is_success() {
                let (tx, rx) = mpsc::channel(256);
                tokio::spawn(forward_anthropic_sse_stream(resp, tx));
                return Ok(rx);
            }

            let status_u16 = status.as_u16();
            let body = resp.text().await.unwrap_or_default();
            let err_summary = Self::anthropic_error_message(&body);

            if let Ok(err_resp) = serde_json::from_str::<MessagesResponse>(&body) {
                if let Some(error) = err_resp.error {
                    match status_u16 {
                        429 if attempt < self.max_retries => {
                            let wait_ms = 1000u64 * 2u64.pow(attempt);
                            tracing::warn!(attempt = attempt + 1, wait_ms, "Anthropic 速率限制（流式首包）");
                            tokio::time::sleep(std::time::Duration::from_millis(wait_ms)).await;
                            attempt += 1;
                            continue;
                        }
                        529 if attempt < self.max_retries => {
                            let wait_ms = 2000u64 * 2u64.pow(attempt);
                            tracing::warn!(attempt = attempt + 1, wait_ms, "Anthropic 服务过载（流式首包）");
                            tokio::time::sleep(std::time::Duration::from_millis(wait_ms)).await;
                            attempt += 1;
                            continue;
                        }
                        _ => {
                            if self
                                .try_fallback_after_primary_failure(
                                    status_u16,
                                    &error.message,
                                    &mut active_model,
                                    &mut switched_to_fallback,
                                    &mut attempt,
                                )
                                .await
                            {
                                continue;
                            }
                            return Err(
                                LlmError::ApiError {
                                    status: status_u16,
                                    message: error.message,
                                }
                                .into(),
                            );
                        }
                    }
                }
            }

            if status_u16 == 429 && attempt < self.max_retries {
                let wait_ms = 1000u64 * 2u64.pow(attempt);
                tokio::time::sleep(std::time::Duration::from_millis(wait_ms)).await;
                attempt += 1;
                continue;
            }
            if status_u16 == 529 && attempt < self.max_retries {
                let wait_ms = 2000u64 * 2u64.pow(attempt);
                tokio::time::sleep(std::time::Duration::from_millis(wait_ms)).await;
                attempt += 1;
                continue;
            }

            if self
                .try_fallback_after_primary_failure(
                    status_u16,
                    &err_summary,
                    &mut active_model,
                    &mut switched_to_fallback,
                    &mut attempt,
                )
                .await
            {
                continue;
            }

            return Err(
                LlmError::ApiError {
                    status: status_u16,
                    message: err_summary,
                }
                .into(),
            );
        }
    }

    fn model(&self) -> &str {
        &self.model
    }

    fn provider(&self) -> &str {
        "Anthropic"
    }
}

async fn forward_anthropic_sse_stream(
    response: reqwest::Response,
    tx: mpsc::Sender<LlmStreamEvent>,
) {
 let mut stream = response.bytes_stream();
            let mut buffer = String::new();
            let mut current_tool_id: Option<String> = None;

            while let Some(chunk_result) = stream.next().await {
                match chunk_result {
                    Ok(bytes) => {
                        buffer.push_str(&String::from_utf8_lossy(&bytes));

                        while let Some(pos) = buffer.find('\n') {
                            let line = buffer[..pos].trim().to_string();
                            buffer = buffer[pos + 1..].to_string();

                            if line.is_empty() || line.starts_with(':') {
                                continue;
                            }

                            let (event_type, data) = if let Some(data) = line.strip_prefix("data: ") {
                                (None, data.to_string())
                            } else if let Some(event) = line.strip_prefix("event: ") {
                                (Some(event.to_string()), String::new())
                            } else {
                                continue;
                            };

                            if event_type.is_some() && data.is_empty() {
                                continue;
                            }

                            let data_str = if data.is_empty() {
                                if let Some(next_pos) = buffer.find('\n') {
                                    let next_line = buffer[..next_pos].trim().to_string();
                                    buffer = buffer[next_pos + 1..].to_string();
                                    next_line.strip_prefix("data: ").unwrap_or("").to_string()
                                } else {
                                    continue;
                                }
                            } else {
                                data
                            };

                            let event_type = event_type.as_deref();

                            match event_type {
                                Some("content_block_start") => {
                                    #[derive(Deserialize)]
                                    struct ContentBlockStart {
                                        content_block: Option<ContentBlockInfo>,
                                    }
                                    #[derive(Deserialize)]
                                    struct ContentBlockInfo {
                                        r#type: String,
                                        id: Option<String>,
                                        name: Option<String>,
                                    }

                                    if let Ok(parsed) = serde_json::from_str::<ContentBlockStart>(&data_str) {
                                        if let Some(block) = parsed.content_block {
                                            match block.r#type.as_str() {
                                                "tool_use" => {
                                                    current_tool_id = block.id.clone();
                                                    if let (Some(id), Some(name)) = (block.id.clone(), block.name.clone()) {
                                                        let _ = tx.send(LlmStreamEvent::ToolUseStart {
                                                            id,
                                                            name,
                                                        }).await;
                                                    }
                                                }
                                                _ => {}
                                            }
                                        }
                                    }
                                }
                                Some("content_block_delta") => {
                                    #[derive(Deserialize)]
                                    struct ContentBlockDelta {
                                        delta: Option<DeltaInfo>,
                                    }
                                    #[derive(Deserialize)]
                                    struct DeltaInfo {
                                        r#type: String,
                                        text: Option<String>,
                                        partial_json: Option<String>,
                                        thinking: Option<String>,
                                    }

                                    if let Ok(parsed) = serde_json::from_str::<ContentBlockDelta>(&data_str) {
                                        if let Some(delta) = parsed.delta {
                                            match delta.r#type.as_str() {
                                                "text_delta" => {
                                                    if let Some(text) = delta.text {
                                                        let _ = tx.send(LlmStreamEvent::Delta { text }).await;
                                                    }
                                                }
                                                "input_json_delta" => {
                                                    if let Some(json) = delta.partial_json {
                                                        if let Some(ref id) = current_tool_id {
                                                            let _ = tx.send(LlmStreamEvent::ToolUseInputDelta {
                                                                id: id.clone(),
                                                                delta: json,
                                                            }).await;
                                                        }
                                                    }
                                                }
                                                "thinking_delta" => {
                                                    if let Some(thinking) = delta.thinking {
                                                        let _ = tx.send(LlmStreamEvent::ThinkingDelta { delta: thinking }).await;
                                                    }
                                                }
                                                _ => {}
                                            }
                                        }
                                    }
                                }
                                Some("content_block_stop") => {
                                    if current_tool_id.is_some() {
                                        let _ = tx.send(LlmStreamEvent::ToolUseEnd {
                                            id: current_tool_id.clone().unwrap(),
                                        }).await;
                                        current_tool_id = None;
                                    }
                                }
                                Some("message_delta") => {
                                    #[derive(Deserialize)]
                                    struct MessageDelta {
                                        stop_reason: Option<String>,
                                        usage: Option<UsageDelta>,
                                    }
                                    #[derive(Deserialize)]
                                    struct UsageDelta {
                                        output_tokens: u32,
                                    }

                                    if let Ok(parsed) = serde_json::from_str::<MessageDelta>(&data_str) {
                                        if let Some(reason) = parsed.stop_reason {
                                            let stop_reason = AnthropicClient::parse_stop_reason(Some(&reason));
                                            let _ = tx.send(LlmStreamEvent::Stop { reason: stop_reason }).await;
                                        }
                                        if let Some(usage) = parsed.usage {
                                            let _ = tx.send(LlmStreamEvent::Usage {
                                                input_tokens: 0,
                                                output_tokens: usage.output_tokens,
                                            }).await;
                                        }
                                    }
                                }
                                Some("message_start") => {
                                    #[derive(Deserialize)]
                                    struct MessageStart {
                                        usage: Option<AnthropicUsage>,
                                    }
                                    if let Ok(parsed) = serde_json::from_str::<MessageStart>(&data_str) {
                                        if let Some(usage) = parsed.usage {
                                            let _ = tx.send(LlmStreamEvent::Usage {
                                                input_tokens: usage.input_tokens,
                                                output_tokens: 0,
                                            }).await;
                                        }
                                    }
                                }
                                Some("ping") | Some("error") => {
                                    // 心跳和错误事件
                                }
                                _ => {}
                            }
                        }
                    }
                    Err(e) => {
                        let _ = tx.send(LlmStreamEvent::Error(format!("流读取错误: {}", e))).await;
                        break;
                    }
                }
            }
}
