import { useEffect, useRef, useState } from "react";
import { api, makeChannel, AgentEvent, MessageView } from "@/lib/tauri";
import { ScrollArea } from "@/components/ui/scroll-area";
import { cn } from "cn";
import { toast } from "sonner";
import { Loader2 } from "lucide-react";
import { Tool, ToolHeader, ToolContent, ToolInput, ToolOutput } from "@/components/ai/tool";
import {
  PromptInput,
  PromptInputBody,
  PromptInputTextarea,
  PromptInputToolbar,
  PromptInputSubmit,
} from "@/components/ai/prompt-input";

interface ChatProps {
  /** work = 项目内会话（有文件工具）；global = 全局对话（无作品上下文） */
  scope: "work" | "global";
  /** scope=work 时必填 */
  workPath?: string;
  sessionId: string | null;
  /** 当前无会话时由父层创建并返回新会话 id */
  onNewSession: () => string;
  /** 一轮对话完成后回调（父层刷新会话列表，自动标题已落盘） */
  onTurnDone: () => void;
}

/** 一次流式运行中的工具调用（按名称配对 call/result） */
interface ToolCallState {
  name: string;
  args: string;
  result: string | null;
  state: "running" | "done";
}

/** 一条流式中的消息：文本增量 + 工具调用卡 */
interface StreamingState {
  text: string;
  toolCalls: ToolCallState[];
}

/** 聊天主面板（R1 起不含会话管理，会话列表由父层持有）。 */
export function ChatPane({
  scope,
  workPath,
  sessionId,
  onNewSession,
  onTurnDone,
}: ChatProps) {
  const [messages, setMessages] = useState<MessageView[]>([]);
  const [input, setInput] = useState("");
  const [streaming, setStreaming] = useState<StreamingState | null>(null);
  const [busy, setBusy] = useState(false);
  const bottomRef = useRef<HTMLDivElement>(null);

  const isWork = scope === "work" && !!workPath;

  useEffect(() => {
    setStreaming(null);
    if (!sessionId) {
      setMessages([]);
      return;
    }
    const p = isWork
      ? api.loadSessionView(workPath!, sessionId)
      : api.loadSessionViewGlobal(sessionId);
    p.then(setMessages).catch(() => setMessages([]));
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [scope, workPath, sessionId]);

  useEffect(() => {
    bottomRef.current?.scrollIntoView({ behavior: "smooth" });
  }, [messages, streaming?.text]);

  async function send() {
    const prompt = input.trim();
    if (!prompt || busy) return;
    const sid = sessionId ?? onNewSession();
    setInput("");
    setBusy(true);
    setMessages((m) => [...m, { role: "user", text: prompt }]);
    setStreaming({ text: "", toolCalls: [] });

    const channel = makeChannel<AgentEvent>();
    channel.onmessage = (ev) => {
      setStreaming((s) => {
        if (!s) return s;
        switch (ev.type) {
          case "text_delta":
            return { ...s, text: s.text + ev.text };
          case "tool_call":
            return {
              ...s,
              toolCalls: [
                ...s.toolCalls,
                { name: ev.name, args: ev.args, result: null, state: "running" },
              ],
            };
          case "tool_result": {
            // 与最近一个同名 running 调用配对
            const calls = [...s.toolCalls];
            for (let i = calls.length - 1; i >= 0; i--) {
              if (calls[i].name === ev.name && calls[i].state === "running") {
                calls[i] = { ...calls[i], result: ev.output, state: "done" };
                break;
              }
            }
            return { ...s, toolCalls: calls };
          }
          case "error":
            return { ...s, text: s.text + `\n\n[错误] ${ev.message}` };
          default:
            return s;
        }
      });
    };

    try {
      if (isWork) {
        await api.chatStream(workPath!, sid, prompt, channel);
      } else {
        await api.chatStreamGlobal(sid, prompt, channel);
      }
      // 落盘完成后从后端拉权威视图
      const view = isWork
        ? await api.loadSessionView(workPath!, sid)
        : await api.loadSessionViewGlobal(sid);
      setMessages(view);
      setStreaming(null);
      onTurnDone();
    } catch (e) {
      setStreaming(null);
      toast.error(String(e));
    } finally {
      setBusy(false);
    }
  }

  return (
    <div className="flex h-full flex-col">
      <ScrollArea className="min-h-0 flex-1">
        <div className="flex flex-col gap-3 p-3">
          {messages.map((m, i) => (
            <ChatBubble key={i} role={m.role} text={m.text} />
          ))}
          {streaming && (
            <div className="flex flex-col gap-2">
              {streaming.toolCalls.map((t, i) => (
                <Tool key={i} defaultOpen={t.state === "running"}>
                  <ToolHeader
                    title={t.name}
                    state={t.state === "running" ? "input-available" : "output-available"}
                  />
                  <ToolContent>
                    <ToolInput input={safeParse(t.args)} />
                    <ToolOutput output={t.result ? safeParse(t.result) : null} />
                  </ToolContent>
                </Tool>
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
              ) : isWork ? (
                "向写作助手提问：设定推演 / 情节排查 / 头脑风暴"
              ) : (
                "找灵感、头脑风暴、写作问答（无作品上下文）"
              )}
            </div>
          )}
          <div ref={bottomRef} />
        </div>
      </ScrollArea>

      <div className="shrink-0 border-t p-2">
        <PromptInput
          value={input}
          setValue={setInput}
          onSubmit={send}
          isLoading={busy}
        >
          <PromptInputBody>
            <PromptInputTextarea placeholder="输入消息，Enter 发送，Shift+Enter 换行" />
          </PromptInputBody>
          <PromptInputToolbar>
            <PromptInputSubmit />
          </PromptInputToolbar>
        </PromptInput>
      </div>
    </div>
  );
}

/** 工具参数/结果的宽容解析：JSON 解析失败原样返回字符串 */
function safeParse(raw: string): unknown {
  try {
    return JSON.parse(raw);
  } catch {
    return raw;
  }
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
