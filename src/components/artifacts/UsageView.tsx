import { useCallback, useEffect, useState } from "react";
import { api } from "@/lib/tauri";
import { Button } from "@/components/ui/button";
import { Loader2, RefreshCw, Wallet } from "lucide-react";

/** 成本制品（R5-②）：get_usage_summary 真实用量（provider 计数）只读。 */

function fmt(n: number): string {
  return n >= 10000 ? `${(n / 10000).toFixed(1)} 万` : n.toLocaleString();
}

export function UsageView({
  workPath,
  refreshKey = 0,
}: {
  workPath: string;
  refreshKey?: number;
}) {
  const [usage, setUsage] = useState<{ input_tokens: number; output_tokens: number } | null>(
    null,
  );
  const [error, setError] = useState("");

  const reload = useCallback(async () => {
    try {
      setUsage(await api.getUsageSummary(workPath));
      setError("");
    } catch (e) {
      setError(String(e));
    }
  }, [workPath]);

  useEffect(() => {
    reload();
  }, [reload, refreshKey]);

  if (error) {
    return (
      <div className="flex flex-1 items-center justify-center p-6 text-center text-sm text-muted-foreground">
        {error}
      </div>
    );
  }
  if (!usage) {
    return (
      <div className="flex flex-1 items-center justify-center text-sm text-muted-foreground">
        <Loader2 className="mr-2 size-4 animate-spin" /> 读取用量…
      </div>
    );
  }

  const total = usage.input_tokens + usage.output_tokens;
  return (
    <div className="flex h-full min-w-0 flex-col">
      <div className="flex h-9 shrink-0 items-center gap-2 border-b px-3 text-xs text-muted-foreground">
        <Wallet className="size-3.5" />
        <span className="font-medium text-foreground/80">成本</span>
        <span>真实值 · provider 计数</span>
        <Button variant="ghost" size="sm" className="ml-auto h-6 px-2 text-[11px]" onClick={reload}>
          <RefreshCw data-icon="inline-start" />
          刷新
        </Button>
      </div>
      <div className="flex flex-1 flex-col items-center justify-center gap-6 p-6">
        <div className="flex items-end gap-8">
          <Stat label="输入 tokens" value={usage.input_tokens} />
          <Stat label="输出 tokens" value={usage.output_tokens} />
        </div>
        <div className="text-center text-sm text-muted-foreground">
          本作品全部 Agent 调用合计 ≈{fmt(total)} tokens
        </div>
      </div>
    </div>
  );
}

function Stat({ label, value }: { label: string; value: number }) {
  return (
    <div className="text-center">
      <div className="text-2xl font-semibold tabular-nums">{fmt(value)}</div>
      <div className="mt-1 text-xs text-muted-foreground">{label}</div>
    </div>
  );
}
