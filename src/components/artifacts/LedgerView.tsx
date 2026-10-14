import { useEffect, useState } from "react";
import { api, Ledger, LedgerEntry } from "@/lib/tauri";
import { Badge } from "@/components/ui/badge";
import { Loader2, ScrollText } from "lucide-react";

/**
 * 账本制品（R5-② · REDESIGN §5.5）：get_ledger 结构化视图（此前 api 已封装未用，此处启用）。
 * 分节：角色状态 / 已交代事实 / 伏笔登记 / 时间线；条目只读浏览。
 */

const SECTIONS: { key: keyof Ledger; label: string }[] = [
  { key: "characters", label: "角色状态" },
  { key: "facts", label: "已交代事实" },
  { key: "foreshadows", label: "伏笔登记" },
  { key: "timeline", label: "时间线" },
];

export function LedgerView({ workPath }: { workPath: string }) {
  const [ledger, setLedger] = useState<Ledger | null>(null);
  const [error, setError] = useState("");

  useEffect(() => {
    api
      .getLedger(workPath)
      .then(setLedger)
      .catch((e) => setError(String(e)));
  }, [workPath]);

  if (error) {
    return (
      <div className="flex flex-1 items-center justify-center p-6 text-center text-sm text-muted-foreground">
        {error}
      </div>
    );
  }
  if (!ledger) {
    return (
      <div className="flex flex-1 items-center justify-center text-sm text-muted-foreground">
        <Loader2 className="mr-2 size-4 animate-spin" /> 读取账本…
      </div>
    );
  }

  const total =
    ledger.characters.length +
    ledger.facts.length +
    ledger.foreshadows.length +
    ledger.timeline.length;

  return (
    <div className="flex h-full min-w-0 flex-col">
      <div className="flex h-9 shrink-0 items-center gap-2 border-b px-3 text-xs text-muted-foreground">
        <ScrollText className="size-3.5" />
        <span className="font-medium text-foreground/80">账本</span>
        <span>共 {total} 条 · 只读</span>
      </div>
      <div className="min-h-0 flex-1 overflow-y-auto p-3">
        {total === 0 ? (
          <div className="flex h-full items-center justify-center text-sm text-muted-foreground">
            账本为空——正文经「/回写」提取状态变化后在这里积累
          </div>
        ) : (
          SECTIONS.map(({ key, label }) =>
            ledger[key].length === 0 ? null : (
              <section key={key} className="mb-4">
                <h3 className="mb-1.5 text-xs font-medium text-foreground/80">
                  {label}
                  <span className="ml-1.5 text-[10px] text-muted-foreground">
                    {ledger[key].length}
                  </span>
                </h3>
                <div className="flex flex-col gap-1.5">
                  {ledger[key].map((e, i) => (
                    <EntryCard key={i} entry={e} />
                  ))}
                </div>
              </section>
            ),
          )
        )}
      </div>
    </div>
  );
}

function EntryCard({ entry }: { entry: LedgerEntry }) {
  return (
    <div className="rounded-md border p-2 text-xs">
      <div className="flex items-center gap-2">
        <span className="truncate font-medium" title={entry.title}>
          {entry.title}
        </span>
        {entry.last_touched != null && (
          <Badge variant="outline" className="shrink-0 text-[10px]">
            第 {entry.last_touched} 章更新
          </Badge>
        )}
      </div>
      <p className="mt-1 whitespace-pre-wrap leading-relaxed text-muted-foreground">
        {entry.body}
      </p>
    </div>
  );
}
