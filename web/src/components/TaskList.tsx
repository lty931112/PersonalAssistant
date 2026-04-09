'use client';

import React, { useState, useEffect, useCallback } from 'react';
import { useApp } from '@/lib/store';
import TaskDetail from './TaskDetail';
import type { TaskInfo, TaskEvent, TaskFilter } from '@/lib/types';
import { getTaskDetail, pauseTask, resumeTask, cancelTask } from '@/lib/api';

/**
 * 任务列表组件
 * 展示所有任务，支持筛选、操作和查看详情
 */
export default function TaskList() {
  const { state, refreshTasks } = useApp();
  const [filter, setFilter] = useState<TaskFilter>('all');
  const [selectedTask, setSelectedTask] = useState<TaskInfo | null>(null);
  const [taskEvents, setTaskEvents] = useState<TaskEvent[]>([]);
  const [loadingDetail, setLoadingDetail] = useState(false);
  const [actionLoading, setActionLoading] = useState<string | null>(null);

  /** 筛选后的任务列表 */
  const filteredTasks = filter === 'all'
    ? state.tasks
    : state.tasks.filter((t) => t.status === filter);

  /** 定期刷新 */
  useEffect(() => {
    const interval = setInterval(() => {
      refreshTasks();
    }, 5000);
    return () => clearInterval(interval);
  }, [refreshTasks]);

  /** 查看任务详情 */
  const handleViewDetail = useCallback(async (task: TaskInfo) => {
    setLoadingDetail(true);
    try {
      const { events } = await getTaskDetail(task.id);
      setSelectedTask(task);
      setTaskEvents(events);
    } catch (err) {
      console.error('获取任务详情失败:', err);
    } finally {
      setLoadingDetail(false);
    }
  }, []);

  /** 暂停任务 */
  const handlePause = useCallback(async (taskId: string) => {
    setActionLoading(taskId);
    try {
      await pauseTask(taskId);
      await refreshTasks();
    } catch (err) {
      console.error('暂停任务失败:', err);
    } finally {
      setActionLoading(null);
    }
  }, [refreshTasks]);

  /** 恢复任务 */
  const handleResume = useCallback(async (taskId: string) => {
    setActionLoading(taskId);
    try {
      await resumeTask(taskId);
      await refreshTasks();
    } catch (err) {
      console.error('恢复任务失败:', err);
    } finally {
      setActionLoading(null);
    }
  }, [refreshTasks]);

  /** 取消任务 */
  const handleCancel = useCallback(async (taskId: string) => {
    setActionLoading(taskId);
    try {
      await cancelTask(taskId);
      await refreshTasks();
    } catch (err) {
      console.error('取消任务失败:', err);
    } finally {
      setActionLoading(null);
    }
  }, [refreshTasks]);

  /** 筛选按钮 */
  const filterButtons: { value: TaskFilter; label: string }[] = [
    { value: 'all', label: '全部' },
    { value: 'Running', label: '运行中' },
    { value: 'Completed', label: '已完成' },
    { value: 'Failed', label: '失败' },
    { value: 'Cancelled', label: '已取消' },
  ];

  return (
    <div>
      {/* 筛选栏 */}
      <div className="flex items-center gap-2 mb-4">
        {filterButtons.map((btn) => (
          <button
            key={btn.value}
            onClick={() => setFilter(btn.value)}
            className={`btn btn-sm ${
              filter === btn.value ? 'btn-primary' : 'btn-secondary'
            }`}
          >
            {btn.label}
            {btn.value !== 'all' && (
              <span className="ml-1.5 text-xs opacity-70">
                ({state.tasks.filter((t) => t.status === btn.value).length})
              </span>
            )}
          </button>
        ))}
        <div className="flex-1" />
        <button
          onClick={() => refreshTasks()}
          className="btn btn-ghost btn-sm"
          title="刷新"
        >
          <svg className="w-4 h-4" fill="none" stroke="currentColor" viewBox="0 0 24 24">
            <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M4 4v5h.582m15.356 2A8.001 8.001 0 004.582 9m0 0H9m11 11v-5h-.581m0 0a8.003 8.003 0 01-15.357-2m15.357 2H15" />
          </svg>
        </button>
      </div>

      {/* 任务表格 */}
      <div className="table-container">
        <table className="data-table">
          <thead>
            <tr>
              <th>ID</th>
              <th>状态</th>
              <th>Agent</th>
              <th>提示词</th>
              <th>优先级</th>
              <th>创建时间</th>
              <th>耗时</th>
              <th>Token</th>
              <th>费用</th>
              <th>操作</th>
            </tr>
          </thead>
          <tbody>
            {filteredTasks.length === 0 ? (
              <tr>
                <td colSpan={10} className="text-center text-muted-foreground py-8">
                  暂无任务记录
                </td>
              </tr>
            ) : (
              filteredTasks.map((task) => (
                <tr key={task.id}>
                  <td className="font-mono text-xs">{task.id.slice(0, 8)}...</td>
                  <td>
                    <span className={`badge ${getStatusBadgeClass(task.status)}`}>
                      {getStatusLabel(task.status)}
                    </span>
                  </td>
                  <td className="text-sm">{task.agent_id}</td>
                  <td className="text-sm max-w-[200px] truncate" title={task.prompt}>
                    {task.prompt}
                  </td>
                  <td>
                    <span className={`badge ${getPriorityBadgeClass(task.priority)}`}>
                      {getPriorityLabel(task.priority)}
                    </span>
                  </td>
                  <td className="text-xs text-muted-foreground">
                    {formatDateTime(task.created_at)}
                  </td>
                  <td className="text-xs text-muted-foreground">
                    {getDuration(task)}
                  </td>
                  <td className="text-xs text-muted-foreground">
                    {formatTokenCount(task.total_input_tokens + task.total_output_tokens)}
                  </td>
                  <td className="text-xs text-muted-foreground">
                    ${task.cost_usd.toFixed(4)}
                  </td>
                  <td>
                    <div className="flex items-center gap-1">
                      <button
                        onClick={() => handleViewDetail(task)}
                        className="btn btn-ghost btn-sm"
                        title="查看详情"
                        disabled={loadingDetail}
                      >
                        <svg className="w-3.5 h-3.5" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                          <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M15 12a3 3 0 11-6 0 3 3 0 016 0z" />
                          <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M2.458 12C3.732 7.943 7.523 5 12 5c4.478 0 8.268 2.943 9.542 7-1.274 4.057-5.064 7-9.542 7-4.477 0-8.268-2.943-9.542-7z" />
                        </svg>
                      </button>
                      {task.status === 'Running' && (
                        <button
                          onClick={() => handlePause(task.id)}
                          className="btn btn-ghost btn-sm"
                          title="暂停"
                          disabled={actionLoading === task.id}
                        >
                          <svg className="w-3.5 h-3.5" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                            <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M10 9v6m4-6v6m7-3a9 9 0 11-18 0 9 9 0 0118 0z" />
                          </svg>
                        </button>
                      )}
                      {task.status === 'Paused' && (
                        <button
                          onClick={() => handleResume(task.id)}
                          className="btn btn-ghost btn-sm"
                          title="恢复"
                          disabled={actionLoading === task.id}
                        >
                          <svg className="w-3.5 h-3.5" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                            <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M14.752 11.168l-3.197-2.132A1 1 0 0010 9.87v4.263a1 1 0 001.555.832l3.197-2.132a1 1 0 000-1.664z" />
                            <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M21 12a9 9 0 11-18 0 9 9 0 0118 0z" />
                          </svg>
                        </button>
                      )}
                      {(task.status === 'Running' || task.status === 'Paused' || task.status === 'Pending') && (
                        <button
                          onClick={() => handleCancel(task.id)}
                          className="btn btn-ghost btn-sm text-red-400 hover:text-red-300"
                          title="取消"
                          disabled={actionLoading === task.id}
                        >
                          <svg className="w-3.5 h-3.5" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                            <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M6 18L18 6M6 6l12 12" />
                          </svg>
                        </button>
                      )}
                    </div>
                  </td>
                </tr>
              ))
            )}
          </tbody>
        </table>
      </div>

      {/* 任务详情弹窗 */}
      {selectedTask && (
        <TaskDetail
          task={selectedTask}
          events={taskEvents}
          onClose={() => {
            setSelectedTask(null);
            setTaskEvents([]);
          }}
        />
      )}
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

/** 获取优先级徽章样式 */
function getPriorityBadgeClass(priority: string): string {
  const map: Record<string, string> = {
    Low: 'bg-gray-500/20 text-gray-400',
    Medium: 'bg-blue-500/20 text-blue-400',
    High: 'bg-orange-500/20 text-orange-400',
    Critical: 'bg-red-500/20 text-red-400',
  };
  return map[priority] || 'bg-gray-500/20 text-gray-400';
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

/** 计算任务耗时 */
function getDuration(task: TaskInfo): string {
  const start = task.started_at ? new Date(task.started_at).getTime() : null;
  const end = task.completed_at
    ? new Date(task.completed_at).getTime()
    : task.status === 'Running'
    ? Date.now()
    : null;

  if (!start || !end) return '-';
  const seconds = Math.floor((end - start) / 1000);
  if (seconds < 60) return `${seconds}s`;
  const minutes = Math.floor(seconds / 60);
  const remainSeconds = seconds % 60;
  if (minutes < 60) return `${minutes}m ${remainSeconds}s`;
  const hours = Math.floor(minutes / 60);
  const remainMinutes = minutes % 60;
  return `${hours}h ${remainMinutes}m`;
}

/** 格式化日期时间 */
function formatDateTime(dateStr: string): string {
  return new Date(dateStr).toLocaleString('zh-CN', {
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
