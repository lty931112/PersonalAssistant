//! 通道插件 trait

use async_trait::async_trait;
use pa_core::CoreError;
use crate::plugin::Plugin;

/// 通道消息
#[derive(Debug, Clone)]
pub struct ChannelMessage {
    pub id: String,
    pub channel: String,
    pub sender: String,
    pub content: String,
    pub timestamp: i64,
}

/// 通道配置
#[derive(Debug, Clone)]
pub struct ChannelConfig {
    pub name: String,
    pub enabled: bool,
    pub settings: serde_json::Value,
}

/// 通道插件 trait
#[async_trait]
pub trait ChannelPlugin: Plugin {
    /// 发送消息
    async fn send(&self, message: &ChannelMessage) -> Result<(), CoreError>;

    /// 接收消息（非阻塞）
    async fn receive(&self) -> Result<Option<ChannelMessage>, CoreError>;

    /// 获取通道配置
    fn config(&self) -> &ChannelConfig;
}
