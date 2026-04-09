//! pa-tools - 工具系统
//!
//! 本 crate 实现了 PersonalAssistant 的工具注册表和内置工具集。
//!
//! 主要功能：
//! - 工具注册表（ToolRegistry）：管理所有可用工具
//! - 工具 trait 定义：统一的工具接口
//! - 内置工具集：Bash、文件读写、搜索、记忆、网页获取等

pub mod builtin;
pub mod registry;

// 重新导出常用类型
pub use registry::ToolRegistry;
