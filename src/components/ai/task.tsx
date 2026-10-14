import { CheckCircle2, Loader2, PanelRightOpen, XCircle } from "lucide-react";

import { cn } from "cn";
import { toolLabel, toolSummary } from "@/components/ai/tool";
import { AGENT_BADGE, AGENT_DOT, resolveAgentColor } from "@/lib/agent-colors";

/**
 * 子任务卡 / 子 Agent 运行卡（R4 起源，R8-③ 兼作子 Agent 卡；D50 照 ZCode 产品边界单行化：
 * 「Agent 在父对话中只保留单行摘要；完整 child timeline 统一从右侧 tab 查看」——
 * 不展开、无卡内轨迹；运行中摘要位显示最新子动作，完成显示耗时·用量；
 * 打开按钮直达制品栏的子会话视图（delegation:open）。
 * 事件源：SubTaskStart/Delta/ToolCall/ToolResult/End（agent/subtask.rs 桥接）。
 */

export interface SubTaskToolCall {
  name: string;
  args: string;
  result: string | null;
  /** rig call id（R8-③：并行子 Agent 同名工具的精确配对；旧数据无 id 按名回退） */
  callId?: string | null;
}

export interface TaskView {
  id: string;
  label: string;
  status: "running" | "done" | "error";
  /** 子流程文本增量累积 */
  text: string;
  /** 子流程内工具轨迹（call/result 配对：call_id 优先，回退按名） */
  toolCalls: SubTaskToolCall[];
  summary: string | null;
  /** 子 Agent 名（spawn_agent 委派时；None=普通子任务） */
  agent?: string | null;
  /** 身份色 key（R9：profile.color；None=按名 hash） */
  color?: string | null;
  /** 完成态用量（R9：header 展示「耗时 · tokens」） */
  tokens?: number | null;
  durationMs?: number | null;
}

function fmtTokens(n: number): string {
  return n >= 1000 ? `${(n / 1000).toFixed(1)}k` : String(n);
}

/** 运行中的「最新子动作」摘要（ZCode collapsedChildSummary 同位；不做轮播，取最新一条） */
function latestActivity(task: TaskView): string | null {
  const line = task.text
    .split("\n")
    .map((s) => s.trim())
    .filter(Boolean)
    .pop();
  if (line) return line.length > 42 ? `${line.slice(0, 42)}…` : line;
  const lastTool = task.toolCalls[task.toolCalls.length - 1];
  if (!lastTool) return null;
  const s = toolSummary(lastTool.args) || toolLabel(lastTool.name);
  return s.length > 42 ? `${s.slice(0, 42)}…` : s;
}

export function TaskCard({ task }: { task: TaskView }) {
  const isAgent = !!task.agent;
  const colorKey = resolveAgentColor(task.agent, task.color);
  const stats =
    task.status !== "running" &&
    (task.durationMs != null || task.tokens != null)
      ? [
          task.durationMs != null
            ? `${Math.max(1, Math.floor(task.durationMs / 1000))}s`
            : null,
          task.tokens != null ? `≈${fmtTokens(task.tokens)} tokens` : null,
        ]
          .filter(Boolean)
          .join(" · ")
      : null;

  return (
    <div className="flex w-full items-center gap-2 rounded-md border bg-muted/20 px-2.5 py-1.5 text-sm">
      {task.status === "running" ? (
        <Loader2 className="size-3.5 shrink-0 animate-spin text-muted-foreground" />
      ) : task.status === "done" ? (
        <CheckCircle2 className="size-3.5 shrink-0 text-emerald-600" />
      ) : (
        <XCircle className="size-3.5 shrink-0 text-red-600" />
      )}
      {isAgent ? (
        <span
          className={cn(
            "flex shrink-0 items-center gap-1.5 rounded-full px-2 py-0.5 text-xs font-medium",
            AGENT_BADGE[colorKey],
          )}
          title="子 Agent 委派"
        >
          <span className={cn("size-1.5 rounded-full", AGENT_DOT[colorKey])} />
          {task.agent}
        </span>
      ) : (
        <span className="shrink-0 text-xs font-medium">子任务</span>
      )}
      <span className="shrink-0 text-xs text-muted-foreground" title={task.label}>
        {task.label.length > 18 ? `${task.label.slice(0, 18)}…` : task.label}
      </span>
      {/* 摘要位：运行中=最新子动作；完成=耗时·用量（无则空） */}
      <span className="min-w-0 flex-1 truncate text-[11px] text-muted-foreground/70">
        {task.status === "running" ? latestActivity(task) ?? "" : stats ?? ""}
      </span>
      <span
        className={cn(
          "shrink-0 text-[11px]",
          task.status === "error" ? "text-red-600" : "text-muted-foreground",
        )}
      >
        {task.status === "running" ? "运行中" : task.status === "done" ? "完成" : "失败"}
      </span>
      {/* D50：直达制品栏的子会话视图（完整 timeline 在那里） */}
      <button
        type="button"
        className="shrink-0 rounded p-0.5 text-muted-foreground/50 transition-colors hover:bg-accent hover:text-foreground"
        title="在制品面板打开子 Agent 会话"
        aria-label="打开子 Agent 会话"
        onClick={(e) => {
          e.stopPropagation();
          window.dispatchEvent(
            new CustomEvent("delegation:open", { detail: { id: task.id } }),
          );
        }}
      >
        <PanelRightOpen className="size-3.5" />
      </button>
    </div>
  );
}
