'use client';

import TaskList from '@/components/TaskList';

/**
 * 任务监控页
 */
export default function TasksPage() {
  return (
    <div className="h-full overflow-y-auto p-6">
      <div className="max-w-[1400px] mx-auto">
        {/* 页面标题 */}
        <div className="mb-6">
          <h1 className="text-2xl font-bold text-foreground">任务监控</h1>
          <p className="text-sm text-muted-foreground mt-1">查看和管理所有任务的执行状态</p>
        </div>

        {/* 任务列表 */}
        <TaskList />
      </div>
    </div>
  );
}
