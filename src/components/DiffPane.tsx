import { useEffect, useState } from "react";
import { listen } from "@tauri-apps/api/event";
import { api, Proposal } from "@/lib/tauri";
import { Button } from "@/components/ui/button";
import { ScrollArea } from "@/components/ui/scroll-area";
import { Badge } from "@/components/ui/badge";
import { Switch } from "@/components/ui/switch";
import { cn } from "cn";
import { Check, FilePlus2, Loader2, X } from "lucide-react";
import { toast } from "sonner";

interface DiffPaneProps {
  workPath: string;
  onApplied: () => void;
}

/** 连续 3 行以上的上下文行折叠为省略号（默认只展示变更±3 行的语义不变，视觉更紧凑） */
function collapseContext(diff: string): string[] {
  const lines = diff.split("\n");
  const out: string[] = [];
  let run: string[] = [];
  const flush = () => {
    if (run.length > 4) {
      out.push(run[0], "⋯", run[run.length - 1]);
    } else {
      out.push(...run);
    }
    run = [];
  };
  for (const line of lines) {
    const isContext = !line.startsWith("+") && !line.startsWith("-") && !line.startsWith("@@");
    if (isContext) {
      run.push(line);
    } else {
      flush();
      out.push(line);
    }
  }
  flush();
  return out;
}

export function DiffPane({ workPath, onApplied }: DiffPaneProps) {
  const [proposals, setProposals] = useState<Proposal[] | null>(null);
  const [busyId, setBusyId] = useState<number | null>(null);
  const [autoLedger, setAutoLedger] = useState(
    () => localStorage.getItem("type-hime:auto-ledger") === "1",
  );

  function toggleAutoLedger(v: boolean) {
    setAutoLedger(v);
    localStorage.setItem("type-hime:auto-ledger", v ? "1" : "0");
  }

  useEffect(() => {
    let alive = true;
    const reload = () => api.listProposals().then((p) => alive && setProposals(p));
    reload();
    const un = listen<Proposal>("review:new", (event) => {
      // 账本回写半自动：开关开启时，账本提议自动接受（低风险结构化更新）
      const p = event.payload;
      if (
        autoLedger &&
        p.rel_path.endsWith("账本.md") &&
        p.work_path === workPath
      ) {
        api
          .applyProposal(workPath, p.id)
          .then(() => {
            toast.success(`已自动接受账本回写（提议 #${p.id}）`);
            onApplied();
          })
          .catch(() => reload());
        return;
      }
      reload();
    });
    return () => {
      alive = false;
      un.then((f) => f());
    };
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [autoLedger, workPath]);

  async function act(id: number, accept: boolean) {
    setBusyId(id);
    try {
      if (accept) {
        const msg = await api.applyProposal(workPath, id);
        toast.success(msg);
        onApplied();
      } else {
        await api.rejectProposal(id);
        toast.info("已拒绝提议");
      }
      setProposals((ps) => (ps ?? []).filter((p) => p.id !== id));
    } catch (e) {
      toast.error(String(e));
    } finally {
      setBusyId(null);
    }
  }

  if (proposals === null) {
    return (
      <div className="flex flex-1 items-center justify-center text-sm text-muted-foreground">
        <Loader2 className="mr-2 size-4 animate-spin" /> 加载提议…
      </div>
    );
  }

  if (proposals.length === 0) {
    return (
      <div className="flex flex-1 flex-col items-center justify-center gap-3 p-6 text-center text-sm text-muted-foreground">
        <div>暂无待审提议。AI 的修改会以提议出现在这里，接受才落盘。</div>
        <label className="flex items-center gap-2 text-xs">
          <Switch checked={autoLedger} onCheckedChange={toggleAutoLedger} />
          账本回写自动接受（低风险结构化更新）
        </label>
      </div>
    );
  }

  return (
    <ScrollArea className="flex-1">
      <div className="flex flex-col gap-4 p-3">
        {proposals.map((p) => (
          <div key={p.id} className="rounded-lg border">
            <div className="flex items-center gap-2 border-b px-3 py-2">
              {p.is_new_file ? (
                <Badge variant="secondary">
                  <FilePlus2 data-icon="inline-start" />
                  新建
                </Badge>
              ) : (
                <Badge variant="secondary">修改</Badge>
              )}
              <span className="truncate font-mono text-xs" title={p.rel_path}>
                {p.rel_path}
              </span>
              <span className="ml-auto flex gap-1">
                <Button
                  size="sm"
                  className="h-7 text-xs"
                  disabled={busyId === p.id}
                  onClick={() => act(p.id, true)}
                >
                  {busyId === p.id ? (
                    <Loader2 data-icon="inline-start" className="animate-spin" />
                  ) : (
                    <Check data-icon="inline-start" />
                  )}
                  接受落盘
                </Button>
                <Button
                  size="sm"
                  variant="outline"
                  className="h-7 text-xs"
                  disabled={busyId === p.id}
                  onClick={() => act(p.id, false)}
                >
                  <X data-icon="inline-start" />
                  拒绝
                </Button>
              </span>
            </div>
            {p.reason && (
              <div className="border-b bg-muted/40 px-3 py-1.5 text-xs text-muted-foreground">
                理由：{p.reason}
              </div>
            )}
            <pre className="overflow-x-auto p-2 font-mono text-[12px] leading-relaxed">
              {collapseContext(p.diff).map((line, i) => (
                <div
                  key={i}
                  className={cn(
                    line.startsWith("+") && !line.startsWith("+++") && "bg-emerald-500/10 text-emerald-700",
                    line.startsWith("-") && !line.startsWith("---") && "bg-red-500/10 text-red-700",
                    (line.startsWith("@@") || line.startsWith("---") || line.startsWith("+++")) &&
                      "text-muted-foreground",
                    line === "⋯" && "text-muted-foreground/50 select-none",
                  )}
                >
                  {line || " "}
                </div>
              ))}
            </pre>
          </div>
        ))}
      </div>
    </ScrollArea>
  );
}
