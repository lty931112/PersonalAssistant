//! 沙箱执行器

use std::process::Command;
use pa_core::CoreError;

/// 沙箱配置
pub struct SandboxConfig {
    pub docker_image: Option<String>,
    pub timeout_secs: u64,
    pub memory_limit: Option<String>,
    pub network_disabled: bool,
}

impl Default for SandboxConfig {
    fn default() -> Self {
        Self {
            docker_image: Some("personalassistant/sandbox:latest".into()),
            timeout_secs: 60,
            memory_limit: Some("512m".into()),
            network_disabled: true,
        }
    }
}

/// 沙箱执行器
pub struct SandboxExecutor {
    config: SandboxConfig,
}

impl SandboxExecutor {
    pub fn new(config: SandboxConfig) -> Self {
        Self { config }
    }

    /// 在沙箱中执行命令
    pub async fn execute(&self, command: &str, working_dir: Option<&str>) -> Result<String, CoreError> {
        if let Some(ref image) = self.config.docker_image {
            self.execute_in_docker(command, working_dir, image).await
        } else {
            self.execute_local(command, working_dir).await
        }
    }

    async fn execute_in_docker(&self, command: &str, working_dir: Option<&str>, image: &str) -> Result<String, CoreError> {
        let mut args: Vec<String> = vec!["run".into(), "--rm".into()];
        
        if self.config.network_disabled {
            args.push("--network=none".into());
        }
        
        if let Some(ref mem) = self.config.memory_limit {
            args.push("--memory".into());
            args.push(mem.clone());
        }
        
        if let Some(dir) = working_dir {
            args.push("-v".into());
            args.push(format!("{}:/workspace", dir));
            args.push("-w".into());
            args.push("/workspace".into());
        }
        
        args.push(image.into());
        args.push("sh".into());
        args.push("-c".into());
        args.push(command.into());

        let output = Command::new("docker")
            .args(&args)
            .output()
            .map_err(|e| CoreError::io_error(e.to_string()))?;

        if output.status.success() {
            String::from_utf8(output.stdout)
                .map_err(|e| CoreError::Internal(e.to_string()))
        } else {
            Err(CoreError::ToolExecutionError {
                tool_name: "sandbox".into(),
                message: String::from_utf8_lossy(&output.stderr).into(),
            })
        }
    }

    async fn execute_local(&self, command: &str, _working_dir: Option<&str>) -> Result<String, CoreError> {
        // 本地执行（无沙箱，仅用于开发环境）
        let output = Command::new("sh")
            .arg("-c")
            .arg(command)
            .output()
            .map_err(|e| CoreError::io_error(e.to_string()))?;

        if output.status.success() {
            String::from_utf8(output.stdout)
                .map_err(|e| CoreError::Internal(e.to_string()))
        } else {
            Err(CoreError::ToolExecutionError {
                tool_name: "local".into(),
                message: String::from_utf8_lossy(&output.stderr).into(),
            })
        }
    }
}
