'use client';

import React, { useState, useEffect, useCallback } from 'react';
import { getMetrics } from '@/lib/api';
import type { MetricsData } from '@/lib/types';

/**
 * 系统监控页面
 *
 * 展示系统资源使用情况和 Prometheus 指标：
 * - 进程信息：运行时间、请求数、连接数
 * - 任务统计：完成数、失败数、运行中
 * - 系统资源：CPU、内存使用率（进度条可视化）
 * - 进程资源：内存占用、CPU、线程数、文件描述符
 * - 原始 Prometheus 指标（可展开查看）
 */
export default function MonitorPage() {
  const [metrics, setMetrics] = useState<MetricsData | null>(null);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);
  const [autoRefresh, setAutoRefresh] = useState(true);
  const [showRaw, setShowRaw] = useState(false);

  /** 加载指标数据 */
  const loadMetrics = useCallback(async () => {
    try {
      const data = await getMetrics();
      setMetrics(data);
      setError(null);
    } catch (e) {
      setError(e instanceof Error ? e.message : '获取指标失败');
    } finally {
      setLoading(false);
    }
  }, []);

  // 初始加载
  useEffect(() => {
    loadMetrics();
  }, [loadMetrics]);

  // 自动刷新（每 5 秒）
  useEffect(() => {
    if (!autoRefresh) return;
    const interval = setInterval(loadMetrics, 5000);
    return () => clearInterval(interval);
  }, [autoRefresh, loadMetrics]);

  return (
    <div className="h-full overflow-y-auto p-6">
      <div className="max-w-[1400px] mx-auto">
        {/* 页面标题 */}
        <div className="mb-6 flex items-center justify-between">
          <div>
            <h1 className="text-2xl font-bold text-foreground">系统监控</h1>
            <p className="text-sm text-muted-foreground mt-1">实时系统资源和 Prometheus 指标</p>
          </div>
          <div className="flex items-center gap-3">
            <button
              onClick={() => setAutoRefresh(!autoRefresh)}
              className={`btn btn-sm ${autoRefresh ? 'btn-primary' : 'btn-ghost'}`}
            >
              {autoRefresh ? '⏸ 暂停刷新' : '▶ 自动刷新'}
            </button>
            <button onClick={loadMetrics} className="btn btn-secondary btn-sm">
              🔄 刷新
            </button>
          </div>
        </div>

        {loading && !metrics ? (
          <div className="card p-12 text-center">
            <div className="text-4xl mb-3">📊</div>
            <p className="text-muted-foreground">正在加载监控数据...</p>
          </div>
        ) : error && !metrics ? (
          <div className="card p-12 text-center">
            <div className="text-4xl mb-3">⚠️</div>
            <p className="text-red-400">{error}</p>
            <button onClick={loadMetrics} className="btn btn-secondary btn-sm mt-4">重试</button>
          </div>
        ) : metrics ? (
          <div className="space-y-6">
            {/* 进程信息 */}
            <section className="card p-6">
              <h2 className="text-lg font-semibold text-foreground mb-4">进程信息</h2>
              <div className="grid grid-cols-3 gap-4">
                <MetricCard label="运行时间" value={formatUptime(metrics.process.uptime_seconds)} icon="⏱️" />
                <MetricCard label="请求总数" value={metrics.process.requests_total.toLocaleString()} icon="📨" />
                <MetricCard label="活跃连接" value={metrics.process.active_connections.toString()} icon="🔗" />
              </div>
            </section>

            {/* 任务统计 */}
            <section className="card p-6">
              <h2 className="text-lg font-semibold text-foreground mb-4">任务统计</h2>
              <div className="grid grid-cols-3 gap-4">
                <MetricCard label="已完成" value={metrics.tasks.completed_total.toLocaleString()} icon="✅" color="text-green-400" />
                <MetricCard label="已失败" value={metrics.tasks.failed_total.toLocaleString()} icon="❌" color="text-red-400" />
                <MetricCard label="运行中" value={metrics.tasks.running.toString()} icon="🔄" color="text-blue-400" />
              </div>
            </section>

            {/* 系统资源 */}
            <section className="card p-6">
              <h2 className="text-lg font-semibold text-foreground mb-4">系统资源</h2>
              <div className="space-y-5">
                <ProgressBar
                  label="CPU 使用率"
                  value={metrics.system.cpu_usage}
                  max={100}
                  unit="%"
                  color={getUsageColor(metrics.system.cpu_usage, 100)}
                />
                <ProgressBar
                  label="内存使用"
                  value={metrics.system.memory_used_bytes}
                  max={metrics.system.memory_total_bytes}
                  unit=""
                  formatValue={() => `${formatBytes(metrics.system.memory_used_bytes)} / ${formatBytes(metrics.system.memory_total_bytes)}`}
                  color={getUsageColor(metrics.system.memory_used_bytes, metrics.system.memory_total_bytes)}
                />
                <ProgressBar
                  label="可用内存"
                  value={metrics.system.memory_available_bytes}
                  max={metrics.system.memory_total_bytes}
                  unit=""
                  formatValue={() => formatBytes(metrics.system.memory_available_bytes)}
                  color="bg-green-500"
                />
              </div>
            </section>

            {/* 进程资源 */}
            <section className="card p-6">
              <h2 className="text-lg font-semibold text-foreground mb-4">进程资源</h2>
              <div className="grid grid-cols-2 gap-4">
                <div className="p-4 bg-secondary rounded-lg">
                  <p className="text-xs text-muted-foreground mb-1">内存占用</p>
                  <p className="text-lg font-bold text-foreground">{formatBytes(metrics.process.memory_bytes)}</p>
                </div>
                <div className="p-4 bg-secondary rounded-lg">
                  <p className="text-xs text-muted-foreground mb-1">CPU 使用率</p>
                  <p className="text-lg font-bold text-foreground">{metrics.process.cpu_usage.toFixed(1)}%</p>
                </div>
                <div className="p-4 bg-secondary rounded-lg">
                  <p className="text-xs text-muted-foreground mb-1">线程数</p>
                  <p className="text-lg font-bold text-foreground">{metrics.process.threads}</p>
                </div>
                <div className="p-4 bg-secondary rounded-lg">
                  <p className="text-xs text-muted-foreground mb-1">文件描述符</p>
                  <p className="text-lg font-bold text-foreground">{metrics.process.open_fds}</p>
                </div>
              </div>
            </section>

            {/* 原始 Prometheus 指标 */}
            <section className="card p-6">
              <div className="flex items-center justify-between mb-4">
                <h2 className="text-lg font-semibold text-foreground">Prometheus 指标</h2>
                <button
                  onClick={() => setShowRaw(!showRaw)}
                  className="btn btn-ghost btn-sm"
                >
                  {showRaw ? '收起 ▲' : '展开 ▼'}
                </button>
              </div>
              {showRaw && (
                <pre className="bg-secondary rounded-lg p-4 text-xs font-mono text-muted-foreground overflow-x-auto max-h-96 overflow-y-auto">
                  {metrics.raw}
                </pre>
              )}
              {!showRaw && (
                <p className="text-sm text-muted-foreground">
                  点击展开查看 Prometheus 格式的原始指标数据，可用于对接 Grafana 等监控平台。
                </p>
              )}
            </section>
          </div>
        ) : null}
      </div>
    </div>
  );
}

// ============================================================
// 子组件
// ============================================================

/** 指标卡片 */
function MetricCard({
  label,
  value,
  icon,
  color = 'text-foreground',
}: {
  label: string;
  value: string;
  icon: string;
  color?: string;
}) {
  return (
    <div className="p-4 bg-secondary rounded-lg">
      <div className="flex items-center gap-2 mb-1">
        <span>{icon}</span>
        <p className="text-xs text-muted-foreground">{label}</p>
      </div>
      <p className={`text-xl font-bold ${color}`}>{value}</p>
    </div>
  );
}

/** 进度条 */
function ProgressBar({
  label,
  value,
  max,
  unit,
  color = 'bg-primary',
  formatValue,
}: {
  label: string;
  value: number;
  max: number;
  unit: string;
  color?: string;
  formatValue?: () => string;
}) {
  const percentage = max > 0 ? Math.min((value / max) * 100, 100) : 0;
  const displayValue = formatValue ? formatValue() : `${value.toFixed(1)}${unit}`;

  return (
    <div>
      <div className="flex items-center justify-between mb-1.5">
        <span className="text-sm text-muted-foreground">{label}</span>
        <span className="text-sm font-medium text-foreground">{displayValue}</span>
      </div>
      <div className="w-full h-2.5 bg-secondary rounded-full overflow-hidden">
        <div
          className={`h-full rounded-full transition-all duration-500 ${color}`}
          style={{ width: `${percentage}%` }}
        />
      </div>
    </div>
  );
}

// ============================================================
// 工具函数
// ============================================================

/** 格式化运行时间 */
function formatUptime(seconds: number): string {
  if (seconds < 60) return `${Math.floor(seconds)}秒`;
  if (seconds < 3600) return `${Math.floor(seconds / 60)}分${Math.floor(seconds % 60)}秒`;
  const hours = Math.floor(seconds / 3600);
  const minutes = Math.floor((seconds % 3600) / 60);
  if (hours < 24) return `${hours}小时${minutes}分`;
  const days = Math.floor(hours / 24);
  return `${days}天${hours % 24}小时`;
}

/** 格式化字节数 */
function formatBytes(bytes: number): string {
  if (bytes === 0) return '0 B';
  const k = 1024;
  const sizes = ['B', 'KB', 'MB', 'GB', 'TB'];
  const i = Math.floor(Math.log(bytes) / Math.log(k));
  return `${(bytes / Math.pow(k, i)).toFixed(1)} ${sizes[i]}`;
}

/** 根据使用率返回颜色 */
function getUsageColor(used: number, total: number): string {
  const percentage = total > 0 ? (used / total) * 100 : 0;
  if (percentage < 60) return 'bg-green-500';
  if (percentage < 80) return 'bg-yellow-500';
  return 'bg-red-500';
}
