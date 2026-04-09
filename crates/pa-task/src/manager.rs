//! 任务管理器模块
//!
//! 实现统一的任务生命周期管理，包括任务创建、状态转换、进度跟踪、
//! 中断恢复和取消控制等功能。

use std::collections::HashMap;
use std::sync::Arc;

use tokio::sync::RwLock;
use tracing::{debug, error, info, warn};

use pa_core::CoreError;

use crate::cancel_token::{CancellationToken, SharedCancellationToken};
use crate::store::TaskStore;
use crate::types::{
    TaskEvent, TaskEventType, TaskFilter, TaskInfo, TaskPriority, TaskSnapshot, TaskStatus,
};

/// 任务管理器
///
/// 统一管理任务的生命周期，提供任务创建、状态转换、进度跟踪、
/// 中断恢复和取消控制等高层接口。
///
/// 内部使用 `Arc<RwLock<HashMap<String, SharedCancellationToken>>>` 管理取消令牌，
/// 支持并发安全的任务取消操作。
pub struct TaskManager {
    /// 任务持久化存储
    store: TaskStore,
    /// 取消令牌映射表（task_id -> CancellationToken）
    cancel_tokens: Arc<RwLock<HashMap<String, SharedCancellationToken>>>,
}

impl TaskManager {
    /// 创建新的任务管理器
    ///
    /// 需要传入已初始化的 `TaskStore` 实例。
    pub fn new(store: TaskStore) -> Self {
        Self {
            store,
            cancel_tokens: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    // ========================================================================
    // 任务生命周期管理
    // ========================================================================

    /// 创建任务
    ///
    /// 创建新的任务并持久化到数据库，返回任务 ID。
    /// 同时为任务创建取消令牌。
    pub async fn create_task(
        &self,
        agent_id: impl Into<String>,
        prompt: impl Into<String>,
        priority: TaskPriority,
    ) -> String {
        let mut info = TaskInfo::new(agent_id, prompt);
        info.priority = priority;

        let task_id = info.id.clone();
        debug!("创建任务: {}", task_id);

        // 持久化到数据库
        if let Err(e) = self.store.create_task(&info).await {
            error!("持久化任务失败: {}", e);
            // 即使持久化失败也返回 ID，任务信息在内存中仍然有效
        }

        // 创建取消令牌
        {
            let mut tokens = self.cancel_tokens.write().await;
            tokens.insert(task_id.clone(), Arc::new(CancellationToken::new()));
        }

        // 记录创建事件
        let event = TaskEvent::new(&task_id, TaskEventType::Created, serde_json::json!({
            "agent_id": info.agent_id,
            "priority": info.priority.as_str(),
        }));
        if let Err(e) = self.store.save_event(&event).await {
            warn!("记录任务创建事件失败: {}", e);
        }

        info!("任务已创建: {}", task_id);
        task_id
    }

    /// 开始任务
    ///
    /// 将任务状态从 Pending 转换为 Running。
    pub async fn start_task(&self, task_id: &str) -> Result<(), CoreError> {
        debug!("开始任务: {}", task_id);

        // 验证任务存在
        let task = self.store.get_task(task_id).await?;

        if task.status != TaskStatus::Pending && task.status != TaskStatus::Paused {
            return Err(CoreError::Internal(format!(
                "无法开始任务 {}: 当前状态为 {}",
                task_id, task.status
            )));
        }

        // 更新状态
        self.store.update_task_status(task_id, TaskStatus::Running).await?;

        // 记录事件
        let event = TaskEvent::new(task_id, TaskEventType::Started, serde_json::json!({}));
        if let Err(e) = self.store.save_event(&event).await {
            warn!("记录任务开始事件失败: {}", e);
        }

        info!("任务已开始: {}", task_id);
        Ok(())
    }

    /// 暂停任务
    ///
    /// 将任务状态从 Running 转换为 Paused，并保存当前快照。
    /// 调用方应在暂停前先保存快照数据。
    pub async fn pause_task(
        &self,
        task_id: &str,
        snapshot: &TaskSnapshot,
    ) -> Result<(), CoreError> {
        debug!("暂停任务: {}", task_id);

        // 验证任务状态
        let task = self.store.get_task(task_id).await?;
        if task.status != TaskStatus::Running {
            return Err(CoreError::Internal(format!(
                "无法暂停任务 {}: 当前状态为 {}",
                task_id, task.status
            )));
        }

        // 保存快照
        self.store.save_snapshot(snapshot).await?;

        // 更新状态
        self.store.update_task_status(task_id, TaskStatus::Paused).await?;

        // 记录事件
        let event = TaskEvent::new(task_id, TaskEventType::Paused, serde_json::json!({
            "turn_count": snapshot.task_info.turn_count,
        }));
        if let Err(e) = self.store.save_event(&event).await {
            warn!("记录任务暂停事件失败: {}", e);
        }

        info!("任务已暂停: {}", task_id);
        Ok(())
    }

    /// 恢复任务
    ///
    /// 将任务状态从 Paused 转换为 Running，并加载最新的快照数据。
    /// 返回快照以便调用方恢复执行上下文。
    pub async fn resume_task(&self, task_id: &str) -> Result<TaskSnapshot, CoreError> {
        debug!("恢复任务: {}", task_id);

        // 验证任务状态
        let task = self.store.get_task(task_id).await?;
        if task.status != TaskStatus::Paused {
            return Err(CoreError::Internal(format!(
                "无法恢复任务 {}: 当前状态为 {}",
                task_id, task.status
            )));
        }

        // 加载最新快照
        let snapshot = self.store.load_snapshot(task_id).await?;

        // 更新状态
        self.store.update_task_status(task_id, TaskStatus::Running).await?;

        // 记录事件
        let event = TaskEvent::new(task_id, TaskEventType::Resumed, serde_json::json!({
            "turn_count": snapshot.task_info.turn_count,
        }));
        if let Err(e) = self.store.save_event(&event).await {
            warn!("记录任务恢复事件失败: {}", e);
        }

        info!("任务已恢复: {}", task_id);
        Ok(snapshot)
    }

    /// 取消任务
    ///
    /// 将任务状态转换为 Cancelled，并触发取消令牌通知正在执行的任务。
    pub async fn cancel_task(&self, task_id: &str) -> Result<(), CoreError> {
        debug!("取消任务: {}", task_id);

        // 验证任务存在
        let task = self.store.get_task(task_id).await?;
        if task.status.is_terminal() {
            return Err(CoreError::Internal(format!(
                "无法取消任务 {}: 当前状态为 {}（终态）",
                task_id, task.status
            )));
        }

        // 触发取消令牌
        {
            let tokens = self.cancel_tokens.read().await;
            if let Some(token) = tokens.get(task_id) {
                token.cancel();
                debug!("已触发取消令牌: {}", task_id);
            }
        }

        // 更新状态
        self.store.update_task_status(task_id, TaskStatus::Cancelled).await?;

        // 记录事件
        let event = TaskEvent::new(task_id, TaskEventType::Cancelled, serde_json::json!({
            "previous_status": task.status.as_str(),
        }));
        if let Err(e) = self.store.save_event(&event).await {
            warn!("记录任务取消事件失败: {}", e);
        }

        info!("任务已取消: {}", task_id);
        Ok(())
    }

    /// 完成任务
    ///
    /// 将任务状态转换为 Completed。
    pub async fn complete_task(&self, task_id: &str) -> Result<(), CoreError> {
        debug!("完成任务: {}", task_id);

        // 验证任务状态
        let task = self.store.get_task(task_id).await?;
        if task.status != TaskStatus::Running {
            return Err(CoreError::Internal(format!(
                "无法完成任务 {}: 当前状态为 {}",
                task_id, task.status
            )));
        }

        // 更新状态
        self.store.update_task_status(task_id, TaskStatus::Completed).await?;

        // 清理取消令牌
        {
            let mut tokens = self.cancel_tokens.write().await;
            tokens.remove(task_id);
        }

        // 记录事件
        let event = TaskEvent::new(task_id, TaskEventType::Completed, serde_json::json!({
            "turn_count": task.turn_count,
            "total_tokens": task.total_tokens(),
            "cost_usd": task.cost_usd,
        }));
        if let Err(e) = self.store.save_event(&event).await {
            warn!("记录任务完成事件失败: {}", e);
        }

        info!("任务已完成: {}", task_id);
        Ok(())
    }

    /// 标记任务失败
    ///
    /// 将任务状态转换为 Failed，并记录错误信息。
    pub async fn fail_task(
        &self,
        task_id: &str,
        error: impl Into<String>,
    ) -> Result<(), CoreError> {
        let error_msg = error.into();
        debug!("标记任务失败: {} - {}", task_id, error_msg);

        // 验证任务状态
        let task = self.store.get_task(task_id).await?;
        if task.status != TaskStatus::Running {
            return Err(CoreError::Internal(format!(
                "无法标记任务失败 {}: 当前状态为 {}",
                task_id, task.status
            )));
        }

        // 更新状态
        self.store.update_task_status(task_id, TaskStatus::Failed).await?;
        self.store.update_task_error(task_id, &error_msg).await?;

        // 清理取消令牌
        {
            let mut tokens = self.cancel_tokens.write().await;
            tokens.remove(task_id);
        }

        // 记录事件
        let event = TaskEvent::new(task_id, TaskEventType::Failed, serde_json::json!({
            "error": error_msg,
            "turn_count": task.turn_count,
            "total_tokens": task.total_tokens(),
            "cost_usd": task.cost_usd,
        }));
        if let Err(e) = self.store.save_event(&event).await {
            warn!("记录任务失败事件失败: {}", e);
        }

        error!("任务失败: {} - {}", task_id, error_msg);
        Ok(())
    }

    // ========================================================================
    // 进度跟踪
    // ========================================================================

    /// 更新任务进度
    ///
    /// 更新任务的轮次计数、token 消耗和费用信息。
    pub async fn update_progress(
        &self,
        task_id: &str,
        turn_count: u32,
        input_tokens: u32,
        output_tokens: u32,
        cost: f64,
    ) -> Result<(), CoreError> {
        debug!(
            "更新任务进度: {} (轮次: {}, 输入: {}, 输出: {}, 费用: ${:.4})",
            task_id, turn_count, input_tokens, output_tokens, cost
        );

        self.store
            .update_task_progress(task_id, turn_count, input_tokens, output_tokens, cost)
            .await?;

        Ok(())
    }

    /// 记录任务事件
    ///
    /// 记录自定义任务事件，用于审计和调试。
    pub async fn record_event(
        &self,
        task_id: &str,
        event_type: TaskEventType,
        data: serde_json::Value,
    ) -> Result<(), CoreError> {
        let event = TaskEvent::new(task_id, event_type, data);
        self.store.save_event(&event).await?;
        Ok(())
    }

    // ========================================================================
    // 查询操作
    // ========================================================================

    /// 获取任务信息
    pub async fn get_task(&self, task_id: &str) -> Result<TaskInfo, CoreError> {
        self.store.get_task(task_id).await
    }

    /// 列出正在运行的任务
    pub async fn list_running_tasks(&self) -> Vec<TaskInfo> {
        let filter = TaskFilter::new()
            .with_status(TaskStatus::Running)
            .with_order_desc(true);

        match self.store.list_tasks(&filter).await {
            Ok(tasks) => tasks,
            Err(e) => {
                error!("查询运行中任务失败: {}", e);
                Vec::new()
            }
        }
    }

    /// 列出所有任务
    pub async fn list_all_tasks(&self) -> Vec<TaskInfo> {
        let filter = TaskFilter::new().with_order_desc(true);

        match self.store.list_tasks(&filter).await {
            Ok(tasks) => tasks,
            Err(e) => {
                error!("查询所有任务失败: {}", e);
                Vec::new()
            }
        }
    }

    /// 列出任务（带过滤条件）
    pub async fn list_tasks(&self, filter: &TaskFilter) -> Result<Vec<TaskInfo>, CoreError> {
        self.store.list_tasks(filter).await
    }

    /// 获取任务事件列表
    pub async fn get_task_events(&self, task_id: &str) -> Vec<TaskEvent> {
        match self.store.list_events(task_id, Some(100)).await {
            Ok(events) => events,
            Err(e) => {
                error!("查询任务事件失败: {}", e);
                Vec::new()
            }
        }
    }

    /// 获取任务事件列表（带数量限制）
    pub async fn get_task_events_with_limit(
        &self,
        task_id: &str,
        limit: usize,
    ) -> Vec<TaskEvent> {
        match self.store.list_events(task_id, Some(limit)).await {
            Ok(events) => events,
            Err(e) => {
                error!("查询任务事件失败: {}", e);
                Vec::new()
            }
        }
    }

    // ========================================================================
    // 取消控制
    // ========================================================================

    /// 检查任务是否已被取消
    pub async fn is_cancelled(&self, task_id: &str) -> bool {
        let tokens = self.cancel_tokens.read().await;
        if let Some(token) = tokens.get(task_id) {
            token.is_cancelled()
        } else {
            // 如果令牌不存在，说明任务已经结束
            true
        }
    }

    /// 获取任务的取消令牌
    ///
    /// 返回取消令牌的共享引用，可用于异步等待取消信号。
    /// 如果任务不存在，返回 None。
    pub async fn get_cancel_token(&self, task_id: &str) -> Option<SharedCancellationToken> {
        let tokens = self.cancel_tokens.read().await;
        tokens.get(task_id).cloned()
    }

    // ========================================================================
    // 快照操作
    // ========================================================================

    /// 保存任务快照
    pub async fn save_snapshot(&self, snapshot: &TaskSnapshot) -> Result<(), CoreError> {
        self.store.save_snapshot(snapshot).await
    }

    /// 加载任务快照
    pub async fn load_snapshot(&self, task_id: &str) -> Result<TaskSnapshot, CoreError> {
        self.store.load_snapshot(task_id).await
    }

    // ========================================================================
    // 清理操作
    // ========================================================================

    /// 删除任务
    pub async fn delete_task(&self, task_id: &str) -> Result<(), CoreError> {
        // 清理取消令牌
        {
            let mut tokens = self.cancel_tokens.write().await;
            tokens.remove(task_id);
        }

        self.store.delete_task(task_id).await
    }

    /// 清理旧任务
    ///
    /// 删除指定天数之前的已完成/失败/取消的任务。
    pub async fn cleanup_old_tasks(&self, days: i64) -> Result<usize, CoreError> {
        self.store.cleanup_old_tasks(days).await
    }

    /// 获取内部存储引用
    ///
    /// 提供对底层 `TaskStore` 的直接访问，用于需要直接操作数据库的场景。
    pub fn store(&self) -> &TaskStore {
        &self.store
    }
}
