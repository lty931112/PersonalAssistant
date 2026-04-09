'use client';

import React, { useEffect } from 'react';
import { useApp } from '@/lib/store';
import type { AgentStatusInfo } from '@/lib/types';

/**
 * Agent 列表组件
 * 展示所有 Agent 的状态信息
 */
export default function AgentList() {
  const { state, refreshAgents } = useApp();

  /** 定期刷新 Agent 列表 */
  useEffect(() => {
    const interval = setInterval(() => {
      refreshAgents();
    }, 5000);
    return () => clearInterval(interval);
  }, [refreshAgents]);

  return (
    <div>
      {/* 刷新按钮 */}
      <div className="flex justify-end mb-4">
        <button onClick={() => refreshAgents()} className="btn btn-ghost btn-sm" title="刷新">
          <svg className="w-4 h-4" fill="none" stroke="currentColor" viewBox="0 0 24 24">
            <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M4 4v5h.582m15.356 2A8.001 8.001 0 004.582 9m0 0H9m11 11v-5h-.581m0 0a8.003 8.003 0 01-15.357-2m15.357 2H15" />
          </svg>
        </button>
      </div>

      {/* Agent 卡片网格 */}
      {state.agents.length === 0 ? (
        <div className="text-center py-16">
          <svg className="w-16 h-16 text-muted-foreground/30 mx-auto mb-4" fill="none" stroke="currentColor" viewBox="0 0 24 24">
            <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M17 20h5v-2a3 3 0 00-5.356-1.857M17 20H7m10 0v-2c0-.656-.126-1.283-.356-1.857M7 20H2v-2a3 3 0 015.356-1.857M7 20v-2c0-.656.126-1.283.356-1.857m0 0a5.002 5.002 0 019.288 0M15 7a3 3 0 11-6 0 3 3 0 016 0zm6 3a2 2 0 11-4 0 2 2 0 014 0zM7 10a2 2 0 11-4 0 2 2 0 014 0z" />
          </svg>
          <p className="text-muted-foreground">暂无 Agent 信息</p>
          <p className="text-xs text-muted-foreground mt-1">请确保后端服务正在运行</p>
        </div>
      ) : (
        <div className="grid grid-cols-1 md:grid-cols-2 lg:grid-cols-3 gap-4">
          {state.agents.map((agent) => (
            <AgentCard key={agent.agent_id} agent={agent} />
          ))}
        </div>
      )}
    </div>
  );
}

/**
 * 单个 Agent 卡片
 */
function AgentCard({ agent }: { agent: AgentStatusInfo }) {
  /** 获取状态颜色 */
  const stateColor = getAgentStateColor(agent.state);

  /** 获取状态中文标签 */
  const stateLabel = getAgentStateLabel(agent.state);

  return (
    <div className="card p-5 hover:border-primary/30 transition-colors">
      {/* 头部：名称和状态 */}
      <div className="flex items-start justify-between mb-4">
        <div className="flex items-center gap-3">
          <div className={`w-10 h-10 rounded-lg flex items-center justify-center ${stateColor.bg}`}>
            <svg className={`w-5 h-5 ${stateColor.text}`} fill="none" stroke="currentColor" viewBox="0 0 24 24">
              <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M9.75 17L9 20l-1 1h8l-1-1-.75-3M3 13h18M5 17h14a2 2 0 002-2V5a2 2 0 00-2-2H5a2 2 0 00-2 2v10a2 2 0 002 2z" />
            </svg>
          </div>
          <div>
            <h3 className="text-sm font-semibold text-foreground">{agent.agent_name || agent.agent_id}</h3>
            <p className="text-xs text-muted-foreground font-mono">{agent.agent_id}</p>
          </div>
        </div>
        <div className={`badge ${stateColor.badge}`}>
          <div className={`w-1.5 h-1.5 rounded-full mr-1.5 ${agent.state === 'Idle' ? 'bg-current' : 'animate-pulse bg-current'}`} />
          {stateLabel}
        </div>
      </div>

      {/* 统计信息 */}
      <div className="grid grid-cols-3 gap-3 mb-4">
        <div className="text-center p-2 bg-muted rounded">
          <div className="text-lg font-semibold text-foreground">{agent.completed_tasks}</div>
          <div className="text-xs text-muted-foreground">已完成</div>
        </div>
        <div className="text-center p-2 bg-muted rounded">
          <div className="text-lg font-semibold text-foreground">{formatTokenCount(agent.total_tokens)}</div>
          <div className="text-xs text-muted-foreground">Token</div>
        </div>
        <div className="text-center p-2 bg-muted rounded">
          <div className="text-lg font-semibold text-foreground">${agent.total_cost.toFixed(2)}</div>
          <div className="text-xs text-muted-foreground">费用</div>
        </div>
      </div>

      {/* 当前任务 */}
      {agent.current_task_id ? (
        <div className="flex items-center gap-2 text-xs p-2 bg-primary/5 border border-primary/10 rounded">
          <svg className="w-3.5 h-3.5 text-primary shrink-0" fill="none" stroke="currentColor" viewBox="0 0 24 24">
            <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M13 10V3L4 14h7v7l9-11h-7z" />
          </svg>
          <span className="text-muted-foreground">当前任务:</span>
          <span className="font-mono text-foreground truncate">{agent.current_task_id}</span>
        </div>
      ) : (
        <div className="flex items-center gap-2 text-xs p-2 bg-muted rounded">
          <svg className="w-3.5 h-3.5 text-muted-foreground shrink-0" fill="none" stroke="currentColor" viewBox="0 0 24 24">
            <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M20 13V6a2 2 0 00-2-2H6a2 2 0 00-2 2v7m16 0v5a2 2 0 01-2 2H6a2 2 0 01-2-2v-5m16 0h-2.586a1 1 0 00-.707.293l-2.414 2.414a1 1 0 01-.707.293h-3.172a1 1 0 01-.707-.293l-2.414-2.414A1 1 0 006.586 13H4" />
          </svg>
          <span className="text-muted-foreground">空闲</span>
        </div>
      )}
    </div>
  );
}

/** 获取 Agent 状态颜色配置 */
function getAgentStateColor(state: string): { bg: string; text: string; badge: string } {
  switch (state) {
    case 'Idle':
      return { bg: 'bg-green-500/10', text: 'text-green-400', badge: 'bg-green-500/20 text-green-400' };
    case 'Running':
    case 'Busy':
      return { bg: 'bg-blue-500/10', text: 'text-blue-400', badge: 'bg-blue-500/20 text-blue-400' };
    case 'Error':
      return { bg: 'bg-red-500/10', text: 'text-red-400', badge: 'bg-red-500/20 text-red-400' };
    default:
      return { bg: 'bg-gray-500/10', text: 'text-gray-400', badge: 'bg-gray-500/20 text-gray-400' };
  }
}

/** 获取 Agent 状态中文标签 */
function getAgentStateLabel(state: string): string {
  const map: Record<string, string> = {
    Idle: '空闲',
    Running: '运行中',
    Busy: '忙碌',
    Error: '错误',
    Paused: '已暂停',
    Offline: '离线',
  };
  return map[state] || state;
}

/** 格式化 Token 数量 */
function formatTokenCount(count: number): string {
  if (count >= 1000000) return (count / 1000000).toFixed(1) + 'M';
  if (count >= 1000) return (count / 1000).toFixed(1) + 'K';
  return String(count);
}
