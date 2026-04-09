//! 飞书配置模块
//!
//! 定义了飞书通道插件所需的配置结构体，包括应用凭证、
//! 事件回调验证、加密密钥等配置项。

use serde::{Deserialize, Serialize};

/// 飞书通道配置
///
/// 包含飞书开放平台应用的所有必要配置信息。
/// 可通过环境变量或配置文件加载。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FeishuConfig {
    /// 飞书应用 App ID
    pub app_id: String,

    /// 飞书应用 App Secret
    pub app_secret: String,

    /// 事件回调验证 Token
    ///
    /// 在飞书开放平台后台 -> 事件订阅中获取，
    /// 用于验证回调请求的合法性。
    pub verification_token: String,

    /// 事件加密密钥（可选）
    ///
    /// 如果在飞书开放平台后台配置了事件加密，
    /// 则需要提供此密钥来解密事件内容。
    pub encrypt_key: Option<String>,

    /// 自定义 Webhook 回调 URL 路径（可选）
    ///
    /// 默认为 "/feishu/webhook"，可自定义路径。
    pub webhook_url: Option<String>,

    /// 飞书 API 基础 URL
    ///
    /// 默认为 "https://open.feishu.cn"，
    /// 可修改为其他环境（如测试环境）的地址。
    #[serde(default = "default_base_url")]
    pub base_url: String,

    /// 允许交互的用户列表
    ///
    /// 空列表表示允许所有用户与 Bot 交互。
    /// 填入用户 open_id 或 user_id 来限制访问。
    #[serde(default)]
    pub allowed_users: Vec<String>,
}

/// 默认飞书 API 基础 URL
fn default_base_url() -> String {
    "https://open.feishu.cn".to_string()
}

impl FeishuConfig {
    /// 创建新的飞书配置
    pub fn new(
        app_id: impl Into<String>,
        app_secret: impl Into<String>,
        verification_token: impl Into<String>,
    ) -> Self {
        Self {
            app_id: app_id.into(),
            app_secret: app_secret.into(),
            verification_token: verification_token.into(),
            encrypt_key: None,
            webhook_url: None,
            base_url: default_base_url(),
            allowed_users: Vec::new(),
        }
    }

    /// 设置事件加密密钥
    pub fn with_encrypt_key(mut self, key: impl Into<String>) -> Self {
        self.encrypt_key = Some(key.into());
        self
    }

    /// 设置自定义 Webhook URL 路径
    pub fn with_webhook_url(mut self, url: impl Into<String>) -> Self {
        self.webhook_url = Some(url.into());
        self
    }

    /// 设置自定义基础 URL
    pub fn with_base_url(mut self, url: impl Into<String>) -> Self {
        self.base_url = url.into();
        self
    }

    /// 设置允许交互的用户列表
    pub fn with_allowed_users(mut self, users: Vec<String>) -> Self {
        self.allowed_users = users;
        self
    }

    /// 获取 Webhook URL 路径
    pub fn webhook_path(&self) -> &str {
        self.webhook_url
            .as_deref()
            .unwrap_or("/feishu/webhook")
    }

    /// 检查用户是否在允许列表中
    ///
    /// 如果允许列表为空，则允许所有用户。
    pub fn is_user_allowed(&self, user_id: &str) -> bool {
        self.allowed_users.is_empty() || self.allowed_users.iter().any(|u| u == user_id)
    }
}
