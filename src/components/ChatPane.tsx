import { useEffect, useRef, useState } from "react";
import { Channel } from "@tauri-apps/api/core";
import { api, AgentEvent, MessageView } from "@/lib/tauri";
import { Button } from "@/components/ui/button";
import { Textarea } from "@/components/ui/textarea";
import { ScrollArea } from "@/components/ui/scroll-area";
import { cn } from "cn";
import { toast } from "sonner";
import { Loader2, MessageSquarePlus, Send, Wrench } from "lucide-react";

interface ChatProps {
  workPath: string;
}

/** 一条流式中的消息：文本增量 + 工具调用轨迹 */
interface StreamingState {
  text: string;
  toolTrace: { name: string; kind: "call" | "result" }[];
}

export function ChatPane({ workPath }: ChatProps) {
  const [sessions, setSessions] = useState<{ id: string; preview: string }[]>([]);
  const [sessionId, setSessionId] = useState<string | null>(null);
  const [messages, setMessages] = useState<MessageView[]>([]);
  const [input, setInput] = useState("");
  const [streaming, setStreaming] = useState<StreamingState | null>(null);
  const [busy, setBusy] = useState(false);
  const bottomRef = useRef<HTMLDivElement>(null);

  async function refreshSessions(preferId?: string) {
    const list = await api.listSessions(workPath);
    setSessions(list.map(([id, , preview]) => ({ id, preview })));
    if (preferId) return;
    if (sessionId === null && list.length > 0) {
      openSession(list[0][0]);
    }
  }

  async function openSession(id: string) {
    setSessionId(id);
    setStreaming(null);
    const msgs = await api.loadSessionView(workPath, id);
    setMessages(msgs);
  }

  useEffect(() => {
    refreshSessions();
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [workPath]);

  useEffect(() => {
    bottomRef.current?.scrollIntoView({ behavior: "smooth" });
  }, [messages, streaming?.text]);

  function newSession() {
    const id = `s${Date.now().toString(16)}-${Math.random().toString(16).slice(2, 6)}`;
    setSessionId(id);
    setMessages([]);
    setStreaming(null);
    refreshSessions(id);
    return id;
  }

  async function send() {
    const prompt = input.trim();
    if (!prompt || busy) return;
    const sid = sessionId ?? newSession();
    setInput("");
    setBusy(true);
    setMessages((m) => [...m, { role: "user", text: prompt }]);
    setStreaming({ text: "", toolTrace: [] });

    const channel = new Channel<AgentEvent>();
    channel.onmessage = (ev) => {
      setStreaming((s) => {
        if (!s) return s;
        switch (ev.type) {
          case "text_delta":
            return { ...s, text: s.text + ev.text };
          case "tool_call":
            return { ...s, toolTrace: [...s.toolTrace, { name: ev.name, kind: "call" }] };
          case "tool_result":
            return { ...s, toolTrace: [...s.toolTrace, { name: ev.name, kind: "result" }] };
          case "error":
            return { ...s, text: s.text + `\n\n[错误] ${ev.message}` };
          default:
            return s;
        }
      });
    };

    try {
      await api.chatStream(workPath, sid, prompt, channel);
      // 落盘完成后从后端拉权威视图
      const msgs = await api.loadSessionView(workPath, sid);
      setMessages(msgs);
      setStreaming(null);
      refreshSessions(sid);
    } catch (e) {
      setStreaming(null);
      toast.error(String(e));
    } finally {
      setBusy(false);
    }
  }

  return (
    <div className="flex h-full flex-col">
      <div className="flex h-9 shrink-0 items-center gap-2 border-b px-3">
        <span className="text-xs font-medium text-muted-foreground">会话</span>
        <Button
          variant="ghost"
          size="sm"
          className="ml-auto h-7 px-2 text-xs"
          onClick={newSession}
        >
          <MessageSquarePlus data-icon="inline-start" />
          新会话
        </Button>
      </div>

      {sessions.length > 0 && (
        <div className="flex shrink-0 flex-wrap gap-1 border-b px-2 py-1.5">
          {sessions.map((s) => (
            <button
              key={s.id}
              type="button"
              onClick={() => openSession(s.id)}
              className={cn(
                "max-w-40 truncate rounded-full px-2 py-0.5 text-[11px] text-muted-foreground hover:bg-accent",
                sessionId === s.id && "bg-accent text-accent-foreground"
              )}
              title={s.preview}
            >
              {s.preview || "（空）"}
            </button>
          ))}
        </div>
      )}

      <ScrollArea className="flex-1">
        <div className="flex flex-col gap-3 p-3">
          {messages.map((m, i) => (
            <ChatBubble key={i} role={m.role} text={m.text} />
          ))}
          {streaming && (
            <div className="flex flex-col gap-1">
              {streaming.toolTrace.map((t, i) => (
                <div
                  key={i}
                  className="flex items-center gap-1 text-[11px] text-muted-foreground"
                >
                  <Wrench className="size-3" />
                  {t.kind === "call" ? "调用" : "结果"}：{t.name}
                </div>
              ))}
              <ChatBubble
                role="assistant"
                text={streaming.text || "…"}
                pulsing
              />
            </div>
          )}
          {messages.length === 0 && !streaming && (
            <div className="flex flex-1 items-center justify-center py-16 text-sm text-muted-foreground">
              {busy ? (
                <>
                  <Loader2 className="mr-2 size-4 animate-spin" /> 连接模型…
                </>
              ) : (
                "向写作助手提问：设定推演 / 情节排查 / 头脑风暴"
              )}
            </div>
          )}
          <div ref={bottomRef} />
        </div>
      </ScrollArea>

      <div className="shrink-0 border-t p-2">
        <Textarea
          value={input}
          onChange={(e) => setInput(e.currentTarget.value)}
          onKeyDown={(e) => {
            if (e.key === "Enter" && !e.shiftKey) {
              e.preventDefault();
              send();
            }
          }}
          placeholder="输入消息，Enter 发送，Shift+Enter 换行"
          className="min-h-16 resize-none text-sm"
          disabled={busy}
        />
        <div className="mt-1 flex justify-end">
          <Button size="sm" onClick={send} disabled={!input.trim() || busy}>
            {busy ? (
              <Loader2 data-icon="inline-start" className="animate-spin" />
            ) : (
              <Send data-icon="inline-start" />
            )}
            发送
          </Button>
        </div>
      </div>
    </div>
  );
}

function ChatBubble({
  role,
  text,
  pulsing,
}: {
  role: string;
  text: string;
  pulsing?: boolean;
}) {
  const isUser = role === "user";
  return (
    <div className={cn("flex", isUser ? "justify-end" : "justify-start")}>
      <div
        className={cn(
          "max-w-[85%] whitespace-pre-wrap rounded-lg px-3 py-2 text-sm leading-relaxed",
          isUser
            ? "bg-primary text-primary-foreground"
            : "bg-muted text-foreground",
          pulsing && "animate-pulse"
        )}
      >
        {text}
      </div>
    </div>
  );
}
