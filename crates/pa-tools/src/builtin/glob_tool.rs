//! Glob 工具 - 文件模式匹配

use async_trait::async_trait;
use pa_core::{CoreError, ToolResult};
use serde_json::{json, Value};

use crate::registry::Tool;

/// Glob 工具
pub struct GlobTool;

impl GlobTool {
    pub fn new() -> Self {
        GlobTool
    }
}

#[async_trait]
impl Tool for GlobTool {
    fn name(&self) -> &str {
        "glob"
    }

    fn description(&self) -> &str {
        "使用 glob 模式匹配文件路径。返回匹配的文件列表。"
    }

    fn input_schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "pattern": { "type": "string", "description": "Glob 模式" },
                "path": { "type": "string", "description": "根目录" },
                "max_results": { "type": "integer", "description": "最大返回数", "default": 100 }
            },
            "required": ["pattern"]
        })
    }

    fn is_read_only(&self) -> bool {
        true
    }

    async fn execute(&self, tool_use_id: &str, input: Value) -> Result<ToolResult, CoreError> {
        let pattern = input["pattern"].as_str().ok_or_else(|| CoreError::ToolExecutionError {
            tool_name: "glob".to_string(),
            message: "缺少必需参数 'pattern'".to_string(),
        })?;

        let base_path = input["path"].as_str().unwrap_or(".");
        let max_results = input["max_results"].as_u64().unwrap_or(100) as usize;

        let full_pattern = if pattern.starts_with('/') {
            pattern.to_string()
        } else {
            format!("{}/{}", base_path, pattern)
        };

        let mut matches: Vec<String> = glob::glob(&full_pattern)
            .map_err(|e| CoreError::ToolExecutionError {
                tool_name: "glob".to_string(),
                message: format!("Glob 模式无效: {}", e),
            })?
            .filter_map(|entry| {
                entry.ok().and_then(|p| if p.is_file() { Some(p.to_string_lossy().to_string()) } else { None })
            })
            .take(max_results)
            .collect();

        matches.sort();

        if matches.is_empty() {
            return Ok(ToolResult::success(tool_use_id, "glob", format!("未找到匹配 '{}' 的文件", pattern)));
        }

        Ok(ToolResult::success(tool_use_id, "glob",
            format!("模式: '{}'\n匹配: {}\n\n{}", pattern, matches.len(), matches.join("\n"))))
    }
}
