import { useEffect, useState } from "react";
import { CheckCircle2, ChevronDown, ChevronRight, Loader2, XCircle } from "lucide-react";

import { cn } from "cn";
import { toolLabel, toolSummary } from "@/components/ai/tool";
import { AGENT_BADGE, AGENT_DOT, resolveAgentColor } from "@/lib/agent-colors";

/**
 * 子任务卡 / 子 Agent 运行卡（R4 起源，R8-③ 兼作子 Agent 卡，R9-③ 视觉深化）。
 * 事件源：SubTaskStart/Delta/ToolCall/ToolResult/End（agent/subtask.rs 桥接）。
 * 头部（R9）：身份色点 + agent 名徽标 + 任务描述 + 耗时·用量（完成态）。
 * 轨迹（R9）：中文工具名 + 参数摘要一行；点击行展开该次调用的参数与结果。
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

export function TaskCard({ task }: { task: TaskView }) {
  const [open, setOpen] = useState(task.status === "running");
  /** 展开查看详情的工具行（按轨迹索引；R9-③） */
  const [detailIdx, setDetailIdx] = useState<number | null>(null);

  // 完成后自动收起为摘要行（运行中保持展开看过程）
  useEffect(() => {
    if (task.status !== "running") {
      setOpen(false);
      setDetailIdx(null);
    }
  }, [task.status]);

  const icon =
    task.status === "running" ? (
      <Loader2 className="size-4 shrink-0 animate-spin text-muted-foreground" />
    ) : task.status === "done" ? (
      <CheckCircle2 className="size-4 shrink-0 text-emerald-600" />
    ) : (
      <XCircle className="size-4 shrink-0 text-red-600" />
    );
  const statusText =
    task.status === "running" ? "运行中" : task.status === "done" ? "完成" : "失败";
  const isAgent = !!task.agent;
  const colorKey = resolveAgentColor(task.agent, task.color);

  return (
    <div className="rounded-lg border bg-muted/30 text-sm">
      <button
        type="button"
        className="flex w-full items-center gap-2 px-3 py-2 text-left"
        onClick={() => setOpen((v) => !v)}
        aria-expanded={open}
      >
        {icon}
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
          <span className="shrink-0 font-medium">子任务</span>
        )}
        <span className="truncate text-xs text-muted-foreground">{task.label}</span>
        {task.status !== "running" && (task.durationMs != null || task.tokens != null) && (
          <span className="ml-1 shrink-0 text-[11px] tabular-nums text-muted-foreground/70">
            {[
              task.durationMs != null ? `${Math.max(1, Math.round(task.durationMs / 1000))}s` : null,
              task.tokens != null ? `≈${fmtTokens(task.tokens)} tokens` : null,
            ]
              .filter(Boolean)
              .join(" · ")}
          </span>
        )}
        <span className="ml-auto shrink-0 text-xs text-muted-foreground">{statusText}</span>
        {open ? (
          <ChevronDown className="size-3.5 shrink-0 text-muted-foreground" />
        ) : (
          <ChevronRight className="size-3.5 shrink-0 text-muted-foreground" />
        )}
      </button>

      {/* 收起态：一行摘要（完成/失败时） */}
      {!open && task.summary && (
        <div className="border-t px-3 py-1.5 text-xs text-muted-foreground">{task.summary}</div>
      )}

      {open && (
        <div className="flex flex-col gap-2 border-t px-3 py-2">
          {task.text.trim() && (
            <pre className="max-h-56 overflow-y-auto whitespace-pre-wrap rounded bg-background p-2 font-sans text-[12px] leading-relaxed text-muted-foreground">
              {task.text}
            </pre>
          )}
          {task.toolCalls.length > 0 && (
            <div className="flex flex-col gap-0.5">
              <span className="text-[10px] tracking-wide text-muted-foreground uppercase">
                工具轨迹（{task.toolCalls.length}）
              </span>
              {task.toolCalls.map((t, i) => {
                const running = t.result === null;
                const detailed = detailIdx === i;
                const summary = toolSummary(t.args);
                return (
                  <div key={i} className="rounded border bg-background/60">
                    <button
                      type="button"
                      className="flex w-full items-center gap-2 px-2 py-1 text-left text-[11px]"
                      onClick={() => setDetailIdx(detailed ? null : i)}
                      aria-expanded={detailed}
                    >
                      <span
                        className={cn(
                          "size-1.5 shrink-0 rounded-full",
                          running ? "animate-pulse bg-amber-400" : "bg-emerald-500",
                        )}
                      />
                      <span className="shrink-0 font-mono text-foreground/80">
                        {toolLabel(t.name)}
                      </span>
                      {summary && (
                        <span className="min-w-0 flex-1 truncate text-muted-foreground" title={summary}>
                          {summary}
                        </span>
                      )}
                      <span className={cn("ml-auto shrink-0 text-muted-foreground/70", running && "animate-pulse")}>
                        {running ? "运行中" : "完成"}
                      </span>
                      {detailed ? (
                        <ChevronDown className="size-3 shrink-0 text-muted-foreground/60" />
                      ) : (
                        <ChevronRight className="size-3 shrink-0 text-muted-foreground/60" />
                      )}
                    </button>
                    {detailed && (
                      <div className="flex flex-col gap-1 border-t px-2 py-1.5">
                        <span className="text-[10px] text-muted-foreground/70">参数</span>
                        <pre className="max-h-32 overflow-auto whitespace-pre-wrap break-all rounded bg-muted/40 p-1.5 font-mono text-[10px] leading-relaxed">
                          {t.args}
                        </pre>
                        {t.result !== null && (
                          <>
                            <span className="text-[10px] text-muted-foreground/70">结果</span>
                            <pre className="max-h-40 overflow-auto whitespace-pre-wrap break-all rounded bg-muted/40 p-1.5 font-mono text-[10px] leading-relaxed">
                              {t.result}
                            </pre>
                          </>
                        )}
                      </div>
                    )}
                  </div>
                );
              })}
            </div>
          )}
          {task.summary && <div className="text-xs">{task.summary}</div>}
        </div>
      )}
    </div>
  );
}
