import { invoke, Channel } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";

// ---------------------------------------------------------------------------
// 传输层适配（D18）：Tauri IPC（桌面）或 HTTP+SSE（浏览器测试通道，配合
// `cargo run --example webapi` + vite /api 代理）。对上层 API 完全透明。

const isTauri = typeof window !== "undefined" && "__TAURI_INTERNALS__" in window;

/** 流式事件通道（Tauri Channel 与 Web 实现的共同接口） */
export interface EventChannel<T> {
  onmessage: ((message: T) => void) | null;
}

class WebChannel<T> implements EventChannel<T> {
  onmessage: ((message: T) => void) | null = null;
}

export function makeChannel<T>(): EventChannel<T> {
  if (isTauri) return new Channel<T>() as unknown as EventChannel<T>;
  return new WebChannel<T>();
}

async function call<T>(cmd: string, args?: Record<string, unknown>): Promise<T> {
  if (isTauri) return invoke<T>(cmd, args ?? {});
  const res = await fetch(`/api/${cmd}`, {
    method: "POST",
    headers: { "Content-Type": "application/json" },
    body: JSON.stringify(args ?? {}),
  });
  const data = await res.json().catch(() => null);
  if (!res.ok) throw new Error((data as { error?: string })?.error ?? `HTTP ${res.status}`);
  return data as T;
}

/** 流式命令：Tauri 走 Channel，Web 走 SSE 流解析 */
async function callStream<T>(
  cmd: string,
  args: Record<string, unknown>,
  channel: EventChannel<T>,
): Promise<void> {
  if (isTauri) {
    return invoke<void>(cmd, { ...args, onEvent: channel as unknown as Channel<T> });
  }
  const res = await fetch(`/api/${cmd}`, {
    method: "POST",
    headers: { "Content-Type": "application/json" },
    body: JSON.stringify(args),
  });
  if (!res.ok || !res.body) {
    const data = await res.json().catch(() => null);
    throw new Error((data as { error?: string })?.error ?? `HTTP ${res.status}`);
  }
  const reader = res.body.getReader();
  const decoder = new TextDecoder();
  let buf = "";
  for (;;) {
    const { done, value } = await reader.read();
    if (done) break;
    buf += decoder.decode(value, { stream: true });
    let idx: number;
    while ((idx = buf.indexOf("\n\n")) >= 0) {
      const chunk = buf.slice(0, idx);
      buf = buf.slice(idx + 2);
      for (const line of chunk.split("\n")) {
        if (line.startsWith("data:")) {
          const payload = line.slice(5).trim();
          if (payload && channel.onmessage) {
            channel.onmessage(JSON.parse(payload) as T);
          }
        }
      }
    }
  }
}

/** 事件订阅：Tauri 走 listen，Web 走 EventSource（/api/events 广播频道） */
export async function listenCompat<T>(
  event: string,
  handler: (e: { payload: T }) => void,
): Promise<() => void> {
  if (isTauri) {
    const un = await listen<T>(event, handler);
    return () => {
      un();
    };
  }
  const es = new EventSource("/api/events");
  es.onmessage = (msg: MessageEvent) => {
    try {
      const data = JSON.parse(msg.data as string) as { event: string; payload: unknown };
      if (data?.event === event) {
        // 兼容旧通道的双重编码（payload 曾为 JSON 字符串）
        const payload =
          typeof data.payload === "string"
            ? (JSON.parse(data.payload) as T)
            : (data.payload as T);
        handler({ payload });
      }
    } catch {
      /* 忽略非 JSON 帧与 keep-alive */
    }
  };
  return () => es.close();
}

export interface WorkSummary {
  name: string;
  path: string;
  created_at: string;
}

export interface TreeNode {
  name: string;
  path: string;
  is_dir: boolean;
  children: TreeNode[];
}

export interface ProviderPreset {
  id: string;
  name: string;
  base_url: string;
  no_key: boolean;
  fallback_model: string;
  context_window: number;
}

export interface ProviderView {
  id: string;
  name: string;
  base_url: string;
  default_model: string;
  masked_key: string;
  no_key: boolean;
  context_window: number;
}

export interface ContextOverview {
  session_usage: { input_tokens: number; output_tokens: number } | null;
  assembly_tokens: number;
  assembly_over_budget: boolean;
  applied_detail: string;
  context_window: number;
  assembly_percent: number;
}

export interface SettingsView {
  providers: ProviderView[];
  active_provider: string | null;
  agent_overrides: Record<string, string>;
}

/** Agent 内核统一事件（与 Rust AgentEvent 对应） */
export type AgentEvent =
  | { type: "text_delta"; text: string }
  | { type: "tool_call"; name: string; args: string }
  | { type: "tool_result"; name: string; output: string }
  | { type: "final"; output: string }
  | { type: "error"; message: string };

export interface SessionMeta {
  id: string;
  title: string;
  preview: string;
  created_at: string;
}

export interface SkillMeta {
  name: string;
  description: string;
  when_to_use: string | null;
  source: "builtin" | "work";
  overridden: boolean;
}

export interface MessageView {
  role: string;
  text: string;
}

export type Stage =
  | "settings"
  | "outline"
  | "volume_outline"
  | "chapter_outline"
  | "chapters"
  | "review";

export interface GateInfo {
  stage: Stage;
  label: string;
  enterable: boolean;
  blocked_by: string | null;
  confirmed: boolean;
  current: boolean;
}

export interface PipelineView {
  stage: Stage;
  stage_label: string;
  confirmed: Stage[];
  gates: GateInfo[];
}

export interface SettingMeta {
  name: string;
  kind: string; // character | worldview | mystery
  visibility: "visible" | "hidden";
  open_at_vol: number | null;
  path: string;
}

export interface Ledger {
  characters: string[];
  facts: string[];
  foreshadows: string[];
}

export interface Proposal {
  id: number;
  work_path: string;
  rel_path: string;
  diff: string;
  reason: string;
  is_new_file: boolean;
  created_at: string;
}

export interface AssembledComponent {
  name: string;
  content: string;
  tokens: number;
  included: boolean;
  missing_hint: string | null;
}

export interface AssemblyResult {
  system: string;
  system_tokens: number;
  components: AssembledComponent[];
  filtered_by_visibility: number;
  total_tokens: number;
  over_budget: boolean;
  applied_detail: "full" | "name_only" | "compact";
  downgraded: boolean;
  cast: string[];
}

export interface IssueRecord {
  id: number;
  chapter: string;
  created_at: string;
  text: string;
  source: string;
  status: "open" | "fixed" | "dismissed" | "needs_ruling";
  ruling: string | null;
}

export const api = {
  getWorksRoot: () => call<string>("get_works_root"),
  listWorks: () => call<WorkSummary[]>("list_works"),
  createWork: (name: string, base?: string) =>
    call<WorkSummary>("create_work", { name, base: base ?? null }),
  readTree: (workPath: string) => call<TreeNode>("read_tree", { workPath }),
  readFile: (workPath: string, relPath: string) =>
    call<string>("read_file", { workPath, relPath }),
  writeFile: (workPath: string, relPath: string, content: string) =>
    call<void>("write_file", { workPath, relPath, content }),
  createFile: (workPath: string, relPath: string, content: string) =>
    call<void>("create_file", { workPath, relPath, content }),
  deleteFile: (workPath: string, relPath: string) =>
    call<void>("delete_file", { workPath, relPath }),
  restoreFile: (workPath: string, relPath: string) =>
    call<void>("restore_file", { workPath, relPath }),
  gitLog: (workPath: string, limit?: number) =>
    call<string[]>("git_log", { workPath, limit: limit ?? null }),

  // 设置
  getSettings: () => call<SettingsView>("get_settings"),
  listProviderPresets: () => call<ProviderPreset[]>("list_provider_presets"),
  upsertProvider: (
    id: string,
    name: string,
    baseUrl: string,
    apiKey: string,
    defaultModel: string,
    activate: boolean,
    contextWindow?: number,
  ) =>
    call<void>("upsert_provider", {
      id,
      name,
      baseUrl,
      apiKey,
      defaultModel,
      activate,
      contextWindow: contextWindow ?? null,
    }),
  removeProvider: (id: string) => call<void>("remove_provider", { id }),
  setActiveProvider: (id: string) =>
    call<void>("set_active_provider", { id }),
  testProvider: (baseUrl: string, apiKey: string, model: string) =>
    call<string>("test_provider", { baseUrl, apiKey, model }),
  listModels: (baseUrl: string, apiKey: string) =>
    call<string[]>("list_models", { baseUrl, apiKey }),
  setAgentModelOverride: (agentId: string, model: string | null) =>
    call<void>("set_agent_model_override", { agentId, model }),

  // 流水线
  getPipeline: (workPath: string) =>
    call<PipelineView>("get_pipeline", { workPath }),
  pipelineEnter: (workPath: string, stage: Stage) =>
    call<PipelineView>("pipeline_enter", { workPath, stage }),
  pipelineConfirm: (workPath: string, stage: Stage) =>
    call<PipelineView>("pipeline_confirm", { workPath, stage }),
  pipelineSkip: (workPath: string, stage: Stage) =>
    call<PipelineView>("pipeline_skip", { workPath, stage }),
  pipelineRedo: (workPath: string, stage: Stage) =>
    call<PipelineView>("pipeline_redo", { workPath, stage }),

  // 设定库 + 账本
  listSettings: (workPath: string) =>
    call<SettingMeta[]>("list_settings", { workPath }),
  createSetting: (
    workPath: string,
    kind: string,
    name: string,
    content: string,
    hidden: boolean,
    openAtVol: number | null,
  ) =>
    call<SettingMeta>("create_setting", {
      workPath,
      kind,
      name,
      content,
      hidden,
      openAtVol,
    }),
  setSettingVisibility: (
    workPath: string,
    relPath: string,
    hidden: boolean,
    openAtVol: number | null,
  ) =>
    call<void>("set_setting_visibility", {
      workPath,
      relPath,
      hidden,
      openAtVol,
    }),
  getLedger: (workPath: string) => call<Ledger>("get_ledger", { workPath }),

  // diff 审阅
  listProposals: () => call<Proposal[]>("list_proposals"),
  applyProposal: (workPath: string, id: number) =>
    call<string>("apply_proposal", { workPath, id }),
  rejectProposal: (id: number, comment?: string) =>
    call<void>("reject_proposal", { id, comment: comment ?? null }),

  // 装配
  previewAssembly: (
    workPath: string,
    volume: number,
    chapter: string,
    settingDetail: "full" | "name_only",
    tokenBudget: number,
  ) =>
    call<AssemblyResult>("preview_assembly", {
      workPath,
      volume,
      chapter,
      settingDetail,
      tokenBudget,
    }),

  // 生成流水线
  generateArtifact: (
    workPath: string,
    volume: number,
    chapter: string,
    target: "chapter_outline" | "chapter",
    onEvent: EventChannel<AgentEvent>,
  ) =>
    callStream<AgentEvent>("generate_artifact", { workPath, volume, chapter, target }, onEvent),
  proposeLedgerUpdate: (
    workPath: string,
    volume: number,
    chapter: string,
    onEvent: EventChannel<AgentEvent>,
  ) =>
    callStream<AgentEvent>(
      "propose_ledger_update",
      {
        workPath,
        volume,
        chapter,
      },
      onEvent,
    ),
  reviewChapter: (
    workPath: string,
    volume: number,
    chapter: string,
    mode: "consistency" | "deai",
    onEvent: EventChannel<AgentEvent>,
  ) =>
    callStream<AgentEvent>(
      "review_chapter",
      {
        workPath,
        volume,
        chapter,
        mode,
      },
      onEvent,
    ),
  recheckChapter: (
    workPath: string,
    volume: number,
    chapter: string,
    onEvent: EventChannel<AgentEvent>,
  ) => callStream<AgentEvent>("recheck_chapter", { workPath, volume, chapter }, onEvent),
  reviewVolume: (
    workPath: string,
    volume: number,
    onEvent: EventChannel<AgentEvent>,
  ) => callStream<AgentEvent>("review_volume", { workPath, volume }, onEvent),
  listIssues: (workPath: string, chapter?: string) =>
    call<IssueRecord[]>("list_issues", { workPath, chapter: chapter ?? null }),
  recordIssueRuling: (
    workPath: string,
    id: number,
    status: "fixed" | "dismissed" | "needs_ruling",
    note?: string,
  ) =>
    call<void>("record_issue_ruling", {
      workPath,
      id,
      status,
      note: note ?? null,
    }),

  // 维护
  checkVisibilityLeak: (workPath: string) =>
    call<string[]>("check_visibility_leak", { workPath }),
  tidyLedger: (workPath: string, onEvent: EventChannel<AgentEvent>) =>
    callStream<AgentEvent>("tidy_ledger", { workPath }, onEvent),
  preflightChapter: (workPath: string, volume: number, chapter: string) =>
    call<string[]>("preflight_chapter", { workPath, volume, chapter }),
  listSkills: (workPath?: string) =>
    call<SkillMeta[]>("list_skills", { workPath: workPath ?? null }),

  // 上下文工程（R3）
  contextOverview: (
    workPath: string,
    sessionId: string | null,
    volume: number,
    chapter: string,
  ) =>
    call<ContextOverview>("context_overview", {
      workPath,
      sessionId,
      volume,
      chapter,
    }),

  // 成本账
  getUsageSummary: (workPath: string) =>
    call<{ input_tokens: number; output_tokens: number }>("get_usage_summary", {
      workPath,
    }),

  // 会话（作品会话 + 全局对话会话）
  listSessions: (workPath: string) =>
    call<SessionMeta[]>("list_sessions", { workPath }),
  renameSession: (workPath: string, sessionId: string, title: string) =>
    call<void>("rename_session", { workPath, sessionId, title }),
  deleteSession: (workPath: string, sessionId: string) =>
    call<void>("delete_session", { workPath, sessionId }),
  loadSessionView: (workPath: string, sessionId: string) =>
    call<MessageView[]>("load_session_view", { workPath, sessionId }),
  chatStream: (
    workPath: string,
    sessionId: string,
    prompt: string,
    onEvent: EventChannel<AgentEvent>,
  ) =>
    callStream<AgentEvent>(
      "chat_stream",
      { workPath, sessionId, prompt },
      onEvent,
    ),
  // 全局对话（rail「对话」页）
  listSessionsGlobal: () => call<SessionMeta[]>("list_sessions_global"),
  renameSessionGlobal: (sessionId: string, title: string) =>
    call<void>("rename_session_global", { sessionId, title }),
  deleteSessionGlobal: (sessionId: string) =>
    call<void>("delete_session_global", { sessionId }),
  loadSessionViewGlobal: (sessionId: string) =>
    call<MessageView[]>("load_session_view_global", { sessionId }),
  chatStreamGlobal: (
    sessionId: string,
    prompt: string,
    onEvent: EventChannel<AgentEvent>,
  ) =>
    callStream<AgentEvent>(
      "chat_stream_global",
      { sessionId, prompt },
      onEvent,
    ),
};
