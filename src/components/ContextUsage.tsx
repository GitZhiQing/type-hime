import { useCallback, useEffect, useState } from "react";
import { api, ContextOverview } from "@/lib/tauri";
import {
  PreviewCard,
  PreviewCardContent,
  PreviewCardTrigger,
} from "@/components/ui/preview-card";
import { cn } from "cn";

/**
 * 上下文容量（R10 · ZCode 上下文占用显示同构）：
 * 默认只显示一个小图标（仪表盘，占用 >=80% 变琥珀 + 警示角标），
 * 悬浮弹出面板显示详情——标题+数值占比、进度条、明细（消息历史/剩余可用）、
 * 底部参考（会话累计/装配估算）。
 * 口径如实：窗口占用 = 最近一轮输入 tokens（provider 真实计数）；装配估算为本地近似（D9）。
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
  const [ov, setOv] = useState<ContextOverview | null>(null);

  const reload = useCallback(async () => {
    if (!sessionId) {
      setOv(null);
      return;
    }
    try {
      setOv(await api.contextOverview(workPath, sessionId, 1, "ch-001"));
    } catch {
      setOv(null);
    }
  }, [workPath, sessionId]);

  useEffect(() => {
    reload();
  }, [reload, refreshKey]);

  if (!ov) return null;
  const pct = Math.round(ov.session_percent ?? 0);
  const nearLimit = pct >= 80;
  const lastInput = ov.last_input_tokens ?? 0;
  const window = ov.context_window ?? 128_000;
  const remain = Math.max(0, window - lastInput);
  // 环形进度（R10 反馈：图标本身直观可见占用）：周长 2πr，r=9 → ≈56.55
  const R = 9;
  const C = 2 * Math.PI * R;
  const dash = Math.min(1, pct / 100) * C;

  return (
    <PreviewCard>
      <PreviewCardTrigger
        render={
          <button
            type="button"
            aria-label="上下文容量"
            className={cn(
              "relative flex size-8 shrink-0 items-center justify-center rounded-md transition-colors hover:bg-accent",
              nearLimit && "text-amber-600",
            )}
          />
        }
      >
        <svg viewBox="0 0 24 24" className="size-5" aria-hidden="true">
          <circle
            cx="12"
            cy="12"
            r={R}
            fill="none"
            strokeWidth="2.5"
            className="stroke-muted-foreground/25"
          />
          <circle
            cx="12"
            cy="12"
            r={R}
            fill="none"
            strokeWidth="2.5"
            strokeLinecap="round"
            strokeDasharray={`${dash} ${C}`}
            transform="rotate(-90 12 12)"
            className={nearLimit ? "stroke-amber-500" : "stroke-primary"}
          />
        </svg>
        <span
          className={cn(
            "absolute text-[8px] leading-none font-medium tabular-nums",
            nearLimit ? "text-amber-700" : "text-muted-foreground",
          )}
        >
          {pct}
        </span>
      </PreviewCardTrigger>
      <PreviewCardContent>
        {/* 头部：标题 + 数值 */}
        <div className="flex items-baseline justify-between">
          <span className="text-sm font-medium">上下文容量</span>
          <span className={cn("text-xs tabular-nums", nearLimit && "font-medium text-amber-600")}>
            ≈{fmtK(lastInput)} / {fmtK(window)}（{pct}%）
          </span>
        </div>
        {/* 进度条 */}
        <div className="mt-2 h-1.5 w-full overflow-hidden rounded-full bg-muted">
          <div
            className={cn(
              "h-full rounded-full transition-all",
              nearLimit ? "bg-amber-500" : "bg-primary",
            )}
            style={{ width: `${Math.min(100, pct)}%` }}
          />
        </div>
        {/* 明细 */}
        <ul className="mt-2.5 flex flex-col gap-1.5 text-xs">
          <li className="flex items-center gap-2">
            <span className="size-1.5 shrink-0 rounded-full bg-primary" />
            <span className="text-muted-foreground">消息历史（最近一轮输入）</span>
            <span className="ml-auto tabular-nums">≈{fmtK(lastInput)}</span>
          </li>
          <li className="flex items-center gap-2">
            <span className="size-1.5 shrink-0 rounded-full bg-muted-foreground/40" />
            <span className="text-muted-foreground">剩余可用</span>
            <span className="ml-auto tabular-nums">≈{fmtK(remain)}</span>
          </li>
        </ul>
        {/* 参考区 */}
        <div className="mt-2.5 border-t pt-2 text-[11px] text-muted-foreground">
          <div className="flex justify-between">
            <span>会话累计</span>
            <span className="tabular-nums">
              {ov.session_usage
                ? `入 ${fmtK(ov.session_usage.input_tokens)} / 出 ${fmtK(ov.session_usage.output_tokens)}`
                : "-"}
            </span>
          </div>
          <div className="mt-1 flex justify-between">
            <span>装配估算（生成流口径 · 本地近似）</span>
            <span className="tabular-nums">≈{fmtK(ov.assembly_tokens ?? 0)}</span>
          </div>
          {nearLimit && (
            <p className="mt-1.5 text-amber-600">
              接近窗口上限：建议新建会话重启上下文（旧会话可随时切回）
            </p>
          )}
        </div>
      </PreviewCardContent>
    </PreviewCard>
  );
}
