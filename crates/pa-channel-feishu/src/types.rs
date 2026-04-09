//! 飞书 API 类型定义
//!
//! 定义了与飞书开放平台 API 交互时使用的所有数据类型，
//! 包括群信息、用户信息、消息内容、API 响应等。

use serde::{Deserialize, Serialize};

/// 群信息
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChatInfo {
    /// 群 ID
    pub chat_id: String,
    /// 群名称
    pub name: String,
    /// 群描述
    pub description: Option<String>,
    /// 群头像 URL
    pub avatar: Option<String>,
    /// 群类型（p2p / group）
    pub chat_type: String,
    /// 是否为外部群
    pub is_external: bool,
    /// 群成员数量
    pub member_count: Option<i64>,
    /// 群创建时间
    pub create_time: Option<String>,
    /// 群所有者 ID
    pub owner_id: Option<String>,
}

/// 用户信息
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UserInfo {
    /// 用户 ID（open_id）
    pub user_id: String,
    /// 用户 ID（union_id）
    pub union_id: Option<String>,
    /// 用户 ID（user_id）
    pub user_id_str: Option<String>,
    /// 用户名称
    pub name: String,
    /// 用户英文名称
    pub en_name: Option<String>,
    /// 用户头像 URL
    pub avatar: Option<String>,
    /// 用户邮箱
    pub email: Option<String>,
    /// 用户手机号
    pub mobile: Option<String>,
    /// 用户是否为租户管理员
    pub is_tenant_admin: Option<bool>,
}

/// 消息内容
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MessageContent {
    /// 消息 ID
    pub message_id: String,
    /// 消息类型（text / post / interactive / image 等）
    pub msg_type: String,
    /// 消息内容（JSON 字符串）
    pub content: serde_json::Value,
    /// 发送者信息
    pub sender: Option<SenderInfo>,
    /// 消息创建时间
    pub create_time: Option<String>,
}

/// 发送者信息
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SenderInfo {
    /// 发送者 ID
    pub sender_id: String,
    /// 发送者类型（user / app）
    pub sender_type: String,
    /// 发送者租户 key
    pub tenant_key: Option<String>,
}

/// 统一 API 响应
///
/// 飞书开放平台 API 的标准响应格式：
/// ```json
/// {
///   "code": 0,
///   "msg": "success",
///   "data": { ... }
/// }
/// ```
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ApiResponse<T> {
    /// 错误码，0 表示成功
    pub code: i64,
    /// 错误消息
    pub msg: String,
    /// 响应数据
    pub data: Option<T>,
}

impl<T> ApiResponse<T> {
    /// 判断响应是否成功
    pub fn is_success(&self) -> bool {
        self.code == 0
    }

    /// 获取错误消息
    pub fn error_message(&self) -> String {
        format!("飞书 API 错误 [code={}]: {}", self.code, self.msg)
    }
}

/// Webhook 响应
///
/// 用于响应飞书事件回调的验证请求和处理结果。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WebhookResponse {
    /// URL 验证时返回的 challenge
    pub challenge: Option<String>,
    /// 处理结果
    pub code: Option<i64>,
    /// 消息
    #[serde(rename = "msg")]
    pub message: Option<String>,
}

impl WebhookResponse {
    /// 创建 URL 验证响应
    pub fn challenge(challenge: impl Into<String>) -> Self {
        Self {
            challenge: Some(challenge.into()),
            code: None,
            message: None,
        }
    }

    /// 创建成功响应
    pub fn ok() -> Self {
        Self {
            challenge: None,
            code: Some(0),
            message: Some("success".to_string()),
        }
    }
}

/// Token 响应
///
/// 飞书 tenant_access_token 获取接口的响应格式。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TokenResponse {
    /// 访问令牌
    pub tenant_access_token: String,
    /// 令牌有效期（秒）
    pub expire: i64,
}

/// 发送消息请求
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SendMessageRequest {
    /// 接收消息的 chat_id
    pub receive_id: String,
    /// 接收者类型（chat / open_id / user_id）
    pub receive_id_type: String,
    /// 消息类型
    pub msg_type: String,
    /// 消息内容（JSON 字符串）
    pub content: String,
}

/// 发送消息响应
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SendMessageResponse {
    /// 消息 ID
    #[serde(default)]
    pub message_id: String,
}

/// 回复消息请求
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReplyMessageRequest {
    /// 被回复的消息 ID
    pub msg_id: String,
    /// 消息类型
    pub msg_type: String,
    /// 消息内容（JSON 字符串）
    pub content: String,
}

/// 卡片消息元素
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "tag")]
pub enum CardElement {
    /// Markdown 文本
    #[serde(rename = "markdown")]
    Markdown {
        /// Markdown 内容
        content: String,
    },
    /// 纯文本
    #[serde(rename = "div")]
    Div {
        /// 文本内容
        text: CardText,
    },
    /// 操作按钮区域
    #[serde(rename = "action")]
    Action {
        /// 操作动作
        actions: Vec<CardAction>,
    },
    /// 分割线
    #[serde(rename = "hr")]
    Hr,
    /// 图片
    #[serde(rename = "img")]
    Img {
        /// 图片 key
        img_key: String,
        /// 图片替代文本
        alt: Option<CardText>,
    },
    /// 备注
    #[serde(rename = "note")]
    Note {
        /// 备注元素列表
        elements: Vec<CardElement>,
    },
}

/// 卡片文本
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CardText {
    /// 文本标签
    pub tag: String,
    /// 文本内容
    pub content: String,
}

/// 卡片操作按钮
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CardAction {
    /// 按钮标签
    pub tag: String,
    /// 按钮文本
    pub text: CardText,
    /// 按钮类型（primary / default / danger）
    #[serde(rename = "type")]
    pub button_type: String,
    /// 多卡片交互时的 value
    pub value: Option<serde_json::Value>,
}

/// 卡片消息头部
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CardHeader {
    /// 标题
    pub title: CardText,
    /// 头部模板颜色（blue / wathet / turquoise / green / yellow |
    ///   orange / red | carmine / violet / purple | indigo | grey）
    #[serde(rename = "template")]
    pub color: Option<String>,
}

/// 卡片消息体
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CardBody {
    /// 卡片元素列表
    pub elements: Vec<CardElement>,
}
