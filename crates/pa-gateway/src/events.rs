//! 事件总线

use std::sync::Arc;
use tokio::sync::{broadcast, RwLock};
use pa_core::GatewayEvent;

/// 事件总线
pub struct EventBus {
    sender: broadcast::Sender<GatewayEvent>,
}

impl EventBus {
    pub fn new() -> Self {
        let (sender, _) = broadcast::channel(1024);
        Self { sender }
    }

    /// 发布事件
    pub fn publish(&self, event: GatewayEvent) {
        let _ = self.sender.send(event);
    }

    /// 订阅事件
    pub fn subscribe(&self) -> broadcast::Receiver<GatewayEvent> {
        self.sender.subscribe()
    }
}

impl Default for EventBus {
    fn default() -> Self {
        Self::new()
    }
}
