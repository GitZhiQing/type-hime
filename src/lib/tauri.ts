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
  /** 会话全部轮次累计用量（成本参考） */
  session_usage: { input_tokens: number; output_tokens: number } | null;
  /** 最近一轮输入 tokens（≈ 当前对话历史大小，窗口占用口径） */
  last_input_tokens: number | null;
  /** 最近一轮输入占窗口比例（0-100） */
  session_percent: number;
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
  | { type: "tool_call"; name: string; args: string; call_id?: string | null }
  | { type: "tool_result"; name: string; output: string; call_id?: string | null }
  | { type: "final"; output: string }
  | { type: "error"; message: string }
  | { type: "approval_request"; id: number; tool: string; title: string; detail: string }
  | { type: "approval_resolved"; id: number; allowed: boolean; note: string | null }
  | { type: "sub_task_start"; id: string; label: string; agent?: string | null; color?: string | null; session?: string | null }
  | { type: "sub_task_delta"; id: string; text: string }
  | { type: "sub_task_tool_call"; id: string; name: string; args: string; call_id?: string | null }
  | { type: "sub_task_tool_result"; id: string; name: string; output: string; call_id?: string | null }
  | {
      type: "sub_task_end";
      id: string;
      status: string;
      summary: string;
      tokens?: number | null;
      duration_ms?: number | null;
    }
  | { type: "proposal_applied"; id: number; rel_path: string; result: string }
  | { type: "goal_update"; goal: SessionGoalView }
  | { type: "goal_round_start"; iteration: number; title: string };

/** 长时目标状态（R4.5 · D31；与 Rust GoalStatus 对应） */
export type GoalStatus = "active" | "paused" | "budget_limited" | "complete";

/** 轮末验证器结论（JSON {passed, reason, nextAction} 的解析产物） */
export interface GoalVerifierOutput {
  passed: boolean;
  reason: string;
  next_action?: string | null;
}

export interface GoalIterationView {
  index: number;
  /** 迭代行标题：第 1 轮=目标本身，第 n 轮=上一轮验证 nextAction */
  title: string;
  ended_at: string | null;
  tokens: number;
  verification: GoalVerifierOutput | null;
}

export interface GoalBudget {
  max_iterations: number;
  max_total_tokens: number | null;
}

/** 会话目标视图（GoalBar 数据源；与 Rust SessionGoalView 对应） */
export interface SessionGoalView {
  objective: string;
  status: GoalStatus;
  status_label: string;
  iterations: GoalIterationView[];
  tokens_used: number;
  budget: GoalBudget;
  /** 运行中下一步（上一轮 nextAction；complete/paused 时为 null） */
  current_title: string | null;
}

/** 权限档位（R4 · D30：会话级两档） */
export type PermissionMode = "confirm" | "full_access";

/** 权限覆盖条目（bash 命令模式 / propose_* 路径模式） */
export interface ToolOverride {
  tool: string;
  pattern: string | null;
  mode: PermissionMode;
}

export interface PermissionConfig {
  overrides: ToolOverride[];
}

/** 工具审批请求（R4：确认档 bash 等逐条确认；Confirmation 卡数据源） */
export interface ApprovalRequestView {
  id: number;
  tool: string;
  title: string;
  detail: string;
  created_at: string;
}

export interface SessionMeta {
  id: string;
  title: string;
  preview: string;
  created_at: string;
  /** 权限档位（R4：输入框左下角下拉的当前值） */
  permission_mode: PermissionMode;
}

export interface SkillMeta {
  name: string;
  description: string;
  when_to_use: string | null;
  source: "builtin" | "work";
  overridden: boolean;
}

/** 技能原始 md（R5-② 技能文档视图；source=work 为作品级覆盖） */
export interface SkillRaw {
  content: string;
  source: "builtin" | "work";
}

/** 子 Agent 清单项（R8-② 多 Agent 团队管理页） */
export interface AgentMeta {
  name: string;
  description: string;
  source: "builtin" | "user";
  overridden: boolean;
  max_turns: number;
  /** 身份色 key（R9；null=按名 hash 取色） */
  color: string | null;
}

/** 子 Agent 原始 md（管理页编辑） */
export interface AgentRaw {
  content: string;
  source: "builtin" | "user";
}

/** 子 Agent 结构化定义（R9 表单视图；与 Rust AgentProfile 对应） */
export interface AgentProfile {
  name: string;
  description: string;
  model: string | null;
  color: string | null;
  tools: string[];
  skills: string[];
  max_turns: number;
  /** 系统提示词（md 正文） */
  body: string;
}

/** 可挂载工具元信息（R9 管理页多选数据源） */
export interface ToolMeta {
  name: string;
  label: string;
  summary: string;
}

export interface MessageView {
  role: string;
  text: string;
  /** 本消息发起的工具调用（已配对结果；纯工具结果消息后端已跳过） */
  tool_calls: ToolCallView[];
}

/** 会话档案的子任务行（D51：spawn_agent 委派历史——历史任务卡与子会话标签的数据源） */
export interface SubTaskRecord {
  id: string;
  agent?: string | null;
  label: string;
  session?: string | null;
  status: "running" | "done" | "error";
  summary?: string | null;
  tokens?: number | null;
  duration_ms?: number | null;
}

export interface ToolCallView {
  name: string;
  args: string;
  result: string | null;
  is_error: boolean;
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

/** 账本条目（R5-② 结构化视图；与 Rust LedgerEntry 对应） */
export interface LedgerEntry {
  title: string;
  body: string;
  /** 最近更新章号（从 body 的 ch-00N 标注提取；null=未标注） */
  last_touched: number | null;
}

export interface Ledger {
  characters: LedgerEntry[];
  facts: LedgerEntry[];
  foreshadows: LedgerEntry[];
  timeline: LedgerEntry[];
}

export interface Proposal {
  id: number;
  work_path: string;
  rel_path: string;
  diff: string;
  reason: string;
  is_new_file: boolean;
  /** 触及可见性 frontmatter（必须人工审阅，永不被完全访问档自动应用） */
  requires_review: boolean;
  created_at: string;
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

/** 文件夹选择器的目录列举（D56；current 空串 = Windows 盘符列表虚拟根） */
export interface DirEntryInfo {
  name: string;
  path: string;
}

export interface DirListing {
  current: string;
  /** Some("") = 回到盘符列表；null = 已在顶层 */
  parent: string | null;
  dirs: DirEntryInfo[];
}

export const api = {
  getWorksRoot: () => call<string>("get_works_root"),
  /** 设置默认工作区（D56：配置页与新建作品共用；校验/建目录后持久化） */
  setWorksRoot: (path: string) => call<string>("set_works_root", { path }),
  /** 应用内文件夹选择器：列举目录（path=null → 盘符列表/主目录） */
  listDirs: (path?: string | null) =>
    call<DirListing>("list_dirs", { path: path ?? null }),
  listWorks: () => call<WorkSummary[]>("list_works"),
  createWork: (name: string, base?: string) =>
    call<WorkSummary>("create_work", { name, base: base ?? null }),
  /** 重命名作品（D57）：目录名与 project.json 同步 */
  renameWork: (workPath: string, newName: string) =>
    call<WorkSummary>("rename_work", { workPath, newName }),
  /** 删除作品（D57）：移入工作区「_回收站」（可找回）；返回回收站落点路径 */
  deleteWork: (workPath: string) => call<string>("delete_work", { workPath }),
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
  loadSessionTasks: (workPath: string, sessionId: string) =>
    call<SubTaskRecord[]>("load_session_tasks", { workPath, sessionId }),
  applyProposal: (workPath: string, id: number) =>
    call<string>("apply_proposal", { workPath, id }),
  rejectProposal: (id: number, comment?: string) =>
    call<void>("reject_proposal", { id, comment: comment ?? null }),

  // 工具审批（R4 Spike A：Confirmation 卡）
  decideApproval: (id: number, allow: boolean) =>
    call<boolean>("decide_approval", { id, allow }),
  listApprovals: () => call<ApprovalRequestView[]>("list_approvals"),

  // 权限（R4-①：档位存会话档案，覆盖表存 project.json）
  setSessionPermissionMode: (
    workPath: string,
    sessionId: string,
    mode: PermissionMode,
  ) => call<void>("set_session_permission_mode", { workPath, sessionId, mode }),
  getPermissionConfig: (workPath: string) =>
    call<PermissionConfig>("get_permission_config", { workPath }),
  setPermissionConfig: (workPath: string, overrides: ToolOverride[]) =>
    call<void>("set_permission_config", { workPath, overrides }),

  // 长时目标（R4.5-②：SessionGoal 存会话档案；设置/恢复经聊天流 /goal）
  getSessionGoal: (workPath: string, sessionId: string) =>
    call<SessionGoalView | null>("get_session_goal", { workPath, sessionId }),
  goalControl: (workPath: string, sessionId: string, action: "pause" | "clear") =>
    call<SessionGoalView | null>("goal_control", { workPath, sessionId, action }),

  // 制品面板（R5-②：git 单提交 diff + 技能文档读写）
  gitShow: (workPath: string, hash: string) =>
    call<string>("git_show", { workPath, hash }),
  readSkill: (workPath: string, name: string) =>
    call<SkillRaw>("read_skill", { workPath, name }),
  writeSkill: (workPath: string, name: string, content: string) =>
    call<void>("write_skill", { workPath, name, content }),

  // 作品文件监听（R13：应用外变更 → work:changed 事件）
  watchWork: (workPath: string) => call<void>("watch_work", { workPath }),
  unwatchWork: () => call<void>("unwatch_work"),

  // 子 Agent 管理（R8-②：用户级 AgentProfile CRUD）
  listAgents: () => call<AgentMeta[]>("list_agents"),
  readAgent: (name: string) => call<AgentRaw>("read_agent", { name }),
  writeAgent: (name: string, content: string) =>
    call<void>("write_agent", { name, content }),
  deleteAgent: (name: string) => call<void>("delete_agent", { name }),
  // R9：结构化表单视图
  getAgentProfile: (name: string) => call<AgentProfile>("get_agent_profile", { name }),
  saveAgentProfile: (profile: AgentProfile) =>
    call<void>("save_agent_profile", { profile }),
  listTools: () => call<ToolMeta[]>("list_tools"),

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
