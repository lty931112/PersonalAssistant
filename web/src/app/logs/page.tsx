'use client';

import React, { useCallback, useEffect, useMemo, useRef, useState } from 'react';
import { getLogsStreamUrl } from '@/lib/api';

const MAX_LINES = 5000;

type LogKind = 'all' | 'event' | 'model' | 'tool' | 'info' | 'error';

const TAGS: Record<Exclude<LogKind, 'all'>, string> = {
  event: '[事件]',
  model: '[模型]',
  tool: '[工具]',
  info: '[信息]',
  error: '[错误]',
};

function lineMatchesKind(line: string, kind: LogKind): boolean {
  if (kind === 'all') return true;
  const tag = TAGS[kind];
  if (kind === 'error') {
    return line.includes('[错误]') || line.includes(' ERROR ') || line.includes('\tERROR ');
  }
  return line.includes(tag);
}

function lineClassName(line: string): string {
  if (line.includes('[错误]') || line.includes(' ERROR ') || line.includes('\tERROR ')) {
    return 'text-red-400/95';
  }
  if (line.includes(' WARN ') || line.includes('\tWARN ')) {
    return 'text-amber-300/90';
  }
  if (line.includes('[模型]')) return 'text-sky-300/95';
  if (line.includes('[工具]')) return 'text-emerald-300/95';
  if (line.includes('[信息]') || line.includes('[事件]')) return 'text-violet-300/90';
  return 'text-foreground/90';
}

/**
 * 实时日志：订阅网关 GET /api/logs/stream（SSE），与后台 tracing 输出一致。
 */
export default function LogsPage() {
  const [lines, setLines] = useState<string[]>([]);
  const [connected, setConnected] = useState(false);
  const [paused, setPaused] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [kindFilter, setKindFilter] = useState<LogKind>('all');
  const esRef = useRef<EventSource | null>(null);
  const bottomRef = useRef<HTMLDivElement | null>(null);
  const pausedRef = useRef(false);
  pausedRef.current = paused;

  const visibleLines = useMemo(
    () => lines.filter((ln) => lineMatchesKind(ln, kindFilter)),
    [lines, kindFilter],
  );

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
  }, [visibleLines, paused, scrollToBottom]);

  const clear = () => setLines([]);

  const filterButtons: { id: LogKind; label: string }[] = [
    { id: 'all', label: '全部' },
    { id: 'event', label: '事件' },
    { id: 'model', label: '模型' },
    { id: 'tool', label: '工具' },
    { id: 'info', label: '信息' },
    { id: 'error', label: '错误' },
  ];

  return (
    <div className="h-full flex flex-col overflow-hidden p-6">
      <div className="max-w-[1600px] mx-auto w-full flex flex-col flex-1 min-h-0">
        <div className="mb-4 flex flex-wrap items-center justify-between gap-3">
          <div>
            <h1 className="text-2xl font-bold text-foreground">日志监控</h1>
            <p className="text-sm text-muted-foreground mt-1">
              实时输出网关进程的 tracing（SSE）。标记行：<code className="text-xs">[模型]</code> LLM
              请求/响应，<code className="text-xs">[工具]</code> 工具调用，<code className="text-xs">[信息]</code>{' '}
              流程与预算，<code className="text-xs">[错误]</code> 失败与 <code className="text-xs">ERROR</code>{' '}
              级别。若启用了网关令牌，请先在「设置」中配置。
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

        <div className="mb-2 flex flex-wrap gap-1.5">
          {filterButtons.map((b) => (
            <button
              key={b.id}
              type="button"
              className={`btn btn-xs ${kindFilter === b.id ? 'btn-primary' : 'btn-ghost'}`}
              onClick={() => setKindFilter(b.id)}
            >
              {b.label}
            </button>
          ))}
          <span className="text-xs text-muted-foreground self-center ml-2">
            显示 {visibleLines.length} / {lines.length} 行
          </span>
        </div>

        {error && (
          <div className="mb-3 text-sm text-amber-400 bg-amber-500/10 border border-amber-500/30 rounded-md px-3 py-2">
            {error}
          </div>
        )}

        <pre className="flex-1 min-h-0 overflow-auto rounded-lg border border-border bg-card p-4 text-xs font-mono whitespace-pre-wrap break-all">
          {visibleLines.length === 0 ? (
            <span className="text-muted-foreground">
              {lines.length === 0 ? '等待日志输出…' : '当前筛选下无匹配行，请换一类试试。'}
            </span>
          ) : (
            visibleLines.map((line, i) => (
              <span key={`${i}-${line.slice(0, 32)}`} className={lineClassName(line)}>
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
