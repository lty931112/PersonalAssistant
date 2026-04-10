//! 执行审计（JSON Lines）与追踪发射器

use std::io::Write;
use std::path::Path;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

use chrono::{DateTime, Utc};
use serde::Serialize;
use tokio::sync::mpsc;

use pa_core::QueryEvent;

/// 单行审计记录（与 `Observability` 事件字段对齐）
#[derive(Debug, Clone, Serialize)]
pub struct AuditRecord {
    pub ts: DateTime<Utc>,
    pub trace_id: String,
    pub seq: u64,
    pub phase: String,
    pub turn: Option<u32>,
    pub detail: serde_json::Value,
}

/// 追加写入 JSONL 审计文件（进程内互斥）
pub struct AuditSink {
    file: Mutex<std::fs::File>,
}

impl AuditSink {
    pub fn open(path: impl AsRef<Path>) -> std::io::Result<Self> {
        let path = path.as_ref();
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir)?;
        }
        let file = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(path)?;
        Ok(Self {
            file: Mutex::new(file),
        })
    }

    pub fn append(&self, record: &AuditRecord) -> std::io::Result<()> {
        let mut line = serde_json::to_string(record)
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;
        line.push('\n');
        let mut g = self.file.lock().map_err(|e| {
            std::io::Error::new(std::io::ErrorKind::Other, format!("audit mutex: {}", e))
        })?;
        g.write_all(line.as_bytes())?;
        g.flush()?;
        Ok(())
    }
}

/// 单次查询追踪：统一 `trace_id`、序号与审计落盘
#[derive(Clone)]
pub struct TraceEmitter {
    trace_id: String,
    seq: Arc<AtomicU64>,
    audit: Option<Arc<AuditSink>>,
}

impl TraceEmitter {
    pub fn new(audit: Option<Arc<AuditSink>>) -> Self {
        Self {
            trace_id: uuid::Uuid::new_v4().to_string(),
            seq: Arc::new(AtomicU64::new(0)),
            audit,
        }
    }

    pub fn trace_id(&self) -> &str {
        &self.trace_id
    }

    pub async fn emit(
        &self,
        event_tx: &mpsc::Sender<QueryEvent>,
        phase: &str,
        turn: Option<u32>,
        detail: serde_json::Value,
    ) {
        let seq = self.seq.fetch_add(1, Ordering::SeqCst);
        let _ = event_tx
            .send(QueryEvent::Observability {
                trace_id: self.trace_id.clone(),
                seq,
                phase: phase.to_string(),
                turn,
                detail: detail.clone(),
            })
            .await;

        if let Some(ref sink) = self.audit {
            let record = AuditRecord {
                ts: Utc::now(),
                trace_id: self.trace_id.clone(),
                seq,
                phase: phase.to_string(),
                turn,
                detail,
            };
            if let Err(e) = sink.append(&record) {
                tracing::warn!(error = %e, "写入审计日志失败");
            }
        }
    }
}
