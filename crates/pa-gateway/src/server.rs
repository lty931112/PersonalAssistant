//! WebSocket 服务器

use std::net::SocketAddr;
use std::sync::Arc;
use tokio::sync::RwLock;
use axum::{
    Router,
    routing::get,
    extract::ws::{WebSocket, WebSocketUpgrade, Message as WsMessage},
    response::Response,
};
use tower_http::cors::CorsLayer;
use futures_util::{SinkExt, StreamExt};
use pa_core::CoreError;
use pa_config::Settings;
use crate::client::ClientRegistry;
use crate::events::EventBus;

/// Gateway 服务器
pub struct GatewayServer {
    addr: SocketAddr,
    clients: Arc<RwLock<ClientRegistry>>,
    event_bus: Arc<EventBus>,
    settings: Settings,
}

impl GatewayServer {
    pub fn new(
        addr: impl Into<String>,
        clients: Arc<RwLock<ClientRegistry>>,
        event_bus: Arc<EventBus>,
        settings: Settings,
    ) -> Self {
        Self {
            addr: addr.into().parse().expect("Invalid address"),
            clients,
            event_bus,
            settings,
        }
    }

    pub async fn run(&mut self) -> Result<(), CoreError> {
        let clients = self.clients.clone();
        let event_bus = self.event_bus.clone();
        let settings = self.settings.clone();

        let app = Router::new()
            .route("/ws", get(|ws: WebSocketUpgrade| async move {
                ws.on_upgrade(|socket| handle_socket(socket, clients, event_bus, settings))
            }))
            .route("/health", get(|| async { "OK" }))
            .layer(CorsLayer::permissive());

        let listener = tokio::net::TcpListener::bind(&self.addr)
            .await
            .map_err(|e| CoreError::io_error(e.to_string()))?;

        tracing::info!("WebSocket 端点: ws://{}/ws", self.addr);

        axum::serve(listener, app)
            .await
            .map_err(|e| CoreError::io_error(e.to_string()))?;

        Ok(())
    }
}

async fn handle_socket(
    socket: WebSocket,
    clients: Arc<RwLock<ClientRegistry>>,
    _event_bus: Arc<EventBus>,
    _settings: Settings,
) {
    let client_id = uuid::Uuid::new_v4().to_string();
    tracing::info!("客户端连接: {}", client_id);

    // 注册客户端
    clients.write().await.register(&client_id);

    // 处理消息
    let (mut sender, mut receiver) = socket.split();

    while let Some(msg) = receiver.next().await {
        match msg {
            Ok(WsMessage::Text(text)) => {
                // 处理文本消息
                tracing::debug!("收到消息: {}", text);
                // Echo back for now
                if sender.send(WsMessage::Text(text)).await.is_err() {
                    break;
                }
            }
            Ok(WsMessage::Close(_)) => {
                tracing::info!("客户端断开: {}", client_id);
                break;
            }
            _ => {}
        }
    }

    // 移除客户端
    clients.write().await.unregister(&client_id);
}
