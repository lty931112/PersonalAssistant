//! Prometheus 指标模块
//!
//! 提供系统资源监控指标采集和 Prometheus 格式输出。
//! 采集的指标包括：
//! - 进程级：CPU 使用率、内存占用、线程数、文件描述符数
//! - 系统级：CPU 总体使用率、内存总量/可用量
//! - 业务级：活跃任务数、已完成任务数、Agent 状态、WebSocket 连接数

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::Instant;
use sysinfo::{System, Pid};
use tokio::sync::RwLock;
use tracing::debug;

/// 指标收集器
#[derive(Clone)]
pub struct MetricsCollector {
    /// 进程启动时间
    start_time: Instant,
    /// 系统信息采集器
    system: Arc<tokio::sync::Mutex<System>>,
    /// 请求总数
    total_requests: Arc<AtomicU64>,
    /// 活跃连接数
    active_connections: Arc<AtomicU64>,
    /// 任务完成总数
    tasks_completed: Arc<AtomicU64>,
    /// 任务失败总数
    tasks_failed: Arc<AtomicU64>,
    /// 当前运行任务数
    tasks_running: Arc<AtomicU64>,
}

impl MetricsCollector {
    /// 创建新的指标收集器
    pub fn new() -> Self {
        Self {
            start_time: Instant::now(),
            system: Arc::new(tokio::sync::Mutex::new(System::new())),
            total_requests: Arc::new(AtomicU64::new(0)),
            active_connections: Arc::new(AtomicU64::new(0)),
            tasks_completed: Arc::new(AtomicU64::new(0)),
            tasks_failed: Arc::new(AtomicU64::new(0)),
            tasks_running: Arc::new(AtomicU64::new(0)),
        }
    }

    /// 增加请求计数
    pub fn inc_requests(&self) {
        self.total_requests.fetch_add(1, Ordering::Relaxed);
    }

    /// 增加活跃连接数
    pub fn inc_connections(&self) {
        self.active_connections.fetch_add(1, Ordering::Relaxed);
    }

    /// 减少活跃连接数
    pub fn dec_connections(&self) {
        self.active_connections.fetch_sub(1, Ordering::Relaxed);
    }

    /// 增加完成任务数
    pub fn inc_tasks_completed(&self) {
        self.tasks_completed.fetch_add(1, Ordering::Relaxed);
    }

    /// 增加失败任务数
    pub fn inc_tasks_failed(&self) {
        self.tasks_failed.fetch_add(1, Ordering::Relaxed);
    }

    /// 设置运行中任务数
    pub fn set_tasks_running(&self, count: u64) {
        self.tasks_running.store(count, Ordering::Relaxed);
    }

    /// 生成 Prometheus 格式的指标输出
    pub async fn render_prometheus(&self) -> String {
        let mut system = self.system.lock().await;
        system.refresh_all();
        system.refresh_cpu_usage();

        let mut output = String::new();

        // 辅助函数
        let mut push = |line: &str| {
            output.push_str(line);
            output.push('\n');
        };

        let uptime_secs = self.start_time.elapsed().as_secs_f64();

        // ===== 进程级指标 =====
        push("# HELP pa_process_uptime_seconds 进程运行时间（秒）");
        push("# TYPE pa_process_uptime_seconds gauge");
        push(&format!("pa_process_uptime_seconds {:.2}", uptime_secs));

        push("# HELP pa_process_requests_total 请求总数");
        push("# TYPE pa_process_requests_total counter");
        push(&format!("pa_process_requests_total {}", self.total_requests.load(Ordering::Relaxed)));

        push("# HELP pa_process_active_connections 当前活跃连接数");
        push("# TYPE pa_process_active_connections gauge");
        push(&format!("pa_process_active_connections {}", self.active_connections.load(Ordering::Relaxed)));

        // ===== 任务指标 =====
        push("# HELP pa_tasks_completed_total 已完成任务总数");
        push("# TYPE pa_tasks_completed_total counter");
        push(&format!("pa_tasks_completed_total {}", self.tasks_completed.load(Ordering::Relaxed)));

        push("# HELP pa_tasks_failed_total 失败任务总数");
        push("# TYPE pa_tasks_failed_total counter");
        push(&format!("pa_tasks_failed_total {}", self.tasks_failed.load(Ordering::Relaxed)));

        push("# HELP pa_tasks_running 当前运行中任务数");
        push("# TYPE pa_tasks_running gauge");
        push(&format!("pa_tasks_running {}", self.tasks_running.load(Ordering::Relaxed)));

        // ===== 系统级指标 =====
        push("# HELP pa_system_cpu_usage 系统总体 CPU 使用率（0-100）");
        push("# TYPE pa_system_cpu_usage gauge");
        push(&format!("pa_system_cpu_usage {:.2}", system.global_cpu_usage()));

        push("# HELP pa_system_memory_total_bytes 系统总内存（字节）");
        push("# TYPE pa_system_memory_total_bytes gauge");
        push(&format!("pa_system_memory_total_bytes {}", system.total_memory()));

        push("# HELP pa_system_memory_available_bytes 系统可用内存（字节）");
        push("# TYPE pa_system_memory_available_bytes gauge");
        push(&format!("pa_system_memory_available_bytes {}", system.available_memory()));

        push("# HELP pa_system_memory_used_bytes 系统已用内存（字节）");
        push("# TYPE pa_system_memory_used_bytes gauge");
        push(&format!("pa_system_memory_used_bytes {}", system.total_memory() - system.available_memory()));

        // ===== 进程内存指标 =====
        if let Some(process) = system.process(Pid::from_u32(std::process::id())) {
            push("# HELP pa_process_memory_bytes 进程内存占用（字节）");
            push("# TYPE pa_process_memory_bytes gauge");
            push(&format!("pa_process_memory_bytes {}", process.memory()));

            push("# HELP pa_process_cpu_usage 进程 CPU 使用率（0-100）");
            push("# TYPE pa_process_cpu_usage gauge");
            push(&format!("pa_process_cpu_usage {:.2}", process.cpu_usage()));

            push("# HELP pa_process_threads 进程线程数");
            push("# TYPE pa_process_threads gauge");
            push(&format!("pa_process_threads {}", process.threads().len()));

            push("# HELP pa_process_open_fds 进程打开的文件描述符数");
            push("# TYPE pa_process_open_fds gauge");
            push(&format!("pa_process_open_fds {}", process.open_files().len()));
        }

        output
    }
}

impl Default for MetricsCollector {
    fn default() -> Self {
        Self::new()
    }
}
