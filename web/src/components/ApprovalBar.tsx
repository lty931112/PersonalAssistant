'use client';

/**
 * 工具调用人工批准条：在 WebSocket 查询阻塞时通过 HTTP 轮询待办并提交决策
 */
import { useApp } from '@/lib/store';

export default function ApprovalBar() {
  const { state, respondToolApproval } = useApp();
  const { pendingApprovals } = state;

  if (pendingApprovals.length === 0) return null;

  return (
    <div className="fixed bottom-0 left-0 right-0 z-50 border-t border-amber-500/40 bg-amber-950/95 px-4 py-3 text-sm text-amber-50 shadow-lg md:left-64">
      <div className="mx-auto max-w-3xl space-y-2">
        <p className="font-medium text-amber-200">
          待确认的工具调用（{pendingApprovals.length}）
        </p>
        <ul className="max-h-40 space-y-2 overflow-y-auto">
          {pendingApprovals.map((p) => (
            <li
              key={p.approval_id}
              className="flex flex-col gap-2 rounded-md border border-amber-700/50 bg-black/30 p-3 sm:flex-row sm:items-center sm:justify-between"
            >
              <div className="min-w-0 flex-1">
                <div className="font-mono text-xs text-amber-300/80">
                  {p.tool_name} · trace {p.trace_id.slice(0, 8)}… · #{p.turn}
                </div>
                <pre className="mt-1 whitespace-pre-wrap break-words text-xs text-amber-100/90">
                  {p.prompt}
                </pre>
              </div>
              <div className="flex shrink-0 gap-2">
                <button
                  type="button"
                  className="rounded bg-emerald-600 px-3 py-1.5 text-xs font-medium text-white hover:bg-emerald-500"
                  onClick={() => void respondToolApproval(p.approval_id, true)}
                >
                  允许
                </button>
                <button
                  type="button"
                  className="rounded bg-red-900 px-3 py-1.5 text-xs font-medium text-red-100 hover:bg-red-800"
                  onClick={() => void respondToolApproval(p.approval_id, false)}
                >
                  拒绝
                </button>
              </div>
            </li>
          ))}
        </ul>
      </div>
    </div>
  );
}
