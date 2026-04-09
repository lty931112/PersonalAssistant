//! 错误分类模块
//!
//! 对 LLM 调用过程中出现的错误进行分类，并决定相应的处理动作。

use pa_core::CoreError;

/// 错误处理动作
#[derive(Debug, Clone, PartialEq)]
pub enum ErrorAction {
    /// 等待指定毫秒数后重试
    Retry { after_ms: u64 },
    /// 切换到备用模型
    FallbackModel,
    /// 自动压缩上下文后重试
    AutoCompact,
    /// 终止查询
    Abort,
}

/// 错误分类器
pub struct ErrorClassifier;

impl ErrorClassifier {
    /// 对错误进行分类并返回处理动作
    pub fn classify(error: &CoreError) -> ErrorAction {
        match error {
            CoreError::RateLimit { retry_after } => {
                let ms = retry_after.map(|s| (s * 1000.0) as u64).unwrap_or(1000);
                ErrorAction::Retry { after_ms: ms }
            }
            CoreError::Overloaded(_) => ErrorAction::FallbackModel,
            CoreError::ContextWindowExceeded => ErrorAction::AutoCompact,
            CoreError::Authentication(_) => ErrorAction::Abort,
            CoreError::PermissionDenied { .. } => ErrorAction::Abort,
            CoreError::ConfigError(_) => ErrorAction::Abort,
            CoreError::BudgetExceeded { .. } => ErrorAction::Abort,
            CoreError::ToolExecutionError { .. } => ErrorAction::Retry { after_ms: 0 },
            CoreError::MemoryError(_) => ErrorAction::Retry { after_ms: 0 },
            CoreError::ApiError { retryable: true, .. } => ErrorAction::Retry { after_ms: 2000 },
            CoreError::ApiError { retryable: false, .. } => ErrorAction::Abort,
            CoreError::MaxTokensReached => ErrorAction::Retry { after_ms: 1000 },
            CoreError::Internal(_) => ErrorAction::Retry { after_ms: 1000 },
            CoreError::IoError(_) => ErrorAction::Retry { after_ms: 2000 },
            CoreError::ToolNotFound(_) => ErrorAction::Abort,
            CoreError::Serialization(_) => ErrorAction::Retry { after_ms: 1000 },
            CoreError::Configuration(_) => ErrorAction::Abort,
            CoreError::LlmClient(_) => ErrorAction::Retry { after_ms: 2000 },
            CoreError::ApiRequest(_) => ErrorAction::Retry { after_ms: 2000 },
            CoreError::ApiResponse(_) => ErrorAction::Retry { after_ms: 2000 },
            CoreError::ContextTooLong { .. } => ErrorAction::AutoCompact,
            CoreError::Memory(_) => ErrorAction::Retry { after_ms: 0 },
        }
    }

    /// 判断错误是否可以重试
    pub fn is_retryable(error: &CoreError) -> bool {
        matches!(
            Self::classify(error),
            ErrorAction::Retry { .. } | ErrorAction::FallbackModel | ErrorAction::AutoCompact
        )
    }
}
