//! Agent 运行时模块
//!
//! 实现智能体运行时，支持多 Agent 路由、认证配置轮换、沙箱执行等。

pub mod agent;
pub mod router;
pub mod sandbox;
pub mod auth_profile;

pub use agent::{Agent, AgentHandle, AgentState, AgentStatusInfo, AgentCommand, AgentEvent};
pub use router::{AgentRouter, RoutingRule, RoutingContext, RoutingResult};
pub use sandbox::SandboxExecutor;
pub use auth_profile::{AuthProfile, AuthProfileManager};

// 重新导出常用类型
pub use pa_core::{AgentId, AgentConfig, AgentStatus, PermissionMode};
