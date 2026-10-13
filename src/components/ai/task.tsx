import { CheckCircle2, Loader2, XCircle } from "lucide-react";

/**
 * 子任务卡（R4：run_flow 触发长流程期间的过程展示；AI Elements Task 同构最小版）。
 * 事件源：SubTaskStart/Delta/ToolCall/ToolResult/End（agent/subtask.rs 桥接）。
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
    <div className="rounded-lg border bg-muted/30 p-3 text-sm">
      <div className="flex items-center gap-2">
        {icon}
        <span className="font-medium">子任务</span>
        <span className="truncate text-xs text-muted-foreground">{task.label}</span>
        <span className="ml-auto shrink-0 text-xs text-muted-foreground">{statusText}</span>
      </div>
      {task.text.trim() && (
        <pre className="mt-2 max-h-40 overflow-y-auto whitespace-pre-wrap rounded bg-background p-2 font-sans text-[12px] leading-relaxed text-muted-foreground">
          {task.text}
        </pre>
      )}
      {task.toolCalls.length > 0 && (
        <div className="mt-1.5 flex flex-col gap-0.5 text-[11px] text-muted-foreground">
          {task.toolCalls.map((t, i) => (
            <div key={i} className="flex items-center gap-1.5">
              <span className="font-mono">{t.name}</span>
              <span className="truncate">{t.args.slice(0, 80)}</span>
              <span className="ml-auto shrink-0">
                {t.result === null ? "运行中…" : "完成"}
              </span>
            </div>
          ))}
        </div>
      )}
      {task.summary && <div className="mt-2 text-xs">{task.summary}</div>}
    </div>
  );
}
