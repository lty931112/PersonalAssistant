//! 扩展管理器

use std::collections::HashMap;
use crate::plugin::Plugin;

/// 扩展类型
#[derive(Debug, Clone)]
pub enum ExtensionType {
    LlmProvider,
    Channel,
    Tool,
    Memory,
    Media,
    Voice,
    Other(String),
}

/// 扩展信息
pub struct Extension {
    pub plugin: Box<dyn Plugin>,
    pub ext_type: ExtensionType,
}

/// 扩展管理器
pub struct ExtensionManager {
    extensions: HashMap<String, Extension>,
}

impl ExtensionManager {
    pub fn new() -> Self {
        Self {
            extensions: HashMap::new(),
        }
    }

    /// 注册扩展
    pub fn register(&mut self, name: &str, extension: Extension) {
        self.extensions.insert(name.to_string(), extension);
    }

    /// 获取扩展
    pub fn get(&self, name: &str) -> Option<&Extension> {
        self.extensions.get(name)
    }

    /// 列出所有扩展
    pub fn list(&self) -> Vec<&str> {
        self.extensions.keys().map(|s| s.as_str()).collect()
    }

    /// 按类型列出扩展
    pub fn list_by_type(&self, ext_type: &ExtensionType) -> Vec<&str> {
        self.extensions
            .iter()
            .filter(|(_, ext)| match (&ext.ext_type, ext_type) {
                (ExtensionType::Other(a), ExtensionType::Other(b)) => a == b,
                (a, b) => std::mem::discriminant(a) == std::mem::discriminant(b),
            })
            .map(|(name, _)| name.as_str())
            .collect()
    }
}
