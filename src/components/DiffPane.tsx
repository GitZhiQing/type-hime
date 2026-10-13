import { useEffect, useState } from "react";
import { listen } from "@tauri-apps/api/event";
import { api, Proposal } from "@/lib/tauri";
import { Button } from "@/components/ui/button";
import { ScrollArea } from "@/components/ui/scroll-area";
import { Badge } from "@/components/ui/badge";
import { cn } from "cn";
import { Check, FilePlus2, Loader2, X } from "lucide-react";
import { toast } from "sonner";

interface DiffPaneProps {
  workPath: string;
  onApplied: () => void;
}

export function DiffPane({ workPath, onApplied }: DiffPaneProps) {
  const [proposals, setProposals] = useState<Proposal[] | null>(null);
  const [busyId, setBusyId] = useState<number | null>(null);

  useEffect(() => {
    let alive = true;
    const reload = () =>
      api.listProposals().then((p) => alive && setProposals(p));
    reload();
    const un = listen<Proposal>("review:new", () => reload());
    return () => {
      alive = false;
      un.then((f) => f());
    };
  }, []);

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
      <div className="flex flex-1 items-center justify-center p-6 text-center text-sm text-muted-foreground">
        暂无待审提议。AI 的修改会以提议出现在这里，接受才落盘。
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
              {p.diff.split("\n").map((line, i) => (
                <div
                  key={i}
                  className={cn(
                    line.startsWith("+") && !line.startsWith("+++") && "bg-emerald-500/10 text-emerald-700",
                    line.startsWith("-") && !line.startsWith("---") && "bg-red-500/10 text-red-700",
                    (line.startsWith("@@") || line.startsWith("---") || line.startsWith("+++")) &&
                      "text-muted-foreground",
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
