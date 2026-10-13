import { api, EventChannel, AgentEvent } from "@/lib/tauri";

/**
 * slash 命令（R2 过渡实现：前端 dispatch 表直达既有命令，流式输出渲染进聊天流；
 * R4 切换为「发给 Agent 的结构化指令」，由 Agent 走 run_flow 编排——REDESIGN §5.3）。
 */

export interface SlashDef {
  cmd: string;
  desc: string;
  /** 参数提示，如 "卷 章号" */
  args: string;
  kind: "flow" | "quick";
  /** 需要章号参数 */
  needsChapter: boolean;
}

export const SLASH_DEFS: SlashDef[] = [
  { cmd: "/章纲", desc: "生成本章章纲（提议）", args: "卷 章号", kind: "flow", needsChapter: true },
  { cmd: "/正文", desc: "生成本章正文（提议）", args: "卷 章号", kind: "flow", needsChapter: true },
  { cmd: "/回写", desc: "账本回写：从正文提取状态变化", args: "卷 章号", kind: "flow", needsChapter: true },
  { cmd: "/一致性", desc: "一致性检查：对照账本与设定找矛盾", args: "卷 章号", kind: "flow", needsChapter: true },
  { cmd: "/去AI味", desc: "文风约束下整章改写", args: "卷 章号", kind: "flow", needsChapter: true },
  { cmd: "/复检", desc: "对照上轮问题清单逐项复检", args: "卷 章号", kind: "flow", needsChapter: true },
  { cmd: "/审卷", desc: "本卷逐章一致性批审", args: "卷号", kind: "flow", needsChapter: false },
  { cmd: "/整理账本", desc: "合并同线索/归档/压缩/补时间线", args: "", kind: "flow", needsChapter: false },
  { cmd: "/泄漏检查", desc: "可见条目是否复述隐藏设定（零 token）", args: "", kind: "quick", needsChapter: false },
  { cmd: "/preflight", desc: "修为层级代码预检（零 token）", args: "卷 章号", kind: "quick", needsChapter: true },
];

export interface ParsedSlash {
  def: SlashDef;
  volume: number;
  chapter: string;
}

/** 解析 slash 输入；非 slash 或参数缺失时返回 error 文案（中文，可直接 toast） */
export function parseSlash(input: string): ParsedSlash | { error: string } {
  const trimmed = input.trim();
  if (!trimmed.startsWith("/")) return { error: "非 slash 命令" };
  const parts = trimmed.split(/\s+/);
  const head = parts[0];
  const def =
    SLASH_DEFS.find((d) => d.cmd === head) ??
    SLASH_DEFS.find((d) => d.cmd.startsWith(head));
  if (!def) return { error: `未知命令 ${head}（可用：${SLASH_DEFS.map((d) => d.cmd).join(" ")}）` };
  const nums = (parts.slice(1).join(" ").match(/\d+/g) ?? []).map(Number);
  const volume = nums[0] ?? 1;
  const chapter =
    nums[1] !== undefined ? String(nums[1]) : nums[0] !== undefined ? String(nums[0]) : "";
  if (def.needsChapter && !chapter) {
    return { error: `${def.cmd} 需要章号，例如：${def.cmd} ${def.cmd === "/preflight" ? "1 3" : "3"}（默认第 1 卷）` };
  }
  return { def, volume, chapter };
}

/** 执行 slash：flow 走流式命令（事件进聊天流），quick 直接返回结果文本 */
export async function runSlash(
  parsed: ParsedSlash,
  workPath: string,
  channel: EventChannel<AgentEvent>,
): Promise<string | null> {
  const { def, volume, chapter } = parsed;
  switch (def.cmd) {
    case "/章纲":
      await api.generateArtifact(workPath, volume, chapter, "chapter_outline", channel);
      return null;
    case "/正文":
      await api.generateArtifact(workPath, volume, chapter, "chapter", channel);
      return null;
    case "/回写":
      await api.proposeLedgerUpdate(workPath, volume, chapter, channel);
      return null;
    case "/一致性":
      await api.reviewChapter(workPath, volume, chapter, "consistency", channel);
      return null;
    case "/去AI味":
      await api.reviewChapter(workPath, volume, chapter, "deai", channel);
      return null;
    case "/复检":
      await api.recheckChapter(workPath, volume, chapter, channel);
      return null;
    case "/审卷":
      await api.reviewVolume(workPath, volume, channel);
      return null;
    case "/整理账本":
      await api.tidyLedger(workPath, channel);
      return null;
    case "/泄漏检查": {
      const warnings = await api.checkVisibilityLeak(workPath);
      return warnings.length > 0
        ? `泄漏检查发现 ${warnings.length} 处疑似复述：\n` + warnings.join("\n")
        : "泄漏检查通过：可见条目未复述隐藏设定。";
    }
    case "/preflight": {
      const problems = await api.preflightChapter(workPath, volume, chapter);
      return problems.length > 0
        ? `preflight 发现 ${problems.length} 处修为层级疑点：\n` + problems.join("\n")
        : "preflight 通过：未发现修为层级矛盾。";
    }
    default:
      return `未实现的命令：${def.cmd}`;
  }
}
