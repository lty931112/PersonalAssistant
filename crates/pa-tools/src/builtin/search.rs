//! 搜索工具 - 搜索文件内容

use async_trait::async_trait;
use pa_core::{CoreError, ToolResult};
use regex::Regex;
use serde_json::{json, Value};
use std::path::{Path, PathBuf};
use std::collections::VecDeque;

use crate::registry::Tool;

/// 搜索工具
pub struct SearchTool;

impl SearchTool {
    pub fn new() -> Self {
        SearchTool
    }
}

#[async_trait]
impl Tool for SearchTool {
    fn name(&self) -> &str {
        "search"
    }

    fn description(&self) -> &str {
        "在文件中搜索内容。使用正则表达式匹配。支持按文件类型过滤。"
    }

    fn input_schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "pattern": { "type": "string", "description": "正则表达式模式" },
                "path": { "type": "string", "description": "搜索目录，默认当前目录" },
                "include": { "type": "array", "items": { "type": "string" }, "description": "文件类型过滤" },
                "max_results": { "type": "integer", "description": "最大结果数", "default": 50 }
            },
            "required": ["pattern"]
        })
    }

    fn is_read_only(&self) -> bool {
        true
    }

    async fn execute(&self, tool_use_id: &str, input: Value) -> Result<ToolResult, CoreError> {
        let pattern = input["pattern"].as_str().ok_or_else(|| CoreError::ToolExecutionError {
            tool_name: "search".to_string(),
            message: "缺少必需参数 'pattern'".to_string(),
        })?;

        let search_path = input["path"].as_str().unwrap_or(".");
        let max_results = input["max_results"].as_u64().unwrap_or(50) as usize;

        let regex = Regex::new(pattern).map_err(|e| CoreError::ToolExecutionError {
            tool_name: "search".to_string(),
            message: format!("正则表达式无效: {}", e),
        })?;

        let mut results = Vec::new();
        let mut dirs_to_visit: VecDeque<PathBuf> = VecDeque::new();
        dirs_to_visit.push_back(PathBuf::from(search_path));

        while let Some(dir) = dirs_to_visit.pop_front() {
            if results.len() >= max_results {
                break;
            }

            let entries = match tokio::fs::read_dir(&dir).await {
                Ok(e) => e,
                Err(_) => continue,
            };

            let mut entries_list: Vec<_> = Vec::new();
            let mut read_dir = entries;
            while let Ok(Some(entry)) = read_dir.next_entry().await {
                entries_list.push(entry);
            }

            entries_list.sort_by_key(|e| e.file_name());

            for entry in entries_list {
                if results.len() >= max_results {
                    break;
                }

                let path = entry.path();
                let file_type = match entry.file_type().await {
                    Ok(ft) => ft,
                    Err(_) => continue,
                };

                if file_type.is_dir() {
                    let dir_name = path.file_name().and_then(|n| n.to_str()).unwrap_or("");
                    if dir_name.starts_with('.') || matches!(dir_name, "node_modules" | "target" | ".git") {
                        continue;
                    }
                    dirs_to_visit.push_back(path);
                } else if file_type.is_file() {
                    if let Ok(content) = tokio::fs::read_to_string(&path).await {
                        for (line_num, line) in content.lines().enumerate() {
                            if regex.is_match(line) {
                                results.push(format!("{}:{}:\t{}", path.display(), line_num + 1, line.trim()));
                                if results.len() >= max_results {
                                    break;
                                }
                            }
                        }
                    }
                }
            }
        }

        if results.is_empty() {
            return Ok(ToolResult::success(tool_use_id, "search",
                format!("在 '{}' 中未找到匹配 '{}' 的结果", search_path, pattern)));
        }

        Ok(ToolResult::success(tool_use_id, "search",
            format!("模式: '{}'\n路径: {}\n匹配: {}\n\n{}", pattern, search_path, results.len(), results.join("\n"))))
    }
}
