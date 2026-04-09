//! 插件 SDK 模块
//!
//! 提供标准化的插件扩展接口，支持 LLM 提供商、消息通道、工具等扩展。

pub mod plugin;
pub mod extension;
pub mod channel;
pub mod tool;
pub mod llm_provider;

pub use plugin::{Plugin, PluginContext, PluginMetadata};
pub use extension::{Extension, ExtensionManager};
pub use channel::{ChannelPlugin, ChannelMessage, ChannelConfig};
pub use tool::ToolPlugin;
pub use pa_core::ToolDefinition;
pub use llm_provider::{LlmProviderPlugin, LlmConfig};
