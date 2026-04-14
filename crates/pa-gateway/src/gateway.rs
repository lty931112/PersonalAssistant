//! Gateway 主入口

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use tokio::sync::RwLock;
use pa_core::{CoreError, AgentId};
use pa_config::Settings;
use pa_task::{TaskManager, TaskStore};
use pa_agent::Agent;
use pa_query::SharedApprovalBroker;
use crate::server::GatewayServer;
use crate::client::ClientRegistry;
use crate::events::EventBus;
use crate::alert::AlertManager;
use crate::watchdog::{Watchdog, WatchdogConfig};

/// Gateway 实例
pub struct Gateway {
    settings: Settings,
    server: Option<GatewayServer>,
    clients: Arc<RwLock<ClientRegistry>>,
    event_bus: Arc<EventBus>,
    agents: HashMap<String, AgentId>,
    /// 任务管理器（统一管理任务生命周期）
    task_manager: Arc<TaskManager>,
    /// Agent 实例映射表（并发安全的 Agent 管理）
    agents_map: Arc<RwLock<HashMap<String, Arc<RwLock<Agent>>>>>,
    /// 告警管理器
    alert_manager: Option<Arc<AlertManager>>,
    /// Watchdog 配置
    watchdog_config: Option<WatchdogConfig>,
    /// 工具调用人工批准（与 Agent 内 QueryEngine 共用）
    approval_broker: Option<Arc<SharedApprovalBroker>>,
}

impl Gateway {
    /// 创建新的 Gateway
    ///
    /// 初始化 TaskManager、ClientRegistry、EventBus 等核心组件。
    pub async fn new(settings: Settings) -> Result<Self, CoreError> {
        // 初始化任务存储（使用内存数据库 ":memory:" 用于开发）
        let task_store = TaskStore::new(":memory:").await?;
        task_store.init().await?;

        // 创建任务管理器
        let task_manager = Arc::new(TaskManager::new(task_store));

        tracing::info!("任务管理器已初始化");

        Ok(Self {
            settings,
            server: None,
            clients: Arc::new(RwLock::new(ClientRegistry::new())),
            event_bus: Arc::new(EventBus::new()),
            agents: HashMap::new(),
            task_manager,
            agents_map: Arc::new(RwLock::new(HashMap::new())),
            alert_manager: None,
            watchdog_config: None,
            approval_broker: None,
        })
    }

    /// 使用自定义 TaskStore 创建 Gateway
    pub async fn with_task_store(
        settings: Settings,
        task_store: TaskStore,
    ) -> Result<Self, CoreError> {
        // 初始化任务存储
        task_store.init().await?;

        let task_manager = Arc::new(TaskManager::new(task_store));

        tracing::info!("任务管理器已初始化（自定义存储）");

        Ok(Self {
            settings,
            server: None,
            clients: Arc::new(RwLock::new(ClientRegistry::new())),
            event_bus: Arc::new(EventBus::new()),
            agents: HashMap::new(),
            task_manager,
            agents_map: Arc::new(RwLock::new(HashMap::new())),
            alert_manager: None,
            watchdog_config: None,
            approval_broker: None,
        })
    }

    /// 注入共享批准 Broker（须与 QueryEngine 为同一实例）
    pub fn with_approval_broker(mut self, broker: Arc<SharedApprovalBroker>) -> Self {
        self.approval_broker = Some(broker);
        self
    }

    /// 设置 Watchdog 配置
    pub fn with_watchdog_config(mut self, config: WatchdogConfig) -> Self {
        self.watchdog_config = Some(config);
        self
    }

    /// 设置告警管理器
    pub fn with_alert_manager(mut self, manager: AlertManager) -> Self {
        self.alert_manager = Some(Arc::new(manager));
        self
    }

    /// 获取告警管理器引用
    pub fn alert_manager(&self) -> Option<&Arc<AlertManager>> {
        self.alert_manager.as_ref()
    }

    /// 启动 Gateway
    pub async fn start(&mut self) -> Result<(), CoreError> {
        let addr = format!("{}:{}", self.settings.gateway.bind, self.settings.gateway.port);
        tracing::info!("Gateway 启动于 {}", addr);

        let cwd = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
        let persona = Arc::new(PersonaRuntime::load(&cwd, &self.settings.persona));

        let server = GatewayServer::new(
            addr,
            self.clients.clone(),
            self.event_bus.clone(),
            self.settings.clone(),
            self.task_manager.clone(),
            self.agents_map.clone(),
            self.approval_broker.clone(),
            persona,
        );

        self.server = Some(server);

        // 启动 Watchdog
        if let Some(ref config) = self.watchdog_config {
            if config.enabled {
                let watchdog = Watchdog::new(
                    config.clone(),
                    self.task_manager.clone(),
                    self.agents_map.clone(),
                );
                watchdog.spawn();
            }
        }

        // 阻塞运行
        if let Some(ref mut server) = self.server {
            server.run().await?;
        }

        Ok(())
    }

    /// 注册 Agent（名称 -> ID 映射）
    pub fn register_agent(&mut self, name: &str, id: AgentId) {
        self.agents.insert(name.to_string(), id);
    }

    /// 获取 Agent（名称 -> ID 映射）
    pub fn get_agent(&self, name: &str) -> Option<&AgentId> {
        self.agents.get(name)
    }

    /// 获取任务管理器引用
    pub fn task_manager(&self) -> &Arc<TaskManager> {
        &self.task_manager
    }

    /// 注册 Agent 实例
    ///
    /// 将 Agent 实例注册到 Gateway 的 Agent 映射表中，
    /// 使其可以通过 HTTP API 和 WebSocket 进行管理。
    pub async fn register_agent_instance(&self, agent: Agent) {
        let agent_id = agent.id().as_str().to_string();
        tracing::info!("注册 Agent 实例: {}", agent_id);
        self.agents_map.write().await.insert(agent_id.clone(), Arc::new(RwLock::new(agent)));

        // 发布配置更新事件
        self.event_bus.publish(pa_core::GatewayEvent::ConfigUpdated {
            key: format!("agent_registered:{}", agent_id),
        });
    }

    /// 获取 Agent 实例
    ///
    /// 根据 Agent ID 获取对应的 Agent 实例引用。
    pub async fn get_agent_instance(&self, id: &str) -> Option<Arc<RwLock<Agent>>> {
        self.agents_map.read().await.get(id).cloned()
    }

    /// 获取所有已注册的 Agent ID 列表
    pub async fn list_agent_instances(&self) -> Vec<String> {
        self.agents_map.read().await.keys().cloned().collect()
    }

    /// 获取 Agent 映射表的共享引用
    ///
    /// 用于将 Agent 映射表传递给其他组件。
    pub fn agents_map(&self) -> Arc<RwLock<HashMap<String, Arc<RwLock<Agent>>>>> {
        self.agents_map.clone()
    }
}
