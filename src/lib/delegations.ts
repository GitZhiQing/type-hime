/**
 * 委派（子 Agent 会话）共享 store（D49 修正，形态对齐 ZCode Task transcript）：
 * 主对话只留紧凑任务卡；每个子 Agent 委派是一条**独立的实时会话记录**——
 * 委派指令（输入）+ 工具轨迹 + 流式文本 + 完成汇报，运行中实时更新、完成后保留可回看。
 * ChatPane 在 SubTask* 事件里写入；制品栏 SubagentView 订阅渲染（会话级，不入 localStorage）。
 */

export interface DelegationToolCall {
  name: string;
  args: string;
  result: string | null;
  /** rig call id（并行同名工具精确配对） */
  callId?: string | null;
}

export interface Delegation {
  id: string;
  agent: string | null;
  label: string;
  /** spawn_agent 的委派 prompt（子会话的「输入」） */
  prompt: string | null;
  /** 子会话档案 id（D50 贯通；完成后/重载后据此 loadSessionView 回看真实子会话） */
  session: string | null;
  status: "running" | "done" | "error";
  /** 子 Agent 流式文本累积 */
  text: string;
  toolCalls: DelegationToolCall[];
  summary: string | null;
  tokens: number | null;
  durationMs: number | null;
}

type Listener = () => void;

const store = new Map<string, Delegation>();
const listeners = new Set<Listener>();

function notify() {
  for (const fn of listeners) fn();
}

export function subscribeDelegations(fn: Listener): () => void {
  listeners.add(fn);
  return () => listeners.delete(fn);
}

export function getDelegation(id: string): Delegation | undefined {
  return store.get(id);
}

/** 创建委派记录（SubTaskStart 时；同 id 幂等） */
export function createDelegation(
  init: Pick<Delegation, "id" | "agent" | "label"> &
    Partial<Pick<Delegation, "prompt" | "session">>,
): void {
  if (store.has(init.id)) return;
  store.set(init.id, {
    prompt: null,
    session: null,
    status: "running",
    text: "",
    toolCalls: [],
    summary: null,
    tokens: null,
    durationMs: null,
    ...init,
  });
  notify();
}

/** 浅合并更新（返回全新对象触发订阅方渲染） */
export function patchDelegation(id: string, patch: Partial<Delegation>): void {
  const cur = store.get(id);
  if (!cur) return;
  store.set(id, { ...cur, ...patch });
  notify();
}

/** 流式文本追加 */
export function appendDelegationText(id: string, delta: string): void {
  const cur = store.get(id);
  if (!cur) return;
  store.set(id, { ...cur, text: cur.text + delta });
  notify();
}

/** 工具调用入轨（运行中，result=null） */
export function pushDelegationToolCall(
  id: string,
  call: DelegationToolCall,
): void {
  const cur = store.get(id);
  if (!cur) return;
  store.set(id, { ...cur, toolCalls: [...cur.toolCalls, call] });
  notify();
}

/** 工具结果回填：call_id 优先精确配对，回退按名找最近的运行中调用 */
export function resolveDelegationToolCall(
  id: string,
  result: { name: string; output: string; callId?: string | null },
): void {
  const cur = store.get(id);
  if (!cur) return;
  const calls = [...cur.toolCalls];
  let idx = -1;
  if (result.callId) {
    idx = calls.findIndex((c) => c.callId === result.callId && c.result === null);
  }
  if (idx < 0) {
    for (let i = calls.length - 1; i >= 0; i--) {
      if (calls[i].name === result.name && calls[i].result === null) {
        idx = i;
        break;
      }
    }
  }
  if (idx < 0) return;
  calls[idx] = { ...calls[idx], result: result.output };
  store.set(id, { ...cur, toolCalls: calls });
  notify();
}
