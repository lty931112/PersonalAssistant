//! 构建脚本
//!
//! 在编译时注入版本信息环境变量：
//! - `BUILD_DATE`: 构建日期
//! - `GIT_COMMIT`: Git 提交哈希

use std::process::Command;

fn main() {
    // 构建日期
    let build_date = chrono::Utc::now().format("%Y-%m-%d %H:%M:%S UTC").to_string();
    println!("cargo:rustc-env=BUILD_DATE={}", build_date);

    // Git 提交哈希
    let git_commit = Command::new("git")
        .args(["rev-parse", "--short", "HEAD"])
        .output()
        .ok()
        .and_then(|output| {
            if output.status.success() {
                Some(String::from_utf8_lossy(&output.stdout).trim().to_string())
            } else {
                None
            }
        })
        .unwrap_or_else(|| "unknown".to_string());
    println!("cargo:rustc-env=GIT_COMMIT={}", git_commit);

    // 仅在 git HEAD 变化时重新运行
    println!("cargo:rerun-if-changed=.git/HEAD");
    println!("cargo:rerun-if-changed=.git/refs/");
}
