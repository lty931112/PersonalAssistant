//! OpenAI 兼容客户端实现
//!
//! 支持 OpenAI API 的 /v1/chat/completions 端点，
//! 同时兼容 Ollama、vLLM、LM Studio 等本地模型服务。

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
// OpenAI API 请求/响应类型
// ============================================================

/// OpenAI 聊天补全请求
#[derive(Debug, Clone, Serialize)]
struct ChatCompletionRequest {
    model: String,
    messages: Vec<OpenAiMessage>,
    max_tokens: u32,
    temperature: f32,
    stream: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    tools: Option<Vec<OpenAiTool>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    stream_options: Option<StreamOptions>,
}

/// 流式选项
#[derive(Debug, Clone, Serialize)]
struct StreamOptions {
    include_usage: bool,
}

/// OpenAI 消息格式
#[derive(Debug, Clone, Serialize, Deserialize)]
struct OpenAiMessage {
    role: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    content: Option<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    tool_calls: Option<Vec<OpenAiToolCall>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    tool_call_id: Option<String>,
}

/// OpenAI 工具定义
#[derive(Debug, Serialize, Clone)]
struct OpenAiTool {
    r#type: String,
    function: OpenAiFunction,
}

/// OpenAI 函数定义
#[derive(Debug, Serialize, Clone)]
struct OpenAiFunction {
    name: String,
    description: String,
    parameters: serde_json::Value,
}

/// OpenAI 工具调用
#[derive(Debug, Clone, Serialize, Deserialize)]
struct OpenAiToolCall {
    id: String,
    r#type: String,
    function: OpenAiToolCallFunction,
}

/// OpenAI 工具调用函数
#[derive(Debug, Clone, Serialize, Deserialize)]
struct OpenAiToolCallFunction {
    name: String,
    arguments: String,
}

/// OpenAI 聊天补全响应
#[derive(Debug, Deserialize)]
struct ChatCompletionResponse {
    choices: Vec<ChatCompletionChoice>,
    usage: Option<OpenAiUsage>,
    model: Option<String>,
    error: Option<OpenAiError>,
}

/// OpenAI 选择项
#[derive(Debug, Deserialize)]
struct ChatCompletionChoice {
    message: OpenAiMessage,
    finish_reason: Option<String>,
}

/// OpenAI 使用量
#[derive(Debug, Deserialize)]
struct OpenAiUsage {
    prompt_tokens: u32,
    completion_tokens: u32,
}

/// OpenAI 错误
#[derive(Debug, Deserialize)]
struct OpenAiError {
    message: String,
}

/// SSE 流式块
#[derive(Debug, Deserialize)]
struct StreamChunk {
    choices: Option<Vec<StreamChoice>>,
    usage: Option<OpenAiUsage>,
}

/// 流式选择项
#[derive(Debug, Deserialize)]
struct StreamChoice {
    delta: StreamDelta,
    finish_reason: Option<String>,
}

/// 流式增量
#[derive(Debug, Deserialize)]
struct StreamDelta {
    content: Option<String>,
    tool_calls: Option<Vec<StreamToolCall>>,
}

/// 流式工具调用
#[derive(Debug, Deserialize)]
struct StreamToolCall {
    index: usize,
    id: Option<String>,
    function: Option<StreamToolCallFunction>,
}

/// 流式工具调用函数
#[derive(Debug, Deserialize)]
struct StreamToolCallFunction {
    name: Option<String>,
    arguments: Option<String>,
}

// ============================================================
// OpenAI 客户端
// ============================================================

/// OpenAI 兼容客户端
///
/// 支持 OpenAI API、Ollama、vLLM、LM Studio 等兼容服务。
pub struct OpenAiClient {
    /// HTTP 客户端
    http_client: reqwest::Client,
    /// API 基础 URL
    base_url: String,
    /// API 密钥
    api_key: String,
    /// 模型名称
    model: String,
    /// 最大输出 token 数
    max_tokens: u32,
    /// 温度参数
    temperature: f32,
    /// 备用模型
    fallback_model: Option<String>,
    /// 主模型不可用时是否允许在探测通过后切换备用模型
    fallback_switch_enabled: bool,
    /// 最大重试次数
    max_retries: u32,
}

impl OpenAiClient {
    /// 创建新的 OpenAI 兼容客户端
    pub fn new(config: &LlmConfig) -> Result<Self, CoreError> {
        let base_url = config
            .base_url
            .clone()
            .unwrap_or_else(|| "https://api.openai.com".to_string());

        let base_url = base_url.trim_end_matches('/').to_string();

        let http_client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(300))
            .build()
            .map_err(|e| CoreError::Configuration(format!("HTTP 客户端创建失败: {}", e)))?;

        Ok(OpenAiClient {
            http_client,
            base_url,
            api_key: config.api_key.clone(),
            model: config.model.clone(),
            max_tokens: config.max_tokens,
            temperature: config.temperature,
            fallback_model: config.fallback_model.clone(),
            fallback_switch_enabled: config.fallback_switch_enabled,
            max_retries: config.max_retries,
        })
    }

    /// 构建请求头
    fn build_headers(&self) -> Result<HeaderMap, CoreError> {
        let mut headers = HeaderMap::new();
        headers.insert(CONTENT_TYPE, HeaderValue::from_static("application/json"));
        headers.insert(
            AUTHORIZATION,
            HeaderValue::from_str(&format!("Bearer {}", self.api_key))
                .map_err(|e| CoreError::Configuration(format!("API 密钥格式错误: {}", e)))?,
        );
        Ok(headers)
    }

    /// 将内部消息格式转换为 OpenAI 格式
    fn convert_messages(messages: &[Message]) -> Vec<OpenAiMessage> {
        let mut openai_messages = Vec::new();

        for msg in messages {
            match msg.role {
                MessageRole::System => {
                    openai_messages.push(OpenAiMessage {
                        role: "system".to_string(),
                        content: Some(serde_json::Value::String(msg.text_content())),
                        tool_calls: None,
                        tool_call_id: None,
                    });
                }
                MessageRole::User => {
                    openai_messages.push(OpenAiMessage {
                        role: "user".to_string(),
                        content: Some(serde_json::Value::String(msg.text_content())),
                        tool_calls: None,
                        tool_call_id: None,
                    });
                }
                MessageRole::Assistant => {
                    let mut tool_calls = Vec::new();
                    let mut text_parts = Vec::new();

                    for block in &msg.content {
                        match block {
                            ContentBlock::Text { text } => {
                                text_parts.push(text.clone());
                            }
                            ContentBlock::ToolUse { id, name, input } => {
                                tool_calls.push(OpenAiToolCall {
                                    id: id.clone(),
                                    r#type: "function".to_string(),
                                    function: OpenAiToolCallFunction {
                                        name: name.clone(),
                                        arguments: serde_json::to_string(input)
                                            .unwrap_or_else(|_| "{}".to_string()),
                                    },
                                });
                            }
                            ContentBlock::ToolResult { .. } | ContentBlock::Thinking { .. } => {
                                // 工具结果和思考内容在 OpenAI 中不直接支持
                            }
                        }
                    }

                    openai_messages.push(OpenAiMessage {
                        role: "assistant".to_string(),
                        content: if text_parts.is_empty() {
                            None
                        } else {
                            Some(serde_json::Value::String(text_parts.join("")))
                        },
                        tool_calls: if tool_calls.is_empty() {
                            None
                        } else {
                            Some(tool_calls)
                        },
                        tool_call_id: None,
                    });

                    // 工具结果作为单独的 tool 消息
                    for block in &msg.content {
                        if let ContentBlock::ToolResult {
                            tool_use_id,
                            content,
                            ..
                        } = block
                        {
                            openai_messages.push(OpenAiMessage {
                                role: "tool".to_string(),
                                content: Some(serde_json::Value::String(content.clone())),
                                tool_calls: None,
                                tool_call_id: Some(tool_use_id.clone()),
                            });
                        }
                    }
                }
            }
        }

        openai_messages
    }

    /// 转换工具定义为 OpenAI 格式
    fn convert_tools(tools: &[ToolDefinition]) -> Vec<OpenAiTool> {
        tools
            .iter()
            .map(|tool| OpenAiTool {
                r#type: "function".to_string(),
                function: OpenAiFunction {
                    name: tool.name.clone(),
                    description: tool.description.clone(),
                    parameters: tool.input_schema.clone(),
                },
            })
            .collect()
    }

    /// 解析停止原因
    fn parse_finish_reason(reason: Option<&str>) -> StopReason {
        match reason {
            Some("stop") => StopReason::EndTurn,
            Some("length") => StopReason::MaxTokens,
            Some("tool_calls") | Some("function_call") => StopReason::ToolUse,
            Some("content_filter") => StopReason::StopSequence,
            Some(other) => StopReason::Other(other.to_string()),
            None => StopReason::EndTurn,
        }
    }

    /// 解析响应消息为内容块列表
    fn parse_response_message(message: &OpenAiMessage) -> Vec<ContentBlock> {
        let mut blocks = Vec::new();

        if let Some(content) = &message.content {
            if let Some(text) = content.as_str() {
                if !text.is_empty() {
                    blocks.push(ContentBlock::text(text));
                }
            }
        }

        if let Some(tool_calls) = &message.tool_calls {
            for tc in tool_calls {
                let input: serde_json::Value = serde_json::from_str(&tc.function.arguments)
                    .unwrap_or(serde_json::Value::Object(serde_json::Map::new()));
                blocks.push(ContentBlock::tool_use(&tc.id, &tc.function.name, input));
            }
        }

        blocks
    }

    fn openai_error_message(body: &str) -> String {
        serde_json::from_str::<ChatCompletionResponse>(body)
            .ok()
            .and_then(|r| r.error.map(|e| e.message))
            .filter(|s| !s.is_empty())
            .unwrap_or_else(|| body.chars().take(4096).collect())
    }

    async fn probe_model_availability(&self, model: &str) -> bool {
        let probe = ChatCompletionRequest {
            model: model.to_string(),
            messages: vec![OpenAiMessage {
                role: "user".to_string(),
                content: Some(serde_json::Value::String(".".into())),
                tool_calls: None,
                tool_call_id: None,
            }],
            max_tokens: 1,
            temperature: self.temperature,
            stream: false,
            tools: None,
            stream_options: None,
        };
        let url = format!("{}/v1/chat/completions", self.base_url);
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
impl LlmClientTrait for OpenAiClient {
    async fn complete(
        &self,
        messages: &[Message],
        tools: &[ToolDefinition],
        system: &str,
    ) -> Result<LlmResponse, CoreError> {
        let mut all_messages = Vec::new();
        if !system.is_empty() {
            all_messages.push(Message::system(system));
        }
        all_messages.extend_from_slice(messages);

        let openai_messages = Self::convert_messages(&all_messages);
        let openai_tools = if tools.is_empty() {
            None
        } else {
            Some(Self::convert_tools(tools))
        };

        let url = format!("{}/v1/chat/completions", self.base_url);
        let headers = self.build_headers()?;

        let mut active_model = self.model.clone();
        let mut switched_to_fallback = false;
        let mut attempt: u32 = 0;

        loop {
            let request_body = ChatCompletionRequest {
                model: active_model.clone(),
                messages: openai_messages.clone(),
                max_tokens: self.max_tokens,
                temperature: self.temperature,
                stream: false,
                tools: openai_tools.clone(),
                stream_options: None,
            };

            tracing::debug!(url = %url, model = %active_model, "发送非流式请求");

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
                let response: ChatCompletionResponse =
                    serde_json::from_str(&body).map_err(|e| {
                        LlmError::ParseError(format!("响应解析失败: {}", e))
                    })?;

                let choice = response.choices.first().ok_or_else(|| {
                    CoreError::ApiResponse("响应中没有选择项".to_string())
                })?;

                let content = Self::parse_response_message(&choice.message);
                let stop_reason = Self::parse_finish_reason(choice.finish_reason.as_deref());

                let usage = response
                    .usage
                    .map(|u| UsageInfo {
                        input_tokens: u.prompt_tokens,
                        output_tokens: u.completion_tokens,
                        cache_read_tokens: 0,
                        cache_creation_tokens: 0,
                    })
                    .unwrap_or_else(|| UsageInfo {
                        input_tokens: 0,
                        output_tokens: 0,
                        cache_read_tokens: 0,
                        cache_creation_tokens: 0,
                    });

                let model = response
                    .model
                    .unwrap_or_else(|| active_model.clone());

                tracing::debug!(
                    stop_reason = %stop_reason,
                    input_tokens = usage.input_tokens,
                    output_tokens = usage.output_tokens,
                    "非流式请求完成"
                );

                return Ok(LlmResponse {
                    content,
                    stop_reason,
                    usage,
                    model,
                });
            }

            let err_text = Self::openai_error_message(&body);

            if let Ok(err_resp) = serde_json::from_str::<ChatCompletionResponse>(&body) {
                if let Some(error) = err_resp.error {
                    let api_err = LlmError::ApiError {
                        status: status_u16,
                        message: error.message.clone(),
                    };
                    match status_u16 {
                        429 if attempt < self.max_retries => {
                            let wait_ms = 1000u64 * 2u64.pow(attempt);
                            tracing::warn!(attempt = attempt + 1, wait_ms, "速率限制，等待后重试");
                            tokio::time::sleep(std::time::Duration::from_millis(wait_ms)).await;
                            attempt += 1;
                            continue;
                        }
                        529 if attempt < self.max_retries => {
                            let wait_ms = 2000u64 * 2u64.pow(attempt);
                            tracing::warn!(attempt = attempt + 1, wait_ms, "服务过载，等待后重试");
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
        let mut all_messages = Vec::new();
        if !system.is_empty() {
            all_messages.push(Message::system(system));
        }
        all_messages.extend_from_slice(messages);

        let openai_messages = Self::convert_messages(&all_messages);
        let openai_tools = if tools.is_empty() {
            None
        } else {
            Some(Self::convert_tools(tools))
        };

        let url = format!("{}/v1/chat/completions", self.base_url);
        let headers = self.build_headers()?;

        let mut active_model = self.model.clone();
        let mut switched_to_fallback = false;
        let mut attempt: u32 = 0;

        loop {
            let request_body = ChatCompletionRequest {
                model: active_model.clone(),
                messages: openai_messages.clone(),
                max_tokens: self.max_tokens,
                temperature: self.temperature,
                stream: true,
                tools: openai_tools.clone(),
                stream_options: Some(StreamOptions { include_usage: true }),
            };

            tracing::debug!(url = %url, model = %active_model, "发送流式请求");

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
                tokio::spawn(forward_openai_sse_stream(resp, tx));
                return Ok(rx);
            }

            let status_u16 = status.as_u16();
            let body = resp.text().await.unwrap_or_default();
            let err_summary = Self::openai_error_message(&body);

            if let Ok(err_resp) = serde_json::from_str::<ChatCompletionResponse>(&body) {
                if let Some(error) = err_resp.error {
                    match status_u16 {
                        429 if attempt < self.max_retries => {
                            let wait_ms = 1000u64 * 2u64.pow(attempt);
                            tracing::warn!(attempt = attempt + 1, wait_ms, "速率限制（流式首包）");
                            tokio::time::sleep(std::time::Duration::from_millis(wait_ms)).await;
                            attempt += 1;
                            continue;
                        }
                        529 if attempt < self.max_retries => {
                            let wait_ms = 2000u64 * 2u64.pow(attempt);
                            tracing::warn!(attempt = attempt + 1, wait_ms, "服务过载（流式首包）");
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
        "OpenAI"
    }
}

async fn forward_openai_sse_stream(
    mut response: reqwest::Response,
    tx: mpsc::Sender<LlmStreamEvent>,
) {
            let mut stream = response.bytes_stream();
            let mut buffer = String::new();
            let mut current_tool_calls: std::collections::HashMap<usize, (String, String, String)> =
                std::collections::HashMap::new();

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

                            if let Some(data) = line.strip_prefix("data: ") {
                                if data.trim() == "[DONE]" {
                                    let _ = tx
                                        .send(LlmStreamEvent::Stop {
                                            reason: StopReason::EndTurn,
                                        })
                                        .await;
                                    continue;
                                }

                                if let Ok(chunk) = serde_json::from_str::<StreamChunk>(data) {
                                    if let Some(usage) = chunk.usage {
                                        let _ = tx
                                            .send(LlmStreamEvent::Usage {
                                                input_tokens: usage.prompt_tokens,
                                                output_tokens: usage.completion_tokens,
                                            })
                                            .await;
                                    }

                                    if let Some(choices) = chunk.choices {
                                        for choice in choices {
                                            if let Some(content) = choice.delta.content {
                                                if !content.is_empty() {
                                                    let _ = tx
                                                        .send(LlmStreamEvent::Delta { text: content })
                                                        .await;
                                                }
                                            }

                                            if let Some(tool_calls) = choice.delta.tool_calls {
                                                for tc in tool_calls {
                                                    let entry = current_tool_calls
                                                        .entry(tc.index)
                                                        .or_insert_with(|| {
                                                            (String::new(), String::new(), String::new())
                                                        });

                                                    if let Some(id) = tc.id {
                                                        if entry.0.is_empty() {
                                                            entry.0 = id;
                                                        }
                                                    }

                                                    if let Some(ref func) = tc.function {
                                                        if let Some(ref name) = func.name {
                                                            if entry.1.is_empty() {
                                                                entry.1 = name.clone();
                                                                let _ = tx
                                                                    .send(LlmStreamEvent::ToolUseStart {
                                                                        id: entry.0.clone(),
                                                                        name: name.clone(),
                                                                    })
                                                                    .await;
                                                            }
                                                        }
                                                        if let Some(ref args) = func.arguments {
                                                            entry.2.push_str(args);
                                                            if !entry.0.is_empty() {
                                                                let _ = tx
                                                                    .send(LlmStreamEvent::ToolUseInputDelta {
                                                                        id: entry.0.clone(),
                                                                        delta: args.clone(),
                                                                    })
                                                                    .await;
                                                            }
                                                        }
                                                    }
                                                }
                                            }

                                            if let Some(finish_reason) = choice.finish_reason {
                                                for (_, (id, _name, _arguments)) in
                                                    current_tool_calls.drain()
                                                {
                                                    let _ = tx
                                                        .send(LlmStreamEvent::ToolUseEnd { id })
                                                        .await;
                                                }

                                                let stop_reason =
                                                    OpenAiClient::parse_finish_reason(Some(&finish_reason));
                                                let _ = tx
                                                    .send(LlmStreamEvent::Stop { reason: stop_reason })
                                                    .await;
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                    Err(e) => {
                        let _ = tx
                            .send(LlmStreamEvent::Error(format!("流读取错误: {}", e)))
                            .await;
                        break;
                    }
                }
            }
}
