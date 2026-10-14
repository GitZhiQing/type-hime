import {
  Message,
  MessageContent,
  MessageText,
} from "@/components/ai/message";
import { MessageResponse } from "@/components/ai/response";
import { ToolCallCard } from "@/components/ai/tool";
import { AgentTaskRow, type AgentTaskRowData } from "@/components/ai/task";
import type { MessageView, SubTaskRecord } from "@/lib/tauri";

/**
 * 会话历史渲染（D50 从 ChatPane 抽出共享；D51 接任务行）：
 * 主聊天与子 Agent 会话视图（只读）复用同一套渲染。
 * - D48 分组：连续 assistant 消息合并为一个视觉流（一轮工具调用循环在档案里是多条消息，
 *   块间 gap-3 与块内 gap-1.5 交替会让工具卡间距忽大忽小）。
 * - D51 任务行：spawn_agent 调用渲染为单行任务卡（ZCode SubagentRow 同语义）——
 *   以会话档案的 SubTaskRecord 配对（(agent, label) 首选匹配，防失败轮孤儿行错位）；
 *   无档案（旧会话/事件未贯通）时从调用参数降级呈现，不退回普通工具卡。
 */

/** 从 spawn_agent 参数构造降级任务行（无档案可用时） */
function degradedRow(args: string, result: string | null, isError?: boolean): AgentTaskRowData | null {
  try {
    const o = JSON.parse(args) as { subagent?: unknown; description?: unknown };
    const agent = typeof o.subagent === "string" ? o.subagent : null;
    const label = typeof o.description === "string" ? o.description : "";
    if (!agent && !label) return null;
    return {
      id: `legacy-${label}`,
      agent,
      label: label || "子 Agent 委派",
      status: result === null ? "running" : isError ? "error" : "done",
      session: null,
    };
  } catch {
    return null;
  }
}

export function ConversationHistory({
  messages,
  tasks,
}: {
  messages: MessageView[];
  /** 会话档案的子任务行（D51；缺省=从参数降级呈现） */
  tasks?: SubTaskRecord[];
}) {
  const groups: MessageView[][] = [];
  for (const m of messages) {
    const g = groups[groups.length - 1];
    if (m.role !== "user" && g && g[g.length - 1].role !== "user") g.push(m);
    else groups.push([m]);
  }

  // 任务行配对：按出现顺序消费；(agent, label) 精确命中才配对——
  // 解析成功但无匹配（旧会话/失败轮）一律走降级呈现，**不做兜底抢占**
  // （曾致：早期无档案的委派吃掉本轮记录，本轮任务行反而失去打开入口）。
  const pool = (tasks ?? []).map((t) => ({ t, used: false }));
  const takeRecord = (args: string): SubTaskRecord | null => {
    let agent: string | null = null;
    let label: string | null = null;
    try {
      const o = JSON.parse(args) as { subagent?: unknown; description?: unknown };
      agent = typeof o.subagent === "string" ? o.subagent : null;
      label = typeof o.description === "string" ? o.description : null;
    } catch {
      return null;
    }
    for (const p of pool) {
      if (p.used) continue;
      if (p.t.agent === agent && (label === null || p.t.label === label)) {
        p.used = true;
        return p.t;
      }
    }
    return null;
  };

  return (
    <>
      {groups.map((g, gi) => (
        <div key={gi} className="flex flex-col gap-1.5">
          {g.flatMap((m, mi) => [
            ...(m.tool_calls ?? []).map((tc, j) => {
              if (tc.name !== "spawn_agent") {
                return (
                  <ToolCallCard
                    key={`t${mi}-${j}`}
                    call={{
                      name: tc.name,
                      args: tc.args,
                      result: tc.result,
                      isError: tc.is_error,
                    }}
                  />
                );
              }
              const rec = takeRecord(tc.args);
              const data: AgentTaskRowData | null = rec
                ? {
                    id: rec.id,
                    agent: rec.agent ?? null,
                    label: rec.label,
                    status: rec.status,
                    tokens: rec.tokens,
                    durationMs: rec.duration_ms,
                    session: rec.session,
                  }
                : degradedRow(tc.args, tc.result, tc.is_error);
              if (!data) {
                return (
                  <ToolCallCard
                    key={`t${mi}-${j}`}
                    call={{
                      name: tc.name,
                      args: tc.args,
                      result: tc.result,
                      isError: tc.is_error,
                    }}
                  />
                );
              }
              return <AgentTaskRow key={`t${mi}-${j}`} data={data} />;
            }),
            m.text.trim() !== "" ? (
              <Message key={`m${mi}`} from={m.role === "user" ? "user" : "assistant"}>
                <MessageContent>
                  {m.role === "user" ? (
                    <MessageText text={m.text} />
                  ) : (
                    <MessageResponse>{m.text}</MessageResponse>
                  )}
                </MessageContent>
              </Message>
            ) : null,
          ])}
        </div>
      ))}
    </>
  );
}
