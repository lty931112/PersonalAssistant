//! Gateway 控制平面模块
//!
//! 实现统一的 WebSocket 控制平面，作为所有客户端、通道、工具和事件的中心枢纽。
//! 参考 OpenClaw 的 Gateway 架构设计。

pub mod gateway;
pub mod log_broadcast;
pub mod server;
pub mod protocol;
pub mod client;
pub mod auth;
pub mod events;
pub mod watchdog;
pub mod metrics;
pub mod alert;

pub use gateway::Gateway;
pub use log_broadcast::LogBroadcast;
pub use server::{GatewayServer, AppState};
pub use protocol::{Message as ProtocolMessage, MethodCall, MethodResponse};
pub use client::ClientConnection;
pub use auth::{
    extract_gateway_credential, gateway_auth_enabled, verify_gateway_credential, Authenticator,
};
pub use events::EventBus;
pub use watchdog::{Watchdog, WatchdogConfig};
pub use metrics::MetricsCollector;
pub use alert::{AlertManager, AlertLevel};
