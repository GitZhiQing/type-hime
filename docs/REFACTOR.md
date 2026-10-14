---
created: 2026-10-14
status: 已确认 · 2026-10-14
note: 基于 REDESIGN.md（已确认）与社区 Rust Agent 调研（2026-10-14）的工程执行计划。每片独立可验收、一片一提交；顺序 R1 → R2 → {R3、R4 可并行} → R4.5 → R5 → R6；R7 为后置可选路线（触发条件表）。进展（2026-10-14）：R1/R2/R3 已完成（另含多轮 UI 打磨：Markdown 渲染、工具卡结构化、消息形态、PLAN 条与上下文面板重设计）；**R4 已完成**（8 提交：双 spike + 权限两档×覆盖 + 工具桥四类 + 技能挂载 + slash 指令化，逐项浏览器验收；落点与教训见 D33）；**R4.5 已完成**（5 提交：提示词 spike 20/20 → 状态机 → 轮末验证器迭代循环 → 前端目标横幅 → 浏览器验收——批量目标"第一卷逐章一致性检查"跑通至 complete（验证器引用逐章证据），中途 pause/resume 生效，六章 issues.json 副作用闭环；落账与观察见 D34/D35）。下一片 R5 制品面板。实施中的选型演化以 DECISIONS D26 起条目为准；实施进度以提交记录为准。
---

# REFACTOR · 重构执行计划

## 1. 社区 Rust Agent 调研与设计采纳

> 原则：**使用经过验证的技术**。本节每项采纳均标注来源项目；未列入采纳且未 spike 验证的机制一律后置（见第 9 节）。

### 1.1 调研对象与可借鉴机制

| 项目 | 定位 | 与本项目相关的机制 |
| --- | --- | --- |
| **goose**（Block，Rust 最成熟的开源 agent 产品，8 crate 分层） | 桌面/CLI/API 三形态 AI agent，MCP 原生扩展生态（70+） | ① **审批模式体系**：`GOOSE_APPROVAL_MODE` = auto / smart（辅助 LLM 风险评估）/ manual，全局 TOML 配置 + **per-tool 覆盖**（`smart_approve` 按工具覆盖，社区公认形态）；会话窗口内 Allow/Deny 卡片拦截工具调用。② token 计数缓存（DashMap 指纹缓存避免重复分词）。③ 对抗性检查器与「上下文韧性」列为三大工程投入。④ two-trait Provider 抽象（构造期 ProviderDef / 运行期 Provider 分离） |
| **aichat**（sigoden，~10.5k stars，单二进制 CLI） | all-in-one LLM CLI：REPL/Shell 助手/RAG/Agent | ① **Role = prompt + model config** 轻量抽象（与本项目 AgentSpec 同构，互相印证）。② Agent = instructions + tools + RAG 三层。③ 会话全文搜索（Tantivy，新版甚至能索引 Claude Code / Codex 会话）。④ 工具生态独立仓库（llm-functions）。⑤ `config.example.yaml` 入库的声明式配置惯例 |
| **neuron**（组合式框架，11 独立 crate 工作区） | 按需组合的 agent 构建块（provider / tool registry 等） | tool registry 独立抽象的思想——本项目 `tools.rs` 已同构；crate 级拆分对单应用无必要，**不采纳拆分** |
| **rig**（本项目在用，Rust agent 框架事实头部） | provider 抽象 + agent loop + 工具调用 | **原生集成 MCP**：经官方 `rmcp` crate 动态发现 MCP server 工具（rig.rs 官方文档）——工具生态扩展路线有经验证基建，列入 R7 |

跨项目共识（goose / Codex / Cursor / Claude Code 交集，arXiv 综述确认）：权限分档 + per-tool 覆盖 + 会话内审批卡片是当前 agent 系统的标配组件——本项目 R4 的设计与该共识对齐。

### 1.2 采纳决策表

| # | 采纳 | 来源 | 落点 |
| --- | --- | --- | --- |
| A1 | 权限两档（修订 2026-10-14，原三档）× **per-tool/per-action 覆盖矩阵**：会话级 `PermissionMode{Confirm,FullAccess}` + project.json `tool_overrides` 表（如「git log 永远自动、正文提议永远人工」） | goose `smart_approve` per-tool 覆盖、Cursor per-tool auto-approval、ZCode 会话模式控件（会话级档位 + 工具权限块门控，见 D30） | R4 |
| A2 | 审批交互 = 会话内 Allow/Deny 卡片（工具调用挂起等待） | goose 会话窗口 Allow/Deny | R4 Spike A（ApprovalBroker） |
| A3 | **bash 白名单 = 配置文件**而非硬编码：全局（app data）+ 作品级两层，内置默认集（git 只读等），提供 example 文件入库 | goose goose_mode TOML 配置化、aichat 声明式配置惯例 | R4 |
| A4 | token 计数指纹缓存：同内容不重复估算 | goose DashMap 计数缓存 | R3 |
| A5 | 会话搜索：前缀/子串匹配起步（零依赖），Tantivy 全文索引后置为量级升级路径 | aichat（Tantivy 方案） | R1（轻量版）+ R7（升级版） |
| A6 | MCP 工具生态：登记 MCP server → rmcp → rig 动态发现工具 → 统一权限检查器管控 | rig 官方 MCP 集成、goose 扩展哲学 | R7（后置） |
| A7 | `config.example` 惯例：权限/白名单/上下文偏好等用户可编辑配置均提供示例文件入库 | aichat | R3/R4（已落 `docs/config.example/`） |

### 1.3 明确不采纳（记录理由，防反复）

- **smart approve（辅助 LLM 风险评估）**：成本与复杂度，且本项目威胁面小（单机写作、无远端）；后置观察。
- **neuron 式多 crate 拆分**：本项目单 crate 内模块化（`agent/` 子模块）已满足，拆分只增加构建面。
- **goose 对抗性检查器**：面向 prompt 注入/恶意工具输出场景，本项目工具全部内置+白名单，威胁面不同；后置。
- **aichat 的 OpenAI 兼容代理层 / Playground / Arena**：评估设施，与产品形态无关。

## 2. 总则

1. **渐进绞杀，不推倒重来**：新形态与旧组件并存过渡，旧 UI（GeneratePane/StageBar/Workbench 旧布局）在 R5 才移除；任意切片结束时应用完整可用、回归绿。
2. **一片一提交**（中文提交信息），每片完成时：`cargo check` 警告 0 + `cargo test` 全绿 + 浏览器测试通道（`cargo run --example webapi` + `pnpm dev`）走查通过。
3. **双通道对等**：每个新增后端命令同时进 `lib.rs` 注册表与 `examples/webapi.rs` dispatch；前端只经 `src/lib/tauri.ts` 调用。
4. **红线贯穿全程**：propose_* 唯一写通道、可见性出口过滤（`lore::is_prompt_visible`）、key 不出 Rust 侧——任何切片不得临时绕过。
5. **使用经过验证的技术**：新增机制须标注来源项目或经 spike 验证（第 1 节）；未验证机制先进 spike 或进 R7 后置。
6. **rig 防御动作**（来源：2026-10-14 rig 选型复核）：R4 开头两个 spike + inner facade（rig 类型收敛在 kernel.rs 内）+ 会话存档 `format_version`。

### 切片总览

| 片 | 主题 | 主要工作量 | 规模 | 依赖 |
| --- | --- | --- | --- | --- |
| R1 | 应用骨架 + 真会话 | 前端重 / 后端轻 | M | — |
| R2 | 聊天升级（AI Elements + slash/@ + 内联提议 + PLAN 条） | 前端重 / 后端轻 | L | R1 |
| R3 | 上下文工程 | 前端中 / 后端中 | M | R2 |
| R4 | Agent 能力 + 权限（bash + 工具桥 + 两档×覆盖） | 后端重 / 前端中 | L | R2（与 R3 可并行） |
| R4.5 | 长时目标（/goal，轮末验证器 + 迭代进度） | 后端中 / 前端中 | M | **R4 完成** |
| R5 | 制品面板 + 旧 UI 退役 | 前端重 | M | **R4 完成**（聊天具备全部能力后才拆按钮；建议排 R4.5 之后） |
| R6 | git 自动化 + 文档对齐收尾 | 后端轻 | S | R5 |

---

## 3. R1 · 应用骨架 + 真会话

**目标**：rail 三入口就位；项目页变成「会话列表 | chat | 制品面板」三栏；会话可管理（标题/重命名/删除/搜索）；全局「对话」页可用（无项目上下文）。

**后端改动**
- `agent/session.rs`：会话结构加 `title` 字段（旧档兼容：无 title 回退首条用户消息截断）；新增 `rename_session` / `delete_session`。
- 全局会话：app data 下 `global-sessions/`，session 存取抽 `session_store` 概念（`Work(PathBuf) | Global`）。
- `chat_flow` 解耦 work 绑定：全局对话用新 spec 变体（**无文件工具**——无 work_root，read_file/propose_* 不适用；纯对话）。
- 会话自动标题：首轮完成后单次轻量调用生成一句标题（失败静默回退首句截断）。
- 会话搜索：`list_sessions` 返回值带 title/摘要，前端做子串匹配过滤（A5 轻量版，零新依赖）。
- 新命令：`rename_session` / `delete_session` / 全局对话入口（`chat_stream` 加 scope 参数或独立命令，实现时定），全部进双通道。

**前端改动**
- `App.tsx`：最左 rail（对话 `/chat`、创作 `/`、配置 `/settings`）。
- 新 `/chat` 全局对话页：左会话列表 + 右 chat（ChatPane 传 scope 复用）。
- `Workbench.tsx` 布局手术：左栏 = 会话列表（上，新建/切换/重命名/删除/搜索）+ 资源树（下，过渡暂留，R5 移入制品面板）；中栏 = ChatPane 独占；右栏 Tabs = 编辑器 | 审阅 | 装配 | **生成**（GeneratePane 整体降级为过渡 Tab，R5 移除）。StageBar 暂留顶栏。
- `Launcher.tsx` → rail「创作」下的项目库页（逻辑不动，入口变化）。

**验证**：全局页多会话增删改查切换；项目页旧功能（8 按钮/审阅/编辑/装配）在新布局全可用；旧会话存档可加载；搜索过滤生效。
**提交拆分**：① 后端会话管理+全局会话 ② rail 与路由骨架 ③ 项目页布局手术 ④ 全局对话页。

---

## 4. R2 · 聊天升级

**目标**：聊天成为唯一主操作面的地基——AI Elements 组件就位、`/` 技能与 `@` 素材引用可用、提议内联聊天流（拒绝带评论）、PLAN 条上线。

**第 0 步（spike，单独提交）**：AI Elements 与 base-ui 兼容性验证——源码复制 `Tool` + `Prompt Input` 两个组件，交互原语改造（radix `asChild` → base-ui `render`）。**通过** → 铺开其余组件；**不通过** → 按 AI Elements 结构自研同构组件，决策记 DECISIONS。

**后端改动**
- 新命令 `list_skills`：内置 + 作品级技能清单（名称/描述/来源/是否覆盖）。
- `reject_proposal` 加 `comment: Option<String>`：评论存入提议记录，该会话下次 `chat_stream` 时注入上下文（驱动 Agent 修改重提）。
- `@` 注入：前端拼装引用块进 prompt；引用块格式约定写入 agent spec 提示词。

**前端改动**
- ChatPane 基于组件重写：`Conversation` / `Message`（流式气泡）、`Tool`（替代「🔧 调用」小字行）、`Task`（R4 子任务预留）、`Confirmation`（R4 审批预留）。
- `Prompt Input` + 自研补全：`/` 技能菜单（`list_skills`，带参数提示如 `/正文 卷1 章3`）；`@` 素材菜单（`read_tree`/`list_settings`：文件/设定条目/账本/问题清单）。
- **slash 过渡实现**：前端 dispatch 表直达既有命令（`generate_artifact`/`review_chapter`/…），流式输出渲染进聊天流；R4 切换为 Agent 指令。
- 内联提议卡片：订阅 `review:new` + `list_proposals`，发起消息下方渲染 diff 卡（复用现 diff 渲染与折叠）+ 接受/拒绝/拒绝带评论（评论输入框）；与右栏「审阅」Tab 同源同步。
- **PLAN 条**：`Plan` 组件改——折叠摘要（阶段+进度点+建议）/展开确认（`pipeline_confirm`）/「…」菜单收 skip/redo；数据源 `get_pipeline`。走查通过后**移除 StageBar**（回退 = revert 该提交）。

**验证**：`/一致性 第3章` 直达命令走通流式渲染；拒绝带评论后 Agent 收到评论并重提；PLAN 条确认推进阶段；补全数据正确。
**提交拆分**：① AI Elements spike ② Conversation/Tool 迁移 ③ slash+@ 补全与 dispatch 表 ④ 内联提议卡片+拒绝评论 ⑤ PLAN 条与 StageBar 移除。

---

## 5. R3 · 上下文工程

**目标**：上下文可见、可组装；预算从硬拦改软提示；窗口用量常驻显示。

**后端改动**
- settings 供应商/模型补 `context_window` 字段（预设带常见值，可手填覆盖）。
- 新命令 `context_overview`：{会话已用（session.usage 真实值，已在存）、装配估算（D9 口径）、模型窗口、占比}。
- **token 计数指纹缓存**（A4）：估算函数加内容指纹缓存，上下文面板高频刷新不重复计算。
- `assembly.rs` 预算语义调整（D11 演化）：超预算/自动降档从**拦截/强制降**改为**返回 warning**（档位、出场过滤、账本分层逻辑不变，只是不再静默强制）；`preview_assembly` 返回明细+警告。**可见性过滤行为一字不动**，红线回归保持绿。
- `cargo test`：预算断言从「拦截」改「警告」；新增缓存命中用例。
- 装配偏好（档位/条目开关）持久化进 project.json；提供 `config.example` 式示例说明（A7）。

**前端改动**
- 上下文面板（`Context` 组件改）：三层展示（会话历史 / @注入 / 装配明细）；条目临时开关（影响后续装配）；一键全量。
- 用量指示：输入框旁常驻（已用/窗口 百分比，口径标注：真实 vs 估算），>80% 提示「建议新建会话」。

**验证**：面板开关反映到 `preview_assembly`；超预算仅提示不拦截、生成照常；隐藏设定仍绝不进 prompt。
**提交拆分**：① context_window+overview+缓存 ② 装配软预算化+测试更新 ③ 上下文面板+用量指示。

---

## 6. R4 · Agent 能力 + 权限

**目标**：聊天 Agent 具备全部操作能力（原 8 按钮 + 检查 + 查询）；bash 白名单 + 权限两档×覆盖矩阵上线；slash 切换为 Agent 指令。

**第 0 步（两个 spike，先于一切工具开发）**
- **Spike A（审批挂起）**：rig 工具内 oneshot channel 挂起等 UI 确认——`ApprovalBroker`（请求队列+超时+取消）；webapi SSE 加心跳防长等待断连，Tauri Channel 天然支持。验证：工具调用暂停→UI Allow/Deny→继续。（与 goose 会话内 Allow/Deny 形态一致，A2）
- **Spike B（子任务事件）**：`AgentEvent` 加 `SubTaskStart{label}` / `SubTaskDelta{...}` / `SubTaskEnd{summary}`；flows 的 `EventSink` 嵌套转发进父会话事件流。验证：run_flow 原型渲染子任务卡。
- **任一失败** → 逃生舱：绕过 AgentBuilder、只用 rig completion 层自写 loop（改动面 kernel.rs）；或降级（审批改「工具返回确认请求」、子任务改「只返最终摘要」）。决策记 DECISIONS。
- 同步 **inner facade**：rig 类型（Message/AgentBuilder/事件）收敛 kernel.rs 内 + 会话存档 `format_version`。

**后端改动**
- **权限模型（A1，2026-10-14 修订为两档）**：`PermissionMode { Confirm 变更前确认, FullAccess 完全访问 }` 会话级（存会话档案，重连恢复，新会话回落确认档）；project.json `tool_overrides` 表——按工具名/命令模式（bash）/路径模式（propose_*）覆盖档位；统一权限检查器：工具执行前 resolve（更具体 pattern 的 override → 会话档位 → 内置默认）。完全访问自动应用一切提议，但触及可见性 frontmatter（visibility/open_at_vol 变化）的提议永不自动应用（`Proposal.requires_review` 硬闸，进人工队列带徽标）。**永远留人硬编码在工具层**：可见性设置、删除/恢复文件、跳过阶段、装配切全量——任何档位与 override 不可豁免（D32）。
- **bash 工具（A3）**：白名单来自配置文件（全局 app data + 作品级两层，`config.example` 入库）+ 内置默认（git status/log/diff/show、登记的 CLI、只读管道组合）；写产物模式黑名单（`>`、`>>`、`sed -i`、`rm`、`mv` 至作品路径等）→ 拦截并提示改用 propose_*；变更前确认档经 ApprovalBroker 逐条审批，完全访问档白名单直通。
- **run_flow 工具**：kind 枚举（章纲/正文/回写/一致性/deai/复检/审卷/整理）→ 调 `flows.rs`（子会话沿用现有 per-flow session 规则）；触发前补 `pipeline::gate_check`（现状 flows 不查门）。
- **直连检查工具**：`check_visibility_leak()` / `preflight_chapter(volume, chapter)`（零 token 薄包装）。
- **查询工具**：`get_ledger` / `list_issues` / `list_lore(filter)`（只读、结构化）。
- **技能挂载**：chat_spec 挂 writer-basics + 动态挂载（slash 指定=强制挂载；自然语言按 whenToUse 关键词命中即挂）；`skills.rs` 支持按名子集装配。
- `cargo test` 新增：白名单解析表驱动用例、**权限矩阵**（档位 × override × 操作类型 × 期望）、gate 前置校验、写产物拦截。

**前端改动**
- `Confirmation` 审批卡（bash 命令 + 白名单标记 + 允许/拒绝 + 「本会话完全访问」升档按钮）；权限档位控件 = **输入框左下角下拉**（当前档图标+名称展示，弹层两项带描述行，当前项打勾；照搬 ZCode 会话模式控件形态，D30）；per-tool override 设置入口置于该下拉内。
- `Task` + `Terminal`：子任务卡（进度/摘要，展开看工具轨迹与 bash 输出）。
- slash 切换：dispatch 表退役 → slash 指令作为结构化消息发 Agent（`/正文 卷1 章3` → 「请执行：正文，卷1 章3」+ 强制挂载对应技能），Agent 走 run_flow。

**验证**：聊天内完成原 8 按钮全部能力且 issues.json/账本副作用闭环不破；`echo > 设定/x.md` 被拦并引导 propose_*；权限矩阵测试全绿；变更前确认档 bash 逐条确认、完全访问档直通；override（如「git log 永远自动」）生效；完全访问档四类留人操作仍弹确认；档位随会话持久化、审批卡升档可用。
**提交拆分**：① Spike A（审批挂起）单独提交 ② Spike B（子任务事件 + run_flow 全量 kind + gate_check + 最小子任务卡）单独提交 ③ 权限模型（两档）+检查器+override 配置 ④ bash 白名单配置化+写产物黑名单+留人 guard ⑤ 权限 UI（输入框左下角档位下拉 + 审批卡 + 自动应用接线） ⑥ 查询/检查工具+技能挂载 ⑦ 子任务卡完善（轨迹展开） ⑧ slash 切换为 Agent 指令 ⑨ 验收走查+文档。

---

## 6.5 R4.5 · 长时目标（/goal，2026-10-14 修订增补）

**目标**：会话级长时目标——「把第一卷 10 章全部审校一遍」这类跨多轮子任务的批量目标，Agent 迭代执行、轮末自动验证、进度可见（形态来源：Codex `/goal` 可验证停止条件 + ZCode goal 验证器迭代，见 D31）。

**第 0 步（提示词 spike，单独提交）**：完成验证器 + 完成审计提示词在真实迭代里调通（在 R4 的 run_flow 之上跑 2-3 轮真实目标）。
**判定标准**：目标真完成时验证器 pass 收尾；AI 假完成（产出过计划/清单、子任务绿但未覆盖目标全部要求、凭耗时与意图）被判 fail 并续跑；非任务目标（如「你好」）一轮 pass 不无限续跑；中文目标的 reason/nextAction 为中文；坏 JSON fail-open 不卡死已交付目标。
**不通过** → 降级为「目标 + 手动任务清单」（agent 只维护 todo 列表，不自动续跑），决策记 DECISIONS。

**后端改动**：`SessionGoal{objective,status(active|paused|budget_limited|complete),iterations[],budget}` 存会话档案；`/goal` 指令解析（设置/暂停/恢复/清除/替换）；轮末自动运行轻量验证器（独立调用、禁工具、JSON `{passed,reason,nextAction}`）；迭代仅由验证器边界推进（用户消息与 TodoWrite 不推进）；目标注入按不可信数据包裹（`<untrusted_objective>` + 转义）；goal 启动时提示切换「完全访问」。
**前端改动**：PLAN 条上方目标横幅（目标摘要 + 迭代行：标题=上一轮 nextAction、完成计数、验证结论；暂停/继续/清除）。

**验证**：浏览器通道跑一个真实批量目标至 complete；中途 pause/resume 生效；假完成场景被验证器拦截并继续迭代。
**提交拆分**：① 提示词 spike ② 后端目标状态机+验证器 ③ /goal 指令+前端进度 UI。

---

## 7. R5 · 制品面板 + 旧 UI 退役

**前置：R4 验收通过**（聊天具备全部能力后才拆按钮）。

**前端改动**
- 制品面板 VS Code 式多标签：标签组状态前端管理（多开/关闭/切换，存 localStorage）；制品注册表：文稿（编辑器+diff 徽标）、审阅、**问题清单**（issues.json 结构化渲染，裁决按钮从 GeneratePane 迁入）、**账本**（`get_ledger` 结构化视图——api 已封装未用，此处启用）、装配检查器、成本（`get_usage_summary`）、git 历史（`git_log` + 单提交 diff，`Commit` 组件）、技能文档（查看/编辑，作品级覆盖）。
- 资源树从左栏移入制品面板（File Tree 或文稿标签侧栏，实现时定）。
- 删除：GeneratePane（右栏过渡 Tab）、旧 Workbench 残余布局代码、StageBar 残余。
- 聊天内联问题卡片：审校完成后聊天流渲染问题摘要卡，点击跳问题清单标签。

**验证**：浏览器通道全流程走查——建作品→设定（含可见性）→总纲→卷纲→`/章纲`→`/正文`→内联审阅→`/回写`→`/一致性`→问题裁决→`/复检`，全程无旧 UI；显式入口数 ≤ REDESIGN §11 清单。
**提交拆分**：① 制品多标签框架 ② 问题清单/账本/成本/git/技能五个制品视图 ③ GeneratePane 与过渡组件移除+清理。

---

## 8. R6 · git 自动化 + 收尾

**后端改动**
- `pipeline_confirm` 成功后自动 commit 里程碑（`阶段确认：<阶段>`，无变更跳过）。
- bash 白名单 git 只读四命令的交互展示（git 历史制品标签查看 Agent 查询结果）。

**文档对齐**（按 REDESIGN §14.3 清单）
- `SPEC.md`：D0-4/D0-5/D0-6/D0-7 修订；走通路径与切片章节按新形态重写。
- `DECISIONS.md`：追加 D19 起——权限分档（D30 收敛为两档）、bash 白名单配置化、上下文三层、AI Elements 引入路线（R2 spike 结果）、rig 防御 facade、会话形态、长时目标（D31）、MCP 后置路线（D19-D32 已随各切片落账，收尾时核对无遗漏）。
- `AGENTS.md`：目录速览、命令、硬约束更新。
- `examples/regression`：扩展为「仅聊天+制品」全流程断言。

**验证**：`cargo test` 全绿；`regression` 通过；阶段确认后 git log 出现里程碑提交；文档与现状一致。
**提交拆分**：① 阶段确认自动 commit ② regression 扩展 ③ 文档对齐。

---

## 9. R7 · 后置路线（可选，不排期）

| 项 | 内容 | 来源 | 触发条件 |
| --- | --- | --- | --- |
| MCP 工具生态 | 登记 MCP server → rmcp → rig 动态发现工具 → 统一权限检查器管控（默认 Review 档逐条审） | rig 官方 MCP 集成、goose 扩展哲学 | 用户提出「加新工具」需求，或 bash CLI 登记不够用 |
| Tantivy 会话全文搜索 | 替换 R1 子串匹配为全文索引 | aichat | 会话量级 > 数百、搜索体验明显不足 |
| smart approve | 辅助 LLM 对工具调用做风险评估，风险低自动放行 | goose / Codex | 多工具引入后审批疲劳明显 |
| 对抗性输出检查 | 工具输出/注入内容的安全检查器 | goose | 引入 MCP 外部工具之后 |
| 会话压缩 compaction | 长会话自动摘要压缩 | Claude Code /compact | 长会话治理需求真实出现 |
| 技能包分发 | 技能/提示模板打包分享 | PI packages | 多作品复用技能成为常态 |

---

## 10. 跨切片注意事项

- **窗口用量口径**：会话 usage 是 rig 返回的真实值、装配是 D9 估算，UI 标注清楚两者口径。
- **提议双入口同步**（R2 起）：聊天卡片与审阅 Tab 并存，状态更新以 `review:new` 事件 + 列表刷新为准，防双击竞态。
- **webapi 对等**：每片新命令同步进 `examples/webapi.rs`，否则浏览器通道验证缺腿。
- **旧数据兼容**：会话存档（title/format_version）、project.json（装配偏好/权限档位/override）、settings.json（context_window）全部向前兼容读取，不写迁移脚本。
- **回退策略**：每片可独立 revert；仅 R2 StageBar 移除、R5 GeneratePane 移除两个删除节点，验收通过才执行。
- **来源标注**：实现中每个借鉴机制在代码注释/提交信息中标注来源项目（如 `// 借鉴 goose smart_approve per-tool 覆盖`），保持「经过验证的技术」可追溯。

## 附：调研来源

- goose：[goose-docs.ai 权限模式](https://goose-docs.ai/docs/guides/managing-tools/goose-permissions) / [aaif-goose/goose](https://github.com/aaif-goose/goose) / [Block 代码库深读（Medium）](https://deshpandetanmay.medium.com/i-analyzed-blocks-ai-agent-codebase-it-bets-on-plumbing-not-models-here-is-the-deep-dive-2efc10d7a578) / [arXiv：Today's and Future AI Agent Systems 设计空间](https://arxiv.org/html/2604.14228v2)（smart approve 综述）/ [arXiv：Anatomy, Architecture, and Evolution of Coding Agents](https://arxiv.org/html/2609.00006v1)
- aichat：[sigoden/aichat](https://github.com/sigoden/aichat)（Role/Agent 抽象、声明式配置、会话搜索）
- neuron：[r/rust 发布帖](https://www.reddit.com/r/rust/comments/1rchad0/neuron_composable_building_blocks_for_ai_agents) / [crates.io/neuron](https://crates.io/crates/neuron)
- rig MCP：[rig.rs 官方 MCP 集成文档](https://rig.rs/docs/integrations/model_context_protocol) / [modelcontextprotocol/rust-sdk（rmcp）](https://github.com/modelcontextprotocol/rust-sdk)
- rig 选型复核（2026-10-14 会话）：rig 为 Rust agent 框架头部、项目自建 harness 路线正确、防御动作三条
- ZCode 开源仓库（2026-10-14 深读，权限两档与 R4.5 来源）：[zai-org/ZCode](https://github.com/zai-org/ZCode)——会话模式控件与持久化（services/session/sessionModeOptions.ts、ui i18n 模式标签）、工具权限块门控（core/permission/service.ts）、审批流授予完全访问（core/runtime/permission-full-access.ts）、goal 验证器提示词全文（contracts/tools/target.ts）与迭代进度模型（ui conversationGoalSummaryModel.ts）、TodoWrite 与提醒（core/tool/handlers/todo.ts、core/runtime/helpers/runtime-reminders.ts）
