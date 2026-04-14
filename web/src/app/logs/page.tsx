'use client';

import React, { useCallback, useEffect, useRef, useState } from 'react';
import { getLogsStreamUrl } from '@/lib/api';

const MAX_LINES = 5000;

/**
 * 实时日志：订阅网关 GET /api/logs/stream（SSE），与后台 tracing 输出一致。
 */
export default function LogsPage() {
  const [lines, setLines] = useState<string[]>([]);
  const [connected, setConnected] = useState(false);
  const [paused, setPaused] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const esRef = useRef<EventSource | null>(null);
  const bottomRef = useRef<HTMLDivElement | null>(null);
  const pausedRef = useRef(false);
  pausedRef.current = paused;

  const scrollToBottom = useCallback(() => {
    bottomRef.current?.scrollIntoView({ behavior: 'smooth' });
  }, []);

  const connect = useCallback(() => {
    esRef.current?.close();
    setError(null);
    const url = getLogsStreamUrl();
    const es = new EventSource(url);
    esRef.current = es;

    es.onopen = () => {
      setConnected(true);
    };

    es.onmessage = (ev) => {
      if (pausedRef.current) return;
      const text = ev.data ?? '';
      setLines((prev) => {
        const next = [...prev, text];
        if (next.length > MAX_LINES) {
          return next.slice(next.length - MAX_LINES);
        }
        return next;
      });
    };

    es.onerror = () => {
      setConnected(false);
      setError('连接中断或网关返回错误，将自动重试（也可手动重连）');
      es.close();
    };
  }, []);

  useEffect(() => {
    connect();
    return () => {
      esRef.current?.close();
      esRef.current = null;
    };
  }, [connect]);

  useEffect(() => {
    if (!paused) {
      scrollToBottom();
    }
  }, [lines, paused, scrollToBottom]);

  const clear = () => setLines([]);

  return (
    <div className="h-full flex flex-col overflow-hidden p-6">
      <div className="max-w-[1600px] mx-auto w-full flex flex-col flex-1 min-h-0">
        <div className="mb-4 flex flex-wrap items-center justify-between gap-3">
          <div>
            <h1 className="text-2xl font-bold text-foreground">日志监控</h1>
            <p className="text-sm text-muted-foreground mt-1">
              实时输出与网关进程控制台相同的 tracing 日志（SSE）。若启用了网关令牌，请先在「设置」中配置。
            </p>
          </div>
          <div className="flex flex-wrap items-center gap-2">
            <span
              className={`text-xs px-2 py-1 rounded ${
                connected ? 'bg-green-500/20 text-green-400' : 'bg-red-500/20 text-red-400'
              }`}
            >
              {connected ? '已连接' : '未连接'}
            </span>
            <button
              type="button"
              className={`btn btn-sm ${paused ? 'btn-primary' : 'btn-ghost'}`}
              onClick={() => setPaused((p) => !p)}
            >
              {paused ? '继续' : '暂停'}
            </button>
            <button type="button" className="btn btn-secondary btn-sm" onClick={clear}>
              清空
            </button>
            <button type="button" className="btn btn-secondary btn-sm" onClick={connect}>
              重连
            </button>
          </div>
        </div>

        {error && (
          <div className="mb-3 text-sm text-amber-400 bg-amber-500/10 border border-amber-500/30 rounded-md px-3 py-2">
            {error}
          </div>
        )}

        <pre className="flex-1 min-h-0 overflow-auto rounded-lg border border-border bg-card p-4 text-xs font-mono text-foreground/90 whitespace-pre-wrap break-all">
          {lines.length === 0 ? (
            <span className="text-muted-foreground">等待日志输出…</span>
          ) : (
            lines.map((line, i) => (
              <span key={`${i}-${line.slice(0, 24)}`}>
                {line}
                {'\n'}
              </span>
            ))
          )}
          <div ref={bottomRef} />
        </pre>
      </div>
    </div>
  );
}
