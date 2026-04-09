'use client';

import React from 'react';
import type { TaskInfo, TaskEvent } from '@/lib/types';

/**
 * 任务详情弹窗组件
 * 展示任务详情和事件时间线
 */
interface TaskDetailProps {
  task: TaskInfo;
  events: TaskEvent[];
  onClose: () => void;
}

export default function TaskDetail({ task, events, onClose }: TaskDetailProps) {
  return (
    <div className="modal-overlay" onClick={onClose}>
      <div className="modal-content" onClick={(e) => e.stopPropagation()}>
        {/* 头部 */}
        <div className="flex items-center justify-between mb-4">
          <h2 className="text-lg font-semibold text-foreground">任务详情</h2>
          <button
            onClick={onClose}
            className="btn btn-ghost btn-sm"
          >
            <svg className="w-4 h-4" fill="none" stroke="currentColor" viewBox="0 0 24 24">
              <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M6 18L18 6M6 6l12 12" />
            </svg>
          </button>
        </div>

        {/* 任务基本信息 */}
        <div className="card p-4 mb-4 space-y-3">
          <div className="grid grid-cols-2 gap-3 text-sm">
            <div>
              <span className="text-muted-foreground">任务 ID</span>
              <p className="font-mono text-xs mt-0.5">{task.id}</p>
            </div>
            <div>
              <span className="text-muted-foreground">状态</span>
              <p className="mt-0.5">
                <span className={`badge ${getStatusBadgeClass(task.status)}`}>
                  {getStatusLabel(task.status)}
                </span>
              </p>
            </div>
            <div>
              <span className="text-muted-foreground">Agent</span>
              <p className="mt-0.5">{task.agent_id}</p>
            </div>
            <div>
              <span className="text-muted-foreground">优先级</span>
              <p className="mt-0.5">{getPriorityLabel(task.priority)}</p>
            </div>
            <div>
              <span className="text-muted-foreground">创建时间</span>
              <p className="mt-0.5">{formatDateTime(task.created_at)}</p>
            </div>
            <div>
              <span className="text-muted-foreground">更新时间</span>
              <p className="mt-0.5">{formatDateTime(task.updated_at)}</p>
            </div>
            {task.started_at && (
              <div>
                <span className="text-muted-foreground">开始时间</span>
                <p className="mt-0.5">{formatDateTime(task.started_at)}</p>
              </div>
            )}
            {task.completed_at && (
              <div>
                <span className="text-muted-foreground">完成时间</span>
                <p className="mt-0.5">{formatDateTime(task.completed_at)}</p>
              </div>
            )}
          </div>

          {/* 提示词 */}
          <div>
            <span className="text-muted-foreground text-sm">提示词</span>
            <p className="text-sm mt-0.5 p-2 bg-muted rounded">{task.prompt}</p>
          </div>

          {/* 统计信息 */}
          <div className="grid grid-cols-3 gap-3">
            <div className="text-center p-2 bg-muted rounded">
              <div className="text-lg font-semibold text-foreground">{task.turn_count}</div>
              <div className="text-xs text-muted-foreground">回合数</div>
            </div>
            <div className="text-center p-2 bg-muted rounded">
              <div className="text-lg font-semibold text-foreground">
                {formatTokenCount(task.total_input_tokens + task.total_output_tokens)}
              </div>
              <div className="text-xs text-muted-foreground">Token 用量</div>
            </div>
            <div className="text-center p-2 bg-muted rounded">
              <div className="text-lg font-semibold text-foreground">${task.cost_usd.toFixed(4)}</div>
              <div className="text-xs text-muted-foreground">费用</div>
            </div>
          </div>

          {/* 错误信息 */}
          {task.error && (
            <div className="p-2 bg-red-500/10 border border-red-500/20 rounded text-sm text-red-400">
              {task.error}
            </div>
          )}
        </div>

        {/* 事件时间线 */}
        <div>
          <h3 className="text-sm font-semibold text-foreground mb-3">事件时间线</h3>
          {events.length === 0 ? (
            <p className="text-sm text-muted-foreground text-center py-4">暂无事件记录</p>
          ) : (
            <div className="space-y-0">
              {events.map((event, index) => (
                <div key={event.id || index} className="flex gap-3 pb-3">
                  {/* 时间线节点 */}
                  <div className="flex flex-col items-center">
                    <div className={`w-2.5 h-2.5 rounded-full mt-1.5 shrink-0 ${getEventDotColor(event.event_type)}`} />
                    {index < events.length - 1 && (
                      <div className="w-px flex-1 bg-border mt-1" />
                    )}
                  </div>
                  {/* 事件内容 */}
                  <div className="flex-1 min-w-0">
                    <div className="flex items-center gap-2">
                      <span className="text-sm font-medium text-foreground">{event.event_type}</span>
                      <span className="text-xs text-muted-foreground">
                        {formatDateTime(event.timestamp)}
                      </span>
                    </div>
                    <div className="text-xs text-muted-foreground mt-0.5 truncate">
                      {JSON.stringify(event.data)}
                    </div>
                  </div>
                </div>
              ))}
            </div>
          )}
        </div>
      </div>
    </div>
  );
}

/** 获取状态徽章样式 */
function getStatusBadgeClass(status: string): string {
  const map: Record<string, string> = {
    Pending: 'bg-gray-500/20 text-gray-400',
    Running: 'bg-blue-500/20 text-blue-400',
    Paused: 'bg-yellow-500/20 text-yellow-400',
    Completed: 'bg-green-500/20 text-green-400',
    Failed: 'bg-red-500/20 text-red-400',
    Cancelled: 'bg-gray-500/20 text-gray-400',
  };
  return map[status] || 'bg-gray-500/20 text-gray-400';
}

/** 获取状态中文标签 */
function getStatusLabel(status: string): string {
  const map: Record<string, string> = {
    Pending: '等待中',
    Running: '运行中',
    Paused: '已暂停',
    Completed: '已完成',
    Failed: '失败',
    Cancelled: '已取消',
  };
  return map[status] || status;
}

/** 获取优先级中文标签 */
function getPriorityLabel(priority: string): string {
  const map: Record<string, string> = {
    Low: '低',
    Medium: '中',
    High: '高',
    Critical: '紧急',
  };
  return map[priority] || priority;
}

/** 获取事件节点颜色 */
function getEventDotColor(eventType: string): string {
  if (eventType.includes('Error') || eventType.includes('Fail')) return 'bg-red-500';
  if (eventType.includes('Complete') || eventType.includes('Success')) return 'bg-green-500';
  if (eventType.includes('Start') || eventType.includes('Stream')) return 'bg-blue-500';
  if (eventType.includes('Tool')) return 'bg-purple-500';
  return 'bg-gray-500';
}

/** 格式化日期时间 */
function formatDateTime(dateStr: string): string {
  return new Date(dateStr).toLocaleString('zh-CN', {
    year: 'numeric',
    month: '2-digit',
    day: '2-digit',
    hour: '2-digit',
    minute: '2-digit',
    second: '2-digit',
  });
}

/** 格式化 Token 数量 */
function formatTokenCount(count: number): string {
  if (count >= 1000000) return (count / 1000000).toFixed(1) + 'M';
  if (count >= 1000) return (count / 1000).toFixed(1) + 'K';
  return String(count);
}
