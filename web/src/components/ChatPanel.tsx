'use client';

import React, { useState, useRef, useEffect } from 'react';
import { useApp } from '@/lib/store';
import MessageBubble from './MessageBubble';
import type { Conversation } from '@/lib/types';

/**
 * 聊天面板组件
 * 包含对话历史列表、聊天区域和工具调用状态
 */
export default function ChatPanel() {
  const { state, dispatch, sendMessage, cancelTask } = useApp();
  const [inputValue, setInputValue] = useState('');
  const [selectedConvId, setSelectedConvId] = useState<string | null>(null);
  const messagesEndRef = useRef<HTMLDivElement>(null);
  const inputRef = useRef<HTMLTextAreaElement>(null);

  /** 当前活跃对话 */
  const activeConversation: Conversation | undefined = state.conversations.find(
    (c) => c.id === (selectedConvId || state.activeConversationId)
  );

  /** 自动滚动到底部 */
  useEffect(() => {
    messagesEndRef.current?.scrollIntoView({ behavior: 'smooth' });
  }, [activeConversation?.messages]);

  /** 处理发送消息 */
  const handleSend = () => {
    const prompt = inputValue.trim();
    if (!prompt) return;

    sendMessage(prompt);
    setInputValue('');

    // 重置输入框高度
    if (inputRef.current) {
      inputRef.current.style.height = 'auto';
    }
  };

  /** 处理键盘事件（Enter 发送，Shift+Enter 换行） */
  const handleKeyDown = (e: React.KeyboardEvent<HTMLTextAreaElement>) => {
    if (e.key === 'Enter' && !e.shiftKey) {
      e.preventDefault();
      handleSend();
    }
  };

  /** 处理输入框自动调整高度 */
  const handleInputChange = (e: React.ChangeEvent<HTMLTextAreaElement>) => {
    setInputValue(e.target.value);
    // 自动调整高度
    const el = e.target;
    el.style.height = 'auto';
    el.style.height = Math.min(el.scrollHeight, 200) + 'px';
  };

  /** 创建新对话 */
  const handleNewConversation = () => {
    dispatch({ type: 'SET_ACTIVE_CONVERSATION', payload: null });
    setSelectedConvId(null);
  };

  /** 选择对话 */
  const handleSelectConversation = (convId: string) => {
    setSelectedConvId(convId);
    dispatch({ type: 'SET_ACTIVE_CONVERSATION', payload: convId });
  };

  /** 中断当前任务 */
  const handleCancel = () => {
    if (state.activeTaskId) {
      cancelTask(state.activeTaskId);
    }
  };

  /** 判断是否有正在进行的任务 */
  const hasActiveTask = state.activeToolCalls.length > 0 ||
    activeConversation?.messages.some((m) => m.isStreaming);

  return (
    <div className="flex h-full">
      {/* 左侧：对话历史列表 */}
      <div className="w-64 border-r border-border flex flex-col bg-card shrink-0">
        {/* 新建对话按钮 */}
        <div className="p-3 border-b border-border">
          <button
            onClick={handleNewConversation}
            className="btn btn-primary w-full text-sm"
          >
            <svg className="w-4 h-4 mr-2" fill="none" stroke="currentColor" viewBox="0 0 24 24">
              <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M12 4v16m8-8H4" />
            </svg>
            新建对话
          </button>
        </div>

        {/* 对话列表 */}
        <div className="flex-1 overflow-y-auto p-2 space-y-1">
          {state.conversations.length === 0 ? (
            <p className="text-xs text-muted-foreground text-center py-8">暂无对话记录</p>
          ) : (
            state.conversations.map((conv) => (
              <button
                key={conv.id}
                onClick={() => handleSelectConversation(conv.id)}
                className={`w-full text-left px-3 py-2 rounded-md text-sm truncate transition-colors ${
                  conv.id === (selectedConvId || state.activeConversationId)
                    ? 'bg-accent text-foreground'
                    : 'text-muted-foreground hover:bg-accent hover:text-foreground'
                }`}
              >
                <div className="truncate">{conv.title || '新对话'}</div>
                <div className="text-xs text-muted-foreground mt-0.5">
                  {conv.messages.length} 条消息
                </div>
              </button>
            ))
          )}
        </div>
      </div>

      {/* 中间：聊天区域 */}
      <div className="flex-1 flex flex-col min-w-0">
        {/* 聊天消息列表 */}
        <div className="flex-1 overflow-y-auto px-4 py-4">
          {activeConversation && activeConversation.messages.length > 0 ? (
            <div className="max-w-3xl mx-auto">
              {activeConversation.messages.map((msg) => (
                <MessageBubble key={msg.id} message={msg} />
              ))}
              <div ref={messagesEndRef} />
            </div>
          ) : (
            <div className="flex items-center justify-center h-full">
              <div className="text-center">
                <div className="w-16 h-16 rounded-full bg-primary/10 flex items-center justify-center mx-auto mb-4">
                  <svg className="w-8 h-8 text-primary" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                    <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M13 10V3L4 14h7v7l9-11h-7z" />
                  </svg>
                </div>
                <h2 className="text-lg font-medium text-foreground mb-2">PersonalAssistant</h2>
                <p className="text-sm text-muted-foreground">输入你的问题，开始与 AI 助手对话</p>
              </div>
            </div>
          )}
        </div>

        {/* 工具调用状态提示 */}
        {state.activeToolCalls.length > 0 && (
          <div className="px-4 py-2 border-t border-border bg-card">
            <div className="max-w-3xl mx-auto space-y-1">
              {state.activeToolCalls.map((tc) => (
                <div key={tc.toolId} className="flex items-center gap-2 text-xs text-muted-foreground">
                  <svg className="w-3.5 h-3.5" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                    <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M10.325 4.317c.426-1.756 2.924-1.756 3.35 0a1.724 1.724 0 002.573 1.066c1.543-.94 3.31.826 2.37 2.37a1.724 1.724 0 001.066 2.573c1.756.426 1.756 2.924 0 3.35a1.724 1.724 0 00-1.066 2.573c.94 1.543-.826 3.31-2.37 2.37a1.724 1.724 0 00-2.573 1.066c-.426 1.756-2.924 1.756-3.35 0a1.724 1.724 0 00-2.573-1.066c-1.543.94-3.31-.826-2.37-2.37a1.724 1.724 0 00-1.066-2.573c-1.756-.426-1.756-2.924 0-3.35a1.724 1.724 0 001.066-2.573c-.94-1.543.826-3.31 2.37-2.37.996.608 2.296.07 2.572-1.065z" />
                    <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M15 12a3 3 0 11-6 0 3 3 0 016 0z" />
                  </svg>
                  <span className="font-medium text-foreground">{tc.toolName}</span>
                  {tc.endTime ? (
                    <span className={tc.isError ? 'text-red-400' : 'text-green-400'}>
                      {tc.isError ? '失败' : '完成'}
                    </span>
                  ) : (
                    <span className="flex items-center">
                      <span className="tool-loading-dot" />
                      <span className="tool-loading-dot" />
                      <span className="tool-loading-dot" />
                      <span className="ml-1">执行中</span>
                    </span>
                  )}
                </div>
              ))}
            </div>
          </div>
        )}

        {/* 输入区域 */}
        <div className="border-t border-border bg-card p-4">
          <div className="max-w-3xl mx-auto">
            <div className="flex gap-2 items-end">
              <textarea
                ref={inputRef}
                value={inputValue}
                onChange={handleInputChange}
                onKeyDown={handleKeyDown}
                placeholder="输入消息... (Enter 发送, Shift+Enter 换行)"
                className="input resize-none min-h-[40px] max-h-[200px]"
                rows={1}
                disabled={state.wsState !== 'connected'}
              />
              {hasActiveTask ? (
                <button
                  onClick={handleCancel}
                  className="btn btn-destructive shrink-0"
                  title="中断当前任务"
                >
                  <svg className="w-4 h-4" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                    <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M21 12a9 9 0 11-18 0 9 9 0 0118 0z" />
                    <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M9 10a1 1 0 011-1h4a1 1 0 011 1v4a1 1 0 01-1 1h-4a1 1 0 01-1-1v-4z" />
                  </svg>
                </button>
              ) : (
                <button
                  onClick={handleSend}
                  disabled={!inputValue.trim() || state.wsState !== 'connected'}
                  className="btn btn-primary shrink-0 disabled:opacity-50 disabled:cursor-not-allowed"
                  title="发送消息"
                >
                  <svg className="w-4 h-4" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                    <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M12 19l9 2-9-18-9 18 9-2zm0 0v-8" />
                  </svg>
                </button>
              )}
            </div>
            {state.wsState !== 'connected' && (
              <p className="text-xs text-red-400 mt-2">
                WebSocket 未连接，请检查后端服务或前往设置页面配置连接地址
              </p>
            )}
          </div>
        </div>
      </div>

      {/* 右侧：当前任务状态面板 */}
      <div className="w-72 border-l border-border bg-card shrink-0 overflow-y-auto">
        <div className="p-4 border-b border-border">
          <h3 className="text-sm font-semibold text-foreground">任务状态</h3>
        </div>

        {/* 当前任务信息 */}
        <div className="p-4">
          {state.activeToolCalls.length > 0 ? (
            <div className="space-y-3">
              <div className="text-xs text-muted-foreground uppercase tracking-wider">正在执行</div>
              {state.activeToolCalls.map((tc) => (
                <div key={tc.toolId} className="card p-3">
                  <div className="flex items-center gap-2 mb-1">
                    <svg className="w-4 h-4 text-primary" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                      <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M10.325 4.317c.426-1.756 2.924-1.756 3.35 0a1.724 1.724 0 002.573 1.066c1.543-.94 3.31.826 2.37 2.37a1.724 1.724 0 001.066 2.573c1.756.426 1.756 2.924 0 3.35a1.724 1.724 0 00-1.066 2.573c.94 1.543-.826 3.31-2.37 2.37a1.724 1.724 0 00-2.573 1.066c-.426 1.756-2.924 1.756-3.35 0a1.724 1.724 0 00-2.573-1.066c-1.543.94-3.31-.826-2.37-2.37a1.724 1.724 0 00-1.066-2.573c-1.756-.426-1.756-2.924 0-3.35a1.724 1.724 0 001.066-2.573c-.94-1.543.826-3.31 2.37-2.37.996.608 2.296.07 2.572-1.065z" />
                      <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M15 12a3 3 0 11-6 0 3 3 0 016 0z" />
                    </svg>
                    <span className="text-sm font-medium">{tc.toolName}</span>
                  </div>
                  <div className="text-xs text-muted-foreground">
                    ID: {tc.toolId.slice(0, 8)}...
                  </div>
                  {tc.endTime && (
                    <div className="text-xs text-muted-foreground mt-1">
                      耗时: {(() => {
                        const start = new Date(tc.startTime).getTime();
                        const end = new Date(tc.endTime).getTime();
                        return ((end - start) / 1000).toFixed(1) + 's';
                      })()}
                    </div>
                  )}
                </div>
              ))}
            </div>
          ) : (
            <div className="text-center py-8">
              <svg className="w-10 h-10 text-muted-foreground/30 mx-auto mb-2" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M20 13V6a2 2 0 00-2-2H6a2 2 0 00-2 2v7m16 0v5a2 2 0 01-2 2H6a2 2 0 01-2-2v-5m16 0h-2.586a1 1 0 00-.707.293l-2.414 2.414a1 1 0 01-.707.293h-3.172a1 1 0 01-.707-.293l-2.414-2.414A1 1 0 006.586 13H4" />
              </svg>
              <p className="text-xs text-muted-foreground">暂无活跃任务</p>
            </div>
          )}
        </div>

        {/* 最近任务 */}
        <div className="p-4 border-t border-border">
          <div className="text-xs text-muted-foreground uppercase tracking-wider mb-3">最近任务</div>
          {state.tasks.length === 0 ? (
            <p className="text-xs text-muted-foreground">暂无任务记录</p>
          ) : (
            <div className="space-y-2">
              {state.tasks.slice(0, 5).map((task) => (
                <div key={task.id} className="card p-2.5">
                  <div className="text-xs font-medium truncate">{task.prompt.slice(0, 40)}</div>
                  <div className="flex items-center justify-between mt-1">
                    <span className={`badge text-[10px] ${getStatusBadgeClass(task.status)}`}>
                      {getStatusLabel(task.status)}
                    </span>
                    <span className="text-[10px] text-muted-foreground">
                      {formatTime(task.created_at)}
                    </span>
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

/** 格式化时间 */
function formatTime(dateStr: string): string {
  const date = new Date(dateStr);
  const now = new Date();
  const diff = now.getTime() - date.getTime();

  if (diff < 60000) return '刚刚';
  if (diff < 3600000) return `${Math.floor(diff / 60000)}分钟前`;
  if (diff < 86400000) return `${Math.floor(diff / 3600000)}小时前`;
  return date.toLocaleDateString('zh-CN');
}
