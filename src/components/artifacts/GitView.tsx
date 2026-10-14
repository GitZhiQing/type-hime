import { useCallback, useEffect, useState } from "react";
import { api } from "@/lib/tauri";
import { collapseContext } from "@/lib/diff";
import { Button } from "@/components/ui/button";
import { cn } from "cn";
import { ChevronLeft, GitBranch, Loader2 } from "lucide-react";

/**
 * git 历史制品（R5-②）：提交列表 + 单提交 diff（git_show 新命令）。
 */

export function GitView({ workPath }: { workPath: string }) {
  const [log, setLog] = useState<string[] | null>(null);
  const [hash, setHash] = useState<string | null>(null);
  const [patch, setPatch] = useState<string | null>(null);
  const [loadingPatch, setLoadingPatch] = useState(false);

  const reload = useCallback(async () => {
    try {
      setLog(await api.gitLog(workPath, 50));
    } catch {
      setLog([]);
    }
  }, [workPath]);

  useEffect(() => {
    reload();
  }, [reload]);

  async function openCommit(line: string) {
    const h = line.split(" ")[0] ?? "";
    if (!h) return;
    setHash(h);
    setLoadingPatch(true);
    try {
      setPatch(await api.gitShow(workPath, h));
    } catch (e) {
      setPatch(`[读取失败] ${e}`);
    } finally {
      setLoadingPatch(false);
    }
  }

  if (hash) {
    return (
      <div className="flex h-full min-w-0 flex-col">
        <div className="flex h-9 shrink-0 items-center gap-2 border-b px-3 text-xs">
          <Button
            variant="ghost"
            size="sm"
            className="h-6 px-2 text-[11px]"
            onClick={() => {
              setHash(null);
              setPatch(null);
            }}
          >
            <ChevronLeft data-icon="inline-start" />
            返回历史
          </Button>
          <span className="font-mono text-muted-foreground">{hash.slice(0, 10)}</span>
        </div>
        <div className="min-h-0 flex-1 overflow-auto p-2">
          {loadingPatch ? (
            <div className="flex items-center justify-center py-10 text-sm text-muted-foreground">
              <Loader2 className="mr-2 size-4 animate-spin" /> 读取补丁…
            </div>
          ) : (
            <pre className="font-mono text-[12px] leading-relaxed">
              {collapseContext(patch ?? "").map((line, i) => (
                <div
                  key={i}
                  className={cn(
                    line.startsWith("+") && !line.startsWith("+++") && "bg-emerald-500/10 text-emerald-700",
                    line.startsWith("-") && !line.startsWith("---") && "bg-red-500/10 text-red-700",
                    (line.startsWith("@@") || line.startsWith("diff") || line.startsWith("index ")) &&
                      "text-muted-foreground",
                    line === "⋯" && "select-none text-muted-foreground/50",
                  )}
                >
                  {line || " "}
                </div>
              ))}
            </pre>
          )}
        </div>
      </div>
    );
  }

  return (
    <div className="flex h-full min-w-0 flex-col">
      <div className="flex h-9 shrink-0 items-center gap-2 border-b px-3 text-xs text-muted-foreground">
        <GitBranch className="size-3.5" />
        <span className="font-medium text-foreground/80">git 历史</span>
        <span>点提交查看 diff · 只读</span>
        <Button variant="ghost" size="sm" className="ml-auto h-6 px-2 text-[11px]" onClick={reload}>
          刷新
        </Button>
      </div>
      <div className="min-h-0 flex-1 overflow-y-auto p-2">
        {log === null ? (
          <div className="flex items-center justify-center py-10 text-sm text-muted-foreground">
            <Loader2 className="mr-2 size-4 animate-spin" /> 读取历史…
          </div>
        ) : log.length === 0 ? (
          <div className="flex h-full items-center justify-center text-sm text-muted-foreground">
            还没有提交
          </div>
        ) : (
          <ul className="flex flex-col gap-0.5">
            {log.map((line, i) => (
              <li key={i}>
                <button
                  type="button"
                  className="w-full truncate rounded px-2 py-1 text-left font-mono text-[11px] text-muted-foreground transition-colors hover:bg-accent hover:text-foreground"
                  title={line}
                  onClick={() => openCommit(line)}
                >
                  {line}
                </button>
              </li>
            ))}
          </ul>
        )}
      </div>
    </div>
  );
}
