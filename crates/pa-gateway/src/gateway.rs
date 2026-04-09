//! Gateway 主入口

use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;
use pa_core::{CoreError, AgentId};
use pa_config::Settings;
use crate::server::GatewayServer;
use crate::client::ClientRegistry;
use crate::events::EventBus;

/// Gateway 实例
pub struct Gateway {
    settings: Settings,
    server: Option<GatewayServer>,
    clients: Arc<RwLock<ClientRegistry>>,
    event_bus: Arc<EventBus>,
    agents: HashMap<String, AgentId>,
}

impl Gateway {
    /// 创建新的 Gateway
    pub async fn new(settings: Settings) -> Result<Self, CoreError> {
        Ok(Self {
            settings,
            server: None,
            clients: Arc::new(RwLock::new(ClientRegistry::new())),
            event_bus: Arc::new(EventBus::new()),
            agents: HashMap::new(),
        })
    }

    /// 启动 Gateway
    pub async fn start(&mut self) -> Result<(), CoreError> {
        let addr = format!("{}:{}", self.settings.gateway.bind, self.settings.gateway.port);
        tracing::info!("🚀 Gateway 启动于 {}", addr);

        let server = GatewayServer::new(
            addr,
            self.clients.clone(),
            self.event_bus.clone(),
            self.settings.clone(),
        );

        self.server = Some(server);
        
        // 阻塞运行
        if let Some(ref mut server) = self.server {
            server.run().await?;
        }

        Ok(())
    }

    /// 注册 Agent
    pub fn register_agent(&mut self, name: &str, id: AgentId) {
        self.agents.insert(name.to_string(), id);
    }

    /// 获取 Agent
    pub fn get_agent(&self, name: &str) -> Option<&AgentId> {
        self.agents.get(name)
    }
}
