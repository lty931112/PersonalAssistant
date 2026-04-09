//! 网页获取工具

use async_trait::async_trait;
use pa_core::{CoreError, ToolResult};
use serde_json::{json, Value};
use tracing;

use crate::registry::Tool;

/// 网页获取工具
pub struct WebFetchTool {
    http_client: reqwest::Client,
}

impl WebFetchTool {
    pub fn new() -> Self {
        let http_client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(30))
            .user_agent("PersonalAssistant/0.1")
            .build()
            .expect("HTTP 客户端创建失败");
        WebFetchTool { http_client }
    }
}

impl Default for WebFetchTool {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl Tool for WebFetchTool {
    fn name(&self) -> &str {
        "web_fetch"
    }

    fn description(&self) -> &str {
        "获取网页内容，返回纯文本。"
    }

    fn input_schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "url": { "type": "string", "description": "网页 URL" },
                "max_length": { "type": "integer", "description": "最大字符数", "default": 10000 }
            },
            "required": ["url"]
        })
    }

    fn is_read_only(&self) -> bool {
        true
    }

    async fn execute(&self, tool_use_id: &str, input: Value) -> Result<ToolResult, CoreError> {
        let url = input["url"].as_str().ok_or_else(|| CoreError::ToolExecutionError {
            tool_name: "web_fetch".to_string(),
            message: "缺少必需参数 'url'".to_string(),
        })?;

        let max_length = input["max_length"].as_u64().unwrap_or(10_000) as usize;

        let parsed: url::Url = url.parse().map_err(|e| CoreError::ToolExecutionError {
            tool_name: "web_fetch".to_string(),
            message: format!("URL 格式无效: {}", e),
        })?;

        match parsed.scheme() {
            "http" | "https" => {}
            other => {
                return Err(CoreError::ToolExecutionError {
                    tool_name: "web_fetch".to_string(),
                    message: format!("不支持的协议: {}", other),
                });
            }
        }

        tracing::info!(url = %url, "获取网页");

        let response = self.http_client.get(url).send().await.map_err(|e| CoreError::ToolExecutionError {
            tool_name: "web_fetch".to_string(),
            message: format!("HTTP 请求失败: {}", e),
        })?;

        let status = response.status();
        if !status.is_success() {
            return Ok(ToolResult::error(tool_use_id, "web_fetch", format!("HTTP 错误: {}", status)));
        }

        let body = response.text().await.map_err(|e| CoreError::ToolExecutionError {
            tool_name: "web_fetch".to_string(),
            message: format!("读取响应失败: {}", e),
        })?;

        // 简单去除 HTML 标签
        let text = Self::strip_html(&body);
        let text_len = text.len();
        let truncated = if text_len > max_length {
            format!("{}\n\n... [截断，共 {} 字符]", &text[..max_length], text_len)
        } else {
            text
        };

        Ok(ToolResult::success(tool_use_id, "web_fetch",
            format!("URL: {}\n状态: {}\n长度: {}\n\n{}", url, status, text_len, truncated)))
    }
}

impl WebFetchTool {
    /// 简单的 HTML 标签去除
    fn strip_html(html: &str) -> String {
        let mut result = String::with_capacity(html.len());
        let mut in_tag = false;

        for ch in html.chars() {
            match ch {
                '<' => in_tag = true,
                '>' => in_tag = false,
                _ if !in_tag => result.push(ch),
                _ => {}
            }
        }

        // 解码常见 HTML 实体
        result = result.replace("&amp;", "&")
            .replace("&lt;", "<")
            .replace("&gt;", ">")
            .replace("&quot;", "\"")
            .replace("&#39;", "'")
            .replace("&nbsp;", " ");

        // 压缩空白
        let lines: Vec<&str> = result.lines()
            .map(|l| l.trim())
            .filter(|l| !l.is_empty())
            .collect();

        lines.join("\n")
    }
}
