//! 配置系统模块
//!
//! 实现层级化配置管理，支持热重载、环境变量替换、配置文件 include。

pub mod settings;
pub mod loader;
pub mod env;
pub mod persona;

pub use settings::{PersonaSettings, Settings};
pub use loader::ConfigLoader;
pub use env::EnvSubstitution;
pub use persona::PersonaRuntime;
