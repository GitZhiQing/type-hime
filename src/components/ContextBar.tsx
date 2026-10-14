import { useCallback, useEffect, useState } from "react";
import { api, ContextOverview } from "@/lib/tauri";
import {
  Collapsible,
  CollapsibleContent,
  CollapsibleTrigger,
} from "@/components/ui/collapsible";
import { cn } from "cn";
import { AlertTriangle, ChevronDown, RefreshCw } from "lucide-react";
import { toast } from "sonner";

/**
 * 上下文条（R3 起源，R10 收缩）：输入框上方的可折叠面板。
 * 折叠 = 单行摘要（会话用量/窗口占比）；展开 = 会话上下文详情 + 工具上下文说明。
 * R8 后聊天路径不再经装配引擎（根 Agent 自行装配并入委派 prompt），
 * 原「装配上下文」偏好编辑区块（档位/预算/组件开关，存 project.json 供生成流应用）
 * 已无消费方（GeneratePane 于 R5 移除），随本批去除；装配的只读检查保留在
 * 制品面板「装配」标签（legacy 生成流检查工具）。
 */

interface ContextBarProps {
  workPath: string;
  sessionId: string | null;
  /** 一轮对话完成后 +1 触发刷新（会话用量变化） */
  refreshKey?: number;
}

function fmtK(n: number): string {
  return n >= 1000 ? `${(n / 1000).toFixed(1)}k` : String(n);
}

export function ContextBar({ workPath, sessionId, refreshKey = 0 }: ContextBarProps) {
  const [overview, setOverview] = useState<ContextOverview | null>(null);

  const reload = useCallback(async () => {
    try {
      setOverview(await api.contextOverview(workPath, sessionId, 1, "ch-001"));
    } catch (e) {
      toast.error(String(e));
    }
  }, [workPath, sessionId]);

  useEffect(() => {
    reload();
  }, [reload, refreshKey]);

  const lastInput = overview?.last_input_tokens ?? 0;
  const cumulative = overview?.session_usage;
  const window = overview?.context_window ?? 128_000;
  const sessionPct = Math.round(overview?.session_percent ?? 0);
  const nearLimit = sessionPct >= 80;

  return (
    <Collapsible className="shrink-0 border-b bg-muted/20">
      <CollapsibleTrigger
        render={
          <button
            type="button"
            className="group flex h-8 w-full items-center gap-2 px-2 text-left text-[11px] text-muted-foreground transition-colors hover:bg-muted/50"
            aria-label="展开或折叠上下文面板"
          />
        }
      >
        <ChevronDown className="size-3.5 transition-transform group-data-[panel-open]:rotate-180" />
        <span className="font-medium text-foreground/80">上下文</span>
        <span>
          会话 ≈{fmtK(lastInput)}/{fmtK(window)}
          <span className={cn("ml-0.5", nearLimit && "font-medium text-amber-600")}>
            ({sessionPct}%)
          </span>
        </span>
        {nearLimit && (
          <span className="flex items-center gap-0.5 text-amber-600">
            <AlertTriangle className="size-3" />
            接近窗口上限，建议新建会话
          </span>
        )}
      </CollapsibleTrigger>

      <CollapsibleContent>
        <div className="border-t">
          {/* 会话上下文 */}
          <section className="p-3 pb-2.5">
            <header className="mb-2 flex items-center justify-between">
              <span className="text-xs font-medium">会话上下文</span>
              <span className="flex items-center gap-1 text-[10px] text-muted-foreground">
                真实值 · provider 计数
                <button
                  type="button"
                  className="ml-0.5 rounded p-0.5 hover:bg-accent hover:text-foreground"
                  onClick={reload}
                  aria-label="刷新上下文"
                >
                  <RefreshCw className="size-3" />
                </button>
              </span>
            </header>
            <div className="flex items-center gap-2">
              <div className="h-1.5 w-40 shrink-0 overflow-hidden rounded-full bg-muted">
                <div
                  className={cn(
                    "h-full rounded-full transition-all",
                    nearLimit ? "bg-amber-500" : "bg-emerald-500",
                  )}
                  style={{ width: `${Math.min(100, sessionPct)}%` }}
                />
              </div>
              <span
                className={cn(
                  "shrink-0 text-[11px] tabular-nums",
                  nearLimit && "font-medium text-amber-600",
                )}
              >
                {sessionPct}%
              </span>
              <span className="truncate text-[11px] text-muted-foreground">
                最近一轮 ≈{fmtK(lastInput)} / 窗口 {fmtK(window)}
                {cumulative && (
                  <span className="ml-2 opacity-70">
                    累计 {fmtK(cumulative.input_tokens)} 入 / {fmtK(cumulative.output_tokens)} 出
                  </span>
                )}
              </span>
            </div>
            <p className="mt-1.5 text-[10px] text-muted-foreground/70">
              {nearLimit
                ? "接近窗口上限：建议新建会话重启上下文（旧会话可随时切回）"
                : "窗口占用 = 最近一轮输入的 token 数；会话无硬上限"}
            </p>
          </section>

          {/* 工具上下文 */}
          <section className="border-t px-3 py-2.5">
            <p className="text-[10px] text-muted-foreground/70">
              <span className="mr-1.5 text-[11px] font-medium text-foreground/80">
                工具上下文
              </span>
              根 Agent 与子 Agent 按需读取作品文件与数据（读取文件 / 账本 / 问题清单等），
              @ 引用随当轮消息注入；装配注入由根 Agent 在委派时完成，轨迹见聊天流。
            </p>
          </section>
        </div>
      </CollapsibleContent>
    </Collapsible>
  );
}
