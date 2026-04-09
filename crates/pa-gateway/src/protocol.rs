//! 通信协议定义

use serde::{Deserialize, Serialize};

/// 协议消息
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Message {
    /// 消息类型
    pub kind: MessageKind,
    /// 消息内容
    pub payload: serde_json::Value,
}

/// 消息类型
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum MessageKind {
    /// 方法调用请求
    Call,
    /// 方法调用响应
    Response,
    /// 事件通知
    Event,
    /// 错误
    Error,
}

/// 方法调用请求
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MethodCall {
    /// 调用 ID
    pub id: String,
    /// 方法名
    pub method: String,
    /// 参数
    pub params: serde_json::Value,
}

/// 方法调用响应
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MethodResponse {
    /// 对应的调用 ID
    pub id: String,
    /// 结果
    pub result: Option<serde_json::Value>,
    /// 错误
    pub error: Option<String>,
}

impl MethodResponse {
    pub fn success(id: impl Into<String>, result: serde_json::Value) -> Self {
        Self {
            id: id.into(),
            result: Some(result),
            error: None,
        }
    }

    pub fn error(id: impl Into<String>, error: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            result: None,
            error: Some(error.into()),
        }
    }
}
