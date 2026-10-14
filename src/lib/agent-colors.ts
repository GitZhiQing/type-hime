/**
 * 子 Agent 身份色（R9 · ZCode subagentColors 同款 8 色板）。
 * 定义处存色 key（profile.color），未定义时按名称 hash 取色；
 * 此处集中映射到 Tailwind 类，供列表色点 / 卡片徽标 / 表单色点选择复用。
 */

export const AGENT_COLOR_KEYS = [
  "yellow",
  "red",
  "orange",
  "green",
  "cyan",
  "blue",
  "purple",
  "pink",
] as const;

export type AgentColorKey = (typeof AGENT_COLOR_KEYS)[number];

/** 列表/表单色点（实心圆） */
export const AGENT_DOT: Record<AgentColorKey, string> = {
  yellow: "bg-amber-400",
  red: "bg-rose-400",
  orange: "bg-orange-400",
  green: "bg-emerald-400",
  cyan: "bg-cyan-400",
  blue: "bg-sky-400",
  purple: "bg-violet-400",
  pink: "bg-pink-400",
};

/** 文本色（名称字） */
export const AGENT_TEXT: Record<AgentColorKey, string> = {
  yellow: "text-amber-700",
  red: "text-rose-700",
  orange: "text-orange-700",
  green: "text-emerald-700",
  cyan: "text-cyan-700",
  blue: "text-sky-700",
  purple: "text-violet-700",
  pink: "text-pink-700",
};

/** 徽标底色（卡片内的 agent 名 pill） */
export const AGENT_BADGE: Record<AgentColorKey, string> = {
  yellow: "bg-amber-500/15 text-amber-800",
  red: "bg-rose-500/15 text-rose-800",
  orange: "bg-orange-500/15 text-orange-800",
  green: "bg-emerald-500/15 text-emerald-800",
  cyan: "bg-cyan-500/15 text-cyan-800",
  blue: "bg-sky-500/15 text-sky-800",
  purple: "bg-violet-500/15 text-violet-800",
  pink: "bg-pink-500/15 text-pink-800",
};

function isKey(v: string | null | undefined): v is AgentColorKey {
  return !!v && (AGENT_COLOR_KEYS as readonly string[]).includes(v);
}

/** 解析生效色：显式定义优先，否则按名称 hash（ZCode resolveSubagentColorFromName）。 */
export function resolveAgentColor(
  name: string | null | undefined,
  defined?: string | null,
): AgentColorKey {
  if (isKey(defined)) return defined;
  if (!name) return "blue";
  let h = 0;
  for (const c of name) h = (h * 31 + c.charCodeAt(0)) >>> 0;
  return AGENT_COLOR_KEYS[h % AGENT_COLOR_KEYS.length];
}
