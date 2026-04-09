//! pa-task - 任务监控、中断恢复和持久化存储
//!
//! 本 crate 实现了 PersonalAssistant 平台的任务管理系统，提供以下核心功能：
//!
//! - **任务状态管理**: 跟踪任务从创建到完成的完整生命周期
//! - **中断恢复**: 通过任务快照实现执行中断后的状态恢复
//! - **SQLite 持久化**: 使用 tokio-rusqlite 实现异步数据库操作
//! - **取消控制**: 基于 tokio::watch 的任务取消信号机制
//!
//! # 模块结构
//!
//! - [`types`]: 任务状态、优先级、快照、事件等核心类型定义
//! - [`cancel_token`]: 基于 tokio::watch 的取消令牌实现
//! - [`store`]: SQLite 持久化存储
//! - [`manager`]: 任务管理器（统一的生命周期管理接口）

// 模块导出
pub mod cancel_token;
pub mod manager;
pub mod store;
pub mod types;

// 重新导出核心类型
pub use cancel_token::{CancellationToken, SharedCancellationToken};
pub use manager::TaskManager;
pub use store::TaskStore;
pub use types::{
    TaskEvent, TaskEventType, TaskFilter, TaskInfo, TaskPriority, TaskSnapshot, TaskStatus,
};
