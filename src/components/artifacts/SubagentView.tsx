import { MessageResponse } from "@/components/ai/response";
import { AGENT_BADGE, AGENT_DOT, resolveAgentColor } from "@/lib/agent-colors";
import { cn } from "cn";
import type { DelegationPayload } from "@/components/artifacts/ArtifactsPane";

/**
 * 委派上下文视图（D49，形态参考 ZCode 子 Agent 文档页）：
 * 每次子 Agent 委派在制品栏开一个标签，展示根 Agent 装配给它的全部上下文
 * （spawn_agent 的 prompt 原文，markdown 渲染）——写作上下文可见清晰。
 * 数据是会话级的（reload 后标签不恢复，prompt 不落盘）。
 */
export function SubagentView({ payload }: { payload?: DelegationPayload }) {
  if (!payload) {
    return (
      <div className="flex flex-1 items-center justify-center p-6 text-sm text-muted-foreground">
        该委派标签缺少上下文数据（重新发起委派后会出现在这里）
      </div>
    );
  }
  const colorKey = resolveAgentColor(payload.agent, null);
  return (
    <div className="flex min-h-0 flex-1 flex-col">
      <div className="flex shrink-0 items-center gap-2 border-b px-3 py-2">
        <span
          className={cn(
            "flex shrink-0 items-center gap-1.5 rounded-full px-2 py-0.5 text-xs font-medium",
            AGENT_BADGE[colorKey],
          )}
        >
          <span className={cn("size-1.5 rounded-full", AGENT_DOT[colorKey])} />
          {payload.agent ?? "子 Agent"}
        </span>
        <span className="min-w-0 truncate text-xs text-muted-foreground" title={payload.label}>
          {payload.label}
        </span>
        <span className="ml-auto shrink-0 text-[10px] text-muted-foreground">
          根 Agent 装配的委派上下文
        </span>
      </div>
      <div className="min-h-0 flex-1 overflow-y-auto p-3">
        {payload.prompt ? (
          <MessageResponse>{payload.prompt}</MessageResponse>
        ) : (
          <p className="text-sm text-muted-foreground">本次委派未携带指令文本。</p>
        )}
      </div>
    </div>
  );
}
