//! pa-query - Reask 查询循环引擎
//!
//! 本 crate 实现了参考 Claude Code 的 reask 架构的核心查询循环引擎。
//!
//! ## 核心概念
//!
//! **Reask 循环**：用户输入 -> 构建请求 -> 调用 LLM -> 处理响应
//!   -> 如果是 tool_use: 执行工具 -> 获取结果 -> **[REASK: 将结果反馈给模型]**
//!   -> 如果是 end_turn: 循环结束
//!
//! ## 主要功能
//!
//! - 多轮 tool_use -> tool_result -> reask 循环
//! - 工具结果预算管理（防止上下文膨胀）
//! - 并发工具执行（is_concurrency_safe 的工具可并行）
//! - 错误分类重试（rate limit 等待、529 切换备用模型）
//! - 自动压缩（上下文使用率过高时）
//! - 权限检查流程
//! - Token 使用警告
//! - MAGMA 记忆检索集成

pub mod config;
pub mod engine;
pub mod error;
pub mod permission;

// 重新导出常用类型
pub use config::QueryConfig;
pub use engine::{QueryEngine, QueryOutcome};
pub use error::ErrorAction;
pub use permission::PermissionDecision;
