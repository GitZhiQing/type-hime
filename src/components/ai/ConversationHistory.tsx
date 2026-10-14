import {
  Message,
  MessageContent,
  MessageText,
} from "@/components/ai/message";
import { MessageResponse } from "@/components/ai/response";
import { ToolCallCard } from "@/components/ai/tool";
import type { MessageView } from "@/lib/tauri";

/**
 * 会话历史渲染（D50 从 ChatPane 抽出共享）：主聊天与子 Agent 会话视图（只读）复用同一套
 * ——ZCode SessionPane readOnly 同思路：transcript 视图零新增组件。
 * D48 分组：连续 assistant 消息合并为一个视觉流（一轮工具调用循环在档案里是多条消息，
 * 块间 gap-3 与块内 gap-1.5 交替会让工具卡间距忽大忽小）。
 */
export function ConversationHistory({ messages }: { messages: MessageView[] }) {
  const groups: MessageView[][] = [];
  for (const m of messages) {
    const g = groups[groups.length - 1];
    if (m.role !== "user" && g && g[g.length - 1].role !== "user") g.push(m);
    else groups.push([m]);
  }
  return (
    <>
      {groups.map((g, gi) => (
        <div key={gi} className="flex flex-col gap-1.5">
          {g.flatMap((m, mi) => [
            ...(m.tool_calls ?? []).map((tc, j) => (
              <ToolCallCard
                key={`t${mi}-${j}`}
                call={{
                  name: tc.name,
                  args: tc.args,
                  result: tc.result,
                  isError: tc.is_error,
                }}
              />
            )),
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
