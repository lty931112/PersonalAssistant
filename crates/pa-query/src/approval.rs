//! 工具执行人工批准（HITL）：CLI 与 HTTP/WS 共用抽象

use std::collections::HashMap;
use std::sync::Arc;

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use tokio::sync::{Mutex, oneshot};

/// 单次批准请求（可序列化，供 API / 前端展示）
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolApprovalRequest {
    pub approval_id: String,
    pub trace_id: String,
    pub turn: u32,
    pub tool_name: String,
    pub tool_id: String,
    pub prompt: String,
    pub input_summary: serde_json::Value,
}

/// 由查询引擎调用：阻塞直到用户批准或拒绝
#[async_trait]
pub trait ToolApprovalProvider: Send + Sync {
    async fn wait_approval(&self, req: ToolApprovalRequest) -> bool;
}

struct BrokerInner {
    /// approval_id -> 等待方 oneshot
    pending: Mutex<HashMap<String, oneshot::Sender<bool>>>,
    /// approval_id -> 展示用请求体
    requests: Mutex<HashMap<String, ToolApprovalRequest>>,
}

/// 进程内共享：Gateway HTTP/WS 与 QueryEngine 共用同一实例
#[derive(Clone)]
pub struct SharedApprovalBroker {
    inner: Arc<BrokerInner>,
}

impl SharedApprovalBroker {
    pub fn new() -> Self {
        Self {
            inner: Arc::new(BrokerInner {
                pending: Mutex::new(HashMap::new()),
                requests: Mutex::new(HashMap::new()),
            }),
        }
    }

    /// 列出待处理批准（供 Web / 监控）
    pub async fn list_pending(&self) -> Vec<ToolApprovalRequest> {
        let g = self.inner.requests.lock().await;
        g.values().cloned().collect()
    }

    /// 用户批准或拒绝（HTTP / WebSocket 调用）
    pub async fn respond(&self, approval_id: &str, approved: bool) -> Result<(), String> {
        let tx = {
            let mut p = self.inner.pending.lock().await;
            p.remove(approval_id)
        };
        let mut r = self.inner.requests.lock().await;
        r.remove(approval_id);
        match tx {
            Some(tx) => {
                let _ = tx.send(approved);
                Ok(())
            }
            None => Err(format!("未找到待处理批准: {}", approval_id)),
        }
    }
}

impl Default for SharedApprovalBroker {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl ToolApprovalProvider for SharedApprovalBroker {
    async fn wait_approval(&self, mut req: ToolApprovalRequest) -> bool {
        if req.approval_id.is_empty() {
            req.approval_id = uuid::Uuid::new_v4().to_string();
        }
        let id = req.approval_id.clone();
        let (tx, rx) = oneshot::channel();
        {
            let mut r = self.inner.requests.lock().await;
            r.insert(id.clone(), req);
        }
        {
            let mut p = self.inner.pending.lock().await;
            p.insert(id.clone(), tx);
        }
        let ok = rx.await.unwrap_or(false);
        let mut r = self.inner.requests.lock().await;
        r.remove(&id);
        let mut p = self.inner.pending.lock().await;
        p.remove(&id);
        ok
    }
}

/// CLI：在终端打印提示并从 stdin 读取 y/n
pub struct CliToolApproval;

impl CliToolApproval {
    pub fn new() -> Self {
        Self
    }
}

impl Default for CliToolApproval {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl ToolApprovalProvider for CliToolApproval {
    async fn wait_approval(&self, req: ToolApprovalRequest) -> bool {
        let prompt = req.prompt.clone();
        let tool = req.tool_name.clone();
        tokio::task::spawn_blocking(move || {
            eprintln!("\n--- 需要确认的工具调用 ({}) ---", tool);
            eprintln!("{}", prompt);
            eprint!("允许执行? [y/N]: ");
            let _ = std::io::Write::flush(&mut std::io::stderr());
            let mut line = String::new();
            if std::io::stdin().read_line(&mut line).is_err() {
                return false;
            }
            matches!(line.trim().to_lowercase().as_str(), "y" | "yes")
        })
        .await
        .unwrap_or(false)
    }
}
