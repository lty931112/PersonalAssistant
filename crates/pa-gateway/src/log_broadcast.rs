//! 将 tracing 格式化输出复制到广播通道，供 SSE 实时推送到前端。

use std::io::{self, Write};
use tokio::sync::broadcast;
use tracing_subscriber::fmt::writer::MakeWriter;

/// 与 Gateway 共享：一份 `tracing` 写副本、多路 `subscribe` 消费。
#[derive(Clone)]
pub struct LogBroadcast {
    tx: broadcast::Sender<String>,
}

impl LogBroadcast {
    pub fn new(capacity: usize) -> Self {
        let (tx, _) = broadcast::channel(capacity);
        Self { tx }
    }

    pub fn subscribe(&self) -> broadcast::Receiver<String> {
        self.tx.subscribe()
    }

    /// 供 `tracing_subscriber::fmt::layer().with_writer(...)` 使用。
    pub fn make_writer(&self) -> BroadcastMakeWriter {
        BroadcastMakeWriter {
            tx: self.tx.clone(),
        }
    }
}

/// [`MakeWriter`]：每条 fmt 记录可能多次 `write`，按行切分后 `send`。
#[derive(Clone)]
pub struct BroadcastMakeWriter {
    tx: broadcast::Sender<String>,
}

impl<'a> MakeWriter<'a> for BroadcastMakeWriter {
    type Writer = BroadcastWriter;
    fn make_writer(&'a self) -> Self::Writer {
        BroadcastWriter {
            tx: self.tx.clone(),
            stderr: io::stderr(),
            line: Vec::new(),
        }
    }
}

pub struct BroadcastWriter {
    tx: broadcast::Sender<String>,
    stderr: io::Stderr,
    line: Vec<u8>,
}

impl BroadcastWriter {
    fn emit_line(&mut self) {
        if self.line.is_empty() {
            return;
        }
        let s = String::from_utf8_lossy(&self.line).to_string();
        self.line.clear();
        if !s.is_empty() {
            let _ = self.tx.send(s);
        }
    }
}

impl Write for BroadcastWriter {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        let _ = self.stderr.write_all(buf);
        for &b in buf {
            if b == b'\n' {
                self.emit_line();
            } else {
                self.line.push(b);
            }
        }
        Ok(buf.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        self.stderr.flush()
    }
}

impl Drop for BroadcastWriter {
    fn drop(&mut self) {
        if !self.line.is_empty() {
            let s = String::from_utf8_lossy(&self.line).to_string();
            let _ = self.tx.send(s);
        }
    }
}
