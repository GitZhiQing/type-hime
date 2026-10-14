import { CheckCircle2, Loader2, XCircle } from "lucide-react";

import { cn } from "cn";
import { toolLabel, toolSummary } from "@/components/ai/tool";
import { AGENT_BADGE, AGENT_DOT, resolveAgentColor } from "@/lib/agent-colors";

/**
 * 子任务/子 Agent 任务行（R4 起源 → D50 照 ZCode 产品边界单行化 → D51 抽共享）。
 * 产品边界（同 ZCode agent.tsx 注释）：父对话只保留**单行摘要**；完整 child timeline
 * 一律在制品栏子会话标签查看（打开按钮 delegation:open）。卡内不展开、无轨迹。
 * 两种数据源共用本组件：
 * - 运行中：TaskCard 包装（SubTask* 事件实时流，摘要位=最新子动作）；
 * - 历史回放：ConversationHistory 用会话档案的 SubTaskRecord（status/tokens/duration 落档）。
 */

/** 任务行数据（实时 TaskView 与档案 SubTaskRecord 的共同投影） */
export interface AgentTaskRowData {
  id: string;
  agent: string | null;
  label: string;
  status: "running" | "done" | "error";
  /** 运行中摘要位（最新子动作；历史回放无值则显示用量） */
  activity?: string | null;
  tokens?: number | null;
  durationMs?: number | null;
  /** 子会话档案 id（有才显示打开按钮） */
  session?: string | null;
}

function fmtTokens(n: number): string {
  return n >= 1000 ? `${(n / 1000).toFixed(1)}k` : String(n);
}

export function AgentTaskRow({ data }: { data: AgentTaskRowData }) {
  const isAgent = !!data.agent;
  const colorKey = resolveAgentColor(data.agent, null);
  const stats = [
    data.durationMs != null
      ? `${Math.max(1, Math.floor(data.durationMs / 1000))}s`
      : null,
    data.tokens != null ? `≈${fmtTokens(data.tokens)} tokens` : null,
  ]
    .filter(Boolean)
    .join(" · ");

  return (
    <div className="flex w-full items-center gap-2 rounded-md border bg-muted/20 px-2.5 py-1.5 text-sm">
      {data.status === "running" ? (
        <Loader2 className="size-3.5 shrink-0 animate-spin text-muted-foreground" />
      ) : data.status === "done" ? (
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
          {data.agent}
        </span>
      ) : (
        <span className="shrink-0 text-xs font-medium">子任务</span>
      )}
      {/* 标题（D52 用户反馈）：有子会话档案时标题即链接按钮——点击打开制品栏子会话标签
          （此前只有右侧小图标，点击目标太小）；无档案时纯文本 */}
      {data.session ? (
        <button
          type="button"
          className="min-w-0 max-w-[45%] shrink truncate rounded-sm text-left text-xs font-medium underline decoration-muted-foreground/40 underline-offset-2 transition-colors hover:decoration-foreground focus-visible:ring-1 focus-visible:ring-ring focus-visible:outline-none"
          title={`打开子 Agent 会话：${data.label}`}
          aria-label="打开子 Agent 会话"
          onClick={() =>
            window.dispatchEvent(
              new CustomEvent("delegation:open", {
                detail: {
                  id: data.id,
                  agent: data.agent,
                  label: data.label,
                  session: data.session,
                },
              }),
            )
          }
        >
          {data.label}
        </button>
      ) : (
        <span
          className="min-w-0 max-w-[45%] shrink truncate text-xs text-muted-foreground"
          title={data.label}
        >
          {data.label}
        </span>
      )}
      {/* 摘要位：运行中=最新子动作；静止=耗时·用量 */}
      <span className="min-w-0 flex-1 truncate text-[11px] text-muted-foreground/70">
        {data.status === "running" ? data.activity ?? "" : stats}
      </span>
      <span
        className={cn(
          "shrink-0 text-[11px]",
          data.status === "error" ? "text-red-600" : "text-muted-foreground",
        )}
      >
        {data.status === "running" ? "运行中" : data.status === "done" ? "完成" : "失败"}
      </span>
    </div>
  );
}

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
  /** 子会话档案 id（D50：打开入口） */
  session?: string | null;
}

/** 运行中的「最新子动作」摘要（ZCode collapsedChildSummary 同位；取最新一条） */
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

/** 实时任务卡（SubTask* 事件流）：运行中显示最新子动作。 */
export function TaskCard({ task }: { task: TaskView }) {
  return (
    <AgentTaskRow
      data={{
        id: task.id,
        agent: task.agent ?? null,
        label: task.label,
        status: task.status,
        activity: task.status === "running" ? latestActivity(task) : null,
        tokens: task.tokens,
        durationMs: task.durationMs,
        session: task.session ?? null,
      }}
    />
  );
}
