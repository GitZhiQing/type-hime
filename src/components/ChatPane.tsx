import { useEffect, useRef, useState, type ReactNode } from "react";
import {
  api,
  makeChannel,
  listenCompat,
  AgentEvent,
  MessageView,
  Proposal,
  TreeNode,
} from "@/lib/tauri";
import { toast } from "sonner";
import { Loader2 } from "lucide-react";
import { ToolCallCard, type ToolCallData } from "@/components/ai/tool";
import {
  Conversation,
  ConversationContent,
  ConversationEmptyState,
  ConversationScrollButton,
} from "@/components/ai/conversation";
import { Message, MessageContent, MessageText } from "@/components/ai/message";
import { MessageResponse } from "@/components/ai/response";
import {
  PromptInput,
  PromptInputBody,
  PromptInputTextarea,
  PromptInputToolbar,
  PromptInputSubmit,
} from "@/components/ai/prompt-input";
import {
  PromptAutocomplete,
  type AutocompleteItem,
} from "@/components/ai/prompt-autocomplete";
import { ProposalCard, type InlineProposal } from "@/components/ai/proposal-card";
import { ConfirmationCard, type ApprovalView } from "@/components/ai/confirmation";
import { SLASH_DEFS, parseSlash, runSlash } from "@/lib/slash";

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
  /** 输入框上方的插槽（work 作用域放 PLAN 条；REDESIGN §5.4） */
  planSlot?: ReactNode;
}

/** 一次流式运行中的工具调用（按名称配对 call/result；结构同历史会话的 ToolCallData） */
interface ToolCallState extends ToolCallData {
  state: "running" | "done";
}

/** 一条流式中的消息：文本增量 + 工具调用卡 */
interface StreamingState {
  text: string;
  toolCalls: ToolCallState[];
}

interface MenuState {
  source: "slash" | "at";
  items: AutocompleteItem[];
  active: number;
  /** @ 触发词在输入中的结束位置（替换用） */
  atEnd?: number;
}

/** 树扁平化为文件清单（@ 引用菜单数据源） */
function flattenTree(node: TreeNode, out: { name: string; path: string }[]) {
  if (!node.is_dir) out.push({ name: node.name, path: node.path });
  for (const c of node.children ?? []) flattenTree(c, out);
}

/** 聊天主面板（R1 起不含会话管理，会话列表由父层持有）。 */
export function ChatPane({
  scope,
  workPath,
  sessionId,
  onNewSession,
  onTurnDone,
  planSlot,
}: ChatProps) {
  const [messages, setMessages] = useState<MessageView[]>([]);
  const [input, setInput] = useState("");
  const [streaming, setStreaming] = useState<StreamingState | null>(null);
  const [busy, setBusy] = useState(false);
  const [menu, setMenu] = useState<MenuState | null>(null);
  const [files, setFiles] = useState<{ name: string; path: string }[]>([]);
  const [inlineProposals, setInlineProposals] = useState<InlineProposal[]>([]);
  /** 本轮的工具审批卡（R4：审批事件驱动的状态机；决策由审批事件回传） */
  const [approvals, setApprovals] = useState<ApprovalView[]>([]);
  const busyRef = useRef(false);

  const isWork = scope === "work" && !!workPath;

  // 内联提议卡片：仅在聊天发起的一轮中捕获本作品的提议（右栏 GeneratePane 触发的不进聊天流）
  useEffect(() => {
    if (!isWork || !workPath) return;
    const un = listenCompat<Proposal>("review:new", (event) => {
      const p = event.payload;
      if (busyRef.current && p.work_path === workPath) {
        setInlineProposals((list) => [...list, { proposal: p, status: "pending" }]);
      }
    });
    return () => {
      un.then((f) => f());
    };
  }, [isWork, workPath]);

  // @ 引用菜单数据：作品文件清单（work 作用域加载一次）
  useEffect(() => {
    if (!isWork || !workPath) {
      setFiles([]);
      return;
    }
    api
      .readTree(workPath)
      .then((t) => {
        const out: { name: string; path: string }[] = [];
        flattenTree(t, out);
        setFiles(out);
      })
      .catch(() => setFiles([]));
  }, [isWork, workPath]);

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

  /** 输入变化 → 补全菜单触发检测（/ 开头命令、@ 素材引用） */
  function detectMenu(value: string) {
    // slash：行首 / 且尚未输入空格（仅 work 作用域）
    if (isWork && value.startsWith("/") && !value.includes(" ")) {
      const q = value.slice(1);
      const items = SLASH_DEFS.filter((d) => d.cmd.slice(1).includes(q))
        .slice(0, 8)
        .map((d) => ({
          replace: d.cmd,
          label: d.cmd,
          hint: `${d.desc}${d.args ? `（${d.args}）` : ""}`,
        }));
      setMenu(items.length > 0 ? { source: "slash", items, active: 0 } : null);
      return;
    }
    // @：光标前的 @ 触发词
    const m = value.match(/(?:^|\s)@([\w\u4e00-\u9fff./-]*)$/);
    if (isWork && m) {
      const q = m[1];
      const atEnd = value.length;
      const items = files
        .filter((f) => !q || f.name.includes(q) || f.path.includes(q))
        .slice(0, 8)
        .map((f) => ({
          replace: `@${f.name}`,
          label: `@${f.name}`,
          hint: f.path,
        }));
      setMenu(items.length > 0 ? { source: "at", items, active: 0, atEnd } : null);
      return;
    }
    setMenu(null);
  }

  function pickMenuItem(item: AutocompleteItem) {
    if (!menu) return;
    if (menu.source === "slash") {
      setInput(item.replace + " ");
    } else {
      // 替换结尾的 @触发词
      const prefix = input.slice(0, menu.atEnd ?? input.length);
      const atStart = prefix.search(/@[\w\u4e00-\u9fff./-]*$/);
      setInput(input.slice(0, atStart) + item.replace + " " + input.slice(menu.atEnd ?? input.length));
    }
    setMenu(null);
  }

  function handleMenuKeys(e: React.KeyboardEvent<HTMLTextAreaElement>) {
    if (!menu) return;
    if (e.key === "ArrowDown" || e.key === "ArrowUp") {
      e.preventDefault();
      const delta = e.key === "ArrowDown" ? 1 : -1;
      setMenu((m) =>
        m
          ? {
              ...m,
              active: (m.active + delta + m.items.length) % m.items.length,
            }
          : m,
      );
    } else if (e.key === "Enter" || e.key === "Tab") {
      e.preventDefault();
      pickMenuItem(menu.items[menu.active]);
    } else if (e.key === "Escape") {
      e.preventDefault();
      setMenu(null);
    }
  }

  /** 把用户消息中的 @名称 展开为引用块（预览 + read_file 指引），追加到 prompt */
  async function expandReferences(text: string): Promise<string> {
    if (!isWork || !workPath || !text.includes("@")) return text;
    const tokens = [...new Set(text.match(/@[\w\u4e00-\u9fff./-]+/g) ?? [])];
    const blocks: string[] = [];
    for (const token of tokens) {
      const q = token.slice(1);
      const file = files.find((f) => f.name === q || f.path === q);
      if (!file) continue;
      try {
        const content = await api.readFile(workPath, file.path);
        const preview = content.slice(0, 300);
        blocks.push(
          `【引用素材 @${file.path}】（完整内容可用 read_file 读取）\n${preview}${content.length > 300 ? "…" : ""}`,
        );
      } catch {
        blocks.push(`【引用素材 @${file.path}】（读取失败，可用 read_file 读取）`);
      }
    }
    return blocks.length > 0 ? `${text}\n\n${blocks.join("\n\n")}` : text;
  }

  async function send() {
    const raw = input.trim();
    if (!raw || busy) return;
    setMenu(null);
    setInlineProposals([]);
    setApprovals([]);
    busyRef.current = true;

    // slash 命令（过渡实现：前端直达命令，R4 切换为 Agent 指令）
    if (isWork && raw.startsWith("/")) {
      const parsed = parseSlash(raw);
      if ("error" in parsed) {
        toast.error(parsed.error);
        return;
      }
      setInput("");
      setBusy(true);
      setMessages((m) => [...m, { role: "user", text: raw, tool_calls: [] }]);
      setStreaming({ text: "", toolCalls: [] });
      const channel = makeChannel<AgentEvent>();
      let acc = "";
      channel.onmessage = (ev) => {
        if (ev.type === "text_delta") acc += ev.text;
        handleStreamEvent(ev);
      };
      try {
        const summary = await runSlash(parsed, workPath!, channel);
        if (summary !== null) {
          setStreaming(null);
          setMessages((m) => [...m, { role: "assistant", text: summary, tool_calls: [] }]);
        } else {
          // 流式 flow：把累积文本与工具调用固化为一条助手消息（工具调用结构化进卡片）
          setStreaming((s) => {
            if (s) {
              setMessages((m) => [
                ...m,
                {
                  role: "assistant",
                  text: s.text,
                  tool_calls: s.toolCalls.map((t) => ({
                    name: t.name,
                    args: t.args,
                    result: t.result,
                    is_error: false,
                  })),
                },
              ]);
            }
            return null;
          });
          onTurnDone();
        }
      } catch (e) {
        setStreaming(null);
        toast.error(String(e));
      } finally {
        setBusy(false);
        busyRef.current = false;
      }
      return;
    }

    // 普通对话（@ 引用展开进 prompt）
    const sid = sessionId ?? onNewSession();
    setInput("");
    setBusy(true);
    setMessages((m) => [...m, { role: "user", text: raw, tool_calls: [] }]);
    setStreaming({ text: "", toolCalls: [] });

    const prompt = await expandReferences(raw);
    const channel = makeChannel<AgentEvent>();
    channel.onmessage = handleStreamEvent;

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
      busyRef.current = false;
    }
  }

  function handleStreamEvent(ev: AgentEvent) {
    // 审批事件（R4）：卡片状态机独立于流式消息区
    if (ev.type === "approval_request") {
      setApprovals((list) => [
        ...list,
        {
          id: ev.id,
          tool: ev.tool,
          title: ev.title,
          detail: ev.detail,
          status: "pending",
          note: null,
        },
      ]);
    } else if (ev.type === "approval_resolved") {
      setApprovals((list) =>
        list.map((a) =>
          a.id === ev.id
            ? { ...a, status: ev.allowed ? "allowed" : "denied", note: ev.note }
            : a,
        ),
      );
    }
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
  }

  /** 审批决策（允许/拒绝）：工具挂起处会随即恢复并回传 approval_resolved 事件 */
  async function decideApproval(id: number, allow: boolean) {
    try {
      await api.decideApproval(id, allow);
    } catch (e) {
      toast.error(String(e));
    }
  }

  return (
    <div className="flex h-full flex-col">
      <Conversation>
        <ConversationContent>
          {messages.map((m, i) => (
            <div key={i} className="flex flex-col gap-1.5">
              {m.tool_calls?.map((tc, j) => (
                <ToolCallCard
                  key={j}
                  call={{
                    name: tc.name,
                    args: tc.args,
                    result: tc.result,
                    isError: tc.is_error,
                  }}
                />
              ))}
              {m.text.trim() !== "" && (
                <Message from={m.role === "user" ? "user" : "assistant"}>
                  <MessageContent>
                    {m.role === "user" ? (
                      <MessageText text={m.text} />
                    ) : (
                      <MessageResponse>{m.text}</MessageResponse>
                    )}
                  </MessageContent>
                </Message>
              )}
            </div>
          ))}
          {streaming && (
            <>
              {streaming.toolCalls.map((t, i) => (
                <ToolCallCard key={i} call={t} />
              ))}
              <Message from="assistant">
                <MessageContent pulsing>
                  <MessageResponse>{streaming.text || "…"}</MessageResponse>
                </MessageContent>
              </Message>
            </>
          )}
          {approvals.length > 0 && (
            <div className="flex flex-col gap-2">
              {approvals.map((a) => (
                <ConfirmationCard key={a.id} approval={a} onDecide={decideApproval} />
              ))}
            </div>
          )}
          {inlineProposals.length > 0 && (
            <div className="flex flex-col gap-2">
              {inlineProposals.map((item) => (
                <ProposalCard
                  key={item.proposal.id}
                  item={item}
                  workPath={workPath!}
                  onResolved={(id, status) =>
                    setInlineProposals((list) =>
                      list.map((it) =>
                        it.proposal.id === id ? { ...it, status } : it,
                      ),
                    )
                  }
                />
              ))}
              <div className="text-center text-[10px] text-muted-foreground">
                提议已入审阅队列；此处或右栏「审阅」任一处处理均可
              </div>
            </div>
          )}
          {messages.length === 0 && !streaming && (
            <ConversationEmptyState
              title={
                busy
                  ? "连接模型…"
                  : isWork
                    ? "向写作助手提问"
                    : "找灵感、头脑风暴、写作问答（无作品上下文）"
              }
              description={
                busy
                  ? undefined
                  : isWork
                    ? "设定推演 / 情节排查 / 头脑风暴；用 / 调技能、@ 引用素材"
                    : undefined
              }
              icon={busy ? <Loader2 className="size-5 animate-spin" /> : undefined}
            />
          )}
        </ConversationContent>
        <ConversationScrollButton />
      </Conversation>

      {planSlot && <div className="shrink-0 border-t">{planSlot}</div>}
      <div className="shrink-0 border-t p-2">
        <PromptInput
          value={input}
          setValue={(v) => {
            setInput(v);
            detectMenu(v);
          }}
          onSubmit={send}
          isLoading={busy}
        >
          <PromptInputBody>
            {menu && (
              <PromptAutocomplete
                items={menu.items}
                activeIndex={menu.active}
                onPick={pickMenuItem}
                onHover={(i) =>
                  setMenu((m) => (m ? { ...m, active: i } : m))
                }
              />
            )}
            <PromptInputTextarea
              placeholder={
                isWork
                  ? "输入消息，或 / 命令、@ 引用素材；Enter 发送"
                  : "输入消息，Enter 发送，Shift+Enter 换行"
              }
              onKeyDown={handleMenuKeys}
            />
          </PromptInputBody>
          <PromptInputToolbar>
            <PromptInputSubmit />
          </PromptInputToolbar>
        </PromptInput>
      </div>
    </div>
  );
}
