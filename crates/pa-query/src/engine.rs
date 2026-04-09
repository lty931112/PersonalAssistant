//! 查询引擎模块
//!
//! 实现核心的 reask 查询循环逻辑。

use tokio::sync::mpsc;
use tracing;

use pa_core::{
    ContentBlock, CoreError, Message, MessageRole, QueryEvent, StopReason,
    UsageInfo,
};
use pa_llm::{LlmClientTrait, LlmResponse};
use pa_memory::MagmaMemoryEngine;
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

/// Reask 查询循环引擎
pub struct QueryEngine {
    llm_client: Box<dyn LlmClientTrait>,
    memory: MagmaMemoryEngine,
    tool_registry: ToolRegistry,
    permission_checker: PermissionChecker,
    total_usage: UsageInfo,
    total_cost_usd: f64,
    conversation_history: Vec<Message>,
    using_fallback: bool,
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
        })
    }

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
                    let tool_results = self.execute_tools(&response, &event_tx).await;

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

    /// 执行所有工具调用
    async fn execute_tools(
        &self,
        response: &LlmResponse,
        event_tx: &mpsc::Sender<QueryEvent>,
    ) -> Vec<(String, pa_core::ToolResult)> {
        let tool_uses = response.tool_uses();
        if tool_uses.is_empty() {
            return Vec::new();
        }

        tracing::info!(tool_count = tool_uses.len(), "执行工具");

        let mut results = Vec::new();

        for block in tool_uses {
            if let Some((id, name, input)) = block.as_tool_use() {
                // 发送工具开始事件
                let _ = event_tx.send(QueryEvent::ToolStart {
                    tool_name: name.to_string(),
                    tool_id: id.to_string(),
                    input_json: input.clone(),
                }).await;

                // 权限检查
                let permission = self.permission_checker.check(name, input);
                match permission {
                    PermissionDecision::Allow => {}
                    PermissionDecision::Deny { reason } => {
                        let _ = event_tx.send(QueryEvent::ToolEnd {
                            tool_name: name.to_string(),
                            tool_id: id.to_string(),
                            result: format!("权限拒绝: {}", reason),
                            is_error: true,
                        }).await;
                        results.push((id.to_string(),
                            pa_core::ToolResult::error(id, name, format!("权限拒绝: {}", reason))));
                        continue;
                    }
                    PermissionDecision::Ask { prompt } => {
                        let _ = event_tx.send(QueryEvent::ToolEnd {
                            tool_name: name.to_string(),
                            tool_id: id.to_string(),
                            result: format!("需要确认: {}", prompt),
                            is_error: true,
                        }).await;
                        results.push((id.to_string(),
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
                    tool_name: name.to_string(),
                    tool_id: id.to_string(),
                    result: tool_result.content.clone(),
                    is_error: tool_result.is_error,
                }).await;

                results.push((id.to_string(), tool_result));
            }
        }

        results
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

    pub fn total_usage(&self) -> &UsageInfo { &self.total_usage }
    pub fn conversation_history(&self) -> &[Message] { &self.conversation_history }

    pub fn clear_history(&mut self) {
        self.conversation_history.clear();
        self.total_usage = UsageInfo::default();
        self.total_cost_usd = 0.0;
        self.using_fallback = false;
    }
}
