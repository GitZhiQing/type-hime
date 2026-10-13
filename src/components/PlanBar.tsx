import { useState } from "react";
import { api, GateInfo, PipelineView } from "@/lib/tauri";
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
import { Check, ChevronDown, RotateCcw } from "lucide-react";
import { toast } from "sonner";

/**
 * PLAN 条（R2-④，替代 StageBar）：聊天输入框上方的可折叠阶段进度条。
 * 折叠=当前阶段+进度点+快捷确认；展开=各阶段详情与进入操作；skip/redo 收进「…」菜单。
 * 推进由 Agent/产物状态发起建议、人点确认（REDESIGN §5.4）。
 */

interface PlanBarProps {
  workPath: string;
  pipeline: PipelineView;
  onChange: (p: PipelineView) => void;
}

export function PlanBar({ workPath, pipeline, onChange }: PlanBarProps) {
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
          {nextHint(pipeline)}
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
              {pipeline.gates
                .filter((g) => g.confirmed)
                .map((g) => (
                  <DropdownMenuItem key={`redo-${g.stage}`} onClick={() => act(api.pipelineRedo, g.stage)}>
                    <RotateCcw data-icon="inline-start" />
                    重做「{g.label}」（其后确认一并取消）
                  </DropdownMenuItem>
                ))}
            </DropdownMenuContent>
          </DropdownMenu>
        </div>
      </div>
      <CollapsibleContent>
        <div className="flex flex-wrap items-center gap-1 px-2 pb-2">
          {pipeline.gates.map((g, i) => (
            <PlanChip key={g.stage} gate={g} first={i === 0} onClick={() => act(api.pipelineEnter, g.stage)} />
          ))}
        </div>
      </CollapsibleContent>
    </Collapsible>
  );
}

type api_stage = Parameters<typeof api.pipelineEnter>[1];

function nextHint(pipeline: PipelineView): string {
  const cur = pipeline.gates.find((g) => g.current);
  if (!cur) return "点击阶段进入流水线";
  if (!cur.confirmed) {
    if (cur.stage === "chapters") return "完成本章正文与账本回写后确认推进";
    if (cur.stage === "review") return "审校收敛（问题清单清零）即完成本卷";
    return `完成「${cur.label}」产物后确认推进`;
  }
  const nextUnconfirmed = pipeline.gates.find((g) => g.enterable && !g.confirmed);
  if (nextUnconfirmed) return `下一步：进入「${nextUnconfirmed.label}」`;
  const anyStage = pipeline.gates.find((g) => g.enterable && !g.current);
  return anyStage ? `可进入「${anyStage.label}」` : "全部阶段已确认";
}

function PlanChip({
  gate,
  first,
  onClick,
}: {
  gate: GateInfo;
  first: boolean;
  onClick: () => void;
}) {
  const clickable = gate.enterable && !gate.current;
  return (
    <button
      type="button"
      onClick={clickable ? onClick : undefined}
      title={gate.blocked_by ?? (gate.confirmed ? "已确认" : gate.current ? "当前阶段" : "")}
      className={cn(
        "flex items-center gap-1 rounded-full px-2 py-0.5 text-[11px] transition-colors",
        first ? "" : "ml-0.5",
        gate.current
          ? "bg-primary text-primary-foreground"
          : gate.confirmed
            ? "bg-primary/10 text-primary"
            : gate.enterable
              ? "text-muted-foreground hover:bg-accent"
              : "cursor-not-allowed text-muted-foreground/50",
      )}
      disabled={!clickable}
    >
      {gate.confirmed && !gate.current && <Check className="size-3" />}
      {gate.label}
    </button>
  );
}
