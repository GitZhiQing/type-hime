import { useEffect, useState } from "react";
import { CheckCircle2, ChevronDown, ChevronRight, Loader2, XCircle } from "lucide-react";

import { cn } from "cn";

/**
 * 子任务卡（R4：run_flow 触发长流程期间的过程展示；AI Elements Task 同构）。
 * 事件源：SubTaskStart/Delta/ToolCall/ToolResult/End（agent/subtask.rs 桥接）。
 * 交互（R4-⑤）：运行中默认展开看流式过程；完成后自动收起为一行摘要，可再展开回看轨迹。
 */

export interface SubTaskToolCall {
  name: string;
  args: string;
  result: string | null;
}

export interface TaskView {
  id: string;
  label: string;
  status: "running" | "done" | "error";
  /** 子流程文本增量累积 */
  text: string;
  /** 子流程内工具轨迹（call/result 按名配对） */
  toolCalls: SubTaskToolCall[];
  summary: string | null;
}

export function TaskCard({ task }: { task: TaskView }) {
  const [open, setOpen] = useState(task.status === "running");

  // 完成后自动收起为摘要行（运行中保持展开看过程）
  useEffect(() => {
    if (task.status !== "running") setOpen(false);
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

  return (
    <div className="rounded-lg border bg-muted/30 text-sm">
      <button
        type="button"
        className="flex w-full items-center gap-2 px-3 py-2 text-left"
        onClick={() => setOpen((v) => !v)}
        aria-expanded={open}
      >
        {icon}
        <span className="shrink-0 font-medium">子任务</span>
        <span className="truncate text-xs text-muted-foreground">{task.label}</span>
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
        <div className="flex flex-col gap-1.5 border-t px-3 py-2">
          {task.text.trim() && (
            <pre className="max-h-56 overflow-y-auto whitespace-pre-wrap rounded bg-background p-2 font-sans text-[12px] leading-relaxed text-muted-foreground">
              {task.text}
            </pre>
          )}
          {task.toolCalls.length > 0 && (
            <div className="flex flex-col gap-1 text-[11px] text-muted-foreground">
              <span className="text-[10px] tracking-wide uppercase">工具轨迹</span>
              {task.toolCalls.map((t, i) => (
                <div key={i} className="flex items-start gap-1.5">
                  <span className="shrink-0 font-mono">{t.name}</span>
                  <span className="min-w-0 flex-1 truncate" title={t.args}>
                    {t.args}
                  </span>
                  <span className={cn("shrink-0", t.result === null && "animate-pulse")}>
                    {t.result === null ? "运行中" : "完成"}
                  </span>
                </div>
              ))}
            </div>
          )}
          {task.summary && <div className="text-xs">{task.summary}</div>}
        </div>
      )}
    </div>
  );
}
