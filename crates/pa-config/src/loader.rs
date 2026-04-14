//! 配置加载器

use std::path::Path;
use pa_core::CoreError;
use crate::settings::Settings;
use crate::env::EnvSubstitution;

/// 配置加载器
pub struct ConfigLoader;

impl ConfigLoader {
    fn load_toml_value(path: &str) -> Result<toml::Value, CoreError> {
        let content = std::fs::read_to_string(path)
            .map_err(|e| CoreError::config_error(format!("无法读取配置文件 {}: {}", path, e)))?;
        let content = EnvSubstitution::substitute(&content);
        toml::from_str::<toml::Value>(&content)
            .map_err(|e| CoreError::config_error(format!("配置解析错误: {}", e)))
    }

    fn merge_toml(base: &mut toml::Value, overlay: toml::Value) {
        match (base, overlay) {
            (toml::Value::Table(base_tbl), toml::Value::Table(overlay_tbl)) => {
                for (k, v) in overlay_tbl {
                    if let Some(base_v) = base_tbl.get_mut(&k) {
                        Self::merge_toml(base_v, v);
                    } else {
                        base_tbl.insert(k, v);
                    }
                }
            }
            (base_v, overlay_v) => {
                *base_v = overlay_v;
            }
        }
    }

    fn runtime_overlay_path(primary_path: &str) -> Option<String> {
        // 可显式覆盖运行时扩展配置路径（如生产环境）
        if let Ok(p) = std::env::var("PA_RUNTIME_CONFIG_PATH") {
            let trimmed = p.trim();
            if !trimmed.is_empty() {
                return Some(trimmed.to_string());
            }
        }
        let p = Path::new(primary_path);
        let parent = p.parent()?;
        Some(parent.join("runtime.toml").to_string_lossy().to_string())
    }

    /// 从文件加载配置
    pub fn load(path: &str) -> Result<Settings, CoreError> {
        let mut merged = Self::load_toml_value(path)?;
        if let Some(runtime_path) = Self::runtime_overlay_path(path) {
            if Path::new(&runtime_path).exists() {
                let overlay = Self::load_toml_value(&runtime_path)?;
                Self::merge_toml(&mut merged, overlay);
            }
        }

        let settings: Settings = merged
            .try_into()
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
