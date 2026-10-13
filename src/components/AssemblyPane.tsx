import { useCallback, useEffect, useState } from "react";
import { api, AssemblyResult } from "@/lib/tauri";
import { Button } from "@/components/ui/button";
import { ScrollArea } from "@/components/ui/scroll-area";
import { Input } from "@/components/ui/input";
import { Badge } from "@/components/ui/badge";
import { Tabs, TabsContent, TabsList, TabsTrigger } from "@/components/ui/tabs";
import { cn } from "cn";
import { AlertTriangle, EyeOff, Loader2, RefreshCw } from "lucide-react";

interface AssemblyPaneProps {
  workPath: string;
}

export function AssemblyPane({ workPath }: AssemblyPaneProps) {
  const [volume, setVolume] = useState("1");
  const [chapter, setChapter] = useState("ch-001");
  const [detail, setDetail] = useState<"full" | "name_only">("full");
  const [budget, setBudget] = useState("16000");
  const [result, setResult] = useState<AssemblyResult | null>(null);
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState("");

  const reload = useCallback(async () => {
    setLoading(true);
    setError("");
    try {
      const r = await api.previewAssembly(
        workPath,
        Number(volume) || 1,
        chapter.trim() || "ch-001",
        detail,
        Number(budget) || 16000,
      );
      setResult(r);
    } catch (e) {
      setError(String(e));
    } finally {
      setLoading(false);
    }
  }, [workPath, volume, chapter, detail, budget]);

  useEffect(() => {
    reload();
  }, [reload]);

  const pct = result
    ? Math.min(100, (result.total_tokens / (Number(budget) || 1)) * 100)
    : 0;
  const over = result?.over_budget ?? false;

  return (
    <div className="flex h-full flex-col">
      <div className="flex shrink-0 flex-wrap items-center gap-2 border-b px-3 py-2 text-xs">
        <label className="flex items-center gap-1">
          卷
          <Input
            value={volume}
            onChange={(e) => setVolume(e.currentTarget.value.replace(/[^0-9]/g, ""))}
            className="h-7 w-12"
          />
        </label>
        <label className="flex items-center gap-1">
          章
          <Input
            value={chapter}
            onChange={(e) => setChapter(e.currentTarget.value)}
            className="h-7 w-24 font-mono"
          />
        </label>
        <label className="flex items-center gap-1">
          档位
          <select
            value={detail}
            onChange={(e) => setDetail(e.currentTarget.value as "full" | "name_only")}
            className="h-7 rounded-md border bg-transparent px-1"
          >
            <option value="full">完整</option>
            <option value="name_only">仅名称</option>
          </select>
        </label>
        <label className="flex items-center gap-1">
          预算
          <Input
            value={budget}
            onChange={(e) => setBudget(e.currentTarget.value.replace(/[^0-9]/g, ""))}
            className="h-7 w-20"
          />
        </label>
        <Button
          variant="ghost"
          size="sm"
          className="ml-auto h-7 px-2 text-xs"
          onClick={reload}
          disabled={loading}
        >
          {loading ? (
            <Loader2 data-icon="inline-start" className="animate-spin" />
          ) : (
            <RefreshCw data-icon="inline-start" />
          )}
          刷新
        </Button>
      </div>

      {/* token 预算条 */}
      <div className="shrink-0 border-b px-3 py-2">
        <div className="mb-1 flex items-center justify-between text-xs">
          <span className={cn(over ? "font-medium text-red-600" : "text-muted-foreground")}>
            {over && <AlertTriangle className="mr-1 inline size-3.5" />}
            估算 {result?.total_tokens ?? 0} / {Number(budget) || 0} tokens
          </span>
          {(result?.filtered_by_visibility ?? 0) > 0 && (
            <span className="flex items-center gap-1 text-amber-600">
              <EyeOff className="size-3.5" />
              {result!.filtered_by_visibility} 条被可见性红线过滤
            </span>
          )}
        </div>
        <div className="h-2 overflow-hidden rounded-full bg-muted">
          <div
            className={cn(
              "h-full rounded-full transition-all",
              over ? "bg-red-500" : "bg-emerald-500",
            )}
            style={{ width: `${pct}%` }}
          />
        </div>
      </div>

      {error && (
        <div className="border-b bg-destructive/10 px-3 py-1.5 text-xs text-destructive">
          {error}
        </div>
      )}

      {result && (
        <Tabs defaultValue="components" className="flex min-h-0 flex-1 flex-col">
          <TabsList className="m-2 grid w-auto grid-cols-2 self-start">
            <TabsTrigger value="components">组件</TabsTrigger>
            <TabsTrigger value="full">完整预览</TabsTrigger>
          </TabsList>
          <TabsContent
            value="components"
            className="mt-0 min-h-0 flex-1 data-[state=inactive]:hidden"
          >
            <ScrollArea className="h-full">
              <div className="flex flex-col gap-2 p-3">
                <div className="rounded-md border bg-muted/30 p-2">
                  <div className="flex items-center gap-2 text-xs font-medium">
                    <Badge>系统提示词</Badge>
                    <span className="text-muted-foreground">
                      {result.system_tokens} tokens
                    </span>
                  </div>
                  <pre className="mt-1 whitespace-pre-wrap font-mono text-[11px] leading-relaxed text-muted-foreground">
                    {result.system}
                  </pre>
                </div>
                {result.components.map((c) => (
                  <div
                    key={c.name}
                    className={cn(
                      "rounded-md border p-2",
                      !c.included && "border-dashed opacity-60",
                    )}
                  >
                    <div className="flex items-center gap-2 text-xs font-medium">
                      <span>{c.name}</span>
                      {c.included ? (
                        <span className="text-muted-foreground">{c.tokens} tokens</span>
                      ) : (
                        <Badge variant="secondary">{c.missing_hint}</Badge>
                      )}
                    </div>
                    {c.included && (
                      <pre className="mt-1 max-h-32 overflow-y-auto whitespace-pre-wrap font-mono text-[11px] leading-relaxed text-muted-foreground">
                        {c.content}
                      </pre>
                    )}
                  </div>
                ))}
              </div>
            </ScrollArea>
          </TabsContent>
          <TabsContent
            value="full"
            className="mt-0 min-h-0 flex-1 data-[state=inactive]:hidden"
          >
            <ScrollArea className="h-full">
              <pre className="whitespace-pre-wrap p-3 font-mono text-[11px] leading-relaxed">
                {result.system}
                {result.components
                  .filter((c) => c.included)
                  .map((c) => `\n\n# ${c.name}\n${c.content}`)
                  .join("")}
              </pre>
            </ScrollArea>
          </TabsContent>
        </Tabs>
      )}
    </div>
  );
}
