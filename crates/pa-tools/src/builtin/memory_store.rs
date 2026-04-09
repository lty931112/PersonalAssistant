//! 记忆存储工具

use async_trait::async_trait;
use pa_core::{CoreError, ToolResult};
use pa_memory::{MemoryNodeType, MagmaMemoryEngine};
use serde_json::{json, Value};
use std::sync::Arc;
use tracing;

use crate::registry::Tool;

/// 记忆存储工具
pub struct MemoryStoreTool {
    memory: Arc<tokio::sync::RwLock<MagmaMemoryEngine>>,
}

impl MemoryStoreTool {
    pub fn new() -> Self {
        let config = pa_memory::MemoryConfig::default();
        let engine = MagmaMemoryEngine::new(&config).expect("记忆引擎创建失败");
        MemoryStoreTool {
            memory: Arc::new(tokio::sync::RwLock::new(engine)),
        }
    }

    pub fn with_engine(engine: Arc<tokio::sync::RwLock<MagmaMemoryEngine>>) -> Self {
        MemoryStoreTool { memory: engine }
    }

    pub fn memory(&self) -> &Arc<tokio::sync::RwLock<MagmaMemoryEngine>> {
        &self.memory
    }
}

impl Default for MemoryStoreTool {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl Tool for MemoryStoreTool {
    fn name(&self) -> &str {
        "memory_store"
    }

    fn description(&self) -> &str {
        "存储信息到长期记忆中。支持设置记忆类型和标签。"
    }

    fn input_schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "content": { "type": "string", "description": "要存储的内容" },
                "node_type": { "type": "string", "description": "类型: observation/action/state_change/inferred", "enum": ["observation", "action", "state_change", "inferred"], "default": "observation" },
                "tags": { "type": "array", "items": { "type": "string" }, "description": "标签列表" }
            },
            "required": ["content"]
        })
    }

    fn is_read_only(&self) -> bool {
        false
    }

    fn is_concurrency_safe(&self) -> bool {
        true
    }

    async fn execute(&self, tool_use_id: &str, input: Value) -> Result<ToolResult, CoreError> {
        let content = input["content"].as_str().ok_or_else(|| CoreError::ToolExecutionError {
            tool_name: "memory_store".to_string(),
            message: "缺少必需参数 'content'".to_string(),
        })?;

        let node_type_str = input["node_type"].as_str().unwrap_or("observation");
        let node_type = match node_type_str {
            "action" => MemoryNodeType::Action,
            "state_change" => MemoryNodeType::StateChange,
            "inferred" => MemoryNodeType::Inferred,
            _ => MemoryNodeType::Observation,
        };

        let tags: Vec<String> = input["tags"]
            .as_array()
            .map(|arr| arr.iter().filter_map(|v| v.as_str().map(|s| s.to_string())).collect())
            .unwrap_or_default();

        tracing::info!(content_len = content.len(), node_type = %node_type_str, "存储记忆");

        let mut engine = self.memory.write().await;
        engine.ingest_fast(content, node_type).await.map_err(|e| CoreError::ToolExecutionError {
            tool_name: "memory_store".to_string(),
            message: format!("记忆存储失败: {}", e),
        })?;

        Ok(ToolResult::success(tool_use_id, "memory_store", "记忆已成功存储"))
    }
}
