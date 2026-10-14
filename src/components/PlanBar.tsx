import { useState } from "react";
import { api, PipelineView } from "@/lib/tauri";
import { Button } from "@/components/ui/button";
import {
  Collapsible,
  CollapsibleContent,
  CollapsibleTrigger,
} from "@/components/ui/collapsible";
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuTrigger,
} from "@/components/ui/dropdown-menu";
import { cn } from "cn";
import { Check, ChevronDown, Circle, Lock, RotateCcw } from "lucide-react";
import { toast } from "sonner";

/**
 * PLAN 条（替代 StageBar）：聊天输入框上方的可折叠阶段进度条。
 * 折叠=当前阶段+进度点+快捷确认；展开=阶段列表（状态图标 + 阶段名 + 状态 + 重做按钮）。
 * 推进由 Agent/产物状态发起建议、人点确认（REDESIGN §5.4）。
 */

interface PlanBarProps {
  workPath: string;
  pipeline: PipelineView;
  /** R15：样章是否已产出（设定阶段提示区分「开始访谈」与「已产出待确认」） */
  hasSample?: boolean;
  onChange: (p: PipelineView) => void;
}

export function PlanBar({ workPath, pipeline, hasSample, onChange }: PlanBarProps) {
  const [busy, setBusy] = useState(false);

  async function act(
    fn: (wp: string, s: api_stage) => Promise<PipelineView>,
    stage: api_stage,
  ) {
    setBusy(true);
    try {
      onChange(await fn(workPath, stage));
    } catch (e) {
      toast.error(String(e));
    } finally {
      setBusy(false);
    }
  }

  const current = pipeline.gates.find((g) => g.current);
  const confirmedCount = pipeline.gates.filter((g) => g.confirmed).length;

  return (
    <Collapsible className="shrink-0 border-b bg-muted/30">
      <div className="flex h-8 items-center gap-2 px-2 text-[11px]">
        <CollapsibleTrigger
          render={
            <button
              type="button"
              className="group flex items-center gap-0.5 font-medium text-muted-foreground hover:text-foreground"
              aria-label="展开或折叠阶段计划"
            />
          }
        >
          <ChevronDown className="size-3.5 transition-transform group-data-[panel-open]:rotate-180" />
          PLAN
        </CollapsibleTrigger>
        <span className="font-medium">{current?.label ?? "未进入阶段"}</span>
        <div className="flex items-center gap-1">
          {pipeline.gates.map((g) => (
            <span
              key={g.stage}
              title={`${g.label}${g.confirmed ? "（已确认）" : g.current ? "（当前）" : ""}`}
              className={cn(
                "size-1.5 rounded-full",
                g.confirmed
                  ? "bg-primary"
                  : g.current
                    ? "ring-2 ring-primary ring-offset-1 ring-offset-muted/30"
                    : "bg-muted-foreground/30",
              )}
            />
          ))}
        </div>
        <span className="text-muted-foreground">{confirmedCount}/{pipeline.gates.length}</span>
        <span className="ml-1 hidden truncate text-muted-foreground sm:inline">
          {nextHint(pipeline, hasSample)}
        </span>
        <div className="ml-auto flex items-center gap-1">
          {current && !current.confirmed && (
            <Button
              size="sm"
              variant="outline"
              className="h-6 px-2 text-[11px]"
              disabled={busy}
              onClick={() => act(api.pipelineConfirm, current.stage)}
            >
              <Check data-icon="inline-start" />
              确认并推进
            </Button>
          )}
          <DropdownMenu>
            <DropdownMenuTrigger
              render={
                <Button
                  size="sm"
                  variant="ghost"
                  className="size-6 p-0 text-[11px]"
                  disabled={busy}
                  aria-label="阶段操作"
                />
              }
            >
              ⋯
            </DropdownMenuTrigger>
            <DropdownMenuContent align="end">
              {current && (
                <>
                  <DropdownMenuItem onClick={() => act(api.pipelineConfirm, current.stage)}>
                    确认当前阶段（校验产物）
                  </DropdownMenuItem>
                  <DropdownMenuItem onClick={() => act(api.pipelineSkip, current.stage)}>
                    显式跳过当前阶段（不校验）
                  </DropdownMenuItem>
                </>
              )}
            </DropdownMenuContent>
          </DropdownMenu>
        </div>
      </div>

      <CollapsibleContent>
        <div className="px-2 pt-0.5 pb-2">
          <ul className="divide-y overflow-hidden rounded-md border bg-background/60">
            {pipeline.gates.map((g) => {
              const enterable = g.enterable && !g.current;
              return (
                <li key={g.stage}>
                  <div
                    role={enterable ? "button" : undefined}
                    onClick={enterable ? () => act(api.pipelineEnter, g.stage) : undefined}
                    title={g.blocked_by ?? undefined}
                    className={cn(
                      "group/row flex items-center gap-2 px-2.5 py-1.5 text-xs transition-colors",
                      enterable && "cursor-pointer hover:bg-accent/50",
                      g.current && "bg-primary/5",
                    )}
                  >
                    {g.confirmed ? (
                      <Check className="size-3.5 shrink-0 text-emerald-600" />
                    ) : g.current ? (
                      <Circle className="size-3.5 shrink-0 fill-primary text-primary" />
                    ) : enterable ? (
                      <Circle className="size-3.5 shrink-0 text-muted-foreground/50" />
                    ) : (
                      <Lock className="size-3.5 shrink-0 text-muted-foreground/40" />
                    )}
                    <span className={cn("flex-1 truncate", g.current && "font-medium")}>
                      {g.label}
                    </span>
                    <span className="shrink-0 text-[10px] text-muted-foreground">
                      {g.current
                        ? "当前阶段"
                        : g.confirmed
                          ? "已确认"
                          : enterable
                            ? "可进入"
                            : "未解锁"}
                    </span>
                    {g.confirmed && (
                      <button
                        type="button"
                        className="shrink-0 rounded p-0.5 text-muted-foreground/35 transition-colors hover:bg-accent hover:text-foreground disabled:opacity-30"
                        disabled={busy}
                        title={`重做「${g.label}」（其后确认一并取消）`}
                        aria-label={`重做 ${g.label}`}
                        onClick={(e) => {
                          e.stopPropagation();
                          act(api.pipelineRedo, g.stage);
                        }}
                      >
                        <RotateCcw className="size-3" />
                      </button>
                    )}
                  </div>
                </li>
              );
            })}
          </ul>
        </div>
      </CollapsibleContent>
    </Collapsible>
  );
}

type api_stage = Parameters<typeof api.pipelineEnter>[1];

function nextHint(pipeline: PipelineView, hasSample?: boolean): string {
  const cur = pipeline.gates.find((g) => g.current);
  if (!cur) return "点击阶段进入流水线";
  if (!cur.confirmed) {
    // R15 开书工作室：设定阶段下一步 = 开书访谈（直接对话即可，零命令）
    if (cur.stage === "settings")
      return hasSample
        ? "开书五件套已产出：审阅接受后确认推进"
        : "下一步：开始开书访谈（直接对话即可）";
    if (cur.stage === "chapters") return "完成本章正文与账本回写后确认推进";
    if (cur.stage === "review") return "审校收敛（问题清单清零）即完成本卷";
    return `完成「${cur.label}」产物后确认推进`;
  }
  const nextUnconfirmed = pipeline.gates.find((g) => g.enterable && !g.confirmed);
  if (nextUnconfirmed) return `下一步：进入「${nextUnconfirmed.label}」`;
  const anyStage = pipeline.gates.find((g) => g.enterable && !g.current);
  return anyStage ? `可进入「${anyStage.label}」` : "全部阶段已确认";
}
