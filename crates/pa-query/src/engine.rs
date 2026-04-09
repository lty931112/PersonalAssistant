//! 查询引擎模块
//!
//! 实现核心的 reask 查询循环逻辑，支持：
//! - 流式 reask（逐 token 返回响应）
//! - 并发工具执行（is_concurrency_safe 的工具可并行）
//! - 中断支持（通过 CancellationToken 取消查询）
//! - 状态快照（保存/恢复查询状态）
//! - 进度回调（通过事件通道通知轮次完成）

use tokio::sync::mpsc;
use tracing;

use pa_core::{
    ContentBlock, CoreError, Message, MessageRole, QueryEvent, StopReason,
    UsageInfo,
};
use pa_llm::{LlmClientTrait, LlmResponse, LlmStreamEvent};
use pa_memory::MagmaMemoryEngine;
use pa_task::CancellationToken;
use pa_tools::ToolRegistry;

use crate::config::QueryConfig;
use crate::error::{ErrorAction, ErrorClassifier};
use crate::permission::{PermissionChecker, PermissionDecision};

/// 查询结果
#[derive(Debug)]
pub enum QueryOutcome {
    /// 正常结束
    EndTurn { message: Message, usage: UsageInfo },
    /// 达到最大 token 数
    MaxTokens { partial_message: Message, usage: UsageInfo },
    /// 被取消
    Cancelled,
    /// 出错
    Error(CoreError),
    /// 超出预算
    BudgetExceeded { cost_usd: f64, limit_usd: f64 },
}

/// 查询状态快照
///
/// 用于保存查询引擎的中间状态，支持中断恢复。
/// 可以通过 `take_snapshot()` 创建，通过 `restore_from_snapshot()` 恢复。
#[derive(Debug, Clone)]
pub struct QuerySnapshot {
    /// 对话历史记录
    pub conversation_history: Vec<Message>,
    /// 累计 token 使用量
    pub total_usage: UsageInfo,
    /// 累计费用（美元）
    pub total_cost_usd: f64,
    /// 是否正在使用备用模型
    pub using_fallback: bool,
}

/// Reask 查询循环引擎
///
/// 支持流式响应、并发工具执行、中断取消和状态快照。
pub struct QueryEngine {
    llm_client: Box<dyn LlmClientTrait>,
    memory: MagmaMemoryEngine,
    tool_registry: ToolRegistry,
    permission_checker: PermissionChecker,
    total_usage: UsageInfo,
    total_cost_usd: f64,
    conversation_history: Vec<Message>,
    using_fallback: bool,
    /// 取消令牌（可选）
    ///
    /// 当设置后，引擎会在 reask 循环的关键点检查取消状态。
    cancel_token: Option<CancellationToken>,
}

impl QueryEngine {
    pub fn new(
        llm_client: Box<dyn LlmClientTrait>,
        memory: MagmaMemoryEngine,
        tool_registry: ToolRegistry,
    ) -> Result<Self, CoreError> {
        Ok(QueryEngine {
            llm_client,
            memory,
            tool_registry,
            permission_checker: PermissionChecker::new(),
            total_usage: UsageInfo::default(),
            total_cost_usd: 0.0,
            conversation_history: Vec::new(),
            using_fallback: false,
            cancel_token: None,
        })
    }

    // ========================================================================
    // 中断支持
    // ========================================================================

    /// 设置取消令牌
    ///
    /// 设置后，引擎会在 reask 循环的每个关键点（每轮开始、LLM 调用前、工具执行前）
    /// 检查取消状态。如果被取消，将返回 `QueryOutcome::Cancelled` 并保存状态快照。
    pub fn set_cancel_token(&mut self, token: CancellationToken) {
        self.cancel_token = Some(token);
    }

    /// 清除取消令牌
    pub fn clear_cancel_token(&mut self) {
        self.cancel_token = None;
    }

    /// 检查是否已被取消
    fn is_cancelled(&self) -> bool {
        self.cancel_token
            .as_ref()
            .map(|t| t.is_cancelled())
            .unwrap_or(false)
    }

    // ========================================================================
    // 状态快照
    // ========================================================================

    /// 获取当前状态快照
    ///
    /// 保存引擎的完整中间状态，包括对话历史、使用量和模型状态。
    /// 可用于中断恢复或调试检查。
    pub fn take_snapshot(&self) -> QuerySnapshot {
        QuerySnapshot {
            conversation_history: self.conversation_history.clone(),
            total_usage: self.total_usage.clone(),
            total_cost_usd: self.total_cost_usd,
            using_fallback: self.using_fallback,
        }
    }

    /// 从快照恢复状态
    ///
    /// 将引擎状态恢复到快照保存时的状态。
    /// 注意：这会覆盖当前的对话历史和使用量统计。
    pub fn restore_from_snapshot(&mut self, snapshot: QuerySnapshot) {
        self.conversation_history = snapshot.conversation_history;
        self.total_usage = snapshot.total_usage;
        self.total_cost_usd = snapshot.total_cost_usd;
        self.using_fallback = snapshot.using_fallback;
    }

    // ========================================================================
    // 简化版执行（保持原有签名不变）
    // ========================================================================

    /// 简化版执行
    pub async fn execute(&mut self, prompt: String, config: QueryConfig) -> Result<String, CoreError> {
        let (tx, _rx) = mpsc::channel(256);
        let outcome = self.execute_with_events(prompt, config, tx).await;

        match outcome {
            Ok(QueryOutcome::EndTurn { message, .. }) => Ok(message.text_content()),
            Ok(QueryOutcome::MaxTokens { partial_message, .. }) => {
                Ok(format!("[达到最大 token]\n{}", partial_message.text_content()))
            }
            Ok(QueryOutcome::Cancelled) => Ok("[已取消]".to_string()),
            Ok(QueryOutcome::Error(e)) => Err(e),
            Ok(QueryOutcome::BudgetExceeded { cost_usd, limit_usd }) => {
                Err(CoreError::BudgetExceeded { cost_usd, limit_usd })
            }
            Err(e) => Err(e),
        }
    }

    // ========================================================================
    // 带事件流的 reask 循环（保持原有签名不变，增加中断检查）
    // ========================================================================

    /// 带事件流的 reask 循环
    pub async fn execute_with_events(
        &mut self,
        prompt: String,
        config: QueryConfig,
        event_tx: mpsc::Sender<QueryEvent>,
    ) -> Result<QueryOutcome, CoreError> {
        tracing::info!(model = %config.model, max_turns = config.max_turns, "开始 reask 循环");

        // 预算检查
        if let Some(limit) = config.max_budget_usd {
            if self.total_cost_usd >= limit {
                return Ok(QueryOutcome::BudgetExceeded {
                    cost_usd: self.total_cost_usd,
                    limit_usd: limit,
                });
            }
        }

        // 添加用户消息
        self.conversation_history.push(Message::user(&prompt));

        let tool_definitions = self.tool_registry.list_definitions();

        // 构建系统提示（含记忆上下文）
        let system_prompt = self.build_system_prompt(&config).await;

        let mut turn = 0u32;

        loop {
            turn += 1;

            // ---- 中断检查：每轮开始 ----
            if self.is_cancelled() {
                tracing::info!(turn, "查询被取消（轮次开始时）");
                let _ = event_tx.send(QueryEvent::Status("查询已被用户取消".to_string())).await;
                return Ok(QueryOutcome::Cancelled);
            }

            if turn > config.max_turns {
                tracing::warn!(turn, max_turns = config.max_turns, "达到最大轮数");
                let _ = event_tx.send(QueryEvent::Error(format!("达到最大轮数 ({})", config.max_turns))).await;
                break;
            }

            // 自动压缩检查
            {
                let total_chars: usize = self.conversation_history.iter().map(|m| m.text_content().len()).sum();
                let estimated_tokens = (total_chars as f64 * 0.25) as u32;
                if (estimated_tokens as f64 / 200_000.0) > 0.9 {
                    tracing::info!("触发自动压缩");
                    Self::auto_compact_static(&mut self.conversation_history);
                }
            }

            // 工具结果预算
            Self::apply_tool_result_budget_static(&mut self.conversation_history, config.tool_result_budget);

            // ---- 中断检查：LLM 调用前 ----
            if self.is_cancelled() {
                tracing::info!(turn, "查询被取消（LLM 调用前）");
                let _ = event_tx.send(QueryEvent::Status("查询已被用户取消".to_string())).await;
                return Ok(QueryOutcome::Cancelled);
            }

            // 调用 LLM
            let response = match self.llm_client.complete(
                &self.conversation_history,
                &tool_definitions,
                &system_prompt,
            ).await {
                Ok(r) => r,
                Err(error) => {
                    let action = ErrorClassifier::classify(&error);
                    match action {
                        ErrorAction::Retry { after_ms } => {
                            if after_ms > 0 {
                                tokio::time::sleep(std::time::Duration::from_millis(after_ms)).await;
                            }
                            turn -= 1;
                            continue;
                        }
                        ErrorAction::FallbackModel => {
                            if !self.using_fallback {
                                self.using_fallback = true;
                                turn -= 1;
                                continue;
                            }
                            return Ok(QueryOutcome::Error(error));
                        }
                        ErrorAction::AutoCompact => {
                            Self::auto_compact_static(&mut self.conversation_history);
                            turn -= 1;
                            continue;
                        }
                        ErrorAction::Abort => {
                            return Ok(QueryOutcome::Error(error));
                        }
                    }
                }
            };

            // 更新使用量
            self.total_usage.input_tokens += response.usage.input_tokens;
            self.total_usage.output_tokens += response.usage.output_tokens;

            // 发送状态事件
            let _ = event_tx.send(QueryEvent::Status(format!("轮次 {}: {} tokens", turn,
                self.total_usage.input_tokens + self.total_usage.output_tokens))).await;

            // Token 使用警告
            self.check_usage_warning(&self.total_usage, &event_tx).await;

            // 构建助手消息
            let assistant_message = Message::assistant(response.content.clone());
            self.conversation_history.push(assistant_message.clone());

            // 发送文本流事件
            for block in &response.content {
                if let ContentBlock::Text { text } = block {
                    let _ = event_tx.send(QueryEvent::Stream { delta: text.clone() }).await;
                }
            }

            match response.stop_reason {
                StopReason::EndTurn => {
                    tracing::info!(turn, "end_turn，查询完成");
                    let _ = event_tx.send(QueryEvent::TurnComplete {
                        turn,
                        stop_reason: StopReason::EndTurn,
                        usage: self.total_usage.clone(),
                    }).await;
                    return Ok(QueryOutcome::EndTurn {
                        message: assistant_message,
                        usage: self.total_usage.clone(),
                    });
                }
                StopReason::MaxTokens => {
                    tracing::warn!(turn, "max_tokens");
                    return Ok(QueryOutcome::MaxTokens {
                        partial_message: assistant_message,
                        usage: self.total_usage.clone(),
                    });
                }
                StopReason::ToolUse => {
                    tracing::debug!(turn, "tool_use，执行工具");

                    // ---- 中断检查：工具执行前 ----
                    if self.is_cancelled() {
                        tracing::info!(turn, "查询被取消（工具执行前）");
                        let _ = event_tx.send(QueryEvent::Status("查询已被用户取消".to_string())).await;
                        return Ok(QueryOutcome::Cancelled);
                    }

                    let tool_results = self.execute_tools(&response, &event_tx, config.concurrent_tools).await;

                    let tool_result_blocks: Vec<ContentBlock> = tool_results
                        .into_iter()
                        .map(|(tool_use_id, result)| {
                            ContentBlock::tool_result(tool_use_id, result.content, result.is_error)
                        })
                        .collect();

                    if !tool_result_blocks.is_empty() {
                        self.conversation_history.push(Message::new(MessageRole::User, tool_result_blocks));
                    }

                    // 发送轮次完成事件
                    let _ = event_tx.send(QueryEvent::TurnComplete {
                        turn,
                        stop_reason: StopReason::ToolUse,
                        usage: self.total_usage.clone(),
                    }).await;

                    continue;
                }
                StopReason::Cancelled => {
                    return Ok(QueryOutcome::Cancelled);
                }
                _ => {
                    return Ok(QueryOutcome::EndTurn {
                        message: assistant_message,
                        usage: self.total_usage.clone(),
                    });
                }
            }
        }

        Ok(QueryOutcome::Error(CoreError::Internal("查询循环异常退出".to_string())))
    }

    // ========================================================================
    // 流式 reask 执行
    // ========================================================================

    /// 流式 reask 执行
    ///
    /// 使用 LLM 的流式接口，逐 token 返回响应内容。
    /// 返回一个 `mpsc::Receiver<QueryEvent>`，调用者可以实时接收事件流。
    ///
    /// 与 `execute_with_events()` 的区别：
    /// - LLM 调用使用 `stream()` 而非 `complete()`，实现逐 token 输出
    /// - 支持中断检查（通过之前设置的 CancellationToken）
    /// - 支持并发工具执行（根据 config.concurrent_tools 配置）
    pub async fn execute_stream(
        &mut self,
        prompt: String,
        config: QueryConfig,
    ) -> Result<mpsc::Receiver<QueryEvent>, CoreError> {
        let (event_tx, event_rx) = mpsc::channel(256);

        // 将必要的引用移动到异步任务中
        // 注意：由于 &mut self 的生命周期限制，我们在当前任务中执行
        self.execute_stream_inner(prompt, config, event_tx).await?;

        Ok(event_rx)
    }

    /// 流式 reask 内部实现
    async fn execute_stream_inner(
        &mut self,
        prompt: String,
        config: QueryConfig,
        event_tx: mpsc::Sender<QueryEvent>,
    ) -> Result<(), CoreError> {
        tracing::info!(model = %config.model, max_turns = config.max_turns, "开始流式 reask 循环");

        // 预算检查
        if let Some(limit) = config.max_budget_usd {
            if self.total_cost_usd >= limit {
                let _ = event_tx.send(QueryEvent::Error(format!(
                    "超出预算: ${:.4} / ${:.4}", self.total_cost_usd, limit
                ))).await;
                return Ok(());
            }
        }

        // 添加用户消息
        self.conversation_history.push(Message::user(&prompt));

        let tool_definitions = self.tool_registry.list_definitions();

        // 构建系统提示（含记忆上下文）
        let system_prompt = self.build_system_prompt(&config).await;

        let mut turn = 0u32;

        loop {
            turn += 1;

            // ---- 中断检查：每轮开始 ----
            if self.is_cancelled() {
                tracing::info!(turn, "流式查询被取消（轮次开始时）");
                let _ = event_tx.send(QueryEvent::Status("查询已被用户取消".to_string())).await;
                return Ok(());
            }

            if turn > config.max_turns {
                tracing::warn!(turn, max_turns = config.max_turns, "达到最大轮数");
                let _ = event_tx.send(QueryEvent::Error(format!("达到最大轮数 ({})", config.max_turns))).await;
                break;
            }

            // 自动压缩检查
            {
                let total_chars: usize = self.conversation_history.iter().map(|m| m.text_content().len()).sum();
                let estimated_tokens = (total_chars as f64 * 0.25) as u32;
                if (estimated_tokens as f64 / 200_000.0) > 0.9 {
                    tracing::info!("触发自动压缩");
                    Self::auto_compact_static(&mut self.conversation_history);
                }
            }

            // 工具结果预算
            Self::apply_tool_result_budget_static(&mut self.conversation_history, config.tool_result_budget);

            // ---- 中断检查：LLM 调用前 ----
            if self.is_cancelled() {
                tracing::info!(turn, "流式查询被取消（LLM 调用前）");
                let _ = event_tx.send(QueryEvent::Status("查询已被用户取消".to_string())).await;
                return Ok(());
            }

            // 根据配置选择流式或非流式 LLM 调用
            let stream_result = if config.enable_streaming {
                self.llm_client.stream(
                    &self.conversation_history,
                    &tool_definitions,
                    &system_prompt,
                ).await
            } else {
                // 非流式模式：调用 complete() 然后将结果转换为事件流
                self.complete_as_stream(
                    &self.conversation_history,
                    &tool_definitions,
                    &system_prompt,
                    &event_tx,
                ).await
            };

            let mut stream_rx = match stream_result {
                Ok(rx) => rx,
                Err(error) => {
                    let action = ErrorClassifier::classify(&error);
                    match action {
                        ErrorAction::Retry { after_ms } => {
                            if after_ms > 0 {
                                tokio::time::sleep(std::time::Duration::from_millis(after_ms)).await;
                            }
                            turn -= 1;
                            continue;
                        }
                        ErrorAction::FallbackModel => {
                            if !self.using_fallback {
                                self.using_fallback = true;
                                turn -= 1;
                                continue;
                            }
                            let _ = event_tx.send(QueryEvent::Error(format!("LLM 错误: {}", error))).await;
                            return Ok(());
                        }
                        ErrorAction::AutoCompact => {
                            Self::auto_compact_static(&mut self.conversation_history);
                            turn -= 1;
                            continue;
                        }
                        ErrorAction::Abort => {
                            let _ = event_tx.send(QueryEvent::Error(format!("LLM 错误（不可恢复）: {}", error))).await;
                            return Ok(());
                        }
                    }
                }
            };

            // 消费流式事件，构建完整响应
            let mut content_blocks: Vec<ContentBlock> = Vec::new();
            let mut current_text = String::new();
            let mut tool_use_inputs: std::collections::HashMap<String, (String, String)> = std::collections::HashMap::new();
            let mut stop_reason = StopReason::EndTurn;
            let mut stream_usage = UsageInfo::default();

            // 用于跟踪当前正在构建的工具使用块
            let mut current_tool_id: Option<String> = None;
            let mut current_tool_name: Option<String> = None;

            while let Some(event) = stream_rx.recv().await {
                // 流式消费过程中也检查取消
                if self.is_cancelled() {
                    tracing::info!(turn, "流式查询被取消（消费流过程中）");
                    let _ = event_tx.send(QueryEvent::Status("查询已被用户取消".to_string())).await;
                    return Ok(());
                }

                match event {
                    LlmStreamEvent::Delta { text } => {
                        // 如果之前有未完成的文本，先保存
                        if !current_text.is_empty() && current_tool_id.is_none() {
                            // 文本正在累积中
                        }
                        current_text.push_str(&text);

                        // 转发流式事件给调用者
                        let _ = event_tx.send(QueryEvent::Stream { delta: text }).await;
                    }
                    LlmStreamEvent::ToolUseStart { id, name } => {
                        // 保存之前累积的文本
                        if !current_text.is_empty() {
                            content_blocks.push(ContentBlock::text(&current_text));
                            current_text.clear();
                        }

                        current_tool_id = Some(id.clone());
                        current_tool_name = Some(name.clone());
                        tool_use_inputs.insert(id.clone(), (name.clone(), String::new()));

                        tracing::debug!(tool_id = %id, tool_name = %name, "流式：工具使用开始");
                    }
                    LlmStreamEvent::ToolUseInputDelta { id, delta } => {
                        if let Some((_, input)) = tool_use_inputs.get_mut(&id) {
                            input.push_str(&delta);
                        }
                    }
                    LlmStreamEvent::ToolUseEnd { id } => {
                        if let Some((name, input_str)) = tool_use_inputs.remove(&id) {
                            // 解析工具输入 JSON
                            let input_value: serde_json::Value = serde_json::from_str(&input_str)
                                .unwrap_or(serde_json::json!({"raw_input": input_str}));

                            content_blocks.push(ContentBlock::tool_use(&id, &name, input_value));
                        }
                        current_tool_id = None;
                        current_tool_name = None;
                    }
                    LlmStreamEvent::ThinkingDelta { delta } => {
                        // 思考内容可以作为状态事件转发
                        let _ = event_tx.send(QueryEvent::Status(format!("[思考] {}", delta))).await;
                    }
                    LlmStreamEvent::Usage { input_tokens, output_tokens } => {
                        stream_usage.input_tokens = input_tokens;
                        stream_usage.output_tokens = output_tokens;
                    }
                    LlmStreamEvent::Stop { reason } => {
                        stop_reason = reason;
                    }
                    LlmStreamEvent::Error(e) => {
                        tracing::error!(error = %e, "流式 LLM 错误");
                        let _ = event_tx.send(QueryEvent::Error(format!("流式错误: {}", e))).await;
                    }
                }
            }

            // 保存剩余的文本
            if !current_text.is_empty() {
                content_blocks.push(ContentBlock::text(&current_text));
            }

            // 更新使用量
            self.total_usage.input_tokens += stream_usage.input_tokens;
            self.total_usage.output_tokens += stream_usage.output_tokens;

            // 发送状态事件
            let _ = event_tx.send(QueryEvent::Status(format!("轮次 {}: {} tokens", turn,
                self.total_usage.input_tokens + self.total_usage.output_tokens))).await;

            // Token 使用警告
            self.check_usage_warning(&self.total_usage, &event_tx).await;

            // 构建助手消息
            let assistant_message = Message::assistant(content_blocks.clone());
            self.conversation_history.push(assistant_message.clone());

            match stop_reason {
                StopReason::EndTurn => {
                    tracing::info!(turn, "流式 end_turn，查询完成");
                    let _ = event_tx.send(QueryEvent::TurnComplete {
                        turn,
                        stop_reason: StopReason::EndTurn,
                        usage: self.total_usage.clone(),
                    }).await;
                    return Ok(());
                }
                StopReason::MaxTokens => {
                    tracing::warn!(turn, "流式 max_tokens");
                    let _ = event_tx.send(QueryEvent::TurnComplete {
                        turn,
                        stop_reason: StopReason::MaxTokens,
                        usage: self.total_usage.clone(),
                    }).await;
                    return Ok(());
                }
                StopReason::ToolUse => {
                    tracing::debug!(turn, "流式 tool_use，执行工具");

                    // ---- 中断检查：工具执行前 ----
                    if self.is_cancelled() {
                        tracing::info!(turn, "流式查询被取消（工具执行前）");
                        let _ = event_tx.send(QueryEvent::Status("查询已被用户取消".to_string())).await;
                        return Ok(());
                    }

                    // 从 content_blocks 中提取工具使用信息并执行
                    let tool_results = self.execute_tools_from_blocks(
                        &content_blocks,
                        &event_tx,
                        config.concurrent_tools,
                    ).await;

                    let tool_result_blocks: Vec<ContentBlock> = tool_results
                        .into_iter()
                        .map(|(tool_use_id, result)| {
                            ContentBlock::tool_result(tool_use_id, result.content, result.is_error)
                        })
                        .collect();

                    if !tool_result_blocks.is_empty() {
                        self.conversation_history.push(Message::new(MessageRole::User, tool_result_blocks));
                    }

                    // 发送轮次完成事件
                    let _ = event_tx.send(QueryEvent::TurnComplete {
                        turn,
                        stop_reason: StopReason::ToolUse,
                        usage: self.total_usage.clone(),
                    }).await;

                    continue;
                }
                StopReason::Cancelled => {
                    let _ = event_tx.send(QueryEvent::Status("查询已被取消".to_string())).await;
                    return Ok(());
                }
                _ => {
                    let _ = event_tx.send(QueryEvent::TurnComplete {
                        turn,
                        stop_reason: stop_reason.clone(),
                        usage: self.total_usage.clone(),
                    }).await;
                    return Ok(());
                }
            }
        }

        Ok(())
    }

    /// 将非流式 complete() 调用包装为流式事件
    ///
    /// 当 `enable_streaming` 为 false 时使用此方法，
    /// 调用 `complete()` 后将完整响应拆分为流式事件发送。
    async fn complete_as_stream(
        &self,
        messages: &[Message],
        tools: &[pa_core::ToolDefinition],
        system: &str,
        event_tx: &mpsc::Sender<QueryEvent>,
    ) -> Result<mpsc::Receiver<LlmStreamEvent>, CoreError> {
        let response = self.llm_client.complete(messages, tools, system).await?;

        // 创建一个内部通道来模拟流式输出
        let (tx, rx) = mpsc::channel(256);

        // 在后台任务中将完整响应转换为流式事件
        let content = response.content.clone();
        let stop_reason = response.stop_reason.clone();
        let usage = response.usage.clone();

        tokio::spawn(async move {
            for block in &content {
                match block {
                    ContentBlock::Text { text } => {
                        // 将文本拆分为小片段模拟流式输出
                        let chunk_size = 20; // 每次发送约 20 个字符
                        for chunk in text.as_bytes().chunks(chunk_size) {
                            let text = String::from_utf8_lossy(chunk).to_string();
                            let _ = tx.send(LlmStreamEvent::Delta { text }).await;
                            // 添加微小延迟模拟流式效果
                            tokio::time::sleep(std::time::Duration::from_millis(1)).await;
                        }
                    }
                    ContentBlock::ToolUse { id, name, input } => {
                        let _ = tx.send(LlmStreamEvent::ToolUseStart {
                            id: id.clone(),
                            name: name.clone(),
                        }).await;
                        let input_str = serde_json::to_string(input).unwrap_or_default();
                        let _ = tx.send(LlmStreamEvent::ToolUseInputDelta {
                            id: id.clone(),
                            delta: input_str,
                        }).await;
                        let _ = tx.send(LlmStreamEvent::ToolUseEnd { id: id.clone() }).await;
                    }
                    ContentBlock::ToolResult { .. } => {
                        // 工具结果块不需要流式处理
                    }
                    ContentBlock::Thinking { thinking, .. } => {
                        let _ = tx.send(LlmStreamEvent::ThinkingDelta { delta: thinking.clone() }).await;
                    }
                }
            }

            // 发送使用量和停止事件
            let _ = tx.send(LlmStreamEvent::Usage {
                input_tokens: usage.input_tokens,
                output_tokens: usage.output_tokens,
            }).await;
            let _ = tx.send(LlmStreamEvent::Stop { reason: stop_reason }).await;
        });

        Ok(rx)
    }

    // ========================================================================
    // 工具执行（支持并发）
    // ========================================================================

    /// 执行所有工具调用（从 LlmResponse 中提取）
    ///
    /// 当 `concurrent_tools` 启用时，将工具分为两组：
    /// - 并发安全组（`is_concurrency_safe == true`）：使用 `join_all` 并行执行
    /// - 顺序执行组：按原始顺序依次执行
    ///
    /// 执行顺序：先执行并发安全组（并行），再执行顺序组。
    async fn execute_tools(
        &self,
        response: &LlmResponse,
        event_tx: &mpsc::Sender<QueryEvent>,
        concurrent_tools: bool,
    ) -> Vec<(String, pa_core::ToolResult)> {
        let tool_uses = response.tool_uses();
        if tool_uses.is_empty() {
            return Vec::new();
        }

        // 提取工具调用信息
        let tool_calls: Vec<(String, String, serde_json::Value)> = tool_uses
            .iter()
            .filter_map(|block| {
                block.as_tool_use().map(|(id, name, input)| {
                    (id.to_string(), name.to_string(), input.clone())
                })
            })
            .collect();

        self.execute_tool_calls(tool_calls, event_tx, concurrent_tools).await
    }

    /// 从 ContentBlock 列表中执行工具调用（流式模式使用）
    ///
    /// 与 `execute_tools()` 类似，但直接接受 ContentBlock 列表，
    /// 用于流式模式下从累积的内容块中提取工具调用。
    async fn execute_tools_from_blocks(
        &self,
        content_blocks: &[ContentBlock],
        event_tx: &mpsc::Sender<QueryEvent>,
        concurrent_tools: bool,
    ) -> Vec<(String, pa_core::ToolResult)> {
        let tool_calls: Vec<(String, String, serde_json::Value)> = content_blocks
            .iter()
            .filter_map(|block| {
                block.as_tool_use().map(|(id, name, input)| {
                    (id.to_string(), name.to_string(), input.clone())
                })
            })
            .collect();

        if tool_calls.is_empty() {
            return Vec::new();
        }

        self.execute_tool_calls(tool_calls, event_tx, concurrent_tools).await
    }

    /// 执行工具调用的核心实现
    ///
    /// 将工具分为并发安全组和顺序执行组，分别执行后合并结果。
    /// 结果保持原始调用顺序返回。
    async fn execute_tool_calls(
        &self,
        tool_calls: Vec<(String, String, serde_json::Value)>,
        event_tx: &mpsc::Sender<QueryEvent>,
        concurrent_tools: bool,
    ) -> Vec<(String, pa_core::ToolResult)> {
        tracing::info!(tool_count = tool_calls.len(), "执行工具");

        if !concurrent_tools || tool_calls.len() == 1 {
            // 不启用并发或只有一个工具，直接顺序执行
            return self.execute_tools_sequential(&tool_calls, event_tx).await;
        }

        // 分组：并发安全 vs 顺序执行
        let mut concurrent_group: Vec<(usize, String, String, serde_json::Value)> = Vec::new();
        let mut sequential_group: Vec<(usize, String, String, serde_json::Value)> = Vec::new();

        for (idx, (id, name, input)) in tool_calls.iter().enumerate() {
            let is_safe = self.tool_registry
                .get(name)
                .map(|tool| tool.is_concurrency_safe())
                .unwrap_or(false);

            if is_safe {
                concurrent_group.push((idx, id.clone(), name.clone(), input.clone()));
            } else {
                sequential_group.push((idx, id.clone(), name.clone(), input.clone()));
            }
        }

        tracing::debug!(
            concurrent_count = concurrent_group.len(),
            sequential_count = sequential_group.len(),
            "工具分组完成"
        );

        // 并发执行安全组
        let mut all_results: std::collections::HashMap<usize, (String, pa_core::ToolResult)> =
            std::collections::HashMap::new();

        if !concurrent_group.is_empty() {
            let mut futures = Vec::new();

            for (idx, id, name, input) in concurrent_group {
                let event_tx = event_tx.clone();
                let registry = &self.tool_registry;
                let permission_checker = &self.permission_checker;

                futures.push(async move {
                    // 权限检查
                    let permission = permission_checker.check(&name, &input);
                    match permission {
                        PermissionDecision::Deny { reason } => {
                            let _ = event_tx.send(QueryEvent::ToolEnd {
                                tool_name: name.clone(),
                                tool_id: id.clone(),
                                result: format!("权限拒绝: {}", reason),
                                is_error: true,
                            }).await;
                            return (idx, id.clone(), pa_core::ToolResult::error(&id, &name, format!("权限拒绝: {}", reason)));
                        }
                        PermissionDecision::Ask { prompt } => {
                            let _ = event_tx.send(QueryEvent::ToolEnd {
                                tool_name: name.clone(),
                                tool_id: id.clone(),
                                result: format!("需要确认: {}", prompt),
                                is_error: true,
                            }).await;
                            return (idx, id.clone(), pa_core::ToolResult::error(&id, &name, format!("需要确认: {}", prompt)));
                        }
                        PermissionDecision::Allow => {}
                    }

                    // 发送工具开始事件
                    let _ = event_tx.send(QueryEvent::ToolStart {
                        tool_name: name.clone(),
                        tool_id: id.clone(),
                        input_json: input.clone(),
                    }).await;

                    // 执行工具
                    let result = registry.execute(&name, &id, input).await;
                    let tool_result = match &result {
                        Ok(r) => r.clone(),
                        Err(e) => pa_core::ToolResult::error(&id, &name, e.to_string()),
                    };

                    // 发送工具结束事件
                    let _ = event_tx.send(QueryEvent::ToolEnd {
                        tool_name: name.clone(),
                        tool_id: id.clone(),
                        result: tool_result.content.clone(),
                        is_error: tool_result.is_error,
                    }).await;

                    (idx, id, tool_result)
                });
            }

            let results = futures::future::join_all(futures).await;
            for (idx, id, tool_result) in results {
                all_results.insert(idx, (id, tool_result));
            }
        }

        // 顺序执行不安全组
        if !sequential_group.is_empty() {
            let sequential_results = self.execute_tools_sequential_inner(
                &sequential_group,
                event_tx,
            ).await;

            for (idx, id, tool_result) in sequential_results {
                all_results.insert(idx, (id, tool_result));
            }
        }

        // 按原始顺序合并结果
        let mut ordered_results = Vec::new();
        let total_count = tool_calls.len();
        for idx in 0..total_count {
            if let Some(result) = all_results.remove(&idx) {
                ordered_results.push(result);
            }
        }

        ordered_results
    }

    /// 顺序执行工具调用列表
    async fn execute_tools_sequential(
        &self,
        tool_calls: &[(String, String, serde_json::Value)],
        event_tx: &mpsc::Sender<QueryEvent>,
    ) -> Vec<(String, pa_core::ToolResult)> {
        // 将 tool_calls 转换为带索引的格式
        let indexed: Vec<(usize, String, String, serde_json::Value)> = tool_calls
            .iter()
            .enumerate()
            .map(|(idx, (id, name, input))| (idx, id.clone(), name.clone(), input.clone()))
            .collect();

        self.execute_tools_sequential_inner(&indexed, event_tx)
            .await
            .into_iter()
            .map(|(_, id, result)| (id, result))
            .collect()
    }

    /// 顺序执行工具调用的内部实现
    async fn execute_tools_sequential_inner(
        &self,
        tool_calls: &[(usize, String, String, serde_json::Value)],
        event_tx: &mpsc::Sender<QueryEvent>,
    ) -> Vec<(usize, String, pa_core::ToolResult)> {
        let mut results = Vec::new();

        for (idx, id, name, input) in tool_calls {
            // 发送工具开始事件
            let _ = event_tx.send(QueryEvent::ToolStart {
                tool_name: name.clone(),
                tool_id: id.clone(),
                input_json: input.clone(),
            }).await;

            // 权限检查
            let permission = self.permission_checker.check(name, input);
            match permission {
                PermissionDecision::Allow => {}
                PermissionDecision::Deny { reason } => {
                    let _ = event_tx.send(QueryEvent::ToolEnd {
                        tool_name: name.clone(),
                        tool_id: id.clone(),
                        result: format!("权限拒绝: {}", reason),
                        is_error: true,
                    }).await;
                    results.push((*idx, id.clone(),
                        pa_core::ToolResult::error(id, name, format!("权限拒绝: {}", reason))));
                    continue;
                }
                PermissionDecision::Ask { prompt } => {
                    let _ = event_tx.send(QueryEvent::ToolEnd {
                        tool_name: name.clone(),
                        tool_id: id.clone(),
                        result: format!("需要确认: {}", prompt),
                        is_error: true,
                    }).await;
                    results.push((*idx, id.clone(),
                        pa_core::ToolResult::error(id, name, format!("需要确认: {}", prompt))));
                    continue;
                }
            }

            // 执行工具
            let result = self.tool_registry.execute(name, id, input.clone()).await;
            let tool_result = match &result {
                Ok(r) => r.clone(),
                Err(e) => pa_core::ToolResult::error(id, name, e.to_string()),
            };

            // 发送工具结束事件
            let _ = event_tx.send(QueryEvent::ToolEnd {
                tool_name: name.clone(),
                tool_id: id.clone(),
                result: tool_result.content.clone(),
                is_error: tool_result.is_error,
            }).await;

            results.push((*idx, id.clone(), tool_result));
        }

        results
    }

    // ========================================================================
    // 辅助方法
    // ========================================================================

    /// 构建系统提示（含记忆上下文）
    async fn build_system_prompt(&mut self, config: &QueryConfig) -> String {
        let mut system = config.system_prompt.clone();

        if config.memory_enabled {
            let recent_prompt = self.conversation_history
                .iter()
                .rev()
                .filter(|m| m.role == MessageRole::User)
                .next()
                .map(|m| m.text_content())
                .unwrap_or_default();

            if !recent_prompt.is_empty() {
                match self.memory.retrieve(&recent_prompt, None).await {
                    Ok(result) if !result.nodes.is_empty() => {
                        let context: Vec<String> = result.nodes.iter()
                            .map(|n| format!("[{}] {}", n.node_type, n.content))
                            .collect();
                        system = format!("{}\n\n相关记忆:\n{}", system, context.join("\n"));
                    }
                    Ok(_) => {}
                    Err(e) => {
                        tracing::warn!("记忆检索失败: {}", e);
                    }
                }
            }
        }

        system
    }

    /// 应用工具结果预算（静态方法）
    fn apply_tool_result_budget_static(messages: &mut Vec<Message>, budget: usize) {
        let mut tool_chars: Vec<(usize, usize, usize)> = Vec::new();
        for (mi, msg) in messages.iter().enumerate() {
            for (bi, block) in msg.content.iter().enumerate() {
                if let ContentBlock::ToolResult { content, .. } = block {
                    tool_chars.push((mi, bi, content.len()));
                }
            }
        }

        let total: usize = tool_chars.iter().map(|(_, _, c)| c).sum();
        if total <= budget {
            return;
        }

        tracing::warn!(total, budget, "工具结果超出预算，截断中");
        let scale = budget as f64 / total as f64;

        for (mi, bi, _) in &tool_chars {
            if let Some(ContentBlock::ToolResult { content, .. }) =
                messages.get_mut(*mi).and_then(|m| m.content.get_mut(*bi))
            {
                let max_len = ((content.len() as f64 * scale) as usize).max(100);
                if content.len() > max_len {
                    *content = format!("{}\n\n... [截断，原始 {} 字符]", &content[..max_len], content.len());
                }
            }
        }
    }

    /// 自动压缩上下文（静态方法）
    fn auto_compact_static(messages: &mut Vec<Message>) {
        if messages.len() <= 4 {
            return;
        }

        let keep = messages.len() / 2;
        let removed = messages.len() - keep;
        *messages = messages.iter().skip(removed).cloned().collect();

        for msg in messages.iter_mut() {
            for block in msg.content.iter_mut() {
                if let ContentBlock::ToolResult { content, .. } = block {
                    if content.len() > 500 {
                        let head = &content[..200.min(content.len())];
                        let tail = if content.len() > 400 { &content[content.len()-200..] } else { "" };
                        *content = format!("{}\n\n... [压缩: {} 字符]\n\n{}", head, content.len(), tail);
                    }
                }
            }
        }

        tracing::info!(before = messages.len() + removed, after = messages.len(), "压缩完成");
    }

    /// Token 使用警告
    async fn check_usage_warning(&self, usage: &UsageInfo, event_tx: &mpsc::Sender<QueryEvent>) {
        let total = usage.input_tokens + usage.output_tokens;
        for threshold in &[50_000u32, 100_000, 200_000, 500_000] {
            if total >= *threshold && total - usage.output_tokens < *threshold {
                let _ = event_tx.send(QueryEvent::TokenWarning {
                    state: pa_core::TokenState::Warning,
                    pct_used: 0.5,
                }).await;
                let _ = event_tx.send(QueryEvent::Error(format!(
                    "Token 警告: 已使用 {} (输入: {}, 输出: {})",
                    total, usage.input_tokens, usage.output_tokens
                ))).await;
                break;
            }
        }
    }

    // ========================================================================
    // 访问器
    // ========================================================================

    pub fn total_usage(&self) -> &UsageInfo { &self.total_usage }
    pub fn conversation_history(&self) -> &[Message] { &self.conversation_history }

    pub fn clear_history(&mut self) {
        self.conversation_history.clear();
        self.total_usage = UsageInfo::default();
        self.total_cost_usd = 0.0;
        self.using_fallback = false;
    }
}
