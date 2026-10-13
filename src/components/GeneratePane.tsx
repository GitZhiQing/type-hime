import { useState } from "react";
import { Channel } from "@tauri-apps/api/core";
import { api, AgentEvent } from "@/lib/tauri";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { ScrollArea } from "@/components/ui/scroll-area";
import { cn } from "cn";
import { BookMarked, FileSignature, Loader2, Play, ScrollText } from "lucide-react";
import { toast } from "sonner";

interface GeneratePaneProps {
  workPath: string;
  onProposal: () => void;
}

type Target = "chapter_outline" | "chapter";

export function GeneratePane({ workPath, onProposal }: GeneratePaneProps) {
  const [volume, setVolume] = useState("1");
  const [chapter, setChapter] = useState("ch-001");
  const [busy, setBusy] = useState<Target | "ledger" | null>(null);
  const [streamText, setStreamText] = useState("");
  const [trace, setTrace] = useState<string[]>([]);
  const [doneHint, setDoneHint] = useState("");

  async function run(
    kind: Target | "ledger",
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

  function generate(target: Target) {
    run(target, (channel) =>
      api.generateArtifact(workPath, vol, ch, target, channel),
    );
  }

  function ledgerUpdate() {
    run("ledger", (channel) =>
      api.proposeLedgerUpdate(workPath, vol, ch, channel),
    );
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
        <div className="ml-auto flex gap-1">
          <Button
            size="sm"
            variant="outline"
            className="h-7 text-xs"
            disabled={busy !== null}
            onClick={() => generate("chapter_outline")}
          >
            {busy === "chapter_outline" ? (
              <Loader2 data-icon="inline-start" className="animate-spin" />
            ) : (
              <BookMarked data-icon="inline-start" />
            )}
            生成章纲
          </Button>
          <Button
            size="sm"
            className="h-7 text-xs"
            disabled={busy !== null}
            onClick={() => generate("chapter")}
          >
            {busy === "chapter" ? (
              <Loader2 data-icon="inline-start" className="animate-spin" />
            ) : (
              <Play data-icon="inline-start" />
            )}
            生成正文
          </Button>
          <Button
            size="sm"
            variant="outline"
            className="h-7 text-xs"
            disabled={busy !== null}
            onClick={ledgerUpdate}
            title="完稿一章后：从正文提取状态变化，提议更新账本"
          >
            {busy === "ledger" ? (
              <Loader2 data-icon="inline-start" className="animate-spin" />
            ) : (
              <ScrollText data-icon="inline-start" />
            )}
            账本回写
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
          {!streamText && !trace.length && !doneHint && (
            <div className="flex flex-1 items-center justify-center py-16 text-center text-xs text-muted-foreground">
              生成流程：生成章纲 →（审阅接受）→ 生成正文 →（审阅接受）→ 账本回写 →（审阅接受）
              <br />
              上下文按装配引擎组装，可在右栏「装配」预览。
            </div>
          )}
        </div>
      </ScrollArea>
    </div>
  );
}
