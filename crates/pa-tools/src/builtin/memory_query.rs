//! 记忆查询工具

use async_trait::async_trait;
use pa_core::{CoreError, ToolResult};
use pa_memory::{MemoryConfig, MagmaMemoryEngine};
use serde_json::{json, Value};
use std::sync::Arc;
use tracing;

use crate::registry::Tool;

/// 记忆查询工具
pub struct MemoryQueryTool {
    memory: Arc<tokio::sync::RwLock<MagmaMemoryEngine>>,
}

impl MemoryQueryTool {
    pub fn new() -> Self {
        let config = pa_memory::MemoryConfig::default();
        let engine = MagmaMemoryEngine::new(&config).expect("记忆引擎创建失败");
        MemoryQueryTool {
            memory: Arc::new(tokio::sync::RwLock::new(engine)),
        }
    }

    pub fn with_engine(engine: Arc<tokio::sync::RwLock<MagmaMemoryEngine>>) -> Self {
        MemoryQueryTool { memory: engine }
    }

    pub fn memory(&self) -> &Arc<tokio::sync::RwLock<MagmaMemoryEngine>> {
        &self.memory
    }
}

impl Default for MemoryQueryTool {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl Tool for MemoryQueryTool {
    fn name(&self) -> &str {
        "memory_query"
    }

    fn description(&self) -> &str {
        "从长期记忆中检索相关信息。返回最相关的记忆节点。"
    }

    fn input_schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "query": { "type": "string", "description": "查询文本" },
                "max_results": { "type": "integer", "description": "最大返回数", "default": 5 }
            },
            "required": ["query"]
        })
    }

    fn is_read_only(&self) -> bool {
        true
    }

    async fn execute(&self, tool_use_id: &str, input: Value) -> Result<ToolResult, CoreError> {
        let query = input["query"].as_str().ok_or_else(|| CoreError::ToolExecutionError {
            tool_name: "memory_query".to_string(),
            message: "缺少必需参数 'query'".to_string(),
        })?;

        tracing::info!(query = %query, "查询记忆");

        let mut engine = self.memory.write().await;
        let result = engine.retrieve(query, None).await.map_err(|e| CoreError::ToolExecutionError {
            tool_name: "memory_query".to_string(),
            message: format!("记忆查询失败: {}", e),
        })?;

        if result.nodes.is_empty() {
            return Ok(ToolResult::success(tool_use_id, "memory_query",
                format!("未找到与 '{}' 相关的记忆", query)));
        }

        let mut parts = Vec::new();
        for (i, node) in result.nodes.iter().enumerate() {
            parts.push(format!("[{}] {} (类型: {})", i + 1, node.content, node.node_type));
        }

        Ok(ToolResult::success(tool_use_id, "memory_query",
            format!("查询: '{}'\n找到 {} 条记忆 (置信度: {:.3}):\n\n{}",
                query, result.nodes.len(), result.confidence, parts.join("\n\n"))))
    }
}
