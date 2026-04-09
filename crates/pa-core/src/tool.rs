//! 工具类型定义
//!
//! 定义了工具系统的核心类型，包括工具定义、工具结果、工具输入和权限模式。
//! 这些类型用于描述智能体可以调用的外部工具及其执行结果。

use serde::{Deserialize, Serialize};

/// 权限模式
///
/// 控制工具调用时的权限检查行为，参考 Claude Code 的权限模型。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum PermissionMode {
    /// 默认模式：每次工具调用都需要用户确认
    Default,
    /// 接受编辑模式：自动允许文件编辑类工具
    AcceptEdits,
    /// 绕过权限模式：自动允许所有工具调用（危险模式）
    BypassPermissions,
    /// 计划模式：只生成计划不执行工具
    Plan,
    /// 自动模式：根据工具安全级别自动决定
    Auto,
}

impl std::fmt::Display for PermissionMode {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            PermissionMode::Default => write!(f, "default"),
            PermissionMode::AcceptEdits => write!(f, "accept-edits"),
            PermissionMode::BypassPermissions => write!(f, "bypass-permissions"),
            PermissionMode::Plan => write!(f, "plan"),
            PermissionMode::Auto => write!(f, "auto"),
        }
    }
}

impl std::default::Default for PermissionMode {
    fn default() -> Self {
        PermissionMode::Default
    }
}

/// 工具定义
///
/// 描述一个可被智能体调用的工具，包括名称、描述、输入 schema 和安全属性。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolDefinition {
    /// 工具名称（唯一标识）
    pub name: String,
    /// 工具描述（供 LLM 理解工具用途）
    pub description: String,
    /// 输入参数的 JSON Schema
    pub input_schema: serde_json::Value,
    /// 是否为只读工具（不修改外部状态）
    pub is_read_only: bool,
    /// 是否支持并发安全调用
    pub is_concurrency_safe: bool,
}

impl ToolDefinition {
    /// 创建新的工具定义
    pub fn new(
        name: impl Into<String>,
        description: impl Into<String>,
        input_schema: serde_json::Value,
    ) -> Self {
        Self {
            name: name.into(),
            description: description.into(),
            input_schema,
            is_read_only: false,
            is_concurrency_safe: true,
        }
    }

    /// 标记为只读工具
    pub fn read_only(mut self) -> Self {
        self.is_read_only = true;
        self
    }

    /// 标记为不支持并发
    pub fn not_concurrency_safe(mut self) -> Self {
        self.is_concurrency_safe = false;
        self
    }

    /// 转换为 Claude API 格式的工具定义
    pub fn to_api_format(&self) -> serde_json::Value {
        serde_json::json!({
            "name": self.name,
            "description": self.description,
            "input_schema": self.input_schema,
        })
    }
}

/// 工具输入
///
/// 封装工具调用的输入数据。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolInput {
    /// 工具名称
    pub tool_name: String,
    /// 工具调用 ID
    pub tool_call_id: String,
    /// 输入参数（JSON 格式）
    pub input: serde_json::Value,
}

impl ToolInput {
    /// 创建新的工具输入
    pub fn new(
        tool_name: impl Into<String>,
        tool_call_id: impl Into<String>,
        input: serde_json::Value,
    ) -> Self {
        Self {
            tool_name: tool_name.into(),
            tool_call_id: tool_call_id.into(),
            input,
        }
    }
}

/// 工具执行结果
///
/// 封装工具调用的执行结果，包含输出内容和状态信息。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolResult {
    /// 对应的工具调用 ID
    pub tool_use_id: String,
    /// 工具名称
    pub tool_name: String,
    /// 输出内容
    pub content: String,
    /// 是否执行出错
    pub is_error: bool,
    /// 执行耗时（毫秒）
    pub duration_ms: Option<u64>,
}

impl ToolResult {
    /// 创建成功的工具结果
    pub fn success(
        tool_use_id: impl Into<String>,
        tool_name: impl Into<String>,
        content: impl Into<String>,
    ) -> Self {
        Self {
            tool_use_id: tool_use_id.into(),
            tool_name: tool_name.into(),
            content: content.into(),
            is_error: false,
            duration_ms: None,
        }
    }

    /// 创建失败的工具结果
    pub fn error(
        tool_use_id: impl Into<String>,
        tool_name: impl Into<String>,
        error_message: impl Into<String>,
    ) -> Self {
        Self {
            tool_use_id: tool_use_id.into(),
            tool_name: tool_name.into(),
            content: error_message.into(),
            is_error: true,
            duration_ms: None,
        }
    }

    /// 创建简化的成功结果（不需要 tool_use_id 和 tool_name）
    pub fn ok(content: impl Into<String>) -> Self {
        Self {
            tool_use_id: String::new(),
            tool_name: String::new(),
            content: content.into(),
            is_error: false,
            duration_ms: None,
        }
    }

    /// 创建简化的错误结果（不需要 tool_use_id 和 tool_name）
    pub fn err(content: impl Into<String>) -> Self {
        Self {
            tool_use_id: String::new(),
            tool_name: String::new(),
            content: content.into(),
            is_error: true,
            duration_ms: None,
        }
    }

    /// 设置执行耗时
    pub fn with_duration(mut self, duration_ms: u64) -> Self {
        self.duration_ms = Some(duration_ms);
        self
    }
}
