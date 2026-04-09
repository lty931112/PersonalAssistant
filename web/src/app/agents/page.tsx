'use client';

import AgentList from '@/components/AgentList';

/**
 * Agent 管理页
 */
export default function AgentsPage() {
  return (
    <div className="h-full overflow-y-auto p-6">
      <div className="max-w-[1400px] mx-auto">
        {/* 页面标题 */}
        <div className="mb-6">
          <h1 className="text-2xl font-bold text-foreground">Agent 管理</h1>
          <p className="text-sm text-muted-foreground mt-1">查看所有 Agent 的运行状态和资源使用情况</p>
        </div>

        {/* Agent 列表 */}
        <AgentList />
      </div>
    </div>
  );
}
