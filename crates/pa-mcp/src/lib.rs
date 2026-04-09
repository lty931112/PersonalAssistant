//! pa-mcp - MCP Host 实现
//!
//! 本 crate 实现了完整的 MCP (Model Context Protocol) Host 功能，
//! 支持通过 stdio 和 HTTP 传输层连接到多个 MCP Server，
//! 并将 MCP 工具桥接到 pa-tools 工具系统。
//!
//! # 主要模块
//!
//! - [`types`]: MCP 协议核心类型定义（JSON-RPC 2.0 消息、能力声明等）
//! - [`transport`]: 传输层实现（StdioTransport、HttpTransport）
//! - [`client`]: MCP 客户端（管理与单个 MCP Server 的连接）
//! - [`host`]: MCP Host（管理多个 MCP Server 连接）
//! - [`bridge`]: MCP 到 pa-tools 的桥接（将 MCP 工具适配为 Tool trait）
//! - [`config`]: MCP 配置（支持 TOML 反序列化）
//!
//! # 使用示例
//!
//! ```rust,ignore
//! use pa_mcp::{McpHost, McpConfig};
//! use std::sync::Arc;
//!
//! #[tokio::main]
//! async fn main() {
//!     // 创建 Host
//!     let host = Arc::new(McpHost::new());
//!
//!     // 添加 stdio server
//!     host.add_stdio_server("filesystem", "npx", vec![
//!         "-y".to_string(),
//!         "@anthropic/mcp-filesystem".to_string(),
//!     ]).await;
//!
//!     // 连接所有 server
//!     host.connect_all().await.unwrap();
//!
//!     // 列出所有工具
//!     let tools = host.list_all_tools().await.unwrap();
//!     for tool in &tools {
//!         println!("工具: {} - {:?}", tool.name, tool.description);
//!     }
//! }
//! ```

// 模块导出
pub mod types;
pub mod transport;
pub mod client;
pub mod host;
pub mod bridge;
pub mod config;

// 重新导出常用类型 - 协议类型
pub use types::{
    ClientCapabilities, ImplementationInfo, InitializeParams, InitializeResult,
    JsonRpcMessage, LoggingLevel, LoggingMessageParams, McpError, McpNotification,
    McpRequest, McpResponse, McpToolDefinition, McpVersion, ProgressToken,
    PromptArgument, PromptDefinition, PromptGetParams, PromptGetResult,
    PromptMessage, PromptMessageContent, PromptsCapability, PromptsListResult,
    RequestId, ResourceContent, ResourceDefinition, ResourceReadParams,
    ResourceReadResult, ResourcesCapability, ResourcesListResult,
    ServerCapabilities, ToolAnnotations, ToolCallContent, ToolCallParams,
    ToolCallResult, ToolsCapability, ToolsListResult,
};

// 重新导出常用类型 - 传输层
pub use transport::{HttpTransport, McpTransport, StdioTransport};

// 重新导出常用类型 - 客户端
pub use client::{ClientState, McpClient};

// 重新导出常用类型 - Host
pub use host::{ConnectionStatus, McpHost};

// 重新导出常用类型 - 桥接
pub use bridge::{McpToolAdapter, McpToolBridge};

// 重新导出常用类型 - 配置
pub use config::{McpConfig, McpServerConfig, TransportType};
