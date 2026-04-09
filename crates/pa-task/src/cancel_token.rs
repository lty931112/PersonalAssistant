//! 取消令牌模块
//!
//! 基于 `tokio::sync::watch` 实现的取消信号机制。
//! 用于在任务执行过程中传递取消请求，支持异步等待取消信号。

use std::sync::Arc;

use tokio::sync::watch;

/// 取消令牌
///
/// 基于 `tokio::sync::watch` channel 的取消信号。
/// 多个持有者可以共享同一个令牌，任何一个调用 `cancel()` 都会通知所有等待者。
///
/// # 示例
///
/// ```ignore
/// let token = CancellationToken::new();
/// let token_clone = token.clone();
///
/// // 在另一个任务中检查取消状态
/// tokio::spawn(async move {
///     if token_clone.is_cancelled() {
///         println!("任务被取消");
///         return;
///     }
///     // 执行工作...
/// });
///
/// // 触发取消
/// token.cancel();
/// ```
#[derive(Clone)]
pub struct CancellationToken {
    /// 内部 watch channel 的发送端
    sender: watch::Sender<bool>,
    /// 内部 watch channel 的接收端
    receiver: watch::Receiver<bool>,
}

impl CancellationToken {
    /// 创建新的取消令牌
    ///
    /// 初始状态为"未取消"。
    pub fn new() -> Self {
        let (sender, receiver) = watch::channel(false);
        Self { sender, receiver }
    }

    /// 检查令牌是否已被取消
    pub fn is_cancelled(&self) -> bool {
        *self.receiver.borrow()
    }

    /// 触发取消
    ///
    /// 将令牌状态设置为"已取消"，并通知所有等待者。
    /// 如果令牌已经被取消，此方法不会产生任何效果。
    pub fn cancel(&self) {
        let _ = self.sender.send(true);
    }

    /// 等待取消信号
    ///
    /// 返回一个 Future，当令牌被取消时完成。
    /// 如果令牌已经被取消，Future 会立即完成。
    pub async fn cancelled(&self) {
        let mut receiver = self.receiver.clone();
        // 如果已经取消，立即返回
        if *receiver.borrow() {
            return;
        }
        // 等待值变为 true
        let _ = receiver.changed().await;
    }
}

impl Default for CancellationToken {
    fn default() -> Self {
        Self::new()
    }
}

impl std::fmt::Debug for CancellationToken {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("CancellationToken")
            .field("is_cancelled", &self.is_cancelled())
            .finish()
    }
}

/// 共享取消令牌的包装类型
///
/// 使用 `Arc` 包装的 `CancellationToken`，便于在多个任务之间共享。
pub type SharedCancellationToken = Arc<CancellationToken>;

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_cancel_token_basic() {
        let token = CancellationToken::new();
        assert!(!token.is_cancelled());

        token.cancel();
        assert!(token.is_cancelled());
    }

    #[tokio::test]
    async fn test_cancel_token_clone() {
        let token = CancellationToken::new();
        let token_clone = token.clone();

        token.cancel();
        assert!(token_clone.is_cancelled());
    }

    #[tokio::test]
    async fn test_cancel_token_wait() {
        let token = CancellationToken::new();
        let token_clone = token.clone();

        let handle = tokio::spawn(async move {
            token_clone.cancelled().await;
            assert!(token_clone.is_cancelled());
        });

        // 给异步任务一点时间启动
        tokio::task::yield_now().await;
        token.cancel();

        handle.await.unwrap();
    }

    #[tokio::test]
    async fn test_cancel_token_already_cancelled() {
        let token = CancellationToken::new();
        token.cancel();

        // 已经取消时，cancelled() 应该立即返回
        token.cancelled().await;
        assert!(token.is_cancelled());
    }
}
