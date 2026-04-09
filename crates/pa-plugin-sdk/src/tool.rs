//! 工具插件 trait

use async_trait::async_trait;
use pa_core::{CoreError, ToolDefinition, ToolResult};
use crate::plugin::Plugin;

/// 工具插件 trait
#[async_trait]
pub trait ToolPlugin: Plugin {
    /// 获取工具定义
    fn definition(&self) -> ToolDefinition;

    /// 执行工具
    async fn execute(&self, input: serde_json::Value, tool_use_id: &str) -> Result<ToolResult, CoreError>;
}
