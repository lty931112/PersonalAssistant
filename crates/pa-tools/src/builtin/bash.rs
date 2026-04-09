//! Bash 工具 - 执行 Shell 命令

use async_trait::async_trait;
use pa_core::{CoreError, ToolResult};
use serde_json::{json, Value};
use tracing;

use crate::registry::Tool;

/// Bash 工具 - 执行 Shell 命令并返回输出
pub struct BashTool;

impl BashTool {
    pub fn new() -> Self {
        BashTool
    }
}

#[async_trait]
impl Tool for BashTool {
    fn name(&self) -> &str {
        "bash"
    }

    fn description(&self) -> &str {
        "执行 Shell 命令。支持超时控制和工作目录设置。"
    }

    fn input_schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "command": {
                    "type": "string",
                    "description": "要执行的 Shell 命令"
                },
                "timeout": {
                    "type": "integer",
                    "description": "超时时间（毫秒），默认 120000",
                    "default": 120000
                },
                "working_dir": {
                    "type": "string",
                    "description": "工作目录路径"
                }
            },
            "required": ["command"]
        })
    }

    fn is_read_only(&self) -> bool {
        false
    }

    fn is_concurrency_safe(&self) -> bool {
        false
    }

    async fn execute(&self, tool_use_id: &str, input: Value) -> Result<ToolResult, CoreError> {
        let command = input["command"].as_str().ok_or_else(|| CoreError::ToolExecutionError {
            tool_name: "bash".to_string(),
            message: "缺少必需参数 'command'".to_string(),
        })?;

        let timeout_ms = input["timeout"].as_u64().unwrap_or(120_000);
        let working_dir = input["working_dir"].as_str();

        tracing::info!(command = %command, timeout_ms, "执行 Bash 命令");

        let mut cmd = tokio::process::Command::new("bash");
        cmd.arg("-c").arg(command);
        if let Some(dir) = working_dir {
            cmd.current_dir(dir);
        }

        let timeout = std::time::Duration::from_millis(timeout_ms);

        match tokio::time::timeout(timeout, cmd.output()).await {
            Ok(Ok(output)) => {
                let stdout = String::from_utf8_lossy(&output.stdout);
                let stderr = String::from_utf8_lossy(&output.stderr);
                let exit_code = output.status.code().unwrap_or(-1);

                let mut parts = Vec::new();
                if !stdout.is_empty() {
                    parts.push(format!("stdout:\n{}", stdout));
                }
                if !stderr.is_empty() {
                    parts.push(format!("stderr:\n{}", stderr));
                }
                parts.push(format!("退出码: {}", exit_code));

                let content = parts.join("\n\n");
                let is_error = exit_code != 0;

                Ok(ToolResult::success(tool_use_id, "bash", content))
                    .map(|mut r| { r.is_error = is_error; r })
            }
            Ok(Err(e)) => Ok(ToolResult::error(tool_use_id, "bash", format!("命令执行失败: {}", e))),
            Err(_) => Ok(ToolResult::error(tool_use_id, "bash", format!("命令执行超时（{}ms）", timeout_ms))),
        }
    }
}
