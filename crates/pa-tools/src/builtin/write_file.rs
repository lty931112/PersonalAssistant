//! 写入文件工具

use async_trait::async_trait;
use pa_core::{CoreError, ToolResult};
use serde_json::{json, Value};

use crate::registry::Tool;

/// 写入文件工具
pub struct WriteFileTool;

impl WriteFileTool {
    pub fn new() -> Self {
        WriteFileTool
    }
}

#[async_trait]
impl Tool for WriteFileTool {
    fn name(&self) -> &str {
        "write_file"
    }

    fn description(&self) -> &str {
        "将内容写入文件。如果文件已存在则覆盖。支持自动创建父目录。"
    }

    fn input_schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "path": { "type": "string", "description": "文件路径" },
                "content": { "type": "string", "description": "要写入的内容" },
                "create_dirs": { "type": "boolean", "description": "是否自动创建父目录", "default": true }
            },
            "required": ["path", "content"]
        })
    }

    fn is_read_only(&self) -> bool {
        false
    }

    fn is_concurrency_safe(&self) -> bool {
        false
    }

    async fn execute(&self, tool_use_id: &str, input: Value) -> Result<ToolResult, CoreError> {
        let path = input["path"].as_str().ok_or_else(|| CoreError::ToolExecutionError {
            tool_name: "write_file".to_string(),
            message: "缺少必需参数 'path'".to_string(),
        })?;

        let content = input["content"].as_str().ok_or_else(|| CoreError::ToolExecutionError {
            tool_name: "write_file".to_string(),
            message: "缺少必需参数 'content'".to_string(),
        })?;

        let create_dirs = input["create_dirs"].as_bool().unwrap_or(true);

        if create_dirs {
            if let Some(parent) = std::path::Path::new(path).parent() {
                tokio::fs::create_dir_all(parent).await.map_err(|e| CoreError::ToolExecutionError {
                    tool_name: "write_file".to_string(),
                    message: format!("无法创建目录: {}", e),
                })?;
            }
        }

        tokio::fs::write(path, content).await.map_err(|e| CoreError::ToolExecutionError {
            tool_name: "write_file".to_string(),
            message: format!("无法写入文件 '{}': {}", path, e),
        })?;

        Ok(ToolResult::success(tool_use_id, "write_file",
            format!("文件 '{}' 已写入（{} 字节，{} 行）", path, content.len(), content.lines().count())))
    }
}
