import { useEffect, useRef, useState, type ReactNode } from "react";
import {
  api,
  makeChannel,
  listenCompat,
  AgentEvent,
  MessageView,
  Proposal,
  PermissionMode,
  TreeNode,
} from "@/lib/tauri";
import { toast } from "sonner";
import { Loader2, Target } from "lucide-react";
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
import { TaskCard, type TaskView } from "@/components/ai/task";
import { PermissionSelect } from "@/components/PermissionSelect";
import { SLASH_DEFS } from "@/lib/slash";

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

/** 一个流式气泡（R4.5-④：长时目标每轮迭代一个气泡，标题=上一轮验证 nextAction） */
interface StreamingRound {
  title: string | null;
  state: StreamingState;
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
  /** 流式轮次（长时目标一轮迭代 = 一个气泡；普通会话单轮也是数组首元素） */
  const [rounds, setRounds] = useState<StreamingRound[]>([]);
  const [busy, setBusy] = useState(false);
  const [menu, setMenu] = useState<MenuState | null>(null);
  const [files, setFiles] = useState<{ name: string; path: string }[]>([]);
  const [inlineProposals, setInlineProposals] = useState<InlineProposal[]>([]);
  /** 本轮的工具审批卡（R4：审批事件驱动的状态机；决策由审批事件回传） */
  const [approvals, setApprovals] = useState<ApprovalView[]>([]);
  /** 本轮的子任务卡（R4：run_flow 长流程的过程展示） */
  const [subtasks, setSubtasks] = useState<TaskView[]>([]);
  /** 当前会话权限档位（R4-③：输入框左下角下拉；scope=work） */
  const [permMode, setPermMode] = useState<PermissionMode>("confirm");
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
    setRounds([]);
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

  // 权限档位：随会话加载/切换（R4-③）。新会话尚无存档时回落默认「变更前确认」，
  // 不得沿用上一个会话的档位显示（曾致：按钮显示完全访问、后端实际确认档）。
  useEffect(() => {
    if (!isWork || !workPath || !sessionId) return;
    let alive = true;
    api
      .listSessions(workPath)
      .then((list) => {
        if (!alive) return;
        const cur = list.find((s) => s.id === sessionId);
        setPermMode(cur ? cur.permission_mode : "confirm");
      })
      .catch(() => {});
    return () => {
      alive = false;
    };
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [isWork, workPath, sessionId]);

  /** 切换会话权限档位（无会话时先建会话） */
  async function changePermissionMode(mode: PermissionMode) {
    if (!isWork || !workPath) return;
    const sid = sessionId ?? onNewSession();
    try {
      await api.setSessionPermissionMode(workPath, sid, mode);
      setPermMode(mode);
      toast.info(
        mode === "full_access"
          ? "已切为「完全访问」：提议自动应用（可见性变更除外）、白名单命令直通"
          : "已切为「变更前确认」：提议逐条审阅、白名单命令逐条确认",
      );
      onTurnDone(); // 会话列表刷新（档位徽标）
    } catch (e) {
      toast.error(String(e));
    }
  }

  /** 审批卡升档：允许本次并切会话为完全访问 */
  async function grantFullAccessAndAllow(id: number) {
    await changePermissionMode("full_access");
    await decideApproval(id, true);
  }

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
    setInput("");
    await sendText(raw);
  }

  /** 发送一条消息（send 与目标横幅「继续」共用；raw 为原始文本，@ 引用在此展开） */
  async function sendText(raw: string) {
    if (!raw || busyRef.current) return;
    setMenu(null);
    setInlineProposals([]);
    setApprovals([]);
    setSubtasks([]);
    busyRef.current = true;

    // R4-⑥：slash 不再前端直调命令——一律作为消息发给 Agent（后端把 /命令 参数
    // 转成结构化指令块，驱动 run_flow / 直连检查工具，Agent 可汇报阶段门与建议）。

    // 普通对话（@ 引用展开进 prompt）
    const sid = sessionId ?? onNewSession();
    setBusy(true);
    setMessages((m) => [...m, { role: "user", text: raw, tool_calls: [] }]);
    setRounds([{ title: null, state: { text: "", toolCalls: [] } }]);

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
      setRounds([]);
      onTurnDone();
    } catch (e) {
      setRounds([]);
      toast.error(String(e));
    } finally {
      setBusy(false);
      busyRef.current = false;
    }
  }

  // 目标横幅「继续」（R4.5-④）：续跑需要流式通道，经 window 事件交本组件以聊天流驱动
  useEffect(() => {
    const onResume = () => {
      if (!isWork || busyRef.current) return;
      sendText("/goal 恢复");
    };
    window.addEventListener("goal:resume", onResume);
    return () => window.removeEventListener("goal:resume", onResume);
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [isWork, workPath, sessionId]);

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
    } else if (ev.type === "sub_task_start") {
      setSubtasks((list) => [
        ...list,
        { id: ev.id, label: ev.label, status: "running", text: "", toolCalls: [], summary: null },
      ]);
    } else if (ev.type === "sub_task_delta") {
      setSubtasks((list) =>
        list.map((t) => (t.id === ev.id ? { ...t, text: t.text + ev.text } : t)),
      );
    } else if (ev.type === "sub_task_tool_call") {
      setSubtasks((list) =>
        list.map((t) =>
          t.id === ev.id
            ? { ...t, toolCalls: [...t.toolCalls, { name: ev.name, args: ev.args, result: null }] }
            : t,
        ),
      );
    } else if (ev.type === "sub_task_tool_result") {
      setSubtasks((list) =>
        list.map((t) => {
          if (t.id !== ev.id) return t;
          const calls = [...t.toolCalls];
          for (let i = calls.length - 1; i >= 0; i--) {
            if (calls[i].name === ev.name && calls[i].result === null) {
              calls[i] = { ...calls[i], result: ev.output };
              break;
            }
          }
          return { ...t, toolCalls: calls };
        }),
      );
    } else if (ev.type === "sub_task_end") {
      setSubtasks((list) =>
        list.map((t) =>
          t.id === ev.id
            ? {
                ...t,
                status: ev.status === "done" ? "done" : "error",
                summary: ev.summary,
              }
            : t,
        ),
      );
    } else if (ev.type === "proposal_applied") {
      // 完全访问档自动应用（R4-③）：内联卡标记为已接受 + 通知审阅 Tab 刷新
      setInlineProposals((list) =>
        list.map((it) =>
          it.proposal.id === ev.id ? { ...it, status: "accepted" as const } : it,
        ),
      );
      window.dispatchEvent(new CustomEvent("review:changed"));
      toast.success(`已自动应用提议 #${ev.id}（${ev.rel_path}）`);
    } else if (ev.type === "goal_update") {
      // 长时目标状态更新（R4.5-④）：GoalBar 经 window 事件刷新
      window.dispatchEvent(new CustomEvent("goal:changed"));
      // 新目标在「变更前确认」档下启动：提示切完全访问（避免每个工具调用都弹审批卡）
      if (
        isWork &&
        ev.goal.status === "active" &&
        ev.goal.iterations.length === 0 &&
        permMode === "confirm"
      ) {
        toast.info("长时目标已启动", {
          description: "建议切换「完全访问」：否则每个 bash/工具确认都会挂起等待",
          action: {
            label: "切完全访问",
            onClick: () => changePermissionMode("full_access"),
          },
        });
      }
    } else if (ev.type === "goal_round_start") {
      // 长时目标续跑轮（R4.5-④）：当前气泡收尾，开新一轮气泡
      setRounds((rs) => [
        ...rs,
        { title: `第 ${ev.iteration} 轮 · ${ev.title}`, state: { text: "", toolCalls: [] } },
      ]);
    }
    setRounds((rs) => {
      if (rs.length === 0) return rs;
      switch (ev.type) {
        case "text_delta":
        case "tool_call":
        case "tool_result":
        case "error": {
          const rs2 = [...rs];
          const last = rs2[rs2.length - 1];
          const s = last.state;
          switch (ev.type) {
            case "text_delta":
              last.state = { ...s, text: s.text + ev.text };
              break;
            case "tool_call":
              last.state = {
                ...s,
                toolCalls: [...s.toolCalls, { name: ev.name, args: ev.args, result: null, state: "running" }],
              };
              break;
            case "tool_result": {
              const calls = [...s.toolCalls];
              for (let i = calls.length - 1; i >= 0; i--) {
                if (calls[i].name === ev.name && calls[i].state === "running") {
                  calls[i] = { ...calls[i], result: ev.output, state: "done" };
                  break;
                }
              }
              last.state = { ...s, toolCalls: calls };
              break;
            }
            case "error":
              last.state = { ...s, text: s.text + `\n\n[错误] ${ev.message}` };
              break;
          }
          return rs2;
        }
        default:
          return rs;
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
          {rounds.map((r, ri) => {
            const isLast = ri === rounds.length - 1;
            return (
              <div key={ri} className="flex flex-col gap-1.5">
                {r.title && (
                  <div className="flex items-center gap-1.5 text-[10px] font-medium text-muted-foreground">
                    <Target className="size-3" />
                    {r.title}
                  </div>
                )}
                {r.state.toolCalls.map((t, i) => (
                  <ToolCallCard key={i} call={t} />
                ))}
                {(r.state.text.trim() !== "" || isLast) && (
                  <Message from="assistant">
                    <MessageContent pulsing={isLast}>
                      <MessageResponse>{r.state.text || "…"}</MessageResponse>
                    </MessageContent>
                  </Message>
                )}
              </div>
            );
          })}
          {subtasks.length > 0 && (
            <div className="flex flex-col gap-2">
              {subtasks.map((t) => (
                <TaskCard key={t.id} task={t} />
              ))}
            </div>
          )}
          {approvals.length > 0 && (
            <div className="flex flex-col gap-2">
              {approvals.map((a) => (
                <ConfirmationCard
                  key={a.id}
                  approval={a}
                  onDecide={decideApproval}
                  onGrantFullAccess={isWork ? grantFullAccessAndAllow : undefined}
                />
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
          {messages.length === 0 && rounds.length === 0 && (
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
            {isWork && workPath && (
              <PermissionSelect
                workPath={workPath}
                mode={permMode}
                onChange={changePermissionMode}
              />
            )}
            <PromptInputSubmit />
          </PromptInputToolbar>
        </PromptInput>
      </div>
    </div>
  );
}
