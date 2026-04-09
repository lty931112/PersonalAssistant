//! 工具注册表模块
//!
//! 提供工具的注册、查找和执行功能。

use async_trait::async_trait;
use pa_core::{CoreError, ToolDefinition, ToolResult};
use std::collections::HashMap;

/// 工具执行 trait
///
/// 所有工具都需要实现此 trait。
#[async_trait]
pub trait Tool: Send + Sync {
    /// 工具名称（唯一标识符）
    fn name(&self) -> &str;

    /// 工具描述（供 LLM 理解工具用途）
    fn description(&self) -> &str;

    /// 输入参数的 JSON Schema
    fn input_schema(&self) -> serde_json::Value;

    /// 是否为只读工具
    fn is_read_only(&self) -> bool {
        false
    }

    /// 是否可并发安全执行
    fn is_concurrency_safe(&self) -> bool {
        self.is_read_only()
    }

    /// 执行工具
    async fn execute(
        &self,
        tool_use_id: &str,
        input: serde_json::Value,
    ) -> Result<ToolResult, CoreError>;

    /// 获取工具定义
    fn definition(&self) -> ToolDefinition {
        ToolDefinition {
            name: self.name().to_string(),
            description: self.description().to_string(),
            input_schema: self.input_schema(),
            is_read_only: self.is_read_only(),
            is_concurrency_safe: self.is_concurrency_safe(),
        }
    }
}

/// 工具注册表
pub struct ToolRegistry {
    tools: HashMap<String, Box<dyn Tool>>,
}

impl ToolRegistry {
    pub fn new() -> Self {
        ToolRegistry {
            tools: HashMap::new(),
        }
    }

    /// 创建包含所有内置工具的注册表
    pub fn with_defaults() -> Self {
        let mut registry = Self::new();
        registry.register_default_tools();
        registry
    }

    /// 注册默认内置工具
    pub fn register_default_tools(&mut self) {
        use crate::builtin::*;

        self.register(Box::new(BashTool::new()));
        self.register(Box::new(ReadFileTool::new()));
        self.register(Box::new(WriteFileTool::new()));
        self.register(Box::new(SearchTool::new()));
        self.register(Box::new(GlobTool::new()));
        self.register(Box::new(MemoryStoreTool::new()));
        self.register(Box::new(MemoryQueryTool::new()));
        self.register(Box::new(WebFetchTool::new()));

        tracing::info!("已注册 {} 个内置工具", self.tools.len());
    }

    /// 注册工具
    pub fn register(&mut self, tool: Box<dyn Tool>) {
        let name = tool.name().to_string();
        tracing::debug!(tool_name = %name, "注册工具");
        self.tools.insert(name, tool);
    }

    /// 获取工具（不可变引用）
    pub fn get(&self, name: &str) -> Option<&dyn Tool> {
        self.tools.get(name).map(|t| t.as_ref())
    }

    /// 列出所有工具定义
    pub fn list_definitions(&self) -> Vec<ToolDefinition> {
        self.tools.values().map(|t| t.definition()).collect()
    }

    /// 执行工具
    pub async fn execute(
        &self,
        name: &str,
        tool_use_id: &str,
        input: serde_json::Value,
    ) -> Result<ToolResult, CoreError> {
        let tool = self.tools.get(name).ok_or_else(|| {
            CoreError::ToolExecutionError {
                tool_name: name.to_string(),
                message: format!("工具 '{}' 未注册", name),
            }
        })?;

        tracing::info!(tool_name = %name, "执行工具");
        let result = tool.execute(tool_use_id, input).await.map_err(|e| {
            CoreError::ToolExecutionError {
                tool_name: name.to_string(),
                message: e.to_string(),
            }
        })?;

        tracing::debug!(
            tool_name = %name,
            is_error = result.is_error,
            "工具执行完成"
        );

        Ok(result)
    }

    /// 检查工具是否存在
    pub fn contains(&self, name: &str) -> bool {
        self.tools.contains_key(name)
    }

    pub fn len(&self) -> usize {
        self.tools.len()
    }

    pub fn is_empty(&self) -> bool {
        self.tools.is_empty()
    }
}

impl Default for ToolRegistry {
    fn default() -> Self {
        Self::new()
    }
}
