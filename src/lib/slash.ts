/**
 * slash 命令定义（`/` 补全菜单数据源）。
 *
 * R4-⑥（REDESIGN §5.3）：slash 已是**发给 Agent 的结构化指令**——前端只负责补全与展示，
 * 不再直调命令；后端 chat_flow 解析 `/命令 参数` 生成结构化指令块（驱动 Agent 走
 * run_flow / 直连检查工具），Agent 可汇报阶段门与后续建议。原 dispatch 表（runSlash）已退役。
 */

export interface SlashDef {
  cmd: string;
  desc: string;
  /** 参数提示，如 "卷 章号" */
  args: string;
  /** 需要章号参数 */
  needsChapter: boolean;
}

export const SLASH_DEFS: SlashDef[] = [
  { cmd: "/goal", desc: "长时目标：跨多轮迭代推进，轮末自动验证（/goal 暂停·恢复·清除）", args: "目标描述", needsChapter: false },
  { cmd: "/章纲", desc: "生成本章章纲（提议）", args: "卷 章号", needsChapter: true },
  { cmd: "/正文", desc: "生成本章正文（提议）", args: "卷 章号", needsChapter: true },
  { cmd: "/回写", desc: "账本回写：从正文提取状态变化", args: "卷 章号", needsChapter: true },
  { cmd: "/一致性", desc: "一致性检查：对照账本与设定找矛盾", args: "卷 章号", needsChapter: true },
  { cmd: "/去AI味", desc: "文风约束下整章改写", args: "卷 章号", needsChapter: true },
  { cmd: "/复检", desc: "对照上轮问题清单逐项复检", args: "卷 章号", needsChapter: true },
  { cmd: "/审卷", desc: "本卷逐章一致性批审", args: "卷号", needsChapter: false },
  { cmd: "/整理账本", desc: "合并同线索/归档/压缩/补时间线", args: "", needsChapter: false },
  { cmd: "/泄漏检查", desc: "可见条目是否复述隐藏设定（零 token）", args: "", needsChapter: false },
  { cmd: "/preflight", desc: "修为层级代码预检（零 token）", args: "卷 章号", needsChapter: true },
];
