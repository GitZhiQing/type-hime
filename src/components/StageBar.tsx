import { useState } from "react";
import { api, GateInfo, PipelineView } from "@/lib/tauri";
import { Button } from "@/components/ui/button";
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuTrigger,
} from "@/components/ui/dropdown-menu";
import { cn } from "cn";
import { Check, ChevronDown, Circle, Lock, RotateCcw } from "lucide-react";
import { toast } from "sonner";

interface StageBarProps {
  workPath: string;
  pipeline: PipelineView;
  onChange: (p: PipelineView) => void;
}

export function StageBar({ workPath, pipeline, onChange }: StageBarProps) {
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

  return (
    <div className="flex h-9 shrink-0 items-center gap-1 border-b bg-muted/30 px-3">
      {pipeline.gates.map((g, i) => (
        <StageChip
          key={g.stage}
          gate={g}
          first={i === 0}
          onClick={() => act(api.pipelineEnter, g.stage)}
        />
      ))}

      <div className="ml-auto flex items-center gap-1">
        {current && !current.confirmed && (
          <Button
            size="sm"
            variant="outline"
            className="h-7 text-xs"
            disabled={busy}
            onClick={() => act(api.pipelineConfirm, current.stage)}
          >
            <Check data-icon="inline-start" />
            确认「{current.label}」并推进
          </Button>
        )}
        <DropdownMenu>
          <DropdownMenuTrigger
            render={
              <Button size="sm" variant="ghost" className="h-7 px-2 text-xs" disabled={busy}>
                <ChevronDown data-icon="inline-end" />
                阶段操作
              </Button>
            }
          >
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
              {pipeline.confirmed.length > 0 && (
                <>
                  {pipeline.gates
                    .filter((g) => g.confirmed)
                    .map((g) => (
                      <DropdownMenuItem
                        key={`redo-${g.stage}`}
                        onClick={() => act(api.pipelineRedo, g.stage)}
                      >
                        <RotateCcw data-icon="inline-start" />
                        重做「{g.label}」（其后确认一并取消）
                      </DropdownMenuItem>
                    ))}
                </>
              )}
            </DropdownMenuContent>
          </DropdownMenuTrigger>
        </DropdownMenu>
      </div>
    </div>
  );
}

type api_stage = Parameters<typeof api.pipelineEnter>[1];

function StageChip({
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
      {gate.confirmed && !gate.current ? (
        <Check className="size-3" />
      ) : gate.enterable ? (
        <Circle className="size-2.5 fill-current" />
      ) : (
        <Lock className="size-3" />
      )}
      {gate.label}
    </button>
  );
}
