//! 错误类型定义
//!
//! 定义了平台的核心错误类型 `CoreError`，覆盖 API 调用、上下文窗口、
//! 速率限制、工具执行、记忆操作等各类错误场景。

use std::fmt;

use serde::{Deserialize, Serialize};

/// 核心错误类型
///
/// PersonalAssistant 平台的统一错误类型，涵盖所有可能的错误场景。
/// 实现了 `std::error::Error` trait，可与其他错误类型无缝集成。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum CoreError {
    /// API 调用错误
    ApiError {
        /// HTTP 状态码
        status: u16,
        /// 错误消息
        message: String,
        /// 是否可重试
        retryable: bool,
    },
    /// 上下文窗口超出限制
    ContextWindowExceeded,
    /// 达到最大 token 数
    MaxTokensReached,
    /// 触发速率限制
    RateLimit {
        /// 建议重试等待时间（秒）
        retry_after: Option<f64>,
    },
    /// 服务过载（529 错误）
    Overloaded(String),
    /// 超出预算限制
    BudgetExceeded {
        /// 当前已消耗费用（美元）
        cost_usd: f64,
        /// 预算上限（美元）
        limit_usd: f64,
    },
    /// 工具执行错误
    ToolExecutionError {
        /// 工具名称
        tool_name: String,
        /// 错误消息
        message: String,
    },
    /// 记忆系统错误
    MemoryError(String),
    /// 配置错误
    ConfigError(String),
    /// 权限被拒绝
    PermissionDenied {
        /// 工具名称
        tool_name: String,
        /// 拒绝原因
        reason: String,
    },
    /// IO 错误
    IoError(String),
    /// 工具未找到
    ToolNotFound(String),
    /// 内部错误
    Internal(String),
    /// 序列化/反序列化错误
    Serialization(String),
    /// 配置错误（兼容别名）
    Configuration(String),
    /// 认证错误
    Authentication(String),
    /// 上下文长度超限
    ContextTooLong {
        /// 输入 token 数
        input_tokens: u32,
        /// 最大 token 限制
        max_tokens: u32,
    },
    /// LLM 客户端错误
    LlmClient(String),
    /// API 请求错误
    ApiRequest(String),
    /// API 响应错误
    ApiResponse(String),
    /// 记忆引擎错误（兼容别名）
    Memory(String),
}

impl CoreError {
    /// 创建 API 错误
    pub fn api_error(status: u16, message: impl Into<String>) -> Self {
        CoreError::ApiError {
            status,
            message: message.into(),
            retryable: status >= 500,
        }
    }

    /// 创建可重试的 API 错误
    pub fn retryable_api_error(status: u16, message: impl Into<String>) -> Self {
        CoreError::ApiError {
            status,
            message: message.into(),
            retryable: true,
        }
    }

    /// 创建工具执行错误
    pub fn tool_error(tool_name: impl Into<String>, message: impl Into<String>) -> Self {
        CoreError::ToolExecutionError {
            tool_name: tool_name.into(),
            message: message.into(),
        }
    }

    /// 创建记忆错误
    pub fn memory_error(message: impl Into<String>) -> Self {
        CoreError::MemoryError(message.into())
    }

    /// 创建配置错误
    pub fn config_error(message: impl Into<String>) -> Self {
        CoreError::ConfigError(message.into())
    }

    /// 创建权限拒绝错误
    pub fn permission_denied(tool_name: impl Into<String>, reason: impl Into<String>) -> Self {
        CoreError::PermissionDenied {
            tool_name: tool_name.into(),
            reason: reason.into(),
        }
    }

    /// 创建 IO 错误
    pub fn io_error(message: impl Into<String>) -> Self {
        CoreError::IoError(message.into())
    }

    /// 判断错误是否可重试
    pub fn is_retryable(&self) -> bool {
        match self {
            CoreError::ApiError { retryable, .. } => *retryable,
            CoreError::RateLimit { .. } => true,
            CoreError::Overloaded { .. } => true,
            CoreError::ContextTooLong { .. } => true,
            CoreError::LlmClient(_) => true,
            CoreError::ApiRequest(_) => true,
            CoreError::ApiResponse(_) => true,
            CoreError::ToolExecutionError { .. } => true,
            CoreError::Memory(_) => true,
            CoreError::Internal(_) => true,
            CoreError::Serialization(_) => true,
            _ => false,
        }
    }
}

impl fmt::Display for CoreError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            CoreError::ApiError { status, message, retryable } => {
                if *retryable {
                    write!(f, "API 错误 (状态码: {}, 可重试): {}", status, message)
                } else {
                    write!(f, "API 错误 (状态码: {}): {}", status, message)
                }
            }
            CoreError::ContextWindowExceeded => {
                write!(f, "上下文窗口超出限制")
            }
            CoreError::MaxTokensReached => {
                write!(f, "已达到最大 token 数量")
            }
            CoreError::RateLimit { retry_after } => {
                match retry_after {
                    Some(after) => write!(f, "触发速率限制，建议在 {:.1} 秒后重试", after),
                    None => write!(f, "触发速率限制"),
                }
            }
            CoreError::Overloaded(msg) => {
                write!(f, "服务过载: {}", msg)
            }
            CoreError::BudgetExceeded { cost_usd, limit_usd } => {
                write!(f, "超出预算限制: 已消耗 ${:.4} / 限制 ${:.4}", cost_usd, limit_usd)
            }
            CoreError::ToolExecutionError { tool_name, message } => {
                write!(f, "工具执行错误 [{}]: {}", tool_name, message)
            }
            CoreError::MemoryError(msg) => {
                write!(f, "记忆系统错误: {}", msg)
            }
            CoreError::ConfigError(msg) => {
                write!(f, "配置错误: {}", msg)
            }
            CoreError::PermissionDenied { tool_name, reason } => {
                write!(f, "权限被拒绝 [{}]: {}", tool_name, reason)
            }
            CoreError::IoError(msg) => {
                write!(f, "IO 错误: {}", msg)
            }
            CoreError::ToolNotFound(name) => {
                write!(f, "工具未找到: {}", name)
            }
            CoreError::Internal(msg) => {
                write!(f, "内部错误: {}", msg)
            }
            CoreError::Serialization(msg) => {
                write!(f, "序列化错误: {}", msg)
            }
            CoreError::Configuration(msg) => {
                write!(f, "配置错误: {}", msg)
            }
            CoreError::Authentication(msg) => {
                write!(f, "认证错误: {}", msg)
            }
            CoreError::ContextTooLong { input_tokens, max_tokens } => {
                write!(f, "上下文长度超限: 输入 {} tokens 超过限制 {}", input_tokens, max_tokens)
            }
            CoreError::LlmClient(msg) => {
                write!(f, "LLM 客户端错误: {}", msg)
            }
            CoreError::ApiRequest(msg) => {
                write!(f, "API 请求错误: {}", msg)
            }
            CoreError::ApiResponse(msg) => {
                write!(f, "API 响应错误: {}", msg)
            }
            CoreError::Memory(msg) => {
                write!(f, "记忆引擎错误: {}", msg)
            }
        }
    }
}

impl std::error::Error for CoreError {}

impl From<std::io::Error> for CoreError {
    fn from(err: std::io::Error) -> Self {
        CoreError::IoError(err.to_string())
    }
}
