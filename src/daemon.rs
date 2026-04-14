//! 守护进程模块
//!
//! 提供进程守护化能力，支持将进程转为后台守护进程。
//! 通过双 fork 方式实现标准的 Unix daemon 模式。

use std::ffi::CString;
use std::fs::{File, OpenOptions};
use std::io::Write;

/// 守护进程配置
pub struct DaemonConfig {
    /// PID 文件路径
    pub pid_file: String,
    /// 工作目录
    pub work_dir: String,
    /// 标准输出重定向文件
    pub stdout_file: String,
    /// 标准错误重定向文件
    pub stderr_file: String,
}

impl Default for DaemonConfig {
    fn default() -> Self {
        Self {
            pid_file: ".pa/personal-assistant.pid".to_string(),
            work_dir: ".".to_string(),
            stdout_file: ".pa/daemon.stdout.log".to_string(),
            stderr_file: ".pa/daemon.stderr.log".to_string(),
        }
    }
}

/// 将当前进程转为守护进程
///
/// 执行以下步骤：
/// 1. 第一次 fork，创建子进程，父进程退出
/// 2. 创建新会话，脱离控制终端
/// 3. 第二次 fork，确保进程永远不会重新获取控制终端
/// 4. 更改工作目录
/// 5. 重设文件权限掩码
/// 6. 关闭所有打开的文件描述符
/// 7. 重定向 stdin/stdout/stderr
/// 8. 写入 PID 文件
pub fn daemonize(config: &DaemonConfig) -> Result<(), String> {
    #[cfg(not(unix))]
    {
        let _ = config;
        return Err("守护进程模式仅在 Unix 类系统上支持".to_string());
    }

    #[cfg(unix)]
    {
        daemonize_unix(config)
    }
}

#[cfg(unix)]
fn daemonize_unix(config: &DaemonConfig) -> Result<(), String> {
    use std::os::unix::io::{AsRawFd, IntoRawFd};

    // 确保日志目录存在
    if let Some(parent) = std::path::Path::new(&config.pid_file).parent() {
        std::fs::create_dir_all(parent).map_err(|e| format!("创建 PID 文件目录失败: {}", e))?;
    }
    if let Some(parent) = std::path::Path::new(&config.stdout_file).parent() {
        std::fs::create_dir_all(parent).map_err(|e| format!("创建日志目录失败: {}", e))?;
    }

    unsafe {
        // 第一次 fork
        let pid = libc::fork();
        if pid < 0 {
            return Err(format!("第一次 fork 失败: {}", std::io::Error::last_os_error()));
        }
        if pid > 0 {
            // 父进程退出
            tracing::info!("守护进程启动，父进程退出，子进程 PID: {}", pid);
            std::process::exit(0);
        }

        // 创建新会话
        if libc::setsid() < 0 {
            return Err(format!("创建新会话失败: {}", std::io::Error::last_os_error()));
        }

        // 第二次 fork
        let pid = libc::fork();
        if pid < 0 {
            return Err(format!("第二次 fork 失败: {}", std::io::Error::last_os_error()));
        }
        if pid > 0 {
            std::process::exit(0);
        }

        // 更改工作目录
        let work_dir_cstr = CString::new(config.work_dir.clone())
            .map_err(|e| format!("工作目录路径包含 null 字节: {}", e))?;
        if libc::chdir(work_dir_cstr.as_ptr()) < 0 {
            return Err(format!("更改工作目录失败: {}", std::io::Error::last_os_error()));
        }

        // 重设文件权限掩码
        libc::umask(0o027);

        // 关闭所有打开的文件描述符
        let max_fd = libc::sysconf(libc::_SC_OPEN_MAX);
        if max_fd > 0 {
            for fd in 3..max_fd as i32 {
                let _ = libc::close(fd);
            }
        }

        // 重定向 stdin/stdout/stderr
        let devnull = File::open("/dev/null").map_err(|e| format!("打开 /dev/null 失败: {}", e))?;
        let stdin_fd = libc::dup(devnull.as_raw_fd());
        let stdout_fd = OpenOptions::new()
            .create(true)
            .append(true)
            .open(&config.stdout_file)
            .map_err(|e| format!("打开 stdout 文件失败: {}", e))?
            .into_raw_fd();
        let stderr_fd = OpenOptions::new()
            .create(true)
            .append(true)
            .open(&config.stderr_file)
            .map_err(|e| format!("打开 stderr 文件失败: {}", e))?
            .into_raw_fd();

        libc::dup2(stdin_fd, 0);
        libc::dup2(stdout_fd, 1);
        libc::dup2(stderr_fd, 2);

        // 关闭原始文件描述符（dup2 后不再需要）
        let _ = libc::close(stdin_fd);
        let _ = libc::close(stdout_fd);
        let _ = libc::close(stderr_fd);
    }

    // 写入 PID 文件
    let pid = std::process::id();
    let mut file = File::create(&config.pid_file)
        .map_err(|e| format!("创建 PID 文件失败: {}", e))?;
    writeln!(file, "{}", pid).map_err(|e| format!("写入 PID 文件失败: {}", e))?;

    tracing::info!("守护进程已启动，PID: {}", pid);
    Ok(())
}

/// 清理 PID 文件
pub fn cleanup_pid_file(pid_file: &str) {
    let _ = std::fs::remove_file(pid_file);
}
