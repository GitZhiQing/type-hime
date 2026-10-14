import { useCallback, useEffect, useState } from "react";
import { api, type SessionGoalView } from "@/lib/tauri";
import { Button } from "@/components/ui/button";
import {
  Collapsible,
  CollapsibleContent,
  CollapsibleTrigger,
} from "@/components/ui/collapsible";
import { cn } from "cn";
import {
  CheckCircle2,
  ChevronDown,
  CircleAlert,
  Loader2,
  Pause,
  Play,
  Target,
  X,
} from "lucide-react";
import { toast } from "sonner";

/**
 * 长时目标横幅（R4.5-④ · REDESIGN §8.6）：PLAN 条上方的会话目标进度条。
 * 折叠 = 目标摘要 + 状态 + 轮次/用量；展开 = 迭代行（标题=上一轮 nextAction）+ 验证结论。
 * 动作：暂停/清除直调命令；继续经 window "goal:resume" 由 ChatPane 以聊天流驱动
 * （续跑需要 SSE 通道，/goal 恢复 在后端翻状态后直接跑恢复轮）。
 * 无目标时整条不渲染；goal 状态随会话切换重载（不放 ChatPane 局部状态，防串会话）。
 */

interface GoalBarProps {
  workPath: string;
  sessionId: string | null;
  /** 会话/一轮结束 +1 触发刷新 */
  refreshKey?: number;
}

function fmtK(n: number): string {
  return n >= 1000 ? `${(n / 1000).toFixed(1)}k` : String(n);
}

const STATUS_STYLE: Record<string, string> = {
  active: "text-emerald-600",
  paused: "text-amber-600",
  budget_limited: "text-orange-600",
  complete: "text-emerald-700",
};

export function GoalBar({ workPath, sessionId, refreshKey = 0 }: GoalBarProps) {
  const [goal, setGoal] = useState<SessionGoalView | null>(null);
  const [busy, setBusy] = useState<"pause" | "clear" | null>(null);

  const reload = useCallback(async () => {
    if (!sessionId) {
      setGoal(null);
      return;
    }
    try {
      setGoal(await api.getSessionGoal(workPath, sessionId));
    } catch {
      // 会话尚无存档等情形按无目标处理
      setGoal(null);
    }
  }, [workPath, sessionId]);

  useEffect(() => {
    reload();
  }, [reload, refreshKey]);

  // 聊天流里的 goal_update 事件经 window 事件通知刷新（ChatPane 发，仿 review:changed）
  useEffect(() => {
    const onGoalChanged = () => reload();
    window.addEventListener("goal:changed", onGoalChanged);
    return () => window.removeEventListener("goal:changed", onGoalChanged);
  }, [reload]);

  if (!goal) return null;

  async function control(action: "pause" | "clear") {
    if (!sessionId || busy) return;
    setBusy(action);
    try {
      await api.goalControl(workPath, sessionId, action);
      await reload();
      toast.success(action === "pause" ? "已暂停长时目标" : "已清除会话目标");
    } catch (e) {
      toast.error(String(e));
    } finally {
      setBusy(null);
    }
  }

  /** 继续：续跑需要流式通道，交 ChatPane 以 /goal 恢复 走聊天流 */
  function resume() {
    window.dispatchEvent(new CustomEvent("goal:resume"));
  }

  const running = goal.status === "active";
  const done = goal.status === "complete";
  const rounds = `${goal.iterations.length}/${goal.budget.max_iterations}`;

  return (
    <Collapsible className="group relative shrink-0 border-b bg-muted/20">
      <CollapsibleTrigger
        render={
          <button
            type="button"
            className="group flex h-8 w-full items-center gap-2 px-2 text-left text-[11px] text-muted-foreground transition-colors hover:bg-muted/50"
            aria-label="展开或折叠长时目标"
          />
        }
      >
        <ChevronDown className="size-3.5 transition-transform group-data-[panel-open]:rotate-180" />
        {running ? (
          <Loader2 className={cn("size-3.5 animate-spin", STATUS_STYLE.active)} />
        ) : (
          <Target className={cn("size-3.5", STATUS_STYLE[goal.status])} />
        )}
        <span className="font-medium text-foreground/80">目标</span>
        <span className={cn("font-medium", STATUS_STYLE[goal.status])}>
          {done ? "✅ 已完成" : goal.status_label}
        </span>
        <span className="min-w-0 flex-1 truncate" title={goal.objective}>
          {goal.objective}
        </span>
        <span className="shrink-0 tabular-nums">
          第 {rounds} 轮 · {fmtK(goal.tokens_used)} tokens
        </span>
      </CollapsibleTrigger>

      {/* D53：展开内容浮层覆盖（absolute），不占布局——与 PLAN 条同一处理 */}
      <CollapsibleContent className="absolute top-full right-0 left-0 z-20 max-h-[60vh] overflow-y-auto border-b bg-background shadow-md">
        <div className="border-t px-3 py-2.5">
          {/* 目标全文 + 预算 */}
          <p className="text-xs leading-relaxed">{goal.objective}</p>
          <p className="mt-1 flex flex-wrap items-center gap-x-3 gap-y-1 text-[10px] text-muted-foreground">
            <span>
              轮数 {rounds}
              {goal.budget.max_total_tokens != null &&
                ` · token 上限 ${fmtK(goal.budget.max_total_tokens)}`}
            </span>
            <span>已用 {fmtK(goal.tokens_used)} tokens</span>
            {running && goal.current_title && (
              <span className="text-emerald-700">
                下一轮：{goal.current_title}
              </span>
            )}
            {done && goal.iterations.length > 0 && (
              <span className="text-emerald-700">
                完成结论：{goal.iterations[goal.iterations.length - 1].verification?.reason}
              </span>
            )}
          </p>

          {/* 迭代行：标题=该轮目标/上一轮 nextAction + 验证结论 */}
          {goal.iterations.length > 0 && (
            <ul className="mt-2 divide-y rounded-md border">
              {goal.iterations.map((it) => (
                <li key={it.index} className="flex items-start gap-2 px-2 py-1.5">
                  <span className="mt-0.5 shrink-0 tabular-nums text-[10px] text-muted-foreground">
                    #{it.index}
                  </span>
                  {it.verification ? (
                    it.verification.passed ? (
                      <CheckCircle2 className="mt-0.5 size-3.5 shrink-0 text-emerald-600" />
                    ) : (
                      <CircleAlert className="mt-0.5 size-3.5 shrink-0 text-amber-600" />
                    )
                  ) : (
                    <Loader2 className="mt-0.5 size-3.5 shrink-0 animate-spin text-muted-foreground" />
                  )}
                  <div className="min-w-0 flex-1">
                    <p className="truncate text-[11px]" title={it.title}>
                      {it.title}
                    </p>
                    {it.verification && (
                      <p
                        className="truncate text-[10px] text-muted-foreground"
                        title={it.verification.reason}
                      >
                        {it.verification.passed ? "验证通过：" : "未过，续跑："}
                        {it.verification.reason}
                      </p>
                    )}
                  </div>
                  <span className="shrink-0 text-[10px] tabular-nums text-muted-foreground">
                    {fmtK(it.tokens)}
                  </span>
                </li>
              ))}
            </ul>
          )}

          {/* 动作：active=暂停；paused=继续+清除；终态=清除 */}
          <div className="mt-2 flex items-center gap-1.5">
            {running && (
              <Button
                size="sm"
                variant="outline"
                className="h-6 px-2 text-[11px]"
                disabled={busy !== null}
                onClick={() => control("pause")}
              >
                {busy === "pause" ? (
                  <Loader2 className="size-3 animate-spin" />
                ) : (
                  <Pause data-icon="inline-start" />
                )}
                暂停（本轮结束后停）
              </Button>
            )}
            {goal.status === "paused" && (
              <Button
                size="sm"
                variant="outline"
                className="h-6 px-2 text-[11px]"
                onClick={resume}
              >
                <Play data-icon="inline-start" />
                继续
              </Button>
            )}
            {!done && (
              <Button
                size="sm"
                variant="ghost"
                className="h-6 px-2 text-[11px] text-muted-foreground"
                disabled={busy !== null}
                onClick={() => control("clear")}
              >
                {busy === "clear" ? (
                  <Loader2 className="size-3 animate-spin" />
                ) : (
                  <X data-icon="inline-start" />
                )}
                清除
              </Button>
            )}
          </div>
        </div>
      </CollapsibleContent>
    </Collapsible>
  );
}
