//! MCP 配置模块
//!
//! 定义 MCP Server 的配置结构，支持从 TOML 文件反序列化。

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// 传输层类型
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum TransportType {
    /// 标准输入输出传输（通过子进程）
    #[serde(rename = "stdio")]
    Stdio,
    /// HTTP 传输（通过 HTTP POST + SSE）
    #[serde(rename = "http")]
    Http,
}

impl std::fmt::Display for TransportType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            TransportType::Stdio => write!(f, "stdio"),
            TransportType::Http => write!(f, "http"),
        }
    }
}

/// 单个 MCP Server 的配置
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct McpServerConfig {
    /// 服务端名称（唯一标识）
    pub name: String,

    /// 传输层类型
    pub transport_type: TransportType,

    /// 启动命令（仅用于 stdio 传输）
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub command: Option<String>,

    /// 命令参数（仅用于 stdio 传输）
    #[serde(default)]
    pub args: Vec<String>,

    /// HTTP endpoint URL（仅用于 HTTP 传输）
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub url: Option<String>,

    /// HTTP 请求头（仅用于 HTTP 传输）
    #[serde(default)]
    pub headers: HashMap<String, String>,

    /// 环境变量
    #[serde(default)]
    pub env: HashMap<String, String>,

    /// 是否启用
    #[serde(default = "default_enabled")]
    pub enabled: bool,
}

/// 默认启用状态
fn default_enabled() -> bool {
    true
}

impl McpServerConfig {
    /// 创建 stdio 类型的 server 配置
    pub fn stdio(
        name: impl Into<String>,
        command: impl Into<String>,
    ) -> Self {
        Self {
            name: name.into(),
            transport_type: TransportType::Stdio,
            command: Some(command.into()),
            args: Vec::new(),
            url: None,
            headers: HashMap::new(),
            env: HashMap::new(),
            enabled: true,
        }
    }

    /// 创建 HTTP 类型的 server 配置
    pub fn http(
        name: impl Into<String>,
        url: impl Into<String>,
    ) -> Self {
        Self {
            name: name.into(),
            transport_type: TransportType::Http,
            command: None,
            args: Vec::new(),
            url: Some(url.into()),
            headers: HashMap::new(),
            env: HashMap::new(),
            enabled: true,
        }
    }

    /// 设置命令参数
    pub fn with_args(mut self, args: Vec<String>) -> Self {
        self.args = args;
        self
    }

    /// 设置环境变量
    pub fn with_env(mut self, key: impl Into<String>, value: impl Into<String>) -> Self {
        self.env.insert(key.into(), value.into());
        self
    }

    /// 设置 HTTP 请求头
    pub fn with_header(mut self, key: impl Into<String>, value: impl Into<String>) -> Self {
        self.headers.insert(key.into(), value.into());
        self
    }

    /// 设置是否启用
    pub fn with_enabled(mut self, enabled: bool) -> Self {
        self.enabled = enabled;
        self
    }

    /// 验证配置是否有效
    pub fn validate(&self) -> Result<(), String> {
        match self.transport_type {
            TransportType::Stdio => {
                if self.command.is_none() || self.command.as_ref().unwrap().is_empty() {
                    return Err(format!(
                        "stdio 类型的 MCP Server '{}' 必须指定 command",
                        self.name
                    ));
                }
            }
            TransportType::Http => {
                if self.url.is_none() || self.url.as_ref().unwrap().is_empty() {
                    return Err(format!(
                        "HTTP 类型的 MCP Server '{}' 必须指定 url",
                        self.name
                    ));
                }
            }
        }

        if self.name.is_empty() {
            return Err("MCP Server 名称不能为空".to_string());
        }

        Ok(())
    }
}

/// 整体 MCP 配置
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct McpConfig {
    /// MCP Server 配置列表
    #[serde(default)]
    pub servers: Vec<McpServerConfig>,
}

impl McpConfig {
    /// 创建空的 MCP 配置
    pub fn new() -> Self {
        Self {
            servers: Vec::new(),
        }
    }

    /// 添加 server 配置
    pub fn add_server(mut self, config: McpServerConfig) -> Self {
        self.servers.push(config);
        self
    }

    /// 从 TOML 字符串解析配置
    pub fn from_toml(toml_str: &str) -> Result<Self, String> {
        toml::from_str(toml_str).map_err(|e| {
            format!("解析 MCP 配置 TOML 失败: {}", e)
        })
    }

    /// 从 TOML 文件加载配置
    pub async fn from_toml_file(path: &str) -> Result<Self, String> {
        let content = tokio::fs::read_to_string(path)
            .await
            .map_err(|e| format!("读取 MCP 配置文件失败: {}", e))?;

        Self::from_toml(&content)
    }

    /// 序列化为 TOML 字符串
    pub fn to_toml(&self) -> Result<String, String> {
        toml::to_string_pretty(self).map_err(|e| {
            format!("序列化 MCP 配置为 TOML 失败: {}", e)
        })
    }

    /// 保存配置到 TOML 文件
    pub async fn save_to_file(&self, path: &str) -> Result<(), String> {
        let content = self.to_toml()?;
        tokio::fs::write(path, content)
            .await
            .map_err(|e| format!("保存 MCP 配置文件失败: {}", e))?;

        Ok(())
    }

    /// 验证所有 server 配置
    pub fn validate(&self) -> Result<(), String> {
        let mut names = std::collections::HashSet::new();

        for server in &self.servers {
            // 检查名称唯一性
            if !names.insert(&server.name) {
                return Err(format!(
                    "MCP Server 名称 '{}' 重复",
                    server.name
                ));
            }

            // 验证单个配置
            server.validate()?;
        }

        Ok(())
    }

    /// 获取启用的 server 配置
    pub fn enabled_servers(&self) -> Vec<&McpServerConfig> {
        self.servers.iter().filter(|s| s.enabled).collect()
    }

    /// 获取指定名称的 server 配置
    pub fn get_server(&self, name: &str) -> Option<&McpServerConfig> {
        self.servers.iter().find(|s| s.name == name)
    }

    /// 移除指定名称的 server 配置
    pub fn remove_server(&mut self, name: &str) -> Option<McpServerConfig> {
        if let Some(pos) = self.servers.iter().position(|s| s.name == name) {
            Some(self.servers.remove(pos))
        } else {
            None
        }
    }
}

impl Default for McpConfig {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_stdio_config() {
        let config = McpServerConfig::stdio("test-server", "node")
            .with_args(vec!["server.js".to_string()])
            .with_env("API_KEY", "test-key");

        assert_eq!(config.name, "test-server");
        assert_eq!(config.command.as_deref(), Some("node"));
        assert_eq!(config.args, vec!["server.js"]);
        assert_eq!(config.env.get("API_KEY").unwrap(), "test-key");
        assert!(config.validate().is_ok());
    }

    #[test]
    fn test_http_config() {
        let config = McpServerConfig::http("test-server", "http://localhost:8080/mcp")
            .with_header("Authorization", "Bearer token123");

        assert_eq!(config.name, "test-server");
        assert_eq!(config.url.as_deref(), Some("http://localhost:8080/mcp"));
        assert_eq!(config.headers.get("Authorization").unwrap(), "Bearer token123");
        assert!(config.validate().is_ok());
    }

    #[test]
    fn test_toml_roundtrip() {
        let config = McpConfig::new()
            .add_server(
                McpServerConfig::stdio("filesystem", "npx")
                    .with_args(vec!["-y".to_string(), "@anthropic/mcp-filesystem".to_string()])
            )
            .add_server(
                McpServerConfig::http("remote-api", "http://api.example.com/mcp")
                    .with_header("Authorization", "Bearer token")
            );

        let toml_str = config.to_toml().unwrap();
        let parsed = McpConfig::from_toml(&toml_str).unwrap();

        assert_eq!(parsed.servers.len(), 2);
        assert_eq!(parsed.servers[0].name, "filesystem");
        assert_eq!(parsed.servers[1].name, "remote-api");
    }

    #[test]
    fn test_validation() {
        // 缺少 command 的 stdio 配置
        let config = McpServerConfig {
            name: "test".to_string(),
            transport_type: TransportType::Stdio,
            command: None,
            args: Vec::new(),
            url: None,
            headers: HashMap::new(),
            env: HashMap::new(),
            enabled: true,
        };
        assert!(config.validate().is_err());

        // 缺少 url 的 http 配置
        let config = McpServerConfig {
            name: "test".to_string(),
            transport_type: TransportType::Http,
            command: None,
            args: Vec::new(),
            url: None,
            headers: HashMap::new(),
            env: HashMap::new(),
            enabled: true,
        };
        assert!(config.validate().is_err());
    }
}
