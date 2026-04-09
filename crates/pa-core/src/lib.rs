//! pa-core - PersonalAssistant 核心类型定义
//!
//! 本 crate 定义了整个 PersonalAssistant 项目共享的核心类型，
//! 包括消息、内容块、工具定义、使用量信息、错误类型、事件类型等。

// 模块导出
pub mod message;
pub mod tool;
pub mod error;
pub mod event;
pub mod agent;

// 重新导出常用类型 - 消息相关
pub use message::{ContentBlock, Message, MessageRole};

// 重新导出常用类型 - 工具相关
pub use tool::{PermissionMode, ToolDefinition, ToolInput, ToolResult};

// 重新导出常用类型 - 错误相关
pub use error::CoreError;

// 重新导出常用类型 - 事件相关
pub use event::{GatewayEvent, MemoryEvent, QueryEvent, StopReason, TokenState, UsageInfo};

// 重新导出常用类型 - 智能体相关
pub use agent::{AgentConfig, AgentId, AgentStatus};
