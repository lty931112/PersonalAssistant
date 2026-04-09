//! pa-channel-feishu - 飞书通道插件
//!
//! 本 crate 实现了通过飞书开放平台 Bot API 进行消息交互的通道插件，
//! 支持消息收发、事件回调、签名验证等功能。
//!
//! # 主要功能
//! - 通过 Webhook 接收飞书事件回调
//! - 发送文本、Markdown、卡片等多种类型的消息
//! - 回复消息
//! - 获取群信息和用户信息
//! - 上传图片
//! - Token 自动缓存和刷新
//! - 事件签名验证
//!
//! # 使用示例
//! ```rust,no_run
//! use pa_channel_feishu::config::FeishuConfig;
//! use pa_channel_feishu::channel::FeishuChannel;
//!
//! let config = FeishuConfig::new("app_id", "app_secret", "verification_token");
//! let channel = FeishuChannel::new(config);
//! ```

// 模块导出
pub mod config;
pub mod client;
pub mod event;
pub mod channel;
pub mod types;

// 公开导出常用类型
pub use config::FeishuConfig;
pub use client::FeishuClient;
pub use event::{FeishuEvent, FeishuEventHandler, EventCallbackHandler};
pub use channel::FeishuChannel;
pub use types::{
    ApiResponse, CardBody, CardElement, CardHeader, CardText, ChatInfo,
    MessageContent, SendMessageRequest, SendMessageResponse, TokenResponse,
    UserInfo, WebhookResponse,
};
