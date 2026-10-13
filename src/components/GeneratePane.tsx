import { useEffect, useState } from "react";
import { Channel } from "@tauri-apps/api/core";
import { api, AgentEvent, IssueRecord } from "@/lib/tauri";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { ScrollArea } from "@/components/ui/scroll-area";
import { Badge } from "@/components/ui/badge";
import { cn } from "cn";
import {
  BookMarked,
  Check,
  FileSignature,
  Layers,
  Loader2,
  Play,
  RotateCcw,
  ScrollText,
  ShieldCheck,
  Wand2,
  X,
} from "lucide-react";
import { toast } from "sonner";

interface GeneratePaneProps {
  workPath: string;
  onProposal: () => void;
}

type Target =
  | "chapter_outline"
  | "chapter"
  | "ledger"
  | "consistency"
  | "deai"
  | "recheck"
  | "volume_review";

export function GeneratePane({ workPath, onProposal }: GeneratePaneProps) {
  const [volume, setVolume] = useState("1");
  const [chapter, setChapter] = useState("ch-001");
  const [busy, setBusy] = useState<Target | null>(null);
  const [streamText, setStreamText] = useState("");
  const [trace, setTrace] = useState<string[]>([]);
  const [doneHint, setDoneHint] = useState("");
  const [issues, setIssues] = useState<IssueRecord[]>([]);

  const openCount = issues.filter(
    (i) => i.status === "open" || i.status === "needs_ruling",
  ).length;

  async function loadIssues() {
    try {
      setIssues(await api.listIssues(workPath));
    } catch {
      /* 无清单文件 */
    }
  }

  useEffect(() => {
    loadIssues();
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [workPath]);

  async function run(
    kind: Target,
    invoke: (ch: Channel<AgentEvent>) => Promise<void>,
  ) {
    setBusy(kind);
    setStreamText("");
    setTrace([]);
    setDoneHint("");
    const channel = new Channel<AgentEvent>();
    channel.onmessage = (ev) => {
      if (ev.type === "text_delta") {
        setStreamText((t) => t + ev.text);
      } else if (ev.type === "tool_call") {
        setTrace((a) => [...a, `调用工具 ${ev.name}`]);
      } else if (ev.type === "tool_result") {
        setTrace((a) => [...a, `工具返回 ${ev.name}`]);
      } else if (ev.type === "error") {
        setStreamText((t) => t + `\n\n[错误] ${ev.message}`);
      }
    };
    try {
      await invoke(channel);
      setDoneHint("完成：提议已进入审阅面板（右栏 → 审阅），接受后落盘。");
      onProposal();
    } catch (e) {
      toast.error(String(e));
    } finally {
      setBusy(null);
    }
  }

  const vol = Number(volume) || 1;
  const ch = chapter.trim() || "ch-001";

  function fire(target: Target) {
    const invocations: Record<
      Target,
      (channel: Channel<AgentEvent>) => Promise<void>
    > = {
      chapter_outline: (c) => api.generateArtifact(workPath, vol, ch, "chapter_outline", c),
      chapter: (c) => api.generateArtifact(workPath, vol, ch, "chapter", c),
      ledger: (c) => api.proposeLedgerUpdate(workPath, vol, ch, c),
      consistency: (c) => api.reviewChapter(workPath, vol, ch, "consistency", c),
      deai: (c) => api.reviewChapter(workPath, vol, ch, "deai", c),
      recheck: (c) => api.recheckChapter(workPath, vol, ch, c),
      volume_review: (c) => api.reviewVolume(workPath, vol, c),
    };
    run(target, invocations[target]);
    if (target === "consistency" || target === "volume_review" || target === "recheck") {
      // 完成后刷新问题清单（延迟等落盘）
      setTimeout(() => loadIssues(), 2000);
    }
  }

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
        <div className="ml-auto flex flex-wrap gap-1">
          <Button
            size="sm"
            variant="outline"
            className="h-7 text-xs"
            disabled={busy !== null}
            onClick={() => fire("chapter_outline")}
          >
            {busy === "chapter_outline" ? (
              <Loader2 data-icon="inline-start" className="animate-spin" />
            ) : (
              <BookMarked data-icon="inline-start" />
            )}
            章纲
          </Button>
          <Button
            size="sm"
            className="h-7 text-xs"
            disabled={busy !== null}
            onClick={() => fire("chapter")}
          >
            {busy === "chapter" ? (
              <Loader2 data-icon="inline-start" className="animate-spin" />
            ) : (
              <Play data-icon="inline-start" />
            )}
            正文
          </Button>
          <Button
            size="sm"
            variant="outline"
            className="h-7 text-xs"
            disabled={busy !== null}
            onClick={() => fire("ledger")}
            title="完稿一章后：从正文提取状态变化，提议更新账本"
          >
            {busy === "ledger" ? (
              <Loader2 data-icon="inline-start" className="animate-spin" />
            ) : (
              <ScrollText data-icon="inline-start" />
            )}
            账本回写
          </Button>
          <Button
            size="sm"
            variant="outline"
            className="h-7 text-xs"
            disabled={busy !== null}
            onClick={() => fire("consistency")}
            title="对照账本与设定找矛盾，出问题清单并提议修复"
          >
            {busy === "consistency" ? (
              <Loader2 data-icon="inline-start" className="animate-spin" />
            ) : (
              <ShieldCheck data-icon="inline-start" />
            )}
            一致性检查
          </Button>
          <Button
            size="sm"
            variant="outline"
            className="h-7 text-xs"
            disabled={busy !== null}
            onClick={() => fire("deai")}
            title="文风指令约束下去 AI 味改写整章"
          >
            {busy === "deai" ? (
              <Loader2 data-icon="inline-start" className="animate-spin" />
            ) : (
              <Wand2 data-icon="inline-start" />
            )}
            去 AI 味
          </Button>
          <Button
            size="sm"
            variant="outline"
            className="h-7 text-xs"
            disabled={busy !== null}
            onClick={() => fire("recheck")}
            title="对照上轮问题清单与裁决逐项复检"
          >
            {busy === "recheck" ? (
              <Loader2 data-icon="inline-start" className="animate-spin" />
            ) : (
              <RotateCcw data-icon="inline-start" />
            )}
            复检本章
          </Button>
          <Button
            size="sm"
            variant="outline"
            className="h-7 text-xs"
            disabled={busy !== null}
            onClick={() => fire("volume_review")}
            title="逐章一致性检查并汇总问题清单"
          >
            {busy === "volume_review" ? (
              <Loader2 data-icon="inline-start" className="animate-spin" />
            ) : (
              <Layers data-icon="inline-start" />
            )}
            审本卷
          </Button>
        </div>
      </div>

      <ScrollArea className="flex-1">
        <div className="flex flex-col gap-2 p-3">
          {trace.map((t, i) => (
            <div
              key={i}
              className={cn(
                "flex items-center gap-1 text-[11px]",
                t.startsWith("调用") ? "text-blue-600" : "text-muted-foreground",
              )}
            >
              <FileSignature className="size-3" />
              {t}
            </div>
          ))}
          {streamText && (
            <div className="whitespace-pre-wrap rounded-md bg-muted p-3 text-sm leading-relaxed">
              {streamText}
            </div>
          )}
          {doneHint && (
            <div className="rounded-md border border-emerald-500/40 bg-emerald-500/10 p-2 text-xs text-emerald-700">
              {doneHint}
            </div>
          )}
          {!streamText && !trace.length && !doneHint && issues.length === 0 && (
            <div className="flex flex-1 items-center justify-center py-16 text-center text-xs text-muted-foreground">
              生成流程：生成章纲 →（审阅接受）→ 生成正文 →（审阅接受）→ 账本回写 →（审阅接受）
              <br />
              上下文按装配引擎组装，可在右栏「装配」预览。
            </div>
          )}
          {issues.length > 0 && (
            <div className="flex flex-col gap-2">
              <div className="flex items-center gap-2 text-xs font-medium text-muted-foreground">
                <ShieldCheck className="size-3.5" />
                问题清单（{openCount} 未决 / 共 {issues.length} 条）
                <Button
                  variant="ghost"
                  size="sm"
                  className="ml-auto h-6 px-2 text-[11px]"
                  onClick={() => loadIssues()}
                >
                  刷新
                </Button>
              </div>
              {issues.map((iss) => (
                <IssueCard
                  key={iss.id}
                  issue={iss}
                  onRule={async (status) => {
                    try {
                      await api.recordIssueRuling(workPath, iss.id, status);
                      loadIssues();
                    } catch (e) {
                      toast.error(String(e));
                    }
                  }}
                />
              ))}
            </div>
          )}
        </div>
      </ScrollArea>
    </div>
  );
}

function IssueCard({
  issue,
  onRule,
}: {
  issue: IssueRecord;
  onRule: (status: "fixed" | "dismissed" | "needs_ruling") => void;
}) {
  const statusBadge = () => {
    switch (issue.status) {
      case "fixed":
        return <Badge className="bg-emerald-500/15 text-emerald-700">已修复</Badge>;
      case "dismissed":
        return <Badge variant="secondary">已驳回</Badge>;
      case "needs_ruling":
        return <Badge className="bg-amber-500/15 text-amber-700">需裁决</Badge>;
      default:
        return <Badge variant="outline">待处理</Badge>;
    }
  };
  return (
    <div className="rounded-md border p-2 text-xs">
      <div className="flex items-center gap-2">
        <span className="font-mono text-muted-foreground">{issue.chapter}</span>
        <span className="text-[10px] text-muted-foreground/70">{issue.source}</span>
        {statusBadge()}
        {(issue.status === "open" || issue.status === "needs_ruling") && (
          <span className="ml-auto flex gap-1">
            <Button
              variant="outline"
              size="sm"
              className="h-6 px-2 text-[11px]"
              onClick={() => onRule("fixed")}
            >
              <Check data-icon="inline-start" />
              确认已修复
            </Button>
            <Button
              variant="outline"
              size="sm"
              className="h-6 px-2 text-[11px]"
              onClick={() => onRule("dismissed")}
            >
              <X data-icon="inline-start" />
              驳回
            </Button>
          </span>
        )}
      </div>
      <p className="mt-1 whitespace-pre-wrap leading-relaxed text-muted-foreground">
        {issue.text.length > 300 ? `${issue.text.slice(0, 300)}…` : issue.text}
      </p>
      {issue.ruling && (
        <p className="mt-1 text-[11px] text-muted-foreground/70">裁决：{issue.ruling}</p>
      )}
    </div>
  );
}
