---
created: 2026-10-14
status: 已确认 · 2026-10-14
note: 产品重设计——从「工作流工具箱」到「Agent 驱动的写作工作台」。工程执行见 REFACTOR.md（R1-R6，渐进绞杀）；SPEC.md 决策表与 DECISIONS.md（D19-D25）已按第 14.3 节同步修订。本文不改变任何现行红线：提议审阅落盘、可见性出口过滤、key 不出 Rust 侧。
---

# REDESIGN · Agent 驱动的写作工作台

> 对齐输入（2026-10-14，均已在对话中拍板）：
> ① 细枝末节操作交给 Agent（工具 + 技能 + 查询），UI 只展示必要工作流；
> ② 系统形态对齐 Coding Agent IDE / 办公 Agent 产品：**一个完整的 Agent 是核心，制品（artifact）展示提升体验**；
> ③ GeneratePane 8 个按钮**全部聊天化**，面板退役；
> ④ 阶段进度做成聊天输入框上方的**可折叠 GOAL/PLAN 式 UI**；
> ⑤ 上下文工程：可查看、可组装、无硬 token 上限、窗口用量提示 + 新会话提醒；
> ⑥ 真会话管理（对标 Coding Agent 的会话是基本功）；
> ⑦ 前端用 shadcn 生态的 AI Elements 组件构建 AI 界面；
> ⑧ 小说 Agent ≈ Spec Coding，可借鉴；
> ⑨ bash 工具：白名单 + 审阅，文件编辑必须走 propose_* 工具，支持用户确认后的自主模式（对标 Codex goal / Claude Code auto）。

## 1. 背景与问题

### 1.1 现状数据（2026-10-14 代码摸底）

- Workbench 共 **50+ 个独立可操作入口、7 个对话框、3 处下拉/右键菜单、5 个 Tab 页签**，全部挤在三栏布局里。
- GeneratePane 的 8 个按钮（章纲/正文/账本回写/一致性检查/去AI味/复检/审本卷/账本整理）中，只有章纲、正文（勉强加回写）算创作主线，其余 5 个是典型**单点工具型操作**——参数只有卷号+章号，天然适合聊天指令化。
- 聊天只是中栏一个 Tab（与「生成」互斥），**没有 slash 命令、没有技能可见性、没有素材引用**；提议发起在中间栏、审阅在右栏，用户必须理解三栏联动模型。
- **会话管理近乎没有**：`.sessions/` 已持久化，但 UI 只有 pill 切换，无标题、无搜索、无重命名、无删除。
- **上下文是黑盒**：装配内容藏在右栏「装配」Tab 里；预算超限走硬拦 + 自动降档（D11），用户既看不清进了什么，也关不掉降档。
- 后端能力与 UI 面严重倒挂：全部业务 flow 已集中在 `flows.rs`（回调参数化、Tauri/Web 双通道对等），提议→diff→落盘红线闭环现成；**唯一结构性缺口是 chat Agent 只有 3 个文件工具（read_file / propose_edit / propose_write），与 flow 层之间没有工具桥，4 个内置技能也没挂到 chat spec**。

### 1.2 设计哲学转变

| | 旧范式 | 新范式 |
| --- | --- | --- |
| 心智模型 | 工作流驱动的工具箱：每个能力做成一个按钮，用户按流水线找按钮 | Agent 驱动的工作台：用户对一个完整 Agent 说意图，Agent 编排能力 |
| 约束观 | 阶段门、预算硬拦、按钮入口 = 约束本身 | **适度的工作流限制**（后台护栏 + 明确的人在环节点）+ **充分的 Agent 能动性**（工具/技能/查询自由组合） |
| UI 职责 | 承载全部操作 | 只展示必要工作流；其余交给聊天 + 制品展示 |

一句话：**Agent（一个真正的、完整的 Agent）是核心，Artifact 展示是体验**。

## 2. 竞品调研

### 2.1 Coding Agent（交互范式母版）

| 产品 | 形态 | 对本项目的关键借鉴 |
| --- | --- | --- |
| **Cursor** | IDE 内 agent chat + **可视化 diff 逐处接受/拒绝** + Plan 模式（先计划、人批准、后执行）+ 规则文件约束 agent | diff 审阅是 agent IDE 的信任基石（本项目已有，需内联到聊天）；Plan 模式 = 阶段 PLAN 条的原型 |
| **Claude Code** | 终端 agent；**slash 命令 + skills（用户可见可编辑的 md，项目级可覆盖）+ 权限模式**（默认逐条确认 / accept-edits / auto）；`/compact` 上下文压缩、`/resume` 会话恢复 | slash + skills 的用户可见性；权限分档；会话是基本功 |
| **Codex CLI** | 权限三档：suggest（只建议）/ auto-edit（自动改）/ full access（完全自主） | 「用户确认后进入自主模式」的档位设计 |
| **PI Agent**（badlogic/pi-mono，用户点名重点） | TypeScript monorepo 7 包：`pi-ai`（统一多供应商 LLM API）/ `pi-agent-core`（约 418 行 agent loop + 状态管理）/ `pi-coding-agent`（CLI）/ `pi-tui` / **`pi-durable`（持久会话/任务/文档运行时）** / `pi-chord`（插件宿主：热加载、RPC）/ `pi-telemetry`。扩展体系四件套：**extensions / skills / prompt templates / themes，可打包分发**（npm/git）。无内置权限系统（靠外部沙箱：Gondolin 微核 VM / Docker / OpenShell）。`!` 前缀直接执行 bash | ① 极简 harness 哲学："Adapt Pi to your workflows, not the other way around"——本项目 `agent/` 内核（AgentSpec 声明式 + 工具注册表 + skills 加载器）同构，方向正确；② durable 会话是一等公民；③ 技能/提示模板是用户可见、可打包分享的对象（本项目作品级 `skills/*.md` 覆盖机制已有雏形） |
| OpenCode / Crush 等 | 开源 TUI coding agent，多 provider、会话管理、上下文压缩 | 佐证「会话列表 + 多会话并行」是此类产品标配 |

### 2.2 AI 界面组件：Vercel AI Elements

[elements.ai-sdk.dev](https://elements.ai-sdk.dev) / [vercel/ai-elements](https://github.com/vercel/ai-elements)，shadcn 式源码复制组件，含：

- 聊天类：**Conversation / Message（含 MessageBranch 分支） / Prompt Input（附件、工具按钮） / Tool（工具调用状态与结果） / Reasoning / Task（任务折叠，含涉及文件） / Plan（计划列表） / Queue / Confirmation（敏感操作确认） / Context（上下文展示与编辑） / Checkpoint（对话检查点） / Suggestion / Model Selector**
- 代码类：**Agent / Artifact / Code Block / File Tree / Terminal（流式输出） / Stack Trace / Test Results / Commit（git 提交展示）** / Web Preview / Sandbox 等

对本项目的意义：聊天流、工具轨迹、子任务、PLAN 条、权限确认、上下文面板、检查点、制品、git 展示**全部有现成组件**，前端主要自研点只剩 slash/`@` 补全与中文文稿渲染。详见第 12 节映射表。

### 2.3 Spec-Driven Development（规格驱动开发）

- **Kiro**（AWS）：Requirements → Design → Tasks 三阶段流程，EARS 风格需求模板。
- **GitHub Spec Kit**：`/specify → /plan → /tasks → /implement` 命令链，厂商无关，spec 存放在代码库中。
- **争议**：① 当前实现被批评为「只是把 markdown 交给 agent」；② spec 演化难题——声称 spec 是唯一真相源，工具却引导不断新建 spec 而非演进旧 spec；③ 本质是文档先行的老概念在新 AI 语境复兴。
- **对小说领域的判断**：代码领域争议根源在于「代码有编译器/测试做客观反馈，spec 层是多余的中间态」；**小说恰好相反——正文没有编译器，设定/大纲/账本就是唯一的约束层**，Spec Coding 的「文档先行」在小说领域不是可选项而是本质形态。本项目六阶段流水线已经是一条 spec 链（详见第 4 节映射）。

### 2.4 写作赛道（2026-10）

| 产品 | 定位与特点 | 与本项目关系 |
| --- | --- | --- |
| 蛙蛙写作 | 投稿生态最全（小说投稿/签约/拆书/转剧本），AI 功能全 | 生态型平台，非工作台；不构成直接竞争 |
| 笔灵 AI | 通用写作，主打 AI 全篇生成 | 表单式生成，无一致性工程 |
| 彩云小梦 | 「你写一段、AI 续一段」续写体验 | 无项目级状态管理 |
| 马良写作 | **7 个专业 Agent 编辑部分工 + 三级大纲 + 知识图谱**，主打百万字不吃书不烂尾，有开源版 | **最近似竞品**：多 Agent 分工思路同源；但仍是表单+按钮形态，无 diff 审阅、无用户可见技能体系 |
| 蛙趣拼文 | 分层大纲解决主线断裂 | 单点方案 |
| Sudowrite | Story Bible 自动化、打包积分、上手简单 | 「简单」路线的代表 |
| NovelCrafter | Codex 手动精细控制、BYO API key | 「控制权」路线的代表；验证了 power user 愿意为控制权买单 |

**格局结论**：写作工具全部停留在「表单 + 按钮 + 多面板」形态；「聊天驱动 + diff 审阅 + slash/skills + 权限模式」的 Coding Agent IDE 形态在写作领域是空白。本项目的差异化定位：**本地优先 + BYO key + 可见性红线的「写作 Coding Agent IDE」**，用户画像（自己，power user 网文作者）天然属于控制权路线。

## 3. 产品重定位

**一句话定位**：本地优先、BYO key、以一个完整 Agent 为核心的中文网文长篇创作工作台——写作者的 Coding Agent IDE。

三条设计原则：

1. **Agent 是核心**：用户面对**一个**主 Agent，它能编排一切操作——创作主线（章纲/正文/回写）与单点工具（审校/整理/检查）统一经由聊天发起。内部可以有专职子 agent（writer/reviewer/ledger spec 保留），但对用户透明，以「子任务」形式呈现在聊天流里。
2. **制品是中心**：一切 AI 产出与项目状态以制品视图呈现——文稿、diff、账本、设定、问题清单、装配上下文、成本、git 历史、技能文档。制品可查看、可引用、可注入上下文。
3. **人在环节点最小且明确**：默认只有四类确认点——**提议接受/拒绝、问题裁决、阶段确认、可见性设置**。四类之外全是 Agent 自主区；用户显式授权后可进一步收缩为自主模式（见 8.3）。

价值主张排序变化：旧「流程保证不烂尾 > 一致性工程 > diff 审阅」是护栏本位；新「**Agent 能动性 > 制品可观测 > 工作流护栏**」——护栏从主操作面降为背景约束 + 确认点，但护栏本身（红线）一条不撤。

## 4. 小说即规格（Spec-Coding 映射）

| Spec Coding 概念 | 代码领域 | 本项目对应 | 现状 |
| --- | --- | --- | --- |
| Requirements（需求） | EARS 需求文档 | **设定库 + 总纲**：世界观/人物/悬念（含可见性） | 已有（lore + 可见性红线） |
| Design（设计） | 架构文档 | **卷纲**：本卷结构、卷内节奏 | 已有 |
| Tasks（任务清单） | 可执行任务分解 | **章纲**：本章的承接/推进/钩子 | 已有 |
| Implementation | 代码 | **正文** | 已有 |
| Tests | 测试套件 | **审校**：一致性检查（含零 token preflight）+ 去 AI 味 | 已有 |
| Re-run tests | 回归测试 | **复检**：对照上轮问题清单逐项核验 | 已有（issues.json 裁决档案） |
| Runtime state | 程序运行时状态 | **账本**：角色状态/事实/伏笔/时间线，随正文推进回写演化 | 已有（分层摘要注入） |
| Spec 演化 | 争议焦点：spec 要演进不要新建 | **账本回写 + 阶段可重做（redo）**：推翻不删除只标注「已被第 N 章推翻」 | 已有——本项目天然免疫 SDD 的 spec 演化难题 |
| 文档先行 | 被质疑多余 | 正文没有编译器，**spec 链是小说唯一的约束层** | 本质形态，无需论证 |

SDD 三条争议教训的对策（均已内建）：spec 可演化 → 账本回写/阶段 redo；不过度形式化 → md + frontmatter 即规格，不引入专用格式语言；spec 是唯一真相源 → 「账本是唯一事实源，正文错为默认」已是审校技能的第一规程。

**流水线的角色重定义**：六阶段不再是 UI 导航结构（StageBar），而是 spec 链的推进状态——以 PLAN 条形式常驻输入框上方（5.4），推进由 Agent 发起建议、人确认。

## 5. 新信息架构

### 5.1 应用骨架：最左 rail

```
┌──┬────────────────────────────────────────────────┐
│对│                                                │
│话│           当前页面内容（对话页/项目页/配置页）      │
│创│                                                │
│作│                                                │
│配│                                                │
│置│                                                │
└──┴────────────────────────────────────────────────┘
```

| Rail 入口 | 页面 | 说明 |
| --- | --- | --- |
| **对话** | 会话列表 + chat（两栏） | 无项目上下文的纯 AI 对话：找灵感、头脑风暴、问写作问题。会话存 app 级 |
| **创作** | 项目库 → 项目页 | 创建或选择一个项目进入（**一本小说 = 一个项目**）；项目库即现 Launcher |
| **配置** | 供应商/模型管理 | 现 Settings 页平移 |

### 5.2 项目页（核心页面）三栏

```
┌──┬─────────┬───────────────────────┬──────────────────────┐
│rail│ 会话列表  │        AI chat         │   制品面板（可折叠）      │
│    │ (本项目) │  ┌─────────────────┐  │  VS Code 式多标签组：    │
│    │  +新建   │  │ 消息流(含制品卡片) │  │  [第3章.md][审阅(2)][账本]│
│    │         │  ├─────────────────┤  │  [问题清单][装配][成本]…  │
│    │         │  │ ▸ PLAN 条(可折叠) │  │   可添加/关闭/分组切换    │
│    │         │  ├─────────────────┤  │                        │
│    │         │  │ 输入框 /技能 @素材 │  │                        │
┌──┴─────────┴──┴─────────────────┴──┴──────────────────────┘
```

- **左：会话列表**——本项目全部会话，新建/重命名/删除/搜索，标题自动生成（7）。
- **中：AI chat**——唯一主操作面（宽度主体）。
- **右：制品面板**——可整体折叠；内部是 **VS Code 式多标签组**：可同时打开多个制品标签、灵活切换与添加（用户原话「像 vscode 文本编辑器那种多个窗口同时存在」）。标签类型见 5.5。

### 5.3 聊天输入框

- **`/` 技能调用**：弹出补全菜单（技能名 + 说明 + 参数提示），如 `/正文 卷1 章3`、`/一致性检查 第3章`、`/整理账本`。slash 是**发给 Agent 的结构化指令**，不是前端直调命令——保留 Agent 的判断与阶段门汇报（对比：前端直调更快但退回按钮心智，不做；快路径后置再议）。
- **`@` 素材引用**：`@第3章`、`@林晚`（角色）、`@账本`、`@问题清单`——把制品显式注入本轮上下文（写进会话消息，Agent 可见）。
- **上方可折叠 PLAN 条**：见 5.4。

### 5.4 PLAN 条（GOAL/PLAN 式阶段 UI）

- 常驻输入框上方、默认折叠为单行摘要（如 `▸ 正文阶段 · 第 3 章 · 全书 4.2 万字`）；展开显示六阶段进度点、当前阶段、**下一步建议动作**（由 Agent/流水线状态生成，如「第 3 章已确认，建议进入正文」）。
- 推进/确认（pipeline_confirm）在条上完成——**Agent 发起建议、人点确认**；skip / redo 收进「…」菜单（危险操作永远留人）。
- 组件形态对应 AI Elements 的 **Plan** 组件；条内预留其他内容槽位（当前子任务进度、成本速览等）。

### 5.5 制品类型枚举

| 制品 | 内容 | 交互 |
| --- | --- | --- |
| 文稿编辑器 | 任意 md（设定/大纲/章纲/正文） | 编辑+保存（人类直写通道保留）；设定文件附可见性控件（红线） |
| 审阅 | 提议 diff 队列 | 接受/拒绝/拒绝带评论（9） |
| 问题清单 | issues.json 结构化视图 | 逐条裁决（fixed/dismissed）、复检档案 |
| 账本 | 账本结构化视图（角色/事实/伏笔/时间线分节） | 只读浏览 +（后置）条目级操作 |
| 装配检查器 | 上下文装配预览（现 AssemblyPane） | 降频入口，与上下文面板（6）互补 |
| 成本 | token 用量汇总 | 只读 |
| git 历史 | 提交列表 + 单提交 diff | 只读 |
| 技能文档 | 内置/作品级技能 md | 查看/编辑（作品级覆盖已有） |

## 6. 上下文工程（三层模型）

原则：**可查看、可组装、无硬上限、Agent 可自主扩充**。现代模型普遍百万级上下文，产品的职责是让用户看清「这次对话里有什么」，并在合适时机建议换新会话——而不是替用户做硬裁剪。

| 层 | 内容 | 规则 |
| --- | --- | --- |
| **会话上下文** | 聊天历史 + `@` 注入的素材 | **无硬 token 上限**；输入框旁常驻**窗口用量指示**（已用/模型窗口，估算口径沿用 D9）；接近上限（如 >80%）时提示「建议新建会话重启上下文」，不强制；会话压缩（compaction）后置为可选项 |
| **装配上下文** | 写作任务注入的项目素材（系统提示词/卷纲/章纲/角色卡/账本摘要/上一章结尾） | 档位（Full/NameOnly/Compact）、出场过滤、账本分层（D11/D12）**保留但性质从「硬约束」变为「默认质量策略」**：超预算只软提示不拦截、不自动降档，用户可在**上下文面板**里查看本轮装配明细、临时增删条目、一键切全量。**可见性过滤仍是出口强制红线，不在用户可关范围** |
| **工具上下文** | Agent 按需拉取 | read_file / get_ledger / list_issues / list_lore / bash 只读命令——Agent 在对话中自主决定「再看点什么」 |

**上下文面板**（新）：AI Elements **Context** 组件；从 PLAN 条或装配检查器进入，展示当前会话上下文构成（历史 token + @注入 + 装配明细），可临时开关条目（仅影响后续装配）。

与现状的变更点：D9 估算保留（用于显示）；D11「降后仍超才拦截」改为「只提示」；D0-7「token 预算条 + 超限飘红」从拦截语义转为显示语义。

## 7. 会话管理

对标 Coding Agent：会话是此类产品的基本功，写作同样成立——「写第 3 章」和「重构这个模块」一样是有边界的任务单元。

- **现状**：`.sessions/*.json` 已持久化（rig Message 原生 serde）、pill 切换可用；缺标题、搜索、重命名、删除、摘要。
- **新形态**：
  - 项目页左侧**会话列表**：新建、切换（= 恢复完整上下文，对应 Claude Code `/resume`）、重命名、删除、按标题/内容搜索。
  - **自动标题**：首轮对话结束后用模型生成一句短标题（廉价单次调用；失败退回首句截断）。
  - **全局对话**（rail「对话」页）与项目会话分层存储：前者无作品上下文、app 级；后者绑定作品。
  - **Checkpoint**（AI Elements 组件）：接受提议、阶段确认等关键落点自动打点，会话列表项可展示；回滚语义后置（git 已兜底，先做展示）。
- **会话粒度建议**（开放问题，见 14.2）：默认按「阶段/章」开新会话，PLAN 条在合适时机提示——既控制窗口用量，也天然对应「一章一任务」。

## 8. Agent 能力与权限模型

### 8.1 工具桥（chat Agent 工具集从 3 个扩展为四类）

| 类别 | 工具 | 说明 |
| --- | --- | --- |
| 文件（已有） | read_file / propose_edit / propose_write | 红线通道不变；**一切作品 md 产物的修改必须走这两个 propose 工具** |
| **bash（新）** | bash(command) | **白名单机制**：仅允许有限命令——git 只读（status/log/diff/show）、零 token 检查 CLI、用户配置的 CLI 工具、只读管道组合。**逐条审阅**（Confirmation 组件，见 8.3 档位）；含写产物模式（重定向、sed -i、编辑器等）的命令直接拦截并提示「文件编辑请使用 propose_edit/propose_write」 |
| **flow 编排（新）** | run_flow(kind, volume?, chapter?) | 触发既有 flows.rs 流程：章纲/正文/回写/一致性/去AI味/复检/审卷/整理账本。**必须走 flow 而非让 Agent 手工模拟**——保住 issues.json 落盘、preflight 注入、装配注入等副作用闭环。触发前补阶段门校验（现状 flows 不查门，门只拦 UI 导航）。子任务事件流（8.4）回流聊天 |
| **零 token 检查（新）** | check_visibility_leak() / preflight_chapter(volume, chapter) | 纯代码检查的直连薄包装（现六项操作中工程量最小的缺口） |
| **结构化查询（新）** | get_ledger() / list_issues() / list_lore(filter?) | 替代 read_file 读 json/md 的绕路，给模型结构化语义；全部只读 |

### 8.2 技能体系

- 4 个内置技能（writer-basics/ledger-update/review-consistency/ledger-tidy）从「对用户不可见」转为一等对象：技能 chips 展示在输入框旁、可点开查看/编辑（制品面板「技能文档」标签）、`/` 补全菜单即技能入口。
- 挂载策略：chat spec 默认挂 writer-basics + 按需动态挂载（slash 指定技能 = 强制挂载该技能；自然语言触达靠 whenToUse 关键词路由——实现从简：关键词命中即挂，未命中不挂）。
- 作品级覆盖（`works/<作品>/skills/*.md` 同名覆盖）已有，保留；PI 式技能包分发后置。

### 8.3 权限三档（对标 Claude Code 权限模式 / Codex 档位）

| 档位 | 提议（propose_*） | bash | 适用 |
| --- | --- | --- | --- |
| **审阅模式**（默认） | 逐条人工接受/拒绝 | 白名单内逐条确认执行 | 日常默认；等价现红线语义 |
| **自主模式** | 自动应用（现「账本回写自动接受」开关的推广：可按提议类型细分为仅账本自动） | 白名单内直通，不再逐条确认 | 用户显式开启（会话级开关，顶栏徽标显示当前档位）；高危命令（写产物类）仍拦截 |
| **完全自主（goal）** | 全自动应用 + 事后汇总汇报 | 同自主模式 | 一次确认后放手跑批量任务（如「把第一卷 10 章全部审校一遍」）；会话结束或手动收回 |

**永远留人（任何档位不可自动）**：可见性设置（隐藏/开放卷）、删除/恢复文件、跳过阶段（skip）、装配档位切全量。这四点直接关联红线与不可逆操作。

### 8.4 子任务事件流

run_flow 触发的 flow 是长流式任务，工具调用是单次同步返回——**AgentEvent 扩展子任务事件**（如 `SubTaskStart{label}` / `SubTaskDelta{...}` / `SubTaskEnd{summary}`），前端在聊天流里渲染为可展开的子任务卡片（进度 + 最终摘要，展开看工具轨迹），对应 AI Elements Task/Terminal 组件。备选降级：工具只返回最终摘要（实现简单但无过程感），路线图 R4 先做事件版，不行再降级。

### 8.5 单一主 Agent 的技术形态

用户只见一个主 Agent；writer/reviewer/ledger 等专职 spec 保留为**内部子 agent**，由 run_flow 在后台调度，成果以子任务卡片 + 提议回流。这与现状架构（flows.rs 按需建会话）一致，无需推翻——变的是入口统一到聊天。

## 9. diff 审阅交互

- **内联提议卡片**：propose_* 产生的提议在聊天流里就地渲染（发起消息下方）——diff 预览（复用现 unified diff 渲染与上下文折叠）+ **接受 / 拒绝** 按钮；与右栏「审阅」标签同源同队列（同一 proposals.json），两处操作实时同步。
- **拒绝带评论**：拒绝时可选填一句评论（如「语气太现代，重写这段」），评论作为 tool_result 回流 Agent 上下文，驱动其修改后重新提议——这是把「拒绝」从终点变成反馈回路的关键交互。
- 「账本回写自动接受」开关并入权限档位（自主模式的细分配置），不再作为孤立开关。

## 10. git 自动化

- **关键环节自动 commit**（已有 + 补齐）：接受提议 = 写盘 + commit（已有）；**阶段确认（pipeline_confirm）后自动 commit 里程碑**（新增，如 `阶段确认：正文 ch-003`）；文件删除走 git rm 留痕（已有）。
- **用户让 Agent 做 git 操作**：自然语言 → Agent 走 bash 白名单（status/log/diff/show 直通或按档位确认）；diff 展示用制品面板「git 历史」标签。
- **不做**：push/远端（本地优先定位，D0-1）；改写历史类命令（reset/rebase）不进白名单——回滚走「重做阶段」与应用层通道。

## 11. UI 简化映射表（50+ 入口 → 约 15）

| 现有入口 | 去向 |
| --- | --- |
| GeneratePane 8 按钮（章纲/正文/回写/一致性/去AI味/复检/审本卷/账本整理） | **全部聊天化**：slash（`/章纲 /正文 /回写 /一致性 /去AI味 /复检 /审卷 /整理账本`）+ 自然语言，Agent 走 run_flow；**面板退役** |
| 问题清单（GeneratePane 内嵌）+ 逐条裁决按钮 | 制品面板「问题清单」标签 + 聊天内联问题卡片（裁决按钮同源） |
| StageBar（6 chip + 确认 + 菜单 3 操作） | **PLAN 条**（折叠摘要 + 确认按钮 + 菜单收纳 skip/redo） |
| DiffPane（Tab + 接受/拒绝 + 自动接受开关） | 制品面板「审阅」标签 + **聊天内联提议卡片**；自动接受并入权限档位 |
| ChatPane 会话 pill + 新会话 | 左侧**会话列表**（7） |
| EditorPane（编辑/保存/新建/可见性/泄漏检查） | 制品面板「文稿编辑器」；保存与可见性控件**保留**（红线）；泄漏检查 → 聊天（check_leak 工具/slash） |
| AssemblyPane | 拆两处：**上下文面板**（常用：查看/增删/全量）+ 装配检查器（完整预览，降频标签） |
| Workbench 顶栏（刷新/用量徽标/设置） | 刷新自动化（事件驱动）+ 用量徽标进 rail/顶栏 + 设置进 rail「配置」 |
| Launcher（作品库/新建） | rail「创作」→ 项目库 |
| Settings 全部 | rail「配置」平移 |

**保留的显式 UI 操作清单（目标 ~15）**：rail 三入口；新建项目；会话新建/重命名/删除；PLAN 条确认；提议接受/拒绝；问题裁决（fixed/dismissed）；编辑器保存；可见性开关 + 开放卷；上下文面板增删；制品标签管理。其余一切经聊天。

## 12. AI Elements 组件映射表

| 界面块 | AI Elements 组件 | 备注 |
| --- | --- | --- |
| 聊天对话区 | Conversation + Message | 流式气泡、回到底部 |
| 输入框 | Prompt Input | **自研**：`/` 技能补全、`@` 素材补全 |
| 工具调用轨迹 | Tool | 替代现「🔧 调用：xxx」小字行 |
| 子任务（run_flow/bash） | Task + Terminal | 子任务折叠卡 + bash 流式输出 |
| PLAN 条 | Plan | 六阶段进度 + 下一步建议 |
| 权限审阅（bash 确认） | Confirmation | 档位感知 |
| 上下文面板 | Context | 上下文明细 + 增删 |
| 会话检查点 | Checkpoint | 7 节 |
| 制品面板 | Artifact / File Tree / Code Block / Test Results（问题清单）/ Commit（git） | 文稿渲染（中文 md）自研轻封装 |
| 消息分支（重生成） | MessageBranch | 后置 |

**技术风险提示**：AI Elements 基于 radix 基座的 shadcn 组件，本项目基座是 **base-ui**（无 asChild、用 render prop）。引入方式按 shadcn 惯例**源码复制**而非包依赖，复制时需做基座适配（主要是 Dialog/Dropdown/Popover 等交互原语），R2 第一步先以 1-2 个组件（Tool + Prompt Input）验证兼容性，可行再铺开；不可行则退化为按 AI Elements 的结构自研同构组件。

## 13. 路线图（每片可验收，一片一提交）

依赖：R1 → R2 → {R3, R4 可并行} → R5 → R6。R5（按钮退役）必须晚于 R4（聊天已具备全部能力）。

| # | 切片 | 内容 | 验收判据 |
| --- | --- | --- | --- |
| R1 | 应用骨架 + 真会话 | rail 三入口；对话页（无项目上下文）；项目页三栏骨架；会话列表（标题/重命名/删除/搜索/自动标题）；全局会话与项目会话分层存储 | 从 rail 进对话页可多会话管理；进项目页会话列表可用；旧 Launcher/Settings 功能不回归 |
| R2 | 聊天升级 | AI Elements 迁移（先 Tool/Prompt Input 验证基座兼容，再 Conversation/Message/Task/Confirmation）；`/` 技能补全；`@` 素材引用；内联提议卡片（接受/拒绝/**拒绝带评论**）；PLAN 条（读 pipeline 状态、确认/菜单） | 浏览器测试通道内：`/一致性 第3章` 走通 run_flow 前置的事件链路可用版（可先直达命令）；拒绝带评论可驱动 Agent 修改重提；PLAN 条折叠/展开/确认可用 |
| R3 | 上下文工程 | 上下文面板（查看/临时增删/全量切换）；窗口用量指示（D9 估算）；接近上限新会话提醒；装配预算硬拦改软提示（D11 语义调整） | 面板改动反映到下次装配预览；超预算仅提示不拦截；可见性出口过滤回归用例仍绿 |
| R4 | Agent 能力 + 权限 | bash 工具（白名单 + 逐条审阅 + 写产物拦截）；run_flow 编排工具 + 子任务事件（AgentEvent 扩展）+ 阶段门校验；check_leak/preflight 直连；get_ledger/list_issues/list_lore；技能挂载 chat spec + 动态选择；权限三档 | 聊天内可完成原 8 按钮全部能力且 issues/账本副作用闭环不破；`echo > xxx.md` 类命令被拦并引导 propose_*；自主模式下白名单直通、四类留人操作仍需确认 |
| R5 | 制品面板 + 按钮退役 | GeneratePane 移除；制品面板 VS Code 式多标签（文稿/审阅/问题/账本/装配/成本/git/技能）；账本与问题清单结构化视图；StageBar 移除 | 全流程（建作品→设定→总纲→卷纲→章纲→正文→回写→审校→复检）仅凭 聊天+PLAN 条+制品面板 完成；显式 UI 入口 ≤ 第 11 节清单 |
| R6 | git 自动化 + 收尾 | 阶段确认自动 commit；bash git 白名单；SPEC.md/DECISIONS.md 修订（第 14 节清单）；AGENTS.md 更新；回归脚本补齐 | cargo test/构建绿；`regression` 示例通过；文档与现状一致 |

## 14. 风险、开放问题与文档修订预告

### 14.1 风险

| 风险 | 应对 |
| --- | --- |
| 子任务事件流复杂度（flow 长流式 vs 工具单次返回） | R4 先做 SubTask 事件版；不行降级为「工具返回最终摘要 + 卡片内 Terminal 回放」 |
| AI Elements 与 base-ui 基座兼容性 | R2 首步以 2 个组件试点；源码复制改造，不引包依赖；兜底自研同构 |
| bash 白名单绕过面（管道、间接写、CLI 工具自身写文件） | 白名单保守起步（git 只读 + 显式登记的 CLI）；写产物模式黑名单拦截；完全自主模式前弹一次性风险确认 |
| 长会话吃窗口 | 默认按阶段/章开新会话（PLAN 条引导）+ 用量指示；compaction 后置 |
| 自主模式与红线边界 | 四类操作永远留人写死在工具层（不是 UI 层约定），权限档位不覆盖 |

### 14.2 开放问题（实施中决策，先给默认）

1. 会话粒度默认值：按章 / 按阶段 / 手动（默认：PLAN 条在章完成+回写后建议开新会话）。
2. 自主模式的细分配置粒度：仅账本自动 / 按提议类型多选（默认：先二档，即现开关的推广）。
3. 技能动态挂载的触发方式：slash 强制 + whenToUse 关键词路由（默认）；检索式语义匹配后置。
4. 全局「对话」页是否允许挂临时上下文（如粘贴设定片段）（默认：允许粘贴，不挂作品文件）。

### 14.3 SPEC / DECISIONS 修订预告（本文确认后执行）

| 条目 | 修订方向 |
| --- | --- |
| SPEC D0-4 工作流形态 | 「固定流水线」保留为 spec 链后台约束；UI 形态改为 PLAN 条；操作面聊天化 |
| SPEC D0-5 约束与自由度 | 「自由旁路」升级为一等形态：聊天即主操作面，受控主线以确认点形式存在 |
| SPEC D0-6 / DECISIONS D6 | 红线精神不变、表述演化：「AI 不静默改稿」= 默认审阅模式；自主模式为用户显式授权的例外；bash 永不绕过 propose_* |
| SPEC D0-7 / DECISIONS D11 | 装配预算从硬拦+自动降档改为默认质量策略 + 上下文面板 + 软提示；可见性过滤仍为出口强制红线 |
| 新增 DECISIONS（D19 起） | 权限三档；bash 白名单机制；上下文三层模型；AI Elements 源码复制引入；会话管理形态 |

## 调研来源

- PI Agent：[pi.dev](https://pi.dev) / [badlogic/pi-mono](https://github.com/badlogic/pi-mono) / [Pi — Anatomy of a Minimal Coding Agent Powering OpenClaw (Medium)](https://medium.com) / [lucumr.pocoo.org：Pi: The Minimal Agent Within OpenClaw](https://lucumr.pocoo.org)
- AI Elements：[elements.ai-sdk.dev](https://elements.ai-sdk.dev) / [vercel/ai-elements](https://github.com/vercel/ai-elements) / [shadcn/ui AI SDK helper](https://ui.shadcn.com/docs/helpers/ai-sdk)
- Spec-Driven Development：[Martin Fowler：Understanding SDD — Kiro, spec-kit, Tessl](https://martinfowler.com/articles/exploring-gen-ai/sdd-3-tools.html)
- Coding Agent 范式：[Cursor：使用智能体编码的最佳实践](https://cursor.com/cn/blog/agent-best-practices) / [Claude Code 文档](https://code.claude.com/docs/zh-CN/overview) / [systemprompt.io：Claude Code vs Cursor](https://systemprompt.io/guides/claude-code-vs-cursor) / [Builder.io：Cursor vs Claude Code 2026](https://www.builder.io/blog/cursor-vs-claude-code) / [Anthropic：Building Effective Agents](https://www.anthropic.com/engineering/building-effective-agents)
- Artifacts 形态：[ChatGPT Canvas vs Claude Artifacts (Reddit)](https://www.reddit.com/r/ClaudeAI/comments/1fvydtc/chatgpt_canvas_vs_claude_artifacts_for_coding) / [MindStudio：On-Demand Generative UI vs Canvas/Artifacts](https://www.mindstudio.ai/blog/claude-on-demand-generative-ui-vs-canvas-artifacts)
- 写作赛道：[蛙蛙写作](https://www.wawawriter.com) / [马良写作](https://maliangwriter.com) / [彩云小梦](https://www.xiaomengai.com) / [笔灵 AI（MS Store）](https://apps.microsoft.com/detail/xp8c6z9rrmwbwm) / [知乎：10 款 AI 写小说工具横评](https://zhuanlan.zhihu.com/p/2079213193512096372) / [CSDN：2026 AI 写作工具测评](https://bbs.csdn.net/weixin_29061509/article/details/100194364) / [NovelCrafter vs Sudowrite 对比](https://novelcanon.com/blog/sudowrite-vs-novelcrafter) / [Best AI Novel Writer 2026](https://www.inkfluenceai.com/best-ai-novel-writer-2026)
- 本项目代码摸底：Workbench UI 全量盘点（50+ 入口映射）与 agent 内核能力评估（工具桥缺口分析），2026-10-14 会话内完成
