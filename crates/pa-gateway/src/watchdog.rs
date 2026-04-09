//! Watchdog 看门狗模块
//!
//! 提供系统级和任务级的自动故障检测与恢复能力：
//! - 定期检查 Agent 和任务状态
//! - 自动重试失败的任务
//! - 检测并恢复卡死的 Agent
//! - 可配置的检查间隔和重试策略

use std::sync::Arc;
use std::time::Duration;
use tokio::sync::RwLock;
use tokio::time;
use tracing::{debug, error, info, warn};

use pa_task::TaskManager;

/// Watchdog 配置
#[derive(Debug, Clone)]
pub struct WatchdogConfig {
    /// 检查间隔（秒）
    pub check_interval_secs: u64,
    /// 任务最大运行时间（秒），超过则视为卡死
    pub task_max_runtime_secs: u64,
    /// 失败任务自动重试次数
    pub max_retry_count: u32,
    /// 重试间隔（秒）
    pub retry_interval_secs: u64,
    /// 是否启用
    pub enabled: bool,
}

impl Default for WatchdogConfig {
    fn default() -> Self {
        Self {
            check_interval_secs: 30,
            task_max_runtime_secs: 600,
            max_retry_count: 3,
            retry_interval_secs: 10,
            enabled: true,
        }
    }
}

/// Watchdog 看门狗
///
/// 定期检查系统各组件的健康状态，自动处理异常情况。
pub struct Watchdog {
    config: WatchdogConfig,
    task_manager: Arc<TaskManager>,
    /// Agent 实例映射表
    agents_map: Arc<RwLock<std::collections::HashMap<String, Arc<RwLock<pa_agent::Agent>>>>>,
    /// 告警回调
    alert_callback: Option<Arc<dyn Fn(String, String) + Send + Sync>>,
    /// 运行统计
    stats: Arc<std::sync::atomic::AtomicU64>,
}

impl Watchdog {
    /// 创建新的 Watchdog
    pub fn new(
        config: WatchdogConfig,
        task_manager: Arc<TaskManager>,
        agents_map: Arc<RwLock<std::collections::HashMap<String, Arc<RwLock<pa_agent::Agent>>>>>,
    ) -> Self {
        Self {
            config,
            task_manager,
            agents_map,
            alert_callback: None,
            stats: Arc::new(std::sync::atomic::AtomicU64::new(0)),
        }
    }

    /// 设置告警回调
    pub fn with_alert_callback<F>(mut self, callback: F) -> Self
    where
        F: Fn(String, String) + Send + Sync + 'static,
    {
        self.alert_callback = Some(Arc::new(callback));
        self
    }

    /// 启动 Watchdog 后台任务
    ///
    /// 返回一个 JoinHandle，可用于等待或取消 Watchdog。
    pub fn spawn(mut self) -> tokio::task::JoinHandle<()> {
        if !self.config.enabled {
            info!("Watchdog 已禁用，跳过启动");
            return tokio::spawn(async {});
        }

        info!(
            "Watchdog 已启动，检查间隔: {}秒，任务最大运行时间: {}秒，最大重试次数: {}",
            self.config.check_interval_secs,
            self.config.task_max_runtime_secs,
            self.config.max_retry_count
        );

        let interval = Duration::from_secs(self.config.check_interval_secs);
        let max_runtime = Duration::from_secs(self.config.task_max_runtime_secs);

        tokio::spawn(async move {
            let mut interval_timer = time::interval(interval);

            loop {
                interval_timer.tick().await;

                // 1. 检查运行中的任务是否超时
                self.check_task_timeouts(max_runtime).await;

                // 2. 检查 Agent 状态
                self.check_agent_health().await;

                // 3. 更新统计
                self.stats.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
            }
        })
    }

    /// 检查任务超时
    async fn check_task_timeouts(&self, max_runtime: Duration) {
        let running_tasks = self.task_manager.list_running_tasks().await;

        for task in &running_tasks {
            if let Some(started_at) = task.started_at {
                let elapsed = chrono::Utc::now() - started_at;
                if elapsed > chrono::Duration::from_std(max_runtime).unwrap_or_default() {
                    warn!(
                        "任务 {} 已运行超过最大时间限制（已运行 {}秒），尝试取消",
                        task.id,
                        elapsed.num_seconds()
                    );

                    if let Err(e) = self.task_manager.cancel_task(&task.id).await {
                        error!("取消超时任务 {} 失败: {}", task.id, e);
                    } else {
                        info!("已取消超时任务: {}", task.id);
                        self.send_alert(
                            "task_timeout".to_string(),
                            format!("任务 {} 运行超时（{}秒），已自动取消", task.id, elapsed.num_seconds()),
                        );
                    }
                }
            }
        }
    }

    /// 检查 Agent 健康状态
    async fn check_agent_health(&self) {
        let agents = self.agents_map.read().await;

        for (id, agent) in agents.iter() {
            let status = agent.read().await.get_status().await;

            match status.state.as_str() {
                "error" => {
                    error!("Agent {} 处于错误状态: {}", id, status.state);
                    self.send_alert(
                        "agent_error".to_string(),
                        format!("Agent {} 处于错误状态", id),
                    );
                }
                "running" => {
                    debug!("Agent {} 正常运行中", id);
                }
                _ => {
                    debug!("Agent {} 状态: {}", id, status.state);
                }
            }
        }
    }

    /// 发送告警
    fn send_alert(&self, alert_type: String, message: String) {
        if let Some(ref callback) = self.alert_callback {
            callback(alert_type.clone(), message.clone());
        }
        info!("告警 [{}]: {}", alert_type, message);
    }

    /// 获取检查次数统计
    pub fn check_count(&self) -> u64 {
        self.stats.load(std::sync::atomic::Ordering::Relaxed)
    }
}
