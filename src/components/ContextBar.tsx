import { useCallback, useEffect, useState } from "react";
import {
  api,
  AssemblyPrefs,
  AssemblyResult,
  ContextOverview,
} from "@/lib/tauri";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import {
  Collapsible,
  CollapsibleContent,
  CollapsibleTrigger,
} from "@/components/ui/collapsible";
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from "@/components/ui/select";
import { cn } from "cn";
import { AlertTriangle, ChevronDown, RefreshCw } from "lucide-react";
import { toast } from "sonner";

/**
 * 上下文条（重设计）：输入框上方的可折叠面板（交互保持折叠展开，不用弹窗）。
 * 折叠 = 单行摘要；展开 = 三层视图重排为分区 + 规范列表：
 * 会话（真实用量）/ 装配（估算 · 可控制，存 project.json）/ 工具（按需拉取）。
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
  const [assembly, setAssembly] = useState<AssemblyResult | null>(null);
  const [prefs, setPrefs] = useState<AssemblyPrefs | null>(null);
  const [volume, setVolume] = useState(1);
  const [chapter, setChapter] = useState("ch-001");

  const reload = useCallback(async () => {
    try {
      const [ov, pr] = await Promise.all([
        api.contextOverview(workPath, sessionId, volume, chapter.trim() || "ch-001"),
        api.getAssemblyPrefs(workPath),
      ]);
      setOverview(ov);
      setPrefs(pr);
      const asm = await api.previewAssembly(
        workPath,
        volume,
        chapter.trim() || "ch-001",
        pr.setting_detail === "name_only" ? "name_only" : "full",
        pr.token_budget ?? 16000,
        pr.exclude_components,
      );
      setAssembly(asm);
    } catch (e) {
      toast.error(String(e));
    }
  }, [workPath, sessionId, volume, chapter]);

  useEffect(() => {
    reload();
  }, [reload, refreshKey]);

  async function savePrefs(next: AssemblyPrefs) {
    setPrefs(next);
    try {
      await api.setAssemblyPrefs(workPath, next);
      const asm = await api.previewAssembly(
        workPath,
        volume,
        chapter.trim() || "ch-001",
        next.setting_detail === "name_only" ? "name_only" : "full",
        next.token_budget ?? 16000,
        next.exclude_components,
      );
      setAssembly(asm);
    } catch (e) {
      toast.error(String(e));
    }
  }

  const lastInput = overview?.last_input_tokens ?? 0;
  const cumulative = overview?.session_usage;
  const window = overview?.context_window ?? 128_000;
  const sessionPct = Math.round(overview?.session_percent ?? 0);
  const nearLimit = sessionPct >= 80;
  const excluded = new Set(prefs?.exclude_components ?? []);

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
        <span className="opacity-60">·</span>
        <span>装配 ≈{fmtK(assembly?.total_tokens ?? 0)}</span>
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

          {/* 装配上下文 */}
          <section className="border-t p-3 pb-2.5">
            <header className="mb-2 flex items-center gap-2">
              <span className="text-xs font-medium">装配上下文</span>
              <span className="text-[10px] text-muted-foreground">
                估算 · 被生成流应用 · 存 project.json
              </span>
              {assembly?.over_budget && (
                <span className="ml-auto flex items-center gap-0.5 text-[10px] text-amber-600">
                  <AlertTriangle className="size-3" />
                  超预算（仅提示）
                </span>
              )}
            </header>
            {prefs && (
              <>
                <div className="mb-2 flex flex-wrap items-center gap-1.5 text-[11px]">
                  <label className="flex items-center gap-1 text-muted-foreground">
                    卷
                    <Input
                      value={String(volume)}
                      onChange={(e) =>
                        setVolume(Number(e.currentTarget.value.replace(/[^0-9]/g, "")) || 1)
                      }
                      className="h-6 w-10 px-1 text-[11px]"
                    />
                  </label>
                  <label className="flex items-center gap-1 text-muted-foreground">
                    章
                    <Input
                      value={chapter}
                      onChange={(e) => setChapter(e.currentTarget.value)}
                      className="h-6 w-20 px-1 font-mono text-[11px]"
                    />
                  </label>
                  <label className="flex items-center gap-1 text-muted-foreground">
                    档位
                    <Select
                      items={{ full: "完整", name_only: "仅名称" }}
                      value={prefs.setting_detail === "name_only" ? "name_only" : "full"}
                      onValueChange={(v) => v && savePrefs({ ...prefs, setting_detail: v })}
                    >
                      <SelectTrigger className="h-6 w-18 px-1.5 text-[11px]">
                        <SelectValue />
                      </SelectTrigger>
                      <SelectContent>
                        <SelectItem value="full">完整</SelectItem>
                        <SelectItem value="name_only">仅名称</SelectItem>
                      </SelectContent>
                    </Select>
                  </label>
                  <label className="flex items-center gap-1 text-muted-foreground">
                    预算
                    <Input
                      value={String(prefs.token_budget ?? 16000)}
                      onChange={(e) =>
                        setPrefs({
                          ...prefs,
                          token_budget:
                            Number(e.currentTarget.value.replace(/[^0-9]/g, "")) || 0,
                        })
                      }
                      onBlur={() => savePrefs(prefs)}
                      className="h-6 w-16 px-1 text-[11px] tabular-nums"
                    />
                  </label>
                  <Button
                    size="sm"
                    variant="outline"
                    className="ml-auto h-6 px-2 text-[11px]"
                    onClick={() =>
                      savePrefs({
                        setting_detail: "full",
                        token_budget: window,
                        exclude_components: [],
                      })
                    }
                    title="档位完整 + 预算拉满到模型窗口 + 恢复全部组件"
                  >
                    一键全量
                  </Button>
                </div>

                <div className="mb-1 flex items-baseline justify-between text-[10px] text-muted-foreground">
                  <span>组件（勾选即注入）</span>
                  <span className="tabular-nums">合计 ≈{fmtK(assembly?.total_tokens ?? 0)}</span>
                </div>
                <ul className="divide-y rounded-md border">
                  {assembly?.components.map((c) => {
                    const isExcluded = excluded.has(c.name);
                    return (
                      <li key={c.name}>
                        <label className="flex cursor-pointer items-center gap-2 px-2 py-1.5 transition-colors hover:bg-accent/50">
                          <input
                            type="checkbox"
                            checked={!isExcluded}
                            onChange={(e) => {
                              const next = e.currentTarget.checked
                                ? prefs.exclude_components.filter((n) => n !== c.name)
                                : [...prefs.exclude_components, c.name];
                              savePrefs({ ...prefs, exclude_components: next });
                            }}
                          />
                          <span
                            className={cn(
                              "flex-1 truncate text-[11px]",
                              isExcluded && "text-muted-foreground line-through",
                            )}
                          >
                            {c.name}
                          </span>
                          {c.included && c.tokens > 0 ? (
                            <span className="shrink-0 text-[10px] tabular-nums text-muted-foreground">
                              {fmtK(c.tokens)}
                            </span>
                          ) : (
                            <span className="max-w-44 shrink-0 truncate text-[10px] text-muted-foreground/70 italic">
                              {c.missing_hint ?? "未装配"}
                            </span>
                          )}
                        </label>
                      </li>
                    );
                  })}
                </ul>
                {assembly?.warning && (
                  <p className="mt-1.5 text-[10px] text-amber-600">{assembly.warning}</p>
                )}
                {(assembly?.filtered_by_visibility ?? 0) > 0 && (
                  <p className="mt-1 text-[10px] text-amber-600">
                    {assembly?.filtered_by_visibility} 条被可见性红线过滤（不可开关）
                  </p>
                )}
              </>
            )}
          </section>

          {/* 工具上下文 */}
          <section className="border-t px-3 py-2.5">
            <p className="text-[10px] text-muted-foreground/70">
              <span className="mr-1.5 text-[11px] font-medium text-foreground/80">
                工具上下文
              </span>
              Agent 按需读取作品文件与数据（读取文件 / 账本 / 问题清单等），@ 引用随当轮消息注入；轨迹见聊天流。
            </p>
          </section>
        </div>
      </CollapsibleContent>
    </Collapsible>
  );
}
