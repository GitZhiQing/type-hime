/**
 * 下一步行动计算（R17）：由流水线状态 + 资源树 + 待审提议数推出「主行动」（PLAN 条可点按钮）
 * 与「建议 chips」（输入框上方快捷入口）。全流程零 slash 可走通的数据源（REFACTOR R17：
 * 命令成为快捷方式而非必经路径）。
 * 纯函数：不触 API、不持状态；章节号由资源树扫描得出。
 */
import type { PipelineView, TreeNode } from "@/lib/tauri";

export type NextAction =
  /** 自动发送一条消息（slash 或自然语；经聊天流发出） */
  | { id: string; label: string; kind: "send"; text: string }
  /** 打开右栏审阅标签（有待审提议时） */
  | { id: string; label: string; kind: "review" };

export interface NextStepPlan {
  /** PLAN 条的下一步按钮（null = 无行动，退回纯文案提示；确认类行动由既有「确认并推进」承担） */
  primary: NextAction | null;
  /** 输入框上方建议 chips（全部可用行动，主行动在前，最多 4 个） */
  chips: NextAction[];
}

/** 资源树章节扫描结果（当前卷 = 有章节文件的最大卷号） */
interface ChapterScan {
  volume: number;
  maxChapter: number;
  /** 已有正文的最新章号（0 = 无） */
  latestText: number;
  /** 有细纲无正文的最小章号（正文待写；null = 无） */
  pendingWrite: number | null;
}

function scanChapters(tree: TreeNode | null): ChapterScan {
  const rows: { v: number; n: number; outline: boolean; text: boolean }[] = [];
  const walk = (node: TreeNode | null) => {
    if (!node) return;
    if (!node.is_dir) {
      const m = node.path.match(/^volumes\/vol-(\d+)\/chapters\/ch-(\d+)(-章纲)?\.md$/);
      if (m) {
        const v = Number(m[1]);
        const n = Number(m[2]);
        const row = rows.find((r) => r.v === v && r.n === n);
        if (row) {
          if (m[3]) row.outline = true;
          else row.text = true;
        } else {
          rows.push({ v, n, outline: !!m[3], text: !m[3] });
        }
      }
    }
    for (const c of node.children ?? []) walk(c);
  };
  walk(tree);
  if (rows.length === 0) return { volume: 1, maxChapter: 0, latestText: 0, pendingWrite: null };
  const volume = Math.max(...rows.map((r) => r.v));
  const inVol = rows.filter((r) => r.v === volume);
  const maxChapter = Math.max(...inVol.map((r) => r.n));
  const textRows = inVol.filter((r) => r.text);
  const latestText = textRows.length ? Math.max(...textRows.map((r) => r.n)) : 0;
  const pending = inVol.filter((r) => r.outline && !r.text).map((r) => r.n).sort((a, b) => a - b);
  return { volume, maxChapter, latestText, pendingWrite: pending[0] ?? null };
}

/**
 * 计算下一步行动。优先级：待审提议 > 按当前阶段的推进动作。
 * settings 有样章 / outline 等确认类场景无行动（primary=null），由 PLAN 条既有确认按钮承担。
 */
export function computeNextStep(
  pipeline: PipelineView | null,
  tree: TreeNode | null,
  pendingProposals: number,
  hasSample: boolean,
): NextStepPlan {
  const chips: NextAction[] = [];
  const push = (a: NextAction) => {
    if (!chips.some((c) => c.id === a.id)) chips.push(a);
  };
  const cur = pipeline?.gates.find((g) => g.current);
  const scan = scanChapters(tree);
  const vol = scan.volume;
  const vol2 = String(vol).padStart(2, "0");

  // 待审提议最优先：先审阅落盘，再继续流程
  if (pendingProposals > 0) {
    push({ id: "review", label: `审 ${pendingProposals} 条提议`, kind: "review" });
  }

  if (cur && !cur.confirmed) {
    switch (cur.stage) {
      case "settings":
        if (!hasSample) {
          push({ id: "kickoff", label: "开始开书访谈", kind: "send", text: "/开书" });
        }
        break;
      case "volume_outline":
        push({
          id: "vol-outline",
          label: "让助手写卷纲",
          kind: "send",
          text: `请按总纲撰写本卷卷纲（volumes/vol-${vol2}/卷纲.md，propose_edit 整文替换）：每章一句话走向与钩子衔接。`,
        });
        break;
      case "chapter_outline": {
        const n = scan.maxChapter + 1;
        push({ id: `outline-${n}`, label: `写第 ${n} 章细纲`, kind: "send", text: `/章纲 ${vol} ${n}` });
        break;
      }
      case "chapters": {
        // 正文待写优先（有细纲无正文）；否则先补下一章细纲（正文依赖细纲，阶段门不拦但纪律拦）
        if (scan.pendingWrite) {
          const n = scan.pendingWrite;
          push({ id: `write-${n}`, label: `写第 ${n} 章正文`, kind: "send", text: `/正文 ${vol} ${n}` });
        } else {
          const n = scan.maxChapter + 1;
          push({ id: `outline-${n}`, label: `写第 ${n} 章细纲`, kind: "send", text: `/章纲 ${vol} ${n}` });
        }
        if (scan.latestText > 0) {
          const m = scan.latestText;
          push({ id: `deai-${m}`, label: `第 ${m} 章去AI味精修`, kind: "send", text: `/去AI味 ${vol} ${m}` });
          push({ id: `ledger-${m}`, label: `第 ${m} 章回写账本`, kind: "send", text: `/回写 ${vol} ${m}` });
          push({ id: `consistency-${m}`, label: `第 ${m} 章一致性审校`, kind: "send", text: `/一致性 ${vol} ${m}` });
        }
        break;
      }
      case "review": {
        if (scan.latestText > 0) {
          const m = scan.latestText;
          push({ id: `consistency-${m}`, label: `第 ${m} 章一致性审校`, kind: "send", text: `/一致性 ${vol} ${m}` });
        }
        push({ id: "vol-review", label: "本卷逐章批审", kind: "send", text: `/审卷 ${vol}` });
        break;
      }
      case "outline":
      default:
        break;
    }
  }

  return { primary: chips[0] ?? null, chips: chips.slice(0, 4) };
}

/** 执行下一步行动（PLAN 条按钮用）：send 经 window 事件交 ChatPane 以聊天流发出；review 打开审阅标签。 */
export function runNextAction(a: NextAction) {
  if (a.kind === "review") {
    window.dispatchEvent(new CustomEvent("artifacts:open", { detail: { kind: "review" } }));
  } else {
    window.dispatchEvent(new CustomEvent("nextstep:send", { detail: { text: a.text } }));
  }
}
