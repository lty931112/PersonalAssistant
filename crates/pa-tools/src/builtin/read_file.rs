//! 读取文件工具

use async_trait::async_trait;
use pa_core::{CoreError, ToolResult};
use serde_json::{json, Value};

use crate::registry::Tool;

/// 读取文件工具
pub struct ReadFileTool;

impl ReadFileTool {
    pub fn new() -> Self {
        ReadFileTool
    }
}

#[async_trait]
impl Tool for ReadFileTool {
    fn name(&self) -> &str {
        "read_file"
    }

    fn description(&self) -> &str {
        "读取文件内容。支持按行号范围读取。"
    }

    fn input_schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "path": { "type": "string", "description": "文件路径" },
                "offset": { "type": "integer", "description": "起始行号（从1开始）", "default": 1 },
                "limit": { "type": "integer", "description": "最大行数", "default": 2000 }
            },
            "required": ["path"]
        })
    }

    fn is_read_only(&self) -> bool {
        true
    }

    async fn execute(&self, tool_use_id: &str, input: Value) -> Result<ToolResult, CoreError> {
        let path = input["path"].as_str().ok_or_else(|| CoreError::ToolExecutionError {
            tool_name: "read_file".to_string(),
            message: "缺少必需参数 'path'".to_string(),
        })?;

        let offset = input["offset"].as_u64().unwrap_or(1) as usize;
        let limit = input["limit"].as_u64().unwrap_or(2000) as usize;

        let content = tokio::fs::read_to_string(path).await.map_err(|e| CoreError::ToolExecutionError {
            tool_name: "read_file".to_string(),
            message: format!("无法读取文件 '{}': {}", path, e),
        })?;

        let lines: Vec<&str> = content.lines().collect();
        let total = lines.len();
        let start = (offset - 1).min(total);
        let end = (start + limit).min(total);

        if start >= total {
            return Ok(ToolResult::success(tool_use_id, "read_file",
                format!("文件 '{}' 共 {} 行，起始行号 {} 超出范围", path, total, offset)));
        }

        let mut output = Vec::new();
        for (i, line) in lines[start..end].iter().enumerate() {
            output.push(format!("{:>6}\t{}", start + i + 1, line));
        }

        Ok(ToolResult::success(tool_use_id, "read_file",
            format!("文件: {}\n总行数: {}\n显示: {}-{}\n\n{}", path, total, start + 1, end, output.join("\n"))))
    }
}
