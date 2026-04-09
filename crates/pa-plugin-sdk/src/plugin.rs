//! 插件核心 trait

use async_trait::async_trait;
use pa_core::CoreError;

/// 插件元数据
#[derive(Debug, Clone)]
pub struct PluginMetadata {
    pub name: String,
    pub version: String,
    pub description: String,
    pub author: String,
}

/// 插件上下文
pub struct PluginContext {
    pub data_dir: std::path::PathBuf,
    pub config: serde_json::Value,
}

/// 插件 trait
#[async_trait]
pub trait Plugin: Send + Sync {
    /// 获取插件元数据
    fn metadata(&self) -> &PluginMetadata;

    /// 初始化插件
    async fn initialize(&mut self, context: PluginContext) -> Result<(), CoreError>;

    /// 关闭插件
    async fn shutdown(&mut self) -> Result<(), CoreError> {
        Ok(())
    }
}
