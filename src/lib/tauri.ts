import { invoke, Channel } from "@tauri-apps/api/core";

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
}

export interface ProviderView {
  id: string;
  name: string;
  base_url: string;
  default_model: string;
  masked_key: string;
  no_key: boolean;
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
  created_at: string;
  preview: string;
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

export const api = {
  getWorksRoot: () => invoke<string>("get_works_root"),
  listWorks: () => invoke<WorkSummary[]>("list_works"),
  createWork: (name: string, base?: string) =>
    invoke<WorkSummary>("create_work", { name, base: base ?? null }),
  readTree: (workPath: string) => invoke<TreeNode>("read_tree", { workPath }),
  readFile: (workPath: string, relPath: string) =>
    invoke<string>("read_file", { workPath, relPath }),
  writeFile: (workPath: string, relPath: string, content: string) =>
    invoke<void>("write_file", { workPath, relPath, content }),
  createFile: (workPath: string, relPath: string, content: string) =>
    invoke<void>("create_file", { workPath, relPath, content }),
  deleteFile: (workPath: string, relPath: string) =>
    invoke<void>("delete_file", { workPath, relPath }),
  restoreFile: (workPath: string, relPath: string) =>
    invoke<void>("restore_file", { workPath, relPath }),
  gitLog: (workPath: string, limit?: number) =>
    invoke<string[]>("git_log", { workPath, limit: limit ?? null }),

  // 设置
  getSettings: () => invoke<SettingsView>("get_settings"),
  listProviderPresets: () => invoke<ProviderPreset[]>("list_provider_presets"),
  upsertProvider: (
    id: string,
    name: string,
    baseUrl: string,
    apiKey: string,
    defaultModel: string,
    activate: boolean,
  ) =>
    invoke<void>("upsert_provider", {
      id,
      name,
      baseUrl,
      apiKey,
      defaultModel,
      activate,
    }),
  removeProvider: (id: string) => invoke<void>("remove_provider", { id }),
  setActiveProvider: (id: string) =>
    invoke<void>("set_active_provider", { id }),
  testProvider: (baseUrl: string, apiKey: string, model: string) =>
    invoke<string>("test_provider", { baseUrl, apiKey, model }),
  listModels: (baseUrl: string, apiKey: string) =>
    invoke<string[]>("list_models", { baseUrl, apiKey }),
  setAgentModelOverride: (agentId: string, model: string | null) =>
    invoke<void>("set_agent_model_override", { agentId, model }),

  // 流水线
  getPipeline: (workPath: string) =>
    invoke<PipelineView>("get_pipeline", { workPath }),
  pipelineEnter: (workPath: string, stage: Stage) =>
    invoke<PipelineView>("pipeline_enter", { workPath, stage }),
  pipelineConfirm: (workPath: string, stage: Stage) =>
    invoke<PipelineView>("pipeline_confirm", { workPath, stage }),
  pipelineSkip: (workPath: string, stage: Stage) =>
    invoke<PipelineView>("pipeline_skip", { workPath, stage }),
  pipelineRedo: (workPath: string, stage: Stage) =>
    invoke<PipelineView>("pipeline_redo", { workPath, stage }),

  // 设定库 + 账本
  listSettings: (workPath: string) =>
    invoke<SettingMeta[]>("list_settings", { workPath }),
  createSetting: (
    workPath: string,
    kind: string,
    name: string,
    content: string,
    hidden: boolean,
    openAtVol: number | null,
  ) =>
    invoke<SettingMeta>("create_setting", {
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
    invoke<void>("set_setting_visibility", {
      workPath,
      relPath,
      hidden,
      openAtVol,
    }),
  getLedger: (workPath: string) => invoke<Ledger>("get_ledger", { workPath }),

  // 会话
  listSessions: (workPath: string) =>
    invoke<[string, string, string][]>("list_sessions", { workPath }),
  loadSessionView: (workPath: string, sessionId: string) =>
    invoke<MessageView[]>("load_session_view", { workPath, sessionId }),
  chatStream: (
    workPath: string,
    sessionId: string,
    prompt: string,
    onEvent: Channel<AgentEvent>,
  ) => invoke<void>("chat_stream", { workPath, sessionId, prompt, onEvent }),
};
