import { useCallback, useEffect, useState } from "react";
import {
  api,
  AssemblyPrefs,
  AssemblyResult,
  ContextOverview,
} from "@/lib/tauri";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { Badge } from "@/components/ui/badge";
import {
  Collapsible,
  CollapsibleContent,
  CollapsibleTrigger,
} from "@/components/ui/collapsible";
import { cn } from "cn";
import { AlertTriangle, ChevronDown, RefreshCw } from "lucide-react";
import { toast } from "sonner";

/**
 * 上下文条（R3）：输入框上方的可折叠上下文面板（PLAN 条并列）。
 * 折叠=会话真实用量 + 装配估算摘要；展开=三层视图（会话/装配/工具）+ 控制。
 * 控制项（档位/预算/组件开关）持久化于 project.json，被生成流读取应用。
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
        api.contextOverview(
          workPath,
          sessionId,
          volume,
          chapter.trim() || "ch-001",
        ),
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

  if (!prefs) {
    return (
      <div className="flex h-8 items-center gap-2 border-b bg-muted/20 px-2 text-[11px] text-muted-foreground">
        上下文加载中…
      </div>
    );
  }

  const excluded = new Set(prefs.exclude_components);

  return (
    <Collapsible className="shrink-0 border-b bg-muted/20">
      <div className="flex h-8 items-center gap-2 px-2 text-[11px]">
        <CollapsibleTrigger
          render={
            <button
              type="button"
              className="group flex items-center gap-0.5 font-medium text-muted-foreground hover:text-foreground"
              aria-label="展开或折叠上下文面板"
            />
          }
        >
          <ChevronDown className="size-3.5 transition-transform group-data-[panel-open]:rotate-180" />
          上下文
        </CollapsibleTrigger>
        <span className="text-muted-foreground">
          会话 ≈{fmtK(lastInput)}/{fmtK(window)}
          <span className={cn("ml-0.5", nearLimit && "font-medium text-amber-600")}>
            ({sessionPct}%)
          </span>
        </span>
        <span className="text-muted-foreground">
          · 装配 ≈{fmtK(assembly?.total_tokens ?? 0)}
        </span>
        {nearLimit && (
          <span className="flex items-center gap-0.5 text-amber-600">
            <AlertTriangle className="size-3" />
            接近窗口上限，建议新建会话
          </span>
        )}
        <div className="ml-auto flex items-center gap-1">
          <button
            type="button"
            className="text-muted-foreground hover:text-foreground"
            onClick={reload}
            aria-label="刷新上下文"
          >
            <RefreshCw className="size-3" />
          </button>
        </div>
      </div>

      <CollapsibleContent>
        <div className="flex flex-col gap-3 border-t px-3 py-2 text-xs">
          {/* 层一：会话上下文（真实用量，provider 计数） */}
          <div>
            <div className="mb-1 font-medium text-muted-foreground">
              会话上下文（真实值 · provider 计数）
            </div>
            <div className="flex items-center gap-2">
              <span>
                最近一轮输入 ≈{fmtK(lastInput)} tokens（窗口占用口径）· 窗口{" "}
                {fmtK(window)}
              </span>
            </div>
            {cumulative && (
              <div className="mt-0.5 text-[11px] text-muted-foreground">
                会话累计：输入 {fmtK(cumulative.input_tokens)} · 输出{" "}
                {fmtK(cumulative.output_tokens)} tokens（成本参考）
              </div>
            )}
            <div className="mt-1 h-1.5 overflow-hidden rounded-full bg-muted">
              <div
                className={cn(
                  "h-full rounded-full",
                  nearLimit ? "bg-amber-500" : "bg-emerald-500",
                )}
                style={{ width: `${Math.min(100, sessionPct)}%` }}
              />
            </div>
            <div className="mt-1 text-[11px] text-muted-foreground">
              会话无硬上限；接近窗口时建议在左侧新建会话重启上下文（旧会话可随时切回）。
            </div>
          </div>

          {/* 层二：装配上下文（估算 · D9 口径；可控制） */}
          <div>
            <div className="mb-1 flex items-center gap-2 font-medium text-muted-foreground">
              装配上下文（估算 · 可控制，存 project.json）
            </div>
            <div className="flex flex-wrap items-center gap-2">
              <label className="flex items-center gap-1">
                卷
                <Input
                  value={String(volume)}
                  onChange={(e) =>
                    setVolume(Number(e.currentTarget.value.replace(/[^0-9]/g, "")) || 1)
                  }
                  className="h-6 w-10 text-xs"
                />
              </label>
              <label className="flex items-center gap-1">
                章
                <Input
                  value={chapter}
                  onChange={(e) => setChapter(e.currentTarget.value)}
                  className="h-6 w-20 font-mono text-xs"
                />
              </label>
              <label className="flex items-center gap-1">
                档位
                <select
                  value={prefs.setting_detail === "name_only" ? "name_only" : "full"}
                  onChange={(e) =>
                    savePrefs({ ...prefs, setting_detail: e.currentTarget.value })
                  }
                  className="h-6 rounded-md border bg-transparent px-1 text-xs"
                >
                  <option value="full">完整</option>
                  <option value="name_only">仅名称</option>
                </select>
              </label>
              <label className="flex items-center gap-1">
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
                  className="h-6 w-20 text-xs"
                />
              </label>
              <Button
                size="sm"
                variant="outline"
                className="h-6 px-2 text-[11px]"
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
              {assembly?.over_budget && (
                <span className="flex items-center gap-1 text-amber-600">
                  <AlertTriangle className="size-3" />
                  超出预算（仅提示，不拦截）
                </span>
              )}
            </div>
            {assembly?.warning && (
              <div className="mt-1 text-[11px] text-amber-600">{assembly.warning}</div>
            )}
            <div className="mt-2 flex flex-col gap-1">
              {assembly?.components.map((c) => {
                const isExcluded = excluded.has(c.name);
                return (
                  <label
                    key={c.name}
                    className="flex items-center gap-2 text-[11px]"
                    title={isExcluded ? "已排除（不注入生成）" : "已包含"}
                  >
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
                    <span className={cn(isExcluded && "text-muted-foreground line-through")}>
                      {c.name}
                    </span>
                    {c.included ? (
                      <span className="text-muted-foreground">{fmtK(c.tokens)}</span>
                    ) : (
                      <Badge variant="secondary" className="text-[10px]">
                        {c.missing_hint ?? "未装配"}
                      </Badge>
                    )}
                  </label>
                );
              })}
              <div className="text-[11px] text-muted-foreground">
                {(assembly?.filtered_by_visibility ?? 0) > 0 && (
                  <span className="mr-2 text-amber-600">
                    {assembly?.filtered_by_visibility} 条被可见性红线过滤（不可开关）
                  </span>
                )}
                <span>排除项不影响可见性红线：隐藏条目永远不进 prompt。</span>
              </div>
            </div>
          </div>

          {/* 层三：工具上下文（按需拉取） */}
          <div>
            <div className="mb-1 font-medium text-muted-foreground">
              工具上下文（按需拉取）
            </div>
            <div className="text-[11px] text-muted-foreground">
              Agent 在对话中自行读取作品文件与数据（read_file / 账本 / 问题清单等），
              调用轨迹见聊天流；@ 引用的素材随当轮消息注入。
            </div>
          </div>
        </div>
      </CollapsibleContent>
    </Collapsible>
  );
}
