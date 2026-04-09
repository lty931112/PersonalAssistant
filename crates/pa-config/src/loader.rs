//! 配置加载器

use std::path::Path;
use pa_core::CoreError;
use crate::settings::Settings;
use crate::env::EnvSubstitution;

/// 配置加载器
pub struct ConfigLoader;

impl ConfigLoader {
    /// 从文件加载配置
    pub fn load(path: &str) -> Result<Settings, CoreError> {
        let content = std::fs::read_to_string(path)
            .map_err(|e| CoreError::config_error(format!("无法读取配置文件 {}: {}", path, e)))?;
        
        // 环境变量替换
        let content = EnvSubstitution::substitute(&content);
        
        // 解析 TOML
        let settings: Settings = toml::from_str(&content)
            .map_err(|e| CoreError::config_error(format!("配置解析错误: {}", e)))?;
        
        Ok(settings)
    }

    /// 加载配置或使用默认值
    pub fn load_or_default() -> Result<Settings, CoreError> {
        let default_paths = ["config/default.toml", "config.toml", ".pa/config.toml"];
        
        for path in &default_paths {
            if Path::new(path).exists() {
                return Self::load(path);
            }
        }
        
        // 从环境变量加载 API 密钥
        let mut settings = Settings::default();
        if let Ok(key) = std::env::var("ANTHROPIC_API_KEY") {
            settings.llm.api_key = key;
        } else if let Ok(key) = std::env::var("OPENAI_API_KEY") {
            settings.llm.provider = "openai".into();
            settings.llm.model = "gpt-4o".into();
            settings.llm.api_key = key;
        }
        
        Ok(settings)
    }

    /// 保存配置到文件
    pub fn save(settings: &Settings, path: &str) -> Result<(), CoreError> {
        let content = toml::to_string_pretty(settings)
            .map_err(|e| CoreError::config_error(format!("配置序列化错误: {}", e)))?;
        
        // 确保目录存在
        if let Some(parent) = Path::new(path).parent() {
            std::fs::create_dir_all(parent)
                .map_err(|e| CoreError::io_error(e.to_string()))?;
        }
        
        std::fs::write(path, content)
            .map_err(|e| CoreError::io_error(e.to_string()))?;
        
        Ok(())
    }
}
