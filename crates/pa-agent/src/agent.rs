//! Agent 核心实现

use std::sync::Arc;
use tokio::sync::{mpsc, RwLock};
use pa_core::*;
use pa_query::QueryEngine;
use pa_memory::MagmaMemoryEngine;
use pa_tools::ToolRegistry;
use crate::auth_profile::AuthProfileManager;

/// Agent 状态
#[derive(Debug, Clone)]
pub enum AgentState {
    Idle,
    Running { task_id: String },
    WaitingPermission { tool_name: String },
    Error(String),
}

/// Agent 句柄（用于远程控制）
pub struct AgentHandle {
    pub id: AgentId,
    pub command_tx: mpsc::Sender<AgentCommand>,
    pub event_rx: mpsc::Receiver<AgentEvent>,
}

/// Agent 命令
pub enum AgentCommand {
    Query { prompt: String, config: pa_query::QueryConfig },
    Stop,
    ClearHistory,
    UpdateConfig { config: AgentConfig },
}

/// Agent 事件
#[derive(Debug, Clone)]
pub enum AgentEvent {
    Started { task_id: String },
    QueryEvent(pa_core::QueryEvent),
    Completed { result: String },
    Error(String),
    StateChanged(AgentState),
}

/// Agent 实例
pub struct Agent {
    config: AgentConfig,
    query_engine: QueryEngine,
    state: Arc<RwLock<AgentState>>,
    auth_manager: Option<AuthProfileManager>,
}

impl Agent {
    /// 创建新的 Agent
    pub fn new(
        config: AgentConfig,
        query_engine: QueryEngine,
    ) -> Self {
        Self {
            config,
            query_engine,
            state: Arc::new(RwLock::new(AgentState::Idle)),
            auth_manager: None,
        }
    }

    /// 获取 Agent ID
    pub fn id(&self) -> &AgentId {
        &self.config.id
    }

    /// 获取当前状态
    pub async fn state(&self) -> AgentState {
        self.state.read().await.clone()
    }

    /// 执行查询
    pub async fn query(&mut self, prompt: String) -> Result<String, CoreError> {
        // 更新状态
        let task_id = uuid::Uuid::new_v4().to_string();
        *self.state.write().await = AgentState::Running { task_id: task_id.clone() };

        let config = pa_query::QueryConfig {
            model: self.config.model.clone(),
            max_tokens: 8192,
            max_turns: self.config.max_turns,
            system_prompt: self.config.system_prompt.clone(),
            memory_enabled: self.config.memory_enabled,
            ..Default::default()
        };

        let result = self.query_engine.execute(prompt, config).await;

        // 恢复空闲状态
        *self.state.write().await = AgentState::Idle;

        result
    }

    /// 清空历史
    pub fn clear_history(&mut self) {
        self.query_engine.clear_history();
    }

    /// 更新配置
    pub fn update_config(&mut self, config: AgentConfig) {
        self.config = config;
    }
}
