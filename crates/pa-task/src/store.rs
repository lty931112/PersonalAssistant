//! SQLite 持久化存储模块
//!
//! 实现基于 SQLite 的任务持久化存储，使用 `tokio-rusqlite` 提供异步接口。
//! 支持任务信息、任务快照和任务事件的 CRUD 操作。

use std::path::Path;

use chrono::{DateTime, Utc};
use tokio_rusqlite::Connection;
use tracing::{debug, info, warn};

use pa_core::CoreError;

use crate::types::{
    TaskEvent, TaskEventType, TaskFilter, TaskInfo, TaskPriority, TaskSnapshot, TaskStatus,
};

// ============================================================================
// 辅助函数：解析 RFC3339 时间字符串
// ============================================================================

/// 解析 RFC3339 格式的时间字符串为 DateTime<Utc>
fn parse_datetime(s: &str) -> DateTime<Utc> {
    chrono::DateTime::parse_from_rfc3339(s)
        .map(|dt| dt.with_timezone(&Utc))
        .unwrap_or_else(|_| Utc::now())
}

/// 解析可选的 RFC3339 格式的时间字符串
fn parse_datetime_opt(s: Option<String>) -> Option<DateTime<Utc>> {
    s.and_then(|ref val| {
        chrono::DateTime::parse_from_rfc3339(val)
            .map(|dt| dt.with_timezone(&Utc))
            .ok()
    })
}

// ============================================================================
// TaskStore
// ============================================================================

/// 任务存储
///
/// 基于 SQLite 的异步任务持久化存储，管理任务信息、快照和事件数据。
pub struct TaskStore {
    /// SQLite 异步连接
    conn: Connection,
}

impl TaskStore {
    /// 打开或创建数据库
    ///
    /// 如果指定的数据库文件不存在，会自动创建。
    /// 调用此方法后需要调用 `init()` 来初始化表结构。
    pub async fn new<P: AsRef<Path>>(db_path: P) -> Result<Self, CoreError> {
        let path = db_path.as_ref().to_string_lossy().to_string();
        info!("打开任务数据库: {}", path);

        let conn = Connection::open(&path)
            .await
            .map_err(|e| CoreError::IoError(format!("无法打开数据库 {}: {}", path, e)))?;

        // 启用 WAL 模式以提升并发性能
        conn.call(|conn| {
            conn.execute_batch("PRAGMA journal_mode=WAL; PRAGMA synchronous=NORMAL;")?;
            Ok(())
        })
        .await
        .map_err(|e| CoreError::Internal(format!("数据库配置失败: {}", e)))?;

        Ok(Self { conn })
    }

    /// 初始化数据库表结构
    ///
    /// 创建 tasks、task_snapshots 和 task_events 三张表。
    /// 如果表已存在则跳过。
    pub async fn init(&self) -> Result<(), CoreError> {
        info!("初始化任务数据库表结构");

        self.conn
            .call(|conn| {
                conn.execute_batch(
                    "
                    -- 任务主表
                    CREATE TABLE IF NOT EXISTS tasks (
                        id              TEXT PRIMARY KEY,
                        agent_id        TEXT NOT NULL,
                        prompt          TEXT NOT NULL,
                        status          TEXT NOT NULL DEFAULT 'pending',
                        priority        TEXT NOT NULL DEFAULT 'medium',
                        created_at      TEXT NOT NULL,
                        updated_at      TEXT NOT NULL,
                        started_at      TEXT,
                        completed_at    TEXT,
                        error           TEXT,
                        turn_count      INTEGER NOT NULL DEFAULT 0,
                        input_tokens    INTEGER NOT NULL DEFAULT 0,
                        output_tokens   INTEGER NOT NULL DEFAULT 0,
                        cost_usd        REAL NOT NULL DEFAULT 0.0,
                        metadata_json   TEXT NOT NULL DEFAULT '{}'
                    );

                    -- 任务快照表（用于中断恢复）
                    CREATE TABLE IF NOT EXISTS task_snapshots (
                        id                  TEXT PRIMARY KEY,
                        task_id             TEXT NOT NULL,
                        conversation_json   TEXT NOT NULL DEFAULT '[]',
                        system_prompt       TEXT NOT NULL DEFAULT '',
                        model               TEXT NOT NULL DEFAULT '',
                        config_json         TEXT NOT NULL DEFAULT '{}',
                        created_at          TEXT NOT NULL,
                        FOREIGN KEY (task_id) REFERENCES tasks(id) ON DELETE CASCADE
                    );

                    -- 任务事件表
                    CREATE TABLE IF NOT EXISTS task_events (
                        id          TEXT PRIMARY KEY,
                        task_id     TEXT NOT NULL,
                        event_type  TEXT NOT NULL,
                        data_json   TEXT NOT NULL DEFAULT '{}',
                        created_at  TEXT NOT NULL,
                        FOREIGN KEY (task_id) REFERENCES tasks(id) ON DELETE CASCADE
                    );

                    -- 索引：按智能体 ID 查询任务
                    CREATE INDEX IF NOT EXISTS idx_tasks_agent_id ON tasks(agent_id);

                    -- 索引：按状态查询任务
                    CREATE INDEX IF NOT EXISTS idx_tasks_status ON tasks(status);

                    -- 索引：按优先级查询任务
                    CREATE INDEX IF NOT EXISTS idx_tasks_priority ON tasks(priority);

                    -- 索引：按创建时间排序
                    CREATE INDEX IF NOT EXISTS idx_tasks_created_at ON tasks(created_at);

                    -- 索引：按任务 ID 查询快照
                    CREATE INDEX IF NOT EXISTS idx_snapshots_task_id ON task_snapshots(task_id);

                    -- 索引：按任务 ID 查询事件
                    CREATE INDEX IF NOT EXISTS idx_events_task_id ON task_events(task_id);

                    -- 索引：按创建时间查询事件
                    CREATE INDEX IF NOT EXISTS idx_events_created_at ON task_events(created_at);
                    ",
                )?;
                Ok(())
            })
            .await
            .map_err(|e| CoreError::Internal(format!("初始化数据库表失败: {}", e)))?;

        info!("任务数据库表结构初始化完成");
        Ok(())
    }

    // ========================================================================
    // 任务 CRUD 操作
    // ========================================================================

    /// 创建任务
    ///
    /// 将任务信息写入数据库，返回任务 ID。
    pub async fn create_task(&self, info: &TaskInfo) -> Result<String, CoreError> {
        let info_clone = info.clone();
        let task_id = info.id.clone();

        debug!("创建任务: {}", task_id);

        self.conn
            .call(move |conn| {
                conn.execute(
                    "INSERT INTO tasks (
                        id, agent_id, prompt, status, priority,
                        created_at, updated_at, started_at, completed_at,
                        error, turn_count, input_tokens, output_tokens,
                        cost_usd, metadata_json
                    ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15)",
                    rusqlite::params![
                        info_clone.id,
                        info_clone.agent_id,
                        info_clone.prompt,
                        info_clone.status.as_str(),
                        info_clone.priority.as_str(),
                        info_clone.created_at.to_rfc3339(),
                        info_clone.updated_at.to_rfc3339(),
                        info_clone.started_at.map(|t| t.to_rfc3339()),
                        info_clone.completed_at.map(|t| t.to_rfc3339()),
                        info_clone.error,
                        info_clone.turn_count,
                        info_clone.total_input_tokens,
                        info_clone.total_output_tokens,
                        info_clone.cost_usd,
                        serde_json::to_string(&info_clone.metadata).unwrap_or_else(|_| "{}".to_string()),
                    ],
                )?;
                Ok(())
            })
            .await
            .map_err(|e| CoreError::Internal(format!("创建任务失败: {}", e)))?;

        info!("任务已创建: {}", task_id);
        Ok(task_id)
    }

    /// 更新任务状态
    pub async fn update_task_status(
        &self,
        task_id: &str,
        status: TaskStatus,
    ) -> Result<(), CoreError> {
        let task_id = task_id.to_string();
        let status_str = status.as_str().to_string();
        let now = Utc::now().to_rfc3339();

        debug!("更新任务状态: {} -> {}", task_id, status_str);

        // 根据状态更新相应的字段
        let (sql, is_terminal) = match status {
            TaskStatus::Running => (
                "UPDATE tasks SET status = ?1, updated_at = ?2, started_at = COALESCE(started_at, ?3) WHERE id = ?4",
                false,
            ),
            TaskStatus::Completed | TaskStatus::Failed | TaskStatus::Cancelled => (
                "UPDATE tasks SET status = ?1, updated_at = ?2, completed_at = ?3 WHERE id = ?4",
                true,
            ),
            _ => (
                "UPDATE tasks SET status = ?1, updated_at = ?2 WHERE id = ?3",
                false,
            ),
        };

        let task_id_for_log = task_id.clone();
        let task_id_for_debug = task_id.clone();
        let status_str_for_debug = status_str.clone();
        self.conn
            .call(move |conn| {
                let affected = if is_terminal {
                    conn.execute(sql, rusqlite::params![status_str, now, now, task_id])?
                } else if status_str == "running" {
                    conn.execute(sql, rusqlite::params![status_str, now, now, task_id])?
                } else {
                    conn.execute(sql, rusqlite::params![status_str, now, task_id])?
                };
                if affected == 0 {
                    return Err(tokio_rusqlite::Error::Rusqlite(rusqlite::Error::QueryReturnedNoRows));
                }
                Ok(())
            })
            .await
            .map_err(|e| {
                CoreError::Internal(format!("更新任务状态失败 ({}): {}", task_id_for_log, e))
            })?;

        debug!("任务状态已更新: {} -> {}", task_id_for_debug, status_str_for_debug);
        Ok(())
    }

    /// 更新任务进度
    ///
    /// 更新任务的轮次计数、token 消耗和费用信息。
    pub async fn update_task_progress(
        &self,
        task_id: &str,
        turn_count: u32,
        input_tokens: u32,
        output_tokens: u32,
        cost_usd: f64,
    ) -> Result<(), CoreError> {
        let task_id = task_id.to_string();
        let now = Utc::now().to_rfc3339();

        debug!(
            "更新任务进度: {} (轮次: {}, 输入: {}, 输出: {}, 费用: ${:.4})",
            task_id, turn_count, input_tokens, output_tokens, cost_usd
        );

        let task_id_for_log = task_id.clone();
        self.conn
            .call(move |conn| {
                let affected = conn.execute(
                    "UPDATE tasks SET
                        turn_count = ?1,
                        input_tokens = ?2,
                        output_tokens = ?3,
                        cost_usd = ?4,
                        updated_at = ?5
                    WHERE id = ?6",
                    rusqlite::params![turn_count, input_tokens, output_tokens, cost_usd, now, task_id],
                )?;
                if affected == 0 {
                    return Err(tokio_rusqlite::Error::Rusqlite(rusqlite::Error::QueryReturnedNoRows));
                }
                Ok(())
            })
            .await
            .map_err(|e| {
                CoreError::Internal(format!("更新任务进度失败 ({}): {}", task_id_for_log, e))
            })?;

        Ok(())
    }

    /// 更新任务错误信息
    pub async fn update_task_error(
        &self,
        task_id: &str,
        error: &str,
    ) -> Result<(), CoreError> {
        let task_id = task_id.to_string();
        let error = error.to_string();
        let now = Utc::now().to_rfc3339();

        let task_id_for_log = task_id.clone();
        self.conn
            .call(move |conn| {
                let affected = conn.execute(
                    "UPDATE tasks SET error = ?1, updated_at = ?2 WHERE id = ?3",
                    rusqlite::params![error, now, task_id],
                )?;
                if affected == 0 {
                    return Err(tokio_rusqlite::Error::Rusqlite(rusqlite::Error::QueryReturnedNoRows));
                }
                Ok(())
            })
            .await
            .map_err(|e| {
                CoreError::Internal(format!("更新任务错误信息失败 ({}): {}", task_id_for_log, e))
            })?;

        Ok(())
    }

    /// 获取任务信息
    pub async fn get_task(&self, task_id: &str) -> Result<TaskInfo, CoreError> {
        let task_id = task_id.to_string();

        debug!("获取任务信息: {}", task_id);

        let task_id_for_log = task_id.clone();
        let result = self
            .conn
            .call(move |conn| {
                let mut stmt = conn.prepare(
                    "SELECT id, agent_id, prompt, status, priority,
                            created_at, updated_at, started_at, completed_at,
                            error, turn_count, input_tokens, output_tokens,
                            cost_usd, metadata_json
                     FROM tasks WHERE id = ?1",
                )?;

                let task = stmt.query_row(rusqlite::params![task_id], |row| {
                    Ok(row_to_task_info(row))
                })?;

                Ok(task)
            })
            .await
            .map_err(|e| match e {
                tokio_rusqlite::Error::Rusqlite(rusqlite::Error::QueryReturnedNoRows) => {
                    CoreError::Internal(format!("任务不存在: {}", task_id_for_log))
                }
                _ => CoreError::Internal(format!("获取任务失败: {}", e)),
            })?;

        Ok(result)
    }

    /// 列出任务
    ///
    /// 根据过滤条件查询任务列表。
    pub async fn list_tasks(&self, filter: &TaskFilter) -> Result<Vec<TaskInfo>, CoreError> {
        let mut where_clauses: Vec<String> = Vec::new();
        let mut params: Vec<String> = Vec::new();

        if let Some(ref status) = filter.status {
            where_clauses.push("status = ?".to_string());
            params.push(status.as_str().to_string());
        }

        if let Some(ref agent_id) = filter.agent_id {
            where_clauses.push("agent_id = ?".to_string());
            params.push(agent_id.clone());
        }

        if let Some(ref priority) = filter.priority {
            where_clauses.push("priority = ?".to_string());
            params.push(priority.as_str().to_string());
        }

        let where_sql = if where_clauses.is_empty() {
            String::new()
        } else {
            format!("WHERE {}", where_clauses.join(" AND "))
        };

        let order_sql = if filter.order_desc {
            "ORDER BY created_at DESC"
        } else {
            "ORDER BY created_at ASC"
        };

        let limit_sql = match filter.limit {
            Some(limit) => format!("LIMIT {}", limit),
            None => String::new(),
        };

        let sql = format!(
            "SELECT id, agent_id, prompt, status, priority,
                    created_at, updated_at, started_at, completed_at,
                    error, turn_count, input_tokens, output_tokens,
                    cost_usd, metadata_json
             FROM tasks {} {} {}",
            where_sql, order_sql, limit_sql
        );

        debug!("查询任务列表: {}", sql);

        let result: Vec<TaskInfo> = self
            .conn
            .call(move |conn| {
                let mut stmt = conn.prepare(&sql)?;

                let tasks = stmt
                    .query_map(rusqlite::params_from_iter(params.iter()), |row| Ok(row_to_task_info(row)))?
                    .collect::<Result<Vec<_>, _>>()?;

                Ok(tasks)
            })
            .await
            .map_err(|e| CoreError::Internal(format!("查询任务列表失败: {}", e)))?;

        debug!("查询到 {} 个任务", result.len());
        Ok(result)
    }

    // ========================================================================
    // 快照操作
    // ========================================================================

    /// 保存任务快照
    ///
    /// 将任务快照写入数据库，用于后续中断恢复。
    pub async fn save_snapshot(&self, snapshot: &TaskSnapshot) -> Result<(), CoreError> {
        let snapshot = snapshot.clone();
        let task_id = snapshot.task_info.id.clone();
        let snapshot_id = uuid::Uuid::new_v4().to_string();
        let now = Utc::now().to_rfc3339();

        debug!("保存任务快照: {}", task_id);

        self.conn
            .call(move |conn| {
                conn.execute(
                    "INSERT INTO task_snapshots (
                        id, task_id, conversation_json, system_prompt,
                        model, config_json, created_at
                    ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
                    rusqlite::params![
                        snapshot_id,
                        snapshot.task_info.id,
                        snapshot.conversation_history_json,
                        snapshot.system_prompt,
                        snapshot.model,
                        snapshot.config_json,
                        now,
                    ],
                )?;
                Ok(())
            })
            .await
            .map_err(|e| {
                CoreError::Internal(format!("保存任务快照失败: {}", e))
            })?;

        info!("任务快照已保存: {}", task_id);
        Ok(())
    }

    /// 加载最新的任务快照
    ///
    /// 根据任务 ID 加载最新的快照数据，用于中断恢复。
    pub async fn load_snapshot(&self, task_id: &str) -> Result<TaskSnapshot, CoreError> {
        let task_id = task_id.to_string();

        debug!("加载任务快照: {}", task_id);

        let task_id_for_log = task_id.clone();
        let task_id_for_info = task_id.clone();
        let result = self
            .conn
            .call(move |conn| {
                // 先获取任务信息
                let task_info = {
                    let mut stmt = conn.prepare(
                        "SELECT id, agent_id, prompt, status, priority,
                                created_at, updated_at, started_at, completed_at,
                                error, turn_count, input_tokens, output_tokens,
                                cost_usd, metadata_json
                         FROM tasks WHERE id = ?1",
                    )?;
                    stmt.query_row(rusqlite::params![task_id], |row| {
                        Ok(row_to_task_info(row))
                    })?
                };

                // 获取最新的快照
                let mut stmt = conn.prepare(
                    "SELECT conversation_json, system_prompt, model, config_json
                     FROM task_snapshots
                     WHERE task_id = ?1
                     ORDER BY created_at DESC
                     LIMIT 1",
                )?;

                let snapshot = stmt.query_row(rusqlite::params![task_id], |row| {
                    Ok(TaskSnapshot {
                        task_info,
                        conversation_history_json: row.get(0)?,
                        system_prompt: row.get(1)?,
                        model: row.get(2)?,
                        config_json: row.get(3)?,
                    })
                })?;

                Ok(snapshot)
            })
            .await
            .map_err(|e| match e {
                tokio_rusqlite::Error::Rusqlite(rusqlite::Error::QueryReturnedNoRows) => {
                    CoreError::Internal(format!("任务快照不存在: {}", task_id_for_log))
                }
                _ => CoreError::Internal(format!("加载任务快照失败: {}", e)),
            })?;

        info!("任务快照已加载: {}", task_id_for_info);
        Ok(result)
    }

    // ========================================================================
    // 事件操作
    // ========================================================================

    /// 保存任务事件
    pub async fn save_event(&self, event: &TaskEvent) -> Result<(), CoreError> {
        let event = event.clone();
        let now = event.timestamp.to_rfc3339();

        debug!(
            "保存任务事件: {} -> {}",
            event.task_id, event.event_type
        );

        self.conn
            .call(move |conn| {
                conn.execute(
                    "INSERT INTO task_events (id, task_id, event_type, data_json, created_at)
                     VALUES (?1, ?2, ?3, ?4, ?5)",
                    rusqlite::params![
                        event.id,
                        event.task_id,
                        event.event_type.as_str(),
                        serde_json::to_string(&event.data).unwrap_or_else(|_| "{}".to_string()),
                        now,
                    ],
                )?;
                Ok(())
            })
            .await
            .map_err(|e| CoreError::Internal(format!("保存任务事件失败: {}", e)))?;

        Ok(())
    }

    /// 列出任务事件
    ///
    /// 获取指定任务的最近事件列表。
    pub async fn list_events(
        &self,
        task_id: &str,
        limit: Option<usize>,
    ) -> Result<Vec<TaskEvent>, CoreError> {
        let task_id = task_id.to_string();
        let limit_val = limit.unwrap_or(100);

        debug!("查询任务事件: {} (限制: {})", task_id, limit_val);

        let result: Vec<TaskEvent> = self
            .conn
            .call(move |conn| {
                let mut stmt = conn.prepare(
                    "SELECT id, task_id, event_type, data_json, created_at
                     FROM task_events
                     WHERE task_id = ?1
                     ORDER BY created_at DESC
                     LIMIT ?2",
                )?;

                let events = stmt
                    .query_map(rusqlite::params![task_id, limit_val], |row| {
                        let id: String = row.get(0)?;
                        let task_id: String = row.get(1)?;
                        let event_type_str: String = row.get(2)?;
                        let data_json: String = row.get(3)?;
                        let created_at_str: String = row.get(4)?;

                        let event_type = TaskEventType::from_str(&event_type_str)
                            .unwrap_or(TaskEventType::Created);

                        let data: serde_json::Value =
                            serde_json::from_str(&data_json).unwrap_or(serde_json::Value::Null);

                        let created_at = parse_datetime(&created_at_str);

                        Ok(TaskEvent {
                            id,
                            task_id,
                            timestamp: created_at,
                            event_type,
                            data,
                        })
                    })?
                    .collect::<Result<Vec<_>, _>>()?;

                Ok(events)
            })
            .await
            .map_err(|e| CoreError::Internal(format!("查询任务事件失败: {}", e)))?;

        debug!("查询到 {} 个事件", result.len());
        Ok(result)
    }

    // ========================================================================
    // 清理操作
    // ========================================================================

    /// 删除任务及相关数据
    ///
    /// 删除指定任务及其所有快照和事件记录。
    pub async fn delete_task(&self, task_id: &str) -> Result<(), CoreError> {
        let task_id = task_id.to_string();

        warn!("删除任务: {}", task_id);

        let task_id_for_log = task_id.clone();
        let task_id_for_info = task_id.clone();
        self.conn
            .call(move |conn| {
                // 先删除关联的快照
                conn.execute(
                    "DELETE FROM task_snapshots WHERE task_id = ?1",
                    rusqlite::params![task_id],
                )?;
                // 再删除关联的事件
                conn.execute(
                    "DELETE FROM task_events WHERE task_id = ?1",
                    rusqlite::params![task_id],
                )?;
                // 最后删除任务本身
                let affected = conn.execute(
                    "DELETE FROM tasks WHERE id = ?1",
                    rusqlite::params![task_id],
                )?;
                if affected == 0 {
                    return Err(tokio_rusqlite::Error::Rusqlite(rusqlite::Error::QueryReturnedNoRows));
                }
                Ok(())
            })
            .await
            .map_err(|e| match e {
                tokio_rusqlite::Error::Rusqlite(rusqlite::Error::QueryReturnedNoRows) => {
                    CoreError::Internal(format!("任务不存在: {}", task_id_for_log))
                }
                _ => CoreError::Internal(format!("删除任务失败: {}", e)),
            })?;

        info!("任务已删除: {}", task_id_for_info);
        Ok(())
    }

    /// 清理旧任务
    ///
    /// 删除指定天数之前的已完成/失败/取消的任务。
    /// 返回被删除的任务数量。
    pub async fn cleanup_old_tasks(&self, days: i64) -> Result<usize, CoreError> {
        let cutoff = Utc::now() - chrono::Duration::days(days);
        let cutoff_str = cutoff.to_rfc3339();

        info!("清理 {} 天前的旧任务 (截止: {})", days, cutoff_str);

        let result: usize = self
            .conn
            .call(move |conn| {
                // 先删除关联的快照
                conn.execute(
                    "DELETE FROM task_snapshots WHERE task_id IN (
                        SELECT id FROM tasks
                        WHERE status IN ('completed', 'failed', 'cancelled')
                        AND updated_at < ?1
                    )",
                    rusqlite::params![cutoff_str],
                )?;

                // 再删除关联的事件
                conn.execute(
                    "DELETE FROM task_events WHERE task_id IN (
                        SELECT id FROM tasks
                        WHERE status IN ('completed', 'failed', 'cancelled')
                        AND updated_at < ?1
                    )",
                    rusqlite::params![cutoff_str],
                )?;

                // 删除旧任务
                let affected = conn.execute(
                    "DELETE FROM tasks
                     WHERE status IN ('completed', 'failed', 'cancelled')
                     AND updated_at < ?1",
                    rusqlite::params![cutoff_str],
                )?;

                Ok(affected as usize)
            })
            .await
            .map_err(|e| CoreError::Internal(format!("清理旧任务失败: {}", e)))?;

        info!("已清理 {} 个旧任务", result);
        Ok(result)
    }

    /// 数据库健康检查
    ///
    /// 执行简单的查询来验证数据库连接是否正常。
    pub async fn check_health(&self) -> Result<(), CoreError> {
        self.conn
            .call(|conn| {
                conn.execute_batch("SELECT 1;")?;
                Ok(())
            })
            .await
            .map_err(|e| CoreError::Internal(format!("数据库健康检查失败: {}", e)))?;
        Ok(())
    }
}

// ============================================================================
// 辅助函数
// ============================================================================

/// 从数据库行构建 TaskInfo
fn row_to_task_info(row: &rusqlite::Row<'_>) -> TaskInfo {
    let created_at_str: String = row.get(5).unwrap_or_default();
    let updated_at_str: String = row.get(6).unwrap_or_default();
    let started_at_str: Option<String> = row.get(7).unwrap_or(None);
    let completed_at_str: Option<String> = row.get(8).unwrap_or(None);
    let metadata_json: String = row.get(13).unwrap_or_else(|_| "{}".to_string());

    let created_at = parse_datetime(&created_at_str);
    let updated_at = parse_datetime(&updated_at_str);
    let started_at = parse_datetime_opt(started_at_str);
    let completed_at = parse_datetime_opt(completed_at_str);

    let status_str: String = row.get(3).unwrap_or_else(|_| "pending".to_string());
    let priority_str: String = row.get(4).unwrap_or_else(|_| "medium".to_string());

    let metadata: serde_json::Value = serde_json::from_str(&metadata_json)
        .unwrap_or(serde_json::Value::Object(serde_json::Map::new()));

    TaskInfo {
        id: row.get(0).unwrap_or_default(),
        agent_id: row.get(1).unwrap_or_default(),
        prompt: row.get(2).unwrap_or_default(),
        status: TaskStatus::from_str(&status_str).unwrap_or(TaskStatus::Pending),
        priority: TaskPriority::from_str(&priority_str).unwrap_or(TaskPriority::Medium),
        created_at,
        updated_at,
        started_at,
        completed_at,
        error: row.get(9).unwrap_or(None),
        turn_count: row.get(10).unwrap_or(0),
        total_input_tokens: row.get(11).unwrap_or(0),
        total_output_tokens: row.get(12).unwrap_or(0),
        cost_usd: row.get(14).unwrap_or(0.0),
        metadata,
    }
}
