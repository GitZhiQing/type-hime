# AGENTS.md

会话级上下文。只在 Agent 犯重复错误时追加纠正，保持 <100 行。

## 命令（全部真实执行过）

```bash
pnpm install            # 装前端依赖（首次）
pnpm dev                # 仅前端 dev server（浏览器预览 UI，无 Tauri 外壳）
pnpm tauri dev          # 完整桌面应用开发模式（Rust + 前端热更）
pnpm build              # 前端类型检查 + 构建（tsc && vite build）
cd src-tauri
cargo check             # Rust 编译检查（警告保持 0）
cargo test              # Rust 单测（75 项：git/works/pipeline/lore/ledger/assembly/settings/issues/preflight/session/kernel/approval/bash/permissions/subtask/skills/flows/goal/profiles）
cargo run --example chat_smoke -- <作品目录>      # 冒烟：模型列表+流式+工具+会话+提议
cargo run --example full_flow -- <作品目录>       # 冒烟：章纲→正文→账本回写
cargo run --example mvp_flow                       # 交付判据验收（真实模型全流程）
cargo run --example write_book -- <作品目录>      # 写书测试（6章全流程+埋错审校验证）
cargo run --example regression                     # 修复回归（2章+8项检查）
cargo run --example goal_spike -- <作品目录>      # R4.5 spike：目标完成验证器 8 场景（真实模型）
cargo run --example webapi                         # Web 测试通道（127.0.0.1:18360，配合 pnpm dev
                                                   #   的 /api 代理，浏览器可自动化测试真实后端）
```

## 目录速览

```
docs/            SPEC.md（范围契约）+ DECISIONS.md（选型账）+ REDESIGN.md（重设计·已确认）+ REFACTOR.md（重构执行：R1-R17 已完成，R18-R20/R6 待排，R7 后置）
src/             前端（React + TS + Tailwind v4 + shadcn/ui，base-ui 基座）
  components/ui/      shadcn 组件（CLI 生成）
  components/ai/      AI 界面组件（Message/Conversation/ToolCallCard 折叠卡/Response/任务行=子 Agent 卡/委派会话视图/会话历史渲染/代码高亮/提议卡/审批卡/输入框补全）
  components/artifacts/  制品面板（R5：ArtifactsPane 多标签框架 + 文稿/问题清单/账本/委派会话/成本/git/技能视图；装配视图已退役 D49）
  components/*.tsx    导航 rail/资源树/编辑器/会话列表/聊天（标题栏：标题/重命名/时间/面板收起）/PLAN 条/上下文条/目标横幅/审阅
  pages/              Launcher（作品库）/ GlobalChat（全局对话）/ Workbench / Settings / Agents（R8 子 Agent 团队管理）
src-tauri/src/
  lib.rs             Tauri 命令注册（唯一入口）
  flows.rs           流式业务体（Tauri/Web 双通道共用；事件与提议回调参数化）+ slash 委派模板 + 子 Agent 调度器（agent_spawner/run_subagent）+ run_flow 执行器（chat 工具面已退役，示例用）+ 长时目标迭代循环（goal_loop）
  agent/             Agent 内核（D5）：spec/kernel/tools/skills/profiles/session/events/goal
    profiles.rs       子 Agent 定义（R8 · D37：md+frontmatter、内置 general+用户级覆盖、CRUD）
    goal.rs           长时目标（R4.5 · D31/D34：完成验证器提示词/解析/边界判定/SessionGoal）
    approval.rs       审批挂起（ApprovalBroker：oneshot + 超时；webapi 决策字段名是 `allow`）
    bash.rs           bash 工具守卫（白名单/黑名单/两层 permissions.json）与无 shell 直执行
    subtask.rs        子任务/子 Agent 事件桥（SubTask*：run_flow 与 spawn_agent 过程回流）
    skills/          内置技能 md（writer-basics/ledger-update/review-consistency/ledger-tidy/开书[R15 访谈规程，条件挂载见 flows::kickoff_pending]）
    profiles/        内置子 Agent md（general 通用基础模板）
  permissions.rs     权限两档 + 覆盖矩阵（resolve：pattern override → 工具级 → 会话档）
  watch.rs           作品目录文件监听（R13 · D40：notify 递归 + .sessions/.git 过滤 + 400ms 防抖 → work:changed）
  works.rs           作品/文件/资源树（路径穿越防护；write_file 不建父目录，create_file 才建）
  git.rs             命令行 git 封装（log/show 单提交 diff；commit_all 供 git_commit 工具）
  pipeline.rs        六阶段状态机 + 阶段门
  lore.rs            设定库 + 可见性（is_prompt_visible 是红线唯一裁决点）+ 4-gram 泄漏检查
  ledger.rs          账本解析（时间线小节/宽容格式）+ 分层摘要（活跃窗口）
  assembly.rs        规则装配引擎（出场过滤/档位精确生效/预算软提示/上一章结尾）
  preflight.rs       修为层级代码预检（零 token 必查）
  issues.rs          审校问题清单（结构化落盘/裁决/复检档案；record_issues 工具基底）
  review.rs          提议队列 + unified diff + 应用 + 持久化
  settings.rs        多供应商管理（key 只存本地）
examples/        chat_smoke / full_flow / mvp_flow / write_book / regression / goal_spike / webapi
```

## 硬约束

- 全程 UTF-8；作品目录/文件名可含中文，所有 IO 显式 UTF-8。
- API key 只存 Rust 侧 app data；任何命令返回值不得包含 key 明文。
- 可见性过滤是红线：被隐藏/未到开放卷的设定条目，无论装配档位怎么设都不进 prompt（lore::is_prompt_visible 在装配出口强制执行）。
- agent 对作品文件的一切修改必须走提议→diff 审阅→应用通道（propose_edit/propose_write 不写盘），禁止工具直接写盘。
- rig 0.44 注意：AgentBuilder 是 typestate（第一个 tool() 转换状态；本项目工具已改走 dynamic_tools 动态注册，见 tools::erase）；ToolCall 用 `.function.name`；OpenAI 兼容服务必须 `Route::Chat`；OpenAIConfig 无 with_api_key。
- shadcn 项目基座是 base-ui（不是 radix）：无 asChild，用 `render={<Component/>}`。
- 单测中的临时目录要按测试名区分后缀，避免并行测试竞态（曾犯）。
- cargo check 警告保持 0（cargo fix --lib 可自动清）。
- 开发纪律：一片一提交（中文提交信息），切片验收通过才进下一片；先 cargo check/test 绿再动前端。
- 文档纪律：产物按现状书写，演进过程交给提交说明与 DECISIONS.md。
- 重构进行中（docs/REFACTOR.md；R1-R17 已完成——R4.5/R5/R8 见 D34-D37、R9 见 D39、R14-R17 见 D41/D44/D45/D46；D47-D55 为用户反馈 UI 批次：装配标签退役、子任务行持久化+子会话标签页、标题栏与面板收起等）：新增命令必须同时进 lib.rs 注册表与 webapi.rs dispatch；新机制在注释/提交信息标注来源（使用经过验证的技术）；剩 R6 收缩版（regression 扩展+文档对齐）+ R18-R20。实施中的选型演化以 DECISIONS D26 起条目为准。
- 多 Agent（R8）：子 Agent 定义=app data/agents/*.md（md+frontmatter，正文=prompt；内置 general 可覆盖不可删）；spawn_agent 递归硬禁（子 Agent 工具面永远剔除）；子 Agent 落稿同样走 propose_*（红线不因多 Agent 放松）；子会话档案 agent-<名>-<st-id>（含 st-id 防并行覆盖）。
- Tailwind v4 不扫描 node_modules：第三方组件（e.g. streamdown）的效用类不会生成 CSS——用 @source 显式纳入或写作用域 CSS（曾致复制按钮 pointer-events 失效、物理不可点；同类：streamdown shiki token 只给 --sdm-c/--shiki-dark 变量、应用规则缺，已在 index.css 补——第三方组件样式缺失先查此坑家族）。
- 交互验证必须用真实事件：合成 el.click() 绕过浏览器命中测试会产生假阳性；原生弹层（select/confirm/dialog）在嵌入环境定位不可控，统一用 shadcn 组件（ui/select、Dialog 等，Select 弹层 alignItemWithTrigger=false 标准下方展开）。
- 装节点（菜单/弹层类）必须真实点开验收：base-ui 的 DropdownMenuLabel 必须包在 DropdownMenuGroup 内（否则展开即白屏 MenuGroupContext is missing，快照看不出）；Playwright 定位点击不可达时，先用 elementFromPoint 核对命中再走坐标级真实点击（cua.click）。
- cargo 构建/测试前先停 webapi（运行中会锁 target/debug/examples/webapi.exe 致 link 失败）：netstat 找 PID → taskkill → 构建 → 重新拉起。
- 布局溢出两坑（R5 两次踩中）：flex-col 容器内子视图 h-full 按容器全高解析（含工具条时视图整体高一条）——内容区套 min-h-0 flex-1 包裹层；overflow-x-auto 会把 overflow-y 的 visible 计算为 auto，横向溢出连带出竖向滚动条——标签条类用隐藏滚动条（.tabbar-scroll）+ 滚轮/箭头/列表下拉导航。
- React setState updater 必须纯函数：浅拷贝数组后原地改元素对象（`arr[i].state = ...`）在 dev StrictMode 双调用下会重复累加（曾致并行双卡时流式文本逐词双倍，流结束全量替换后恢复——症状迷惑性强，先查 updater 纯度）。
- 内嵌浏览器（IAB）会陷入"渲染进程半故障"态：DOM 查询正常但截图残缺/点击 actionability 超时/坐标点击无反应；刷新无效，**关闭标签重开**即恢复（本会话两次踩中）。遇"元素存在但点不动"先怀疑它，别改代码。
