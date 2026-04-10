//! 飞书通道插件
//!
//! 实现了 `pa_plugin_sdk::ChannelPlugin` trait，
//! 通过飞书开放平台 Bot API 进行消息交互。
//!
//! 内部使用 axum HTTP 服务器接收飞书事件回调 webhook，
//! 并通过 mpsc channel 将事件传递给 `receive()` 方法。

use std::sync::Arc;

use async_trait::async_trait;
use axum::{
    extract::State,
    http::{HeaderMap, StatusCode},
    routing::post,
    Json, Router,
};
use chrono::Utc;
use serde_json::{json, Value};
use tokio::sync::{mpsc, RwLock};
use tracing::{debug, error, info, warn};

use pa_core::CoreError;
use pa_plugin_sdk::channel::{ChannelConfig, ChannelMessage, ChannelPlugin};
use pa_plugin_sdk::plugin::{Plugin, PluginContext, PluginMetadata};

use crate::client::FeishuClient;
use crate::config::FeishuConfig;
use crate::event::{FeishuEvent, FeishuEventHandler};

/// 飞书通道插件
///
/// 实现了 `ChannelPlugin` trait，提供飞书 Bot 的消息收发能力。
/// 内部启动一个 axum HTTP 服务器来接收飞书事件回调。
pub struct FeishuChannel {
    /// 插件元数据
    metadata: PluginMetadata,
    /// 通道配置
    channel_config: ChannelConfig,
    /// 飞书配置
    feishu_config: FeishuConfig,
    /// 飞书 API 客户端
    client: Arc<FeishuClient>,
    /// 事件处理器
    event_handler: Arc<FeishuEventHandler>,
    /// 事件接收通道（发送端）
    event_sender: Arc<RwLock<Option<mpsc::Sender<ChannelMessage>>>>,
    /// 事件接收通道（接收端）
    event_receiver: Arc<RwLock<Option<mpsc::Receiver<ChannelMessage>>>>,
    /// HTTP 服务器句柄
    server_handle: Arc<RwLock<Option<tokio::task::JoinHandle<()>>>>,
    /// 监听端口
    port: u16,
}

impl FeishuChannel {
    /// 创建新的飞书通道
    ///
    /// # 参数
    /// - `config`: 飞书配置
    pub fn new(config: FeishuConfig) -> Self {
        let client = Arc::new(FeishuClient::new(config.clone()));
        let event_handler = Arc::new(FeishuEventHandler::new(client.clone()));

        let channel_config = ChannelConfig {
            name: "feishu".to_string(),
            enabled: true,
            settings: serde_json::to_value(&config).unwrap_or_default(),
        };

        let metadata = PluginMetadata {
            name: "pa-channel-feishu".to_string(),
            version: env!("CARGO_PKG_VERSION").to_string(),
            description: "飞书通道插件 - 通过飞书开放平台 Bot API 进行消息交互".to_string(),
            author: "PersonalAssistant Contributors".to_string(),
        };

        Self {
            metadata,
            channel_config,
            feishu_config: config,
            client,
            event_handler,
            event_sender: Arc::new(RwLock::new(None)),
            event_receiver: Arc::new(RwLock::new(None)),
            server_handle: Arc::new(RwLock::new(None)),
            port: 19871,
        }
    }

    /// 设置监听端口
    pub fn with_port(mut self, port: u16) -> Self {
        self.port = port;
        self
    }

    /// 启动 Webhook 服务器
    ///
    /// 启动一个 axum HTTP 服务器，监听飞书事件回调。
    /// 事件通过 mpsc channel 传递给 `receive()` 方法。
    pub async fn start_server(&self) -> Result<(), CoreError> {
        // 创建 mpsc channel 用于事件传递
        let (tx, rx) = mpsc::channel(256);
        {
            let mut sender = self.event_sender.write().await;
            *sender = Some(tx);
        }
        {
            let mut receiver = self.event_receiver.write().await;
            *receiver = Some(rx);
        }

        let webhook_path = self.feishu_config.webhook_path().to_string();
        let client = self.client.clone();
        let event_handler = self.event_handler.clone();
        let event_sender = self.event_sender.clone();
        let config = self.feishu_config.clone();

        // 构建 axum 路由
        let app_state = AppState {
            client,
            event_handler,
            event_sender,
            config,
        };

        let app = Router::new()
            .route(&webhook_path, post(handle_webhook))
            .with_state(app_state);

        let addr = format!("0.0.0.0:{}", self.port);
        info!("飞书 Webhook 服务器启动，监听地址: {}，路径: {}", addr, webhook_path);

        // 启动 HTTP 服务器
        let handle = tokio::spawn(async move {
            let listener = match tokio::net::TcpListener::bind(&addr).await {
                Ok(l) => l,
                Err(e) => {
                    error!("飞书 Webhook 服务器绑定地址失败: {}", e);
                    return;
                }
            };

            info!("飞书 Webhook 服务器已绑定到 {}", addr);

            if let Err(e) = axum::serve(listener, app).await {
                error!("飞书 Webhook 服务器运行错误: {}", e);
            }
        });

        {
            let mut server_handle = self.server_handle.write().await;
            *server_handle = Some(handle);
        }

        info!("飞书 Webhook 服务器已启动");
        Ok(())
    }

    /// 停止 Webhook 服务器
    pub async fn stop_server(&self) {
        let mut handle = self.server_handle.write().await;
        if let Some(h) = handle.take() {
            h.abort();
            info!("飞书 Webhook 服务器已停止");
        }
    }

    /// 获取飞书 API 客户端的引用
    pub fn client(&self) -> &FeishuClient {
        &self.client
    }

    /// 获取飞书配置的引用
    pub fn feishu_config(&self) -> &FeishuConfig {
        &self.feishu_config
    }
}

/// axum 应用状态
#[derive(Clone)]
struct AppState {
    /// 飞书 API 客户端
    client: Arc<FeishuClient>,
    /// 事件处理器
    event_handler: Arc<FeishuEventHandler>,
    /// 事件发送通道
    event_sender: Arc<RwLock<Option<mpsc::Sender<ChannelMessage>>>>,
    /// 飞书配置
    config: FeishuConfig,
}

/// 处理飞书 Webhook 回调
async fn handle_webhook(
    State(state): State<AppState>,
    headers: HeaderMap,
    body: Result<Json<Value>, axum::extract::rejection::JsonRejection>,
) -> Result<Json<Value>, StatusCode> {
    // 解析请求体
    let Json(body_value) = match body {
        Ok(b) => b,
        Err(e) => {
            error!("解析 Webhook 请求体失败: {}", e);
            return Err(StatusCode::BAD_REQUEST);
        }
    };

    debug!("收到飞书 Webhook 回调: {}", body_value);

    // 提取签名相关头信息
    let timestamp = headers
        .get("X-Lark-Request-Timestamp")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("");

    let signature = headers
        .get("X-Lark-Signature")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("");

    // 处理 webhook
    let (response, event) = match state.event_handler.handle_webhook(&body_value, timestamp, signature) {
        Ok(result) => result,
        Err(e) => {
            error!("处理 Webhook 失败: {}", e);
            return Err(StatusCode::INTERNAL_SERVER_ERROR);
        }
    };

    // 如果有事件，转换为 ChannelMessage 并发送到通道
    if let Some(feishu_event) = event {
        let channel_message = match feishu_event {
            FeishuEvent::MessageReceived {
                chat_id,
                message_id,
                sender_id,
                content,
                msg_type,
                chat_type: _,
            } => {
                // 检查用户是否在允许列表中
                if !state.config.is_user_allowed(&sender_id) {
                    warn!("用户 {} 不在允许列表中，忽略消息", sender_id);
                    return Ok(Json(json!(response)));
                }

                // 解析消息内容
                let text_content = parse_message_content(&msg_type, &content);

                ChannelMessage {
                    id: message_id,
                    channel: chat_id.clone(),
                    sender: sender_id,
                    content: text_content,
                    timestamp: Utc::now().timestamp(),
                }
            }
            FeishuEvent::MessageRead { chat_id } => ChannelMessage {
                id: uuid::Uuid::new_v4().to_string(),
                channel: chat_id,
                sender: "system".to_string(),
                content: "__message_read__".to_string(),
                timestamp: Utc::now().timestamp(),
            },
            FeishuEvent::P2pChatCreated { chat_id } => ChannelMessage {
                id: uuid::Uuid::new_v4().to_string(),
                channel: chat_id,
                sender: "system".to_string(),
                content: "__p2p_chat_created__".to_string(),
                timestamp: Utc::now().timestamp(),
            },
            FeishuEvent::BotAddedToGroup { chat_id } => ChannelMessage {
                id: uuid::Uuid::new_v4().to_string(),
                channel: chat_id,
                sender: "system".to_string(),
                content: "__bot_added_to_group__".to_string(),
                timestamp: Utc::now().timestamp(),
            },
        };

        // 发送到 mpsc channel
        let sender = state.event_sender.read().await;
        if let Some(tx) = sender.as_ref() {
            if let Err(e) = tx.send(channel_message).await {
                error!("发送事件到通道失败: {}", e);
            }
        }
    }

    Ok(Json(json!(response)))
}

/// 解析消息内容
///
/// 根据消息类型提取纯文本内容。
fn parse_message_content(msg_type: &str, content: &str) -> String {
    match msg_type {
        "text" => {
            // 文本消息格式：{"text":"消息内容"}
            if let Ok(v) = serde_json::from_str::<Value>(content) {
                return v
                    .get("text")
                    .and_then(|t| t.as_str())
                    .unwrap_or("")
                    .to_string();
            }
            content.to_string()
        }
        "post" => {
            // 富文本消息格式：{"title":"标题","content":[[{"tag":"text","text":"内容"}]]}
            if let Ok(v) = serde_json::from_str::<Value>(content) {
                // 尝试提取标题
                let title = v
                    .get("title")
                    .and_then(|t| t.as_str())
                    .unwrap_or("");

                // 尝试提取正文文本
                let body_text = v
                    .get("content")
                    .and_then(|c| extract_post_text(c));

                if !title.is_empty() && body_text.is_some() {
                    format!("{}\n{}", title, body_text.unwrap())
                } else if !title.is_empty() {
                    title.to_string()
                } else {
                    body_text.unwrap_or_else(|| content.to_string())
                }
            } else {
                content.to_string()
            }
        }
        "interactive" => {
            // 卡片消息，返回标记
            "[卡片消息]".to_string()
        }
        "image" => {
            // 图片消息
            "[图片消息]".to_string()
        }
        "file" => {
            // 文件消息
            "[文件消息]".to_string()
        }
        "audio" => {
            // 音频消息
            "[音频消息]".to_string()
        }
        "video" => {
            // 视频消息
            "[视频消息]".to_string()
        }
        "sticker" => {
            // 表情消息
            "[表情消息]".to_string()
        }
        _ => {
            // 未知消息类型
            format!("[未知消息类型: {}]", msg_type)
        }
    }
}

/// 从富文本内容中提取纯文本
///
/// 递归遍历富文本 JSON 结构，提取所有文本标签的内容。
fn extract_post_text(content: &Value) -> Option<String> {
    let mut texts = Vec::new();

    if let Some(arr) = content.as_array() {
        for item in arr {
            if let Some(inner_arr) = item.as_array() {
                for node in inner_arr {
                    if let Some(tag) = node.get("tag").and_then(|t| t.as_str()) {
                        if tag == "text" {
                            if let Some(text) = node.get("text").and_then(|t| t.as_str()) {
                                texts.push(text.to_string());
                            }
                        } else if tag == "a" {
                            if let Some(text) = node.get("text").and_then(|t| t.as_str()) {
                                let href = node
                                    .get("href")
                                    .and_then(|h| h.as_str())
                                    .unwrap_or("");
                                texts.push(format!("{}({})", text, href));
                            }
                        } else if tag == "at" {
                            if let Some(user_name) =
                                node.get("user_name").and_then(|n| n.as_str())
                            {
                                texts.push(format!("@{}", user_name));
                            }
                        }
                    }
                }
            }
        }
    }

    if texts.is_empty() {
        None
    } else {
        Some(texts.join(""))
    }
}

#[async_trait]
impl Plugin for FeishuChannel {
    /// 获取插件元数据
    fn metadata(&self) -> &PluginMetadata {
        &self.metadata
    }

    /// 初始化插件
    ///
    /// 初始化时会启动 Webhook 服务器。
    async fn initialize(&mut self, _context: PluginContext) -> Result<(), CoreError> {
        info!("初始化飞书通道插件");
        self.start_server().await?;
        Ok(())
    }

    /// 关闭插件
    ///
    /// 关闭时会停止 Webhook 服务器。
    async fn shutdown(&mut self) -> Result<(), CoreError> {
        info!("关闭飞书通道插件");
        self.stop_server().await;
        Ok(())
    }
}

#[async_trait]
impl ChannelPlugin for FeishuChannel {
    /// 发送消息
    ///
    /// 通过飞书 API 向指定聊天发送文本消息。
    async fn send(&self, message: &ChannelMessage) -> Result<(), CoreError> {
        debug!(
            "通过飞书发送消息: channel={}, content={}",
            message.channel, message.content
        );

        // 跳过系统消息
        if message.content.starts_with("__") && message.content.ends_with("__") {
            debug!("跳过系统消息: {}", message.content);
            return Ok(());
        }

        self.client
            .send_text_message(&message.channel, &message.content)
            .await?;

        Ok(())
    }

    /// 接收消息（非阻塞）
    ///
    /// 从内部 mpsc channel 中接收飞书事件转换后的消息。
    /// 如果没有消息，返回 None。
    async fn receive(&self) -> Result<Option<ChannelMessage>, CoreError> {
        let mut receiver = self.event_receiver.write().await;
        match receiver.as_mut() {
            Some(rx) => {
                // 使用 try_recv 实现非阻塞接收
                match rx.try_recv() {
                    Ok(msg) => {
                        debug!("接收到飞书消息: id={}, sender={}", msg.id, msg.sender);
                        Ok(Some(msg))
                    }
                    Err(mpsc::error::TryRecvError::Empty) => Ok(None),
                    Err(mpsc::error::TryRecvError::Disconnected) => {
                        error!("飞书事件接收通道已断开");
                        Err(CoreError::Internal("事件接收通道已断开".to_string()))
                    }
                }
            }
            None => {
                warn!("事件接收通道未初始化，请先调用 initialize()");
                Ok(None)
            }
        }
    }

    /// 获取通道配置
    fn config(&self) -> &ChannelConfig {
        &self.channel_config
    }
}
