//! 客户端连接管理

use std::collections::HashMap;
use chrono::{DateTime, Utc};

/// 客户端信息
#[derive(Debug, Clone)]
pub struct ClientInfo {
    pub id: String,
    pub connected_at: DateTime<Utc>,
    pub last_activity: DateTime<Utc>,
    pub metadata: serde_json::Value,
}

/// 客户端注册表
pub struct ClientRegistry {
    clients: HashMap<String, ClientInfo>,
}

impl ClientRegistry {
    pub fn new() -> Self {
        Self {
            clients: HashMap::new(),
        }
    }

    /// 注册客户端
    pub fn register(&mut self, id: &str) {
        let now = Utc::now();
        self.clients.insert(id.to_string(), ClientInfo {
            id: id.to_string(),
            connected_at: now,
            last_activity: now,
            metadata: serde_json::Value::Null,
        });
    }

    /// 注销客户端
    pub fn unregister(&mut self, id: &str) {
        self.clients.remove(id);
    }

    /// 更新活动时间
    pub fn update_activity(&mut self, id: &str) {
        if let Some(client) = self.clients.get_mut(id) {
            client.last_activity = Utc::now();
        }
    }

    /// 获取客户端数量
    pub fn count(&self) -> usize {
        self.clients.len()
    }

    /// 获取所有客户端 ID
    pub fn list(&self) -> Vec<&String> {
        self.clients.keys().collect()
    }
}

/// 客户端连接（用于导出）
pub type ClientConnection = ClientInfo;
