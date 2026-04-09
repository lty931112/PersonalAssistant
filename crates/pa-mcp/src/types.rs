//! MCP 协议核心类型定义
//!
//! 定义了 MCP (Model Context Protocol) 的所有核心数据类型，
//! 包括 JSON-RPC 2.0 消息、能力声明、工具/资源/提示词定义等。

use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::HashMap;

/// MCP 协议版本
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct McpVersion(pub String);

impl McpVersion {
    /// 当前支持的 MCP 协议版本
    pub const CURRENT: &'static str = "2025-03-26";

    /// 创建协议版本
    pub fn new(version: &str) -> Self {
        McpVersion(version.to_string())
    }

    /// 获取版本字符串
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl Default for McpVersion {
    fn default() -> Self {
        McpVersion::new(Self::CURRENT)
    }
}

impl std::fmt::Display for McpVersion {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

// ============================================================
// JSON-RPC 2.0 消息类型
// ============================================================

/// JSON-RPC 2.0 消息
///
/// MCP 协议基于 JSON-RPC 2.0，支持三种消息类型：
/// - 请求（Request）：需要响应
/// - 响应（Response）：对请求的回复
/// - 通知（Notification）：不需要响应
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum JsonRpcMessage {
    /// JSON-RPC 请求
    Request(McpRequest),
    /// JSON-RPC 响应
    Response(McpResponse),
    /// JSON-RPC 通知
    Notification(McpNotification),
}

/// JSON-RPC 请求 ID
///
/// 可以是数字、字符串或 null
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(untagged)]
pub enum RequestId {
    /// 数字 ID
    Number(i64),
    /// 字符串 ID
    String(String),
    /// 空 ID（仅用于通知）
    Null,
}

impl std::fmt::Display for RequestId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            RequestId::Number(n) => write!(f, "{}", n),
            RequestId::String(s) => write!(f, "{}", s),
            RequestId::Null => write!(f, "null"),
        }
    }
}

/// MCP 请求
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct McpRequest {
    /// JSON-RPC 版本，固定为 "2.0"
    pub jsonrpc: String,
    /// 请求 ID
    pub id: RequestId,
    /// 方法名
    pub method: String,
    /// 参数
    #[serde(skip_serializing_if = "Option::is_none")]
    pub params: Option<Value>,
}

impl McpRequest {
    /// 创建新的 MCP 请求
    pub fn new(id: RequestId, method: impl Into<String>, params: Option<Value>) -> Self {
        Self {
            jsonrpc: "2.0".to_string(),
            id,
            method: method.into(),
            params,
        }
    }
}

/// MCP 响应
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct McpResponse {
    /// JSON-RPC 版本，固定为 "2.0"
    pub jsonrpc: String,
    /// 对应的请求 ID
    pub id: RequestId,
    /// 成功时的结果
    #[serde(skip_serializing_if = "Option::is_none")]
    pub result: Option<Value>,
    /// 失败时的错误
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<McpError>,
}

impl McpResponse {
    /// 创建成功响应
    pub fn success(id: RequestId, result: Value) -> Self {
        Self {
            jsonrpc: "2.0".to_string(),
            id,
            result: Some(result),
            error: None,
        }
    }

    /// 创建错误响应
    pub fn error(id: RequestId, error: McpError) -> Self {
        Self {
            jsonrpc: "2.0".to_string(),
            id,
            result: None,
            error: Some(error),
        }
    }
}

/// MCP 通知
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct McpNotification {
    /// JSON-RPC 版本，固定为 "2.0"
    pub jsonrpc: String,
    /// 方法名
    pub method: String,
    /// 参数
    #[serde(skip_serializing_if = "Option::is_none")]
    pub params: Option<Value>,
}

impl McpNotification {
    /// 创建新的 MCP 通知
    pub fn new(method: impl Into<String>, params: Option<Value>) -> Self {
        Self {
            jsonrpc: "2.0".to_string(),
            method: method.into(),
            params,
        }
    }
}

/// MCP 错误
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct McpError {
    /// 错误码
    pub code: i64,
    /// 错误消息
    pub message: String,
    /// 附加数据
    #[serde(skip_serializing_if = "Option::is_none")]
    pub data: Option<Value>,
}

impl McpError {
    /// 创建新的 MCP 错误
    pub fn new(code: i64, message: impl Into<String>) -> Self {
        Self {
            code,
            message: message.into(),
            data: None,
        }
    }

    /// 设置附加数据
    pub fn with_data(mut self, data: Value) -> Self {
        self.data = Some(data);
        self
    }

    // 标准错误码
    /// 解析错误
    pub const PARSE_ERROR: i64 = -32700;
    /// 无效请求
    pub const INVALID_REQUEST: i64 = -32600;
    /// 方法未找到
    pub const METHOD_NOT_FOUND: i64 = -32601;
    /// 无效参数
    pub const INVALID_PARAMS: i64 = -32602;
    /// 内部错误
    pub const INTERNAL_ERROR: i64 = -32603;
}

impl std::fmt::Display for McpError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "MCP 错误 ({}): {}", self.code, self.message)
    }
}

impl std::error::Error for McpError {}

// ============================================================
// 能力声明
// ============================================================

/// 服务端能力声明
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ServerCapabilities {
    /// 是否支持工具
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tools: Option<ToolsCapability>,
    /// 是否支持资源
    #[serde(skip_serializing_if = "Option::is_none")]
    pub resources: Option<ResourcesCapability>,
    /// 是否支持提示词
    #[serde(skip_serializing_if = "Option::is_none")]
    pub prompts: Option<PromptsCapability>,
    /// 是否支持日志
    #[serde(skip_serializing_if = "Option::is_none")]
    pub logging: Option<Value>,
}

/// 工具能力
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ToolsCapability {
    /// 是否支持 list_changed 通知
    #[serde(skip_serializing_if = "Option::is_none")]
    pub list_changed: Option<bool>,
}

/// 资源能力
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ResourcesCapability {
    /// 是否支持订阅
    #[serde(skip_serializing_if = "Option::is_none")]
    pub subscribe: Option<bool>,
    /// 是否支持 list_changed 通知
    #[serde(skip_serializing_if = "Option::is_none")]
    pub list_changed: Option<bool>,
}

/// 提示词能力
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PromptsCapability {
    /// 是否支持 list_changed 通知
    #[serde(skip_serializing_if = "Option::is_none")]
    pub list_changed: Option<bool>,
}

/// 客户端能力声明
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ClientCapabilities {
    /// 根能力
    #[serde(skip_serializing_if = "Option::is_none")]
    pub roots: Option<RootsCapability>,
    /// 采样能力
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sampling: Option<Value>,
}

/// 根能力
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RootsCapability {
    /// 是否支持 list_changed 通知
    #[serde(skip_serializing_if = "Option::is_none")]
    pub list_changed: Option<bool>,
}

// ============================================================
// 初始化相关
// ============================================================

/// 初始化参数（客户端发送）
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct InitializeParams {
    /// 协议版本
    pub protocol_version: String,
    /// 客户端能力
    pub capabilities: ClientCapabilities,
    /// 客户端信息
    pub client_info: ImplementationInfo,
}

impl InitializeParams {
    /// 创建默认的初始化参数
    pub fn new() -> Self {
        Self {
            protocol_version: McpVersion::CURRENT.to_string(),
            capabilities: ClientCapabilities::default(),
            client_info: ImplementationInfo {
                name: "pa-mcp".to_string(),
                version: env!("CARGO_PKG_VERSION").to_string(),
            },
        }
    }
}

impl Default for InitializeParams {
    fn default() -> Self {
        Self::new()
    }
}

/// 初始化结果（服务端返回）
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct InitializeResult {
    /// 协议版本
    pub protocol_version: String,
    /// 服务端能力
    pub capabilities: ServerCapabilities,
    /// 服务端信息
    pub server_info: ImplementationInfo,
    /// 服务端说明
    #[serde(skip_serializing_if = "Option::is_none")]
    pub instructions: Option<String>,
}

/// 实现信息
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ImplementationInfo {
    /// 名称
    pub name: String,
    /// 版本
    pub version: String,
}

// ============================================================
// 进度跟踪
// ============================================================

/// 进度令牌
///
/// 用于跟踪长时间运行操作的进度
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(untagged)]
pub enum ProgressToken {
    /// 数字令牌
    Number(i64),
    /// 字符串令牌
    String(String),
}

// ============================================================
// 工具相关类型
// ============================================================

/// MCP 工具定义
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct McpToolDefinition {
    /// 工具名称
    pub name: String,
    /// 工具描述
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    /// 输入参数的 JSON Schema
    #[serde(skip_serializing_if = "Option::is_none")]
    pub input_schema: Option<Value>,
    /// 工具注解
    #[serde(skip_serializing_if = "Option::is_none")]
    pub annotations: Option<ToolAnnotations>,
}

/// 工具注解（描述工具的行为特征）
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ToolAnnotations {
    /// 工具是否只读
    #[serde(skip_serializing_if = "Option::is_none")]
    pub read_only_hint: Option<bool>,
    /// 工具是否具有破坏性
    #[serde(skip_serializing_if = "Option::is_none")]
    pub destructive_hint: Option<bool>,
    /// 工具是否需要用户确认
    #[serde(skip_serializing_if = "Option::is_none")]
    pub idempotent_hint: Option<bool>,
    /// 工具是否打开第三方世界
    #[serde(skip_serializing_if = "Option::is_none")]
    pub open_world_hint: Option<bool>,
}

/// 工具列表结果
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolsListResult {
    /// 工具列表
    pub tools: Vec<McpToolDefinition>,
    /// 下一页游标
    #[serde(skip_serializing_if = "Option::is_none")]
    pub next_cursor: Option<String>,
}

/// 工具调用参数
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ToolCallParams {
    /// 工具名称
    pub name: String,
    /// 调用参数
    #[serde(default)]
    pub arguments: Value,
    /// 进度令牌
    #[serde(skip_serializing_if = "Option::is_none")]
    pub _meta: Option<Value>,
}

/// 工具调用结果中的内容项
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum ToolCallContent {
    /// 文本内容
    #[serde(rename = "text")]
    Text {
        /// 文本内容
        text: String,
    },
    /// 图片内容
    #[serde(rename = "image")]
    Image {
        /// 图片数据（Base64 编码）
        data: String,
        /// MIME 类型
        mime_type: String,
    },
    /// 资源嵌入
    #[serde(rename = "resource")]
    Resource {
        /// 资源统一资源标识
        resource: EmbeddedResource,
    },
}

/// 嵌入的资源
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EmbeddedResource {
    /// 资源 URI
    pub uri: String,
    /// 资源名称
    pub name: String,
    /// MIME 类型
    #[serde(skip_serializing_if = "Option::is_none")]
    pub mime_type: Option<String>,
    /// 文本内容
    #[serde(skip_serializing_if = "Option::is_none")]
    pub text: Option<String>,
    /// 二进制内容（Base64 编码）
    #[serde(skip_serializing_if = "Option::is_none")]
    pub blob: Option<String>,
}

/// 工具调用结果
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolCallResult {
    /// 内容列表
    pub content: Vec<ToolCallContent>,
    /// 是否出错
    #[serde(default)]
    pub is_error: bool,
}

impl ToolCallResult {
    /// 创建成功的工具调用结果
    pub fn ok(content: Vec<ToolCallContent>) -> Self {
        Self {
            content,
            is_error: false,
        }
    }

    /// 创建文本成功结果
    pub fn text_ok(text: impl Into<String>) -> Self {
        Self {
            content: vec![ToolCallContent::Text { text: text.into() }],
            is_error: false,
        }
    }

    /// 创建错误的工具调用结果
    pub fn error(text: impl Into<String>) -> Self {
        Self {
            content: vec![ToolCallContent::Text { text: text.into() }],
            is_error: true,
        }
    }
}

// ============================================================
// 资源相关类型
// ============================================================

/// MCP 资源定义
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ResourceDefinition {
    /// 资源 URI
    pub uri: String,
    /// 资源名称
    pub name: String,
    /// 资源描述
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    /// MIME 类型
    #[serde(skip_serializing_if = "Option::is_none")]
    pub mime_type: Option<String>,
}

/// 资源列表结果
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResourcesListResult {
    /// 资源列表
    pub resources: Vec<ResourceDefinition>,
    /// 下一页游标
    #[serde(skip_serializing_if = "Option::is_none")]
    pub next_cursor: Option<String>,
}

/// 资源读取参数
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ResourceReadParams {
    /// 资源 URI
    pub uri: String,
}

/// 资源内容项
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum ResourceContent {
    /// 文本资源
    #[serde(rename = "text")]
    Text {
        /// 资源 URI
        uri: String,
        /// MIME 类型
        #[serde(skip_serializing_if = "Option::is_none")]
        mime_type: Option<String>,
        /// 文本内容
        text: String,
    },
    /// 二进制资源
    #[serde(rename = "blob")]
    Blob {
        /// 资源 URI
        uri: String,
        /// MIME 类型
        #[serde(skip_serializing_if = "Option::is_none")]
        mime_type: Option<String>,
        /// 二进制数据（Base64 编码）
        blob: String,
    },
}

/// 资源读取结果
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResourceReadResult {
    /// 内容列表
    pub contents: Vec<ResourceContent>,
}

// ============================================================
// 提示词相关类型
// ============================================================

/// MCP 提示词定义
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PromptDefinition {
    /// 提示词名称
    pub name: String,
    /// 提示词描述
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    /// 参数定义
    #[serde(skip_serializing_if = "Option::is_none")]
    pub arguments: Option<Vec<PromptArgument>>,
}

/// 提示词参数定义
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PromptArgument {
    /// 参数名称
    pub name: String,
    /// 参数描述
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    /// 是否必填
    #[serde(default)]
    pub required: bool,
}

/// 提示词列表结果
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PromptsListResult {
    /// 提示词列表
    pub prompts: Vec<PromptDefinition>,
    /// 下一页游标
    #[serde(skip_serializing_if = "Option::is_none")]
    pub next_cursor: Option<String>,
}

/// 获取提示词参数
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PromptGetParams {
    /// 提示词名称
    pub name: String,
    /// 提示词参数
    #[serde(default)]
    pub arguments: HashMap<String, String>,
}

/// 提示词消息中的内容
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum PromptMessageContent {
    /// 文本内容
    #[serde(rename = "text")]
    Text {
        /// 文本内容
        text: String,
    },
    /// 图片内容
    #[serde(rename = "image")]
    Image {
        /// 图片数据（Base64 编码）
        data: String,
        /// MIME 类型
        mime_type: String,
    },
}

/// 提示词消息
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PromptMessage {
    /// 角色
    pub role: String,
    /// 内容
    pub content: PromptMessageContent,
}

/// 提示词获取结果
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PromptGetResult {
    /// 描述
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    /// 消息列表
    pub messages: Vec<PromptMessage>,
}

// ============================================================
// 日志相关
// ============================================================

/// 日志级别
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum LoggingLevel {
    /// 调试
    #[serde(rename = "debug")]
    Debug,
    /// 信息
    #[serde(rename = "info")]
    Info,
    /// 警告
    #[serde(rename = "warning")]
    Warning,
    /// 错误
    #[serde(rename = "error")]
    Error,
    /// 严重
    #[serde(rename = "critical")]
    Critical,
}

/// 日志消息通知参数
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LoggingMessageParams {
    /// 日志级别
    pub level: LoggingLevel,
    /// 日志消息
    pub logger: String,
    /// 发送者
    pub data: Value,
}

// ============================================================
// 分页游标参数
// ============================================================

/// 分页参数
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct PaginationParams {
    /// 分页游标
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cursor: Option<String>,
}

// ============================================================
// 辅助 trait
// ============================================================

/// 从 JSON Value 中提取 MCP 响应结果
pub trait ExtractResult {
    /// 提取结果
    fn extract<T: for<'de> Deserialize<'de>>(&self) -> Result<T, String>;
}

impl ExtractResult for McpResponse {
    fn extract<T: for<'de> Deserialize<'de>>(&self) -> Result<T, String> {
        match &self.result {
            Some(value) => serde_json::from_value(value.clone())
                .map_err(|e| format!("反序列化结果失败: {}", e)),
            None => match &self.error {
                Some(err) => Err(format!("MCP 错误: {}", err)),
                None => Err("响应中既没有 result 也没有 error".to_string()),
            },
        }
    }
}

/// 从 JSON Value 中提取带游标的结果
pub trait ExtractPaginatedResult {
    /// 提取分页结果
    fn extract_paginated<T: for<'de> Deserialize<'de>>(&self) -> Result<(Vec<T>, Option<String>), String>;
}
