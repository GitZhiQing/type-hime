import { useEffect, useState, useSyncExternalStore } from "react";
import { Conversation, ConversationContent } from "@/components/ai/conversation";
import { ConversationHistory } from "@/components/ai/ConversationHistory";
import { Message, MessageContent } from "@/components/ai/message";
import { MessageResponse } from "@/components/ai/response";
import { ToolCallCard } from "@/components/ai/tool";
import { api, type MessageView } from "@/lib/tauri";
import {
  getDelegation,
  subscribeDelegations,
} from "@/lib/delegations";
import { AGENT_BADGE, AGENT_DOT, resolveAgentColor } from "@/lib/agent-colors";
import { CheckCircle2, Loader2, XCircle } from "lucide-react";
import { cn } from "cn";
import { TypingDots } from "@/components/ai/typing-dots";
import type { DelegationPayload } from "@/components/artifacts/ArtifactsPane";

/**
 * 委派会话视图（D50，源码级对齐 ZCode subagent-session tab）：
 * 每次子 Agent 委派 = 一条独立会话，右侧标签页查看——
 * - 运行中：SubTask* 事件实时流（委派指令 + 工具轨迹 + 流式文本）；
 * - 完成后 / 重载回看：loadSessionView 拉真实子会话档案，复用主对话的只读渲染
 *   （ConversationHistory——ZCode SessionPane readOnly 同思路，transcript 零新增组件）。
 * 标签元数据（agent/label/session id）随制品面板持久化；prompt 等过程数据不落盘。
 */
function statusBadge(d: { status: "running" | "done" | "error"; tokens: number | null; durationMs: number | null }) {
  if (d.status === "running")
    return (
      <span className="flex shrink-0 items-center gap-1 text-[11px] text-muted-foreground">
        <Loader2 className="size-3 animate-spin" />
        运行中
      </span>
    );
  if (d.status === "error")
    return (
      <span className="flex shrink-0 items-center gap-1 text-[11px] text-red-600">
        <XCircle className="size-3" />
        失败
      </span>
    );
  const stats = [
    d.durationMs != null ? `${Math.max(1, Math.floor(d.durationMs / 1000))}s` : null,
    d.tokens != null
      ? `≈${d.tokens >= 1000 ? `${(d.tokens / 1000).toFixed(1)}k` : d.tokens} tokens`
      : null,
  ]
    .filter(Boolean)
    .join(" · ");
  return (
    <span className="flex shrink-0 items-center gap-1 text-[11px] text-muted-foreground">
      <CheckCircle2 className="size-3 text-emerald-600" />
      完成{stats ? ` · ${stats}` : ""}
    </span>
  );
}

export function SubagentView({
  workPath,
  payload,
}: {
  workPath: string;
  payload?: DelegationPayload;
}) {
  const id = payload?.id ?? "";
  const d = useSyncExternalStore(
    subscribeDelegations,
    () => getDelegation(id),
    () => undefined,
  );

  // 子会话档案：完成后（或重载后无活数据时）拉真实子会话，只读渲染
  const sessionId = d?.session ?? payload?.session ?? null;
  const wantArchive = !!sessionId && (!d || d.status !== "running");
  const [archive, setArchive] = useState<MessageView[] | null>(null);
  const [archiveTried, setArchiveTried] = useState(false);
  useEffect(() => {
    if (!wantArchive || !sessionId) return;
    let cancelled = false;
    setArchiveTried(false);
    api
      .loadSessionView(workPath, sessionId)
      .then((m) => {
        if (!cancelled) {
          setArchive(m);
          setArchiveTried(true);
        }
      })
      .catch(() => {
        if (!cancelled) setArchiveTried(true);
      });
    return () => {
      cancelled = true;
    };
  }, [workPath, sessionId, wantArchive]);

  const agent = d?.agent ?? payload?.agent ?? null;
  const label = d?.label ?? payload?.label ?? "";
  const colorKey = resolveAgentColor(agent, null);

  return (
    <div className="flex min-h-0 flex-1 flex-col">
      <div className="flex shrink-0 items-center gap-2 border-b px-3 py-2">
        <span
          className={cn(
            "flex shrink-0 items-center gap-1.5 rounded-full px-2 py-0.5 text-xs font-medium",
            AGENT_BADGE[colorKey],
          )}
        >
          <span className={cn("size-1.5 rounded-full", AGENT_DOT[colorKey])} />
          {agent ?? "子 Agent"}
        </span>
        <span className="min-w-0 truncate text-xs text-muted-foreground" title={label}>
          {label}
        </span>
        <span className="ml-auto flex shrink-0 items-center gap-2">
          {d ? (
            statusBadge(d)
          ) : (
            <span className="text-[11px] text-muted-foreground">
              {wantArchive ? "已归档会话" : "历史委派"}
            </span>
          )}
          <span className="hidden text-[10px] text-muted-foreground/60 sm:inline">
            子 Agent 会话
          </span>
        </span>
      </div>

      <Conversation>
        <ConversationContent className="gap-2 p-3">
          {/* 档案模式（完成后/重载回看）：真实子会话，只读复用主对话渲染 */}
          {wantArchive && archive && <ConversationHistory messages={archive} />}

          {/* 档案缺失但已尝试：说明态 */}
          {wantArchive && !archive && archiveTried && (
            <div className="flex flex-1 items-center justify-center p-6 text-center text-sm text-muted-foreground">
              子会话档案未找到（运行中的委派在完成后归档；早期委派可能未贯通会话 id）
            </div>
          )}
          {wantArchive && !archive && !archiveTried && (
            <div className="flex flex-1 items-center justify-center p-6 text-sm text-muted-foreground">
              正在读取子会话…
            </div>
          )}

          {/* 活数据模式（运行中，或完成但档案未及拉取时兜底）：事件流实时渲染 */}
          {d && (!wantArchive || !archive) && (
            <>
              {d.prompt && (
                <div className="rounded-md border bg-muted/30 px-3 py-2">
                  <div className="mb-1 text-[10px] font-medium tracking-wide text-muted-foreground">
                    委派指令（根 Agent 装配的上下文）
                  </div>
                  <MessageResponse>{d.prompt}</MessageResponse>
                </div>
              )}
              {d.toolCalls.map((c, i) => (
                <ToolCallCard
                  key={`${c.callId ?? c.name}-${i}`}
                  call={{ name: c.name, args: c.args, result: c.result, callId: c.callId ?? null }}
                />
              ))}
              {(d.text.trim() !== "" || d.status === "running") && (
                <Message from="assistant">
                  <MessageContent>
                    {d.text.trim() !== "" ? (
                      <MessageResponse>{d.text}</MessageResponse>
                    ) : (
                      <TypingDots />
                    )}
                  </MessageContent>
                </Message>
              )}
              {d.status !== "running" && d.summary && (
                <div className="rounded-md border bg-background px-3 py-2 text-xs">
                  <div className="mb-1 text-[10px] font-medium tracking-wide text-muted-foreground">
                    完成汇报
                  </div>
                  <div className="whitespace-pre-wrap leading-relaxed text-muted-foreground">
                    {d.summary}
                  </div>
                </div>
              )}
            </>
          )}

          {/* 无任何数据（无 session 的历史 tab 且无活数据） */}
          {!d && !wantArchive && (
            <div className="flex flex-1 items-center justify-center p-6 text-center text-sm text-muted-foreground">
              该委派标签缺少数据（重新发起委派后这里会展示完整子会话）
            </div>
          )}
        </ConversationContent>
      </Conversation>
    </div>
  );
}
