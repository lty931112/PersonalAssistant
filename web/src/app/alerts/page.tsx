'use client';

import React, { useState, useEffect, useCallback } from 'react';
import { useApp } from '@/lib/store';
import type { AlertRecord, AlertLevel } from '@/lib/types';

/**
 * 告警页面
 *
 * 展示系统告警记录列表，支持按级别筛选和自动刷新。
 * 告警数据来源于全局状态（由 WebSocket 事件推送和 API 轮询填充）。
 */
export default function AlertsPage() {
  const { state } = useApp();
  const [levelFilter, setLevelFilter] = useState<AlertLevel | 'all'>('all');
  const [autoRefresh, setAutoRefresh] = useState(true);

  /** 筛选后的告警列表 */
  const filteredAlerts = levelFilter === 'all'
    ? state.alerts
    : state.alerts.filter((a) => a.level === levelFilter);

  /** 各级别告警计数 */
  const counts = {
    all: state.alerts.length,
    critical: state.alerts.filter((a) => a.level === 'critical').length,
    warning: state.alerts.filter((a) => a.level === 'warning').length,
    info: state.alerts.filter((a) => a.level === 'info').length,
  };

  return (
    <div className="h-full overflow-y-auto p-6">
      <div className="max-w-[1400px] mx-auto">
        {/* 页面标题 */}
        <div className="mb-6 flex items-center justify-between">
          <div>
            <h1 className="text-2xl font-bold text-foreground">告警中心</h1>
            <p className="text-sm text-muted-foreground mt-1">查看系统告警记录和异常通知</p>
          </div>
          <div className="flex items-center gap-3">
            {/* 自动刷新开关 */}
            <button
              onClick={() => setAutoRefresh(!autoRefresh)}
              className={`btn btn-sm ${autoRefresh ? 'btn-primary' : 'btn-ghost'}`}
            >
              {autoRefresh ? '⏸ 暂停刷新' : '▶ 自动刷新'}
            </button>
          </div>
        </div>

        {/* 统计卡片 */}
        <div className="grid grid-cols-4 gap-4 mb-6">
          <StatCard
            label="全部告警"
            count={counts.all}
            color="bg-blue-500/10 text-blue-400 border-blue-500/20"
          />
          <StatCard
            label="严重"
            count={counts.critical}
            color="bg-red-500/10 text-red-400 border-red-500/20"
          />
          <StatCard
            label="警告"
            count={counts.warning}
            color="bg-yellow-500/10 text-yellow-400 border-yellow-500/20"
          />
          <StatCard
            label="信息"
            count={counts.info}
            color="bg-green-500/10 text-green-400 border-green-500/20"
          />
        </div>

        {/* 筛选栏 */}
        <div className="flex items-center gap-2 mb-4">
          <span className="text-sm text-muted-foreground">筛选：</span>
          {(['all', 'critical', 'warning', 'info'] as const).map((level) => (
            <button
              key={level}
              onClick={() => setLevelFilter(level)}
              className={`btn btn-sm ${levelFilter === level ? 'btn-primary' : 'btn-ghost'}`}
            >
              {level === 'all' ? '全部' : level === 'critical' ? '🔴 严重' : level === 'warning' ? '🟡 警告' : '🔵 信息'}
              {counts[level] > 0 && (
                <span className="ml-1.5 px-1.5 py-0.5 rounded-full bg-white/10 text-xs">
                  {counts[level]}
                </span>
              )}
            </button>
          ))}
        </div>

        {/* 告警列表 */}
        <div className="space-y-2">
          {filteredAlerts.length === 0 ? (
            <div className="card p-12 text-center">
              <div className="text-4xl mb-3">🔔</div>
              <p className="text-muted-foreground">暂无告警记录</p>
              <p className="text-xs text-muted-foreground mt-1">系统运行正常时，告警将显示在这里</p>
            </div>
          ) : (
            filteredAlerts.map((alert) => (
              <AlertCard key={alert.id} alert={alert} />
            ))
          )}
        </div>
      </div>
    </div>
  );
}

// ============================================================
// 子组件
// ============================================================

/** 统计卡片 */
function StatCard({ label, count, color }: { label: string; count: number; color: string }) {
  return (
    <div className={`card p-4 border ${color}`}>
      <p className="text-xs font-medium opacity-80">{label}</p>
      <p className="text-2xl font-bold mt-1">{count}</p>
    </div>
  );
}

/** 告警卡片 */
function AlertCard({ alert }: { alert: AlertRecord }) {
  const levelConfig = {
    info: {
      icon: '🔵',
      label: '信息',
      badgeClass: 'bg-blue-500/20 text-blue-400',
      borderClass: 'border-l-blue-500',
    },
    warning: {
      icon: '🟡',
      label: '警告',
      badgeClass: 'bg-yellow-500/20 text-yellow-400',
      borderClass: 'border-l-yellow-500',
    },
    critical: {
      icon: '🔴',
      label: '严重',
      badgeClass: 'bg-red-500/20 text-red-400',
      borderClass: 'border-l-red-500',
    },
  };

  const config = levelConfig[alert.level];

  return (
    <div className={`card p-4 border-l-4 ${config.borderClass}`}>
      <div className="flex items-start justify-between">
        <div className="flex items-start gap-3 flex-1">
          <span className="text-lg mt-0.5">{config.icon}</span>
          <div className="flex-1 min-w-0">
            <div className="flex items-center gap-2 mb-1">
              <span className={`badge ${config.badgeClass}`}>{config.label}</span>
              <span className="text-xs text-muted-foreground font-mono">{alert.alert_type}</span>
            </div>
            <p className="text-sm font-medium text-foreground">{alert.title}</p>
            <p className="text-sm text-muted-foreground mt-1">{alert.message}</p>
          </div>
        </div>
        <span className="text-xs text-muted-foreground whitespace-nowrap ml-4">
          {formatAlertTime(alert.timestamp)}
        </span>
      </div>
    </div>
  );
}

/** 格式化告警时间 */
function formatAlertTime(timestamp: string): string {
  try {
    const date = new Date(timestamp);
    const now = new Date();
    const diffMs = now.getTime() - date.getTime();
    const diffSec = Math.floor(diffMs / 1000);
    const diffMin = Math.floor(diffSec / 60);
    const diffHour = Math.floor(diffMin / 60);

    if (diffSec < 60) return `${diffSec}秒前`;
    if (diffMin < 60) return `${diffMin}分钟前`;
    if (diffHour < 24) return `${diffHour}小时前`;

    return date.toLocaleString('zh-CN', {
      month: '2-digit',
      day: '2-digit',
      hour: '2-digit',
      minute: '2-digit',
    });
  } catch {
    return timestamp;
  }
}
