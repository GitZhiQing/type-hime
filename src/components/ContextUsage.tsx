import { useCallback, useEffect, useState } from "react";
import { api } from "@/lib/tauri";
import { cn } from "cn";
import { AlertTriangle } from "lucide-react";

/**
 * 上下文窗口占用（R10 精简形态）：输入框下方工具条内的一行小字。
 * 折叠条/展开面板已删（原 ContextBar）——只保留用量口径：
 * 窗口占用 = 最近一轮输入 tokens（真实值）；累计用量置于 hover 详情。
 * 接近上限（>80%）变琥珀并提示新建会话。
 */

interface ContextUsageProps {
  workPath: string;
  sessionId: string | null;
  /** 一轮完成 +1 触发刷新（ChatPane 维护） */
  refreshKey?: number;
}

function fmtK(n: number): string {
  return n >= 1000 ? `${(n / 1000).toFixed(1)}k` : String(n);
}

export function ContextUsage({ workPath, sessionId, refreshKey = 0 }: ContextUsageProps) {
  const [pct, setPct] = useState<number | null>(null);
  const [lastInput, setLastInput] = useState(0);
  const [window, setWindow] = useState(128_000);
  const [ cumulative, setCumulative] = useState<{ input_tokens: number; output_tokens: number } | null>(null);

  const reload = useCallback(async () => {
    if (!sessionId) {
      setPct(null);
      return;
    }
    try {
      const ov = await api.contextOverview(workPath, sessionId, 1, "ch-001");
      setPct(Math.round(ov.session_percent ?? 0));
      setLastInput(ov.last_input_tokens ?? 0);
      setWindow(ov.context_window ?? 128_000);
      setCumulative(ov.session_usage);
    } catch {
      setPct(null);
    }
  }, [workPath, sessionId]);

  useEffect(() => {
    reload();
  }, [reload, refreshKey]);

  if (pct === null) return null;
  const nearLimit = pct >= 80;
  const detail = [
    `窗口占用 = 最近一轮输入 ≈${lastInput.toLocaleString()} / 窗口 ${window.toLocaleString()} tokens（真实值 · provider 计数）`,
    cumulative
      ? `会话累计 ≈${cumulative.input_tokens.toLocaleString()} 入 / ${cumulative.output_tokens.toLocaleString()} 出`
      : null,
    nearLimit ? "接近窗口上限：建议新建会话重启上下文（旧会话可随时切回）" : "会话无硬上限",
  ]
    .filter(Boolean)
    .join("\n");

  return (
    <span
      className={cn(
        "ml-auto flex items-center gap-1 text-[11px] tabular-nums text-muted-foreground",
        nearLimit && "text-amber-600",
      )}
      title={detail}
    >
      {nearLimit && <AlertTriangle className="size-3" />}
      上下文 ≈{fmtK(lastInput)}/{fmtK(window)}
      <span className={cn(nearLimit && "font-medium")}>({pct}%)</span>
    </span>
  );
}
