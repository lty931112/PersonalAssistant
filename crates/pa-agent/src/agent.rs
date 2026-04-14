//! Agent 核心实现

use std::sync::Arc;
use tokio::sync::{mpsc, RwLock};
use pa_core::*;
use pa_query::QueryEngine;
use pa_task::{TaskManager, CancellationToken, TaskSnapshot, TaskPriority};
use crate::auth_profile::AuthProfileManager;

/// Agent 状态
#[derive(Debug, Clone)]
pub enum AgentState {
    /// 空闲状态
    Idle,
    /// 正在运行
    Running { task_id: String },
    /// 已暂停（可恢复）
    Paused { task_id: String },
    /// 等待用户权限确认
    WaitingPermission { tool_name: String },
    /// 错误状态
    Error(String),
}

/// Agent 状态信息（用于外部查询）
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct AgentStatusInfo {
    /// Agent ID
    pub agent_id: String,
    /// Agent 名称
    pub agent_name: String,
    /// 当前状态
    pub state: String,
    /// 当前关联的任务 ID（如果有）
    pub current_task_id: Option<String>,
    /// 已完成的任务数
    pub completed_tasks: u64,
    /// 累计 token 消耗
    pub total_tokens: u64,
    /// 累计费用（美元）
    pub total_cost: f64,
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
    _auth_manager: Option<AuthProfileManager>,
    /// 任务管理器（用于任务生命周期管理）
    task_manager: Arc<TaskManager>,
    /// 当前任务的取消令牌
    cancel_token: Option<Arc<CancellationToken>>,
    /// 已完成任务计数
    completed_tasks: Arc<std::sync::atomic::AtomicU64>,
    /// 累计 token 消耗
    total_tokens: Arc<std::sync::atomic::AtomicU64>,
    /// 累计费用
    total_cost: Arc<std::sync::atomic::AtomicU64>, // 以分为单位存储，避免浮点原子操作
}

impl Agent {
    /// 创建新的 Agent
    pub fn new(
        config: AgentConfig,
        query_engine: QueryEngine,
        task_manager: Arc<TaskManager>,
    ) -> Self {
        Self {
            config,
            query_engine,
            state: Arc::new(RwLock::new(AgentState::Idle)),
            _auth_manager: None,
            task_manager,
            cancel_token: None,
            completed_tasks: Arc::new(std::sync::atomic::AtomicU64::new(0)),
            total_tokens: Arc::new(std::sync::atomic::AtomicU64::new(0)),
            total_cost: Arc::new(std::sync::atomic::AtomicU64::new(0)),
        }
    }

    /// 获取 Agent ID
    pub fn id(&self) -> &AgentId {
        &self.config.id
    }

    /// 展示用名称（可与山海经代号一致）
    pub fn display_name(&self) -> &str {
        self.config.name.as_str()
    }

    /// 获取当前状态
    pub async fn state(&self) -> AgentState {
        self.state.read().await.clone()
    }

    /// 基于 Agent 全局配置构造 [`QueryConfig`]（`max_tokens` 等其余字段走默认值）。
    /// Gateway 可在其上叠加「本会话人格」等临时覆盖。
    pub fn build_query_config(&self) -> pa_query::QueryConfig {
        let mut q = pa_query::QueryConfig::default();
        q.model = self.config.model.clone();
        q.max_turns = self.config.max_turns;
        q.system_prompt = self.config.system_prompt.clone();
        q.memory_enabled = self.config.memory_enabled;
        q
    }

    /// 执行查询（原始方法，保持向后兼容）
    pub async fn query(&mut self, prompt: String) -> Result<String, CoreError> {
        // 更新状态
        let task_id = uuid::Uuid::new_v4().to_string();
        *self.state.write().await = AgentState::Running { task_id: task_id.clone() };

        let config = self.build_query_config();

        let result = self.query_engine.execute(prompt, config).await;

        // 恢复空闲状态
        *self.state.write().await = AgentState::Idle;

        result
    }

    /// 带任务管理的查询方法
    ///
    /// 创建任务 -> 设置取消令牌 -> 执行查询循环 -> 每轮更新进度 -> 检查取消状态 -> 完成后更新任务状态
    pub async fn query_with_task(
        &mut self,
        prompt: String,
        config: pa_query::QueryConfig,
        task_metadata: Option<serde_json::Value>,
    ) -> String {
        // 1. 创建任务
        let task_id = self.task_manager.create_task(
            self.config.id.as_str(),
            &prompt,
            TaskPriority::Medium,
            task_metadata,
        ).await;

        tracing::info!("Agent {} 创建任务: {}", self.config.id.as_str(), task_id);

        // 2. 设置取消令牌
        if let Some(token) = self.task_manager.get_cancel_token(&task_id).await {
            self.cancel_token = Some(token);
        }

        // 3. 更新 Agent 状态为 Running
        *self.state.write().await = AgentState::Running { task_id: task_id.clone() };

        // 4. 开始任务
        if let Err(e) = self.task_manager.start_task(&task_id).await {
            tracing::error!("启动任务失败: {}", e);
            *self.state.write().await = AgentState::Error(e.to_string());
            return format!("任务启动失败: {}", e);
        }

        // 5. 执行查询（`config` 已由调用方基于 Agent 默认值并合并会话覆盖）
        let exec = self
            .query_engine
            .execute_with_usage(prompt.clone(), config)
            .await;

        // 6. 检查取消状态（在落库进度前读取）
        let cancelled = self.task_manager.is_cancelled(&task_id).await;

        // 7. 写入真实轮次与 Token（来自 QueryEngine 累计用量）
        match &exec {
            Ok((_text, turn, usage)) => {
                let _ = self
                    .task_manager
                    .update_progress(
                        &task_id,
                        *turn,
                        usage.input_tokens,
                        usage.output_tokens,
                        0.0,
                    )
                    .await;
            }
            Err(_) => {}
        }

        // 8. 完成后更新任务状态
        match exec {
            Ok((output, _, _)) => {
                if cancelled {
                    let _ = self.task_manager.cancel_task(&task_id).await;
                    tracing::info!("任务已取消: {}", task_id);
                } else {
                    let _ = self.task_manager.complete_task(&task_id).await;
                    tracing::info!("任务已完成: {}", task_id);

                    self.completed_tasks.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                }
                *self.state.write().await = AgentState::Idle;
                self.cancel_token = None;
                output
            }
            Err(e) => {
                let _ = self.task_manager.fail_task(&task_id, e.to_string()).await;
                *self.state.write().await = AgentState::Error(e.to_string());
                self.cancel_token = None;
                format!("任务执行失败: {}", e)
            }
        }
    }

    /// 暂停当前任务
    ///
    /// 保存当前任务快照并将状态切换为 Paused。
    pub async fn pause(&mut self) -> Result<(), CoreError> {
        let current_state = self.state.read().await.clone();
        let task_id = match current_state {
            AgentState::Running { task_id } => task_id,
            _ => {
                return Err(CoreError::Internal(
                    "当前没有正在运行的任务，无法暂停".to_string()
                ));
            }
        };

        tracing::info!("暂停任务: {}", task_id);

        // 创建任务快照
        let task_info = self.task_manager.get_task(&task_id).await
            .map_err(|e| CoreError::Internal(format!("获取任务信息失败: {}", e)))?;

        let snapshot = TaskSnapshot::new(
            task_info,
            "[]", // 对话历史（简化处理）
            self.config.system_prompt.clone(),
            self.config.model.clone(),
            "{}", // 查询配置（简化处理）
        );

        // 暂停任务
        self.task_manager.pause_task(&task_id, &snapshot).await?;

        // 更新 Agent 状态
        *self.state.write().await = AgentState::Paused { task_id: task_id.clone() };

        tracing::info!("任务已暂停: {}", task_id);
        Ok(())
    }

    /// 从快照恢复任务
    ///
    /// 加载最新的任务快照，恢复执行上下文并继续查询。
    pub async fn resume(&mut self) -> Result<String, CoreError> {
        let current_state = self.state.read().await.clone();
        let task_id = match current_state {
            AgentState::Paused { task_id } => task_id,
            _ => {
                return Err(CoreError::Internal(
                    "当前没有已暂停的任务，无法恢复".to_string()
                ));
            }
        };

        tracing::info!("恢复任务: {}", task_id);

        // 恢复任务（加载快照）
        let snapshot = self.task_manager.resume_task(&task_id).await?;

        // 重新设置取消令牌
        if let Some(token) = self.task_manager.get_cancel_token(&task_id).await {
            self.cancel_token = Some(token);
        }

        // 更新 Agent 状态为 Running
        *self.state.write().await = AgentState::Running { task_id: task_id.clone() };

        // 使用快照中的信息继续执行查询
        let query_config = pa_query::QueryConfig {
            model: snapshot.model.clone(),
            max_tokens: 8192,
            max_turns: self.config.max_turns,
            system_prompt: snapshot.system_prompt.clone(),
            memory_enabled: self.config.memory_enabled,
            ..Default::default()
        };

        // 从快照中获取原始 prompt 继续执行
        let prompt = snapshot.task_info.prompt.clone();
        let result = self.query_engine.execute(prompt, query_config).await;

        // 更新任务状态
        match &result {
            Ok(output) => {
                let _ = self.task_manager.complete_task(&task_id).await;
                *self.state.write().await = AgentState::Idle;
                self.cancel_token = None;
                Ok(output.clone())
            }
            Err(e) => {
                let _ = self.task_manager.fail_task(&task_id, e.to_string()).await;
                *self.state.write().await = AgentState::Error(e.to_string());
                self.cancel_token = None;
                Err(CoreError::Internal(e.to_string()))
            }
        }
    }

    /// 取消当前任务
    ///
    /// 触发取消令牌并更新任务状态为 Cancelled。
    pub async fn cancel(&mut self) -> Result<(), CoreError> {
        let current_state = self.state.read().await.clone();
        let task_id = match current_state {
            AgentState::Running { task_id } => task_id,
            AgentState::Paused { task_id } => task_id,
            _ => {
                return Err(CoreError::Internal(
                    "当前没有活跃的任务，无法取消".to_string()
                ));
            }
        };

        tracing::info!("取消任务: {}", task_id);

        // 通过 TaskManager 取消任务（会触发取消令牌）
        self.task_manager.cancel_task(&task_id).await?;

        // 清理取消令牌
        self.cancel_token = None;

        // 恢复空闲状态
        *self.state.write().await = AgentState::Idle;

        tracing::info!("任务已取消: {}", task_id);
        Ok(())
    }

    /// 获取 Agent 状态信息
    ///
    /// 返回包含详细统计信息的 AgentStatusInfo 结构。
    pub async fn get_status(&self) -> AgentStatusInfo {
        let state = self.state.read().await;
        let current_task_id = match &*state {
            AgentState::Running { task_id } => Some(task_id.clone()),
            AgentState::Paused { task_id } => Some(task_id.clone()),
            _ => None,
        };

        let state_str = match &*state {
            AgentState::Idle => "idle",
            AgentState::Running { .. } => "running",
            AgentState::Paused { .. } => "paused",
            AgentState::WaitingPermission { .. } => "waiting_permission",
            AgentState::Error(msg) => msg,
        };

        AgentStatusInfo {
            agent_id: self.config.id.as_str().to_string(),
            agent_name: self.config.name.clone(),
            state: state_str.to_string(),
            current_task_id,
            completed_tasks: self.completed_tasks.load(std::sync::atomic::Ordering::Relaxed),
            total_tokens: self.total_tokens.load(std::sync::atomic::Ordering::Relaxed),
            total_cost: self.total_cost.load(std::sync::atomic::Ordering::Relaxed) as f64 / 100.0,
        }
    }

    /// 清空历史
    pub fn clear_history(&mut self) {
        self.query_engine.clear_history();
    }

    /// 更新配置
    pub fn update_config(&mut self, config: AgentConfig) {
        self.config = config;
    }

    /// 获取任务管理器引用
    pub fn task_manager(&self) -> &Arc<TaskManager> {
        &self.task_manager
    }
}
