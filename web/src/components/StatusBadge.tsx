'use client';

import React from 'react';

/**
 * 状态标签组件
 * 用于展示各种状态的彩色标签
 */
interface StatusBadgeProps {
  /** 状态文本 */
  status: string;
  /** 自定义样式类名 */
  className?: string;
}

export default function StatusBadge({ status, className = '' }: StatusBadgeProps) {
  const { badgeClass } = getStatusConfig(status);

  return (
    <span className={`badge ${badgeClass} ${className}`}>
      {status}
    </span>
  );
}

/** 获取状态配置 */
function getStatusConfig(status: string): { badgeClass: string } {
  const configs: Record<string, { badgeClass: string }> = {
    // 任务状态
    Pending: { badgeClass: 'bg-gray-500/20 text-gray-400' },
    Running: { badgeClass: 'bg-blue-500/20 text-blue-400' },
    Paused: { badgeClass: 'bg-yellow-500/20 text-yellow-400' },
    Completed: { badgeClass: 'bg-green-500/20 text-green-400' },
    Failed: { badgeClass: 'bg-red-500/20 text-red-400' },
    Cancelled: { badgeClass: 'bg-gray-500/20 text-gray-400' },
    // Agent 状态
    Idle: { badgeClass: 'bg-green-500/20 text-green-400' },
    Busy: { badgeClass: 'bg-blue-500/20 text-blue-400' },
    Error: { badgeClass: 'bg-red-500/20 text-red-400' },
    Offline: { badgeClass: 'bg-gray-500/20 text-gray-400' },
    // 连接状态
    connected: { badgeClass: 'bg-green-500/20 text-green-400' },
    connecting: { badgeClass: 'bg-yellow-500/20 text-yellow-400' },
    disconnected: { badgeClass: 'bg-gray-500/20 text-gray-400' },
  };

  return configs[status] || { badgeClass: 'bg-gray-500/20 text-gray-400' };
}
