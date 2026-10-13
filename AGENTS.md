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
cargo test              # Rust 单测（50 项：git/works/pipeline/lore/ledger/assembly/settings/issues/preflight/session/kernel/approval/bash/permissions/subtask/skills/flows）
cargo run --example chat_smoke -- <作品目录>      # 冒烟：模型列表+流式+工具+会话+提议
cargo run --example full_flow -- <作品目录>       # 冒烟：章纲→正文→账本回写
cargo run --example mvp_flow                       # 交付判据验收（真实模型全流程）
cargo run --example write_book -- <作品目录>      # 写书测试（6章全流程+埋错审校验证）
cargo run --example regression                     # 修复回归（2章+8项检查）
cargo run --example webapi                         # Web 测试通道（127.0.0.1:18360，配合 pnpm dev
                                                   #   的 /api 代理，浏览器可自动化测试真实后端）
```

## 目录速览

```
docs/            SPEC.md（范围契约）+ DECISIONS.md（选型账）+ REDESIGN.md（重设计·已确认）+ REFACTOR.md（重构执行：R1-R6 + R4.5，R7 后置）
src/             前端（React + TS + Tailwind v4 + shadcn/ui，base-ui 基座）
  components/ui/      shadcn 组件（CLI 生成）
  components/ai/      AI 界面组件（Message/Conversation/ToolCallCard/Response/提议卡/审批卡/子任务卡/输入框补全）
  components/*.tsx    导航 rail/资源树/编辑器/会话列表/聊天/PLAN 条/上下文条/生成/审阅/装配
  pages/              Launcher（作品库）/ GlobalChat（全局对话）/ Workbench / Settings
src-tauri/src/
  lib.rs             Tauri 命令注册（唯一入口）
  flows.rs           流式业务体（Tauri/Web 双通道共用；事件与提议回调参数化）+ slash 指令化 + run_flow 执行器
  agent/             Agent 内核（D5）：spec/kernel/tools/skills/session/events
    approval.rs       审批挂起（ApprovalBroker：oneshot + 超时）
    bash.rs           bash 工具守卫（白名单/黑名单/两层 permissions.json）与无 shell 直执行
    subtask.rs        子任务事件桥（SubTask*：run_flow 过程回流）
    skills/          内置技能 md（writer-basics/ledger-update/review-consistency/ledger-tidy）
  permissions.rs     权限两档 + 覆盖矩阵（resolve：pattern override → 工具级 → 会话档）
  works.rs           作品/文件/资源树（路径穿越防护）
  git.rs             命令行 git 封装
  pipeline.rs        六阶段状态机 + 阶段门
  lore.rs            设定库 + 可见性（is_prompt_visible 是红线唯一裁决点）+ 4-gram 泄漏检查
  ledger.rs          账本解析（时间线小节/宽容格式）+ 分层摘要（活跃窗口）
  assembly.rs        规则装配引擎（出场过滤/档位精确生效/预算软提示/上一章结尾）
  preflight.rs       修为层级代码预检（零 token 必查）
  issues.rs          审校问题清单（结构化落盘/裁决/复检档案）
  review.rs          提议队列 + unified diff + 应用 + 持久化
  settings.rs        多供应商管理（key 只存本地）
examples/        chat_smoke / full_flow / mvp_flow / write_book / regression / webapi
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
- 重构进行中（docs/REFACTOR.md；R1-R4 已完成，下一片 R4.5 长时目标，R5-R6 待实施；权限两档见 D30，R4 落点与教训见 D33）：新增命令必须同时进 lib.rs 注册表与 webapi.rs dispatch；新机制在注释/提交信息标注来源（使用经过验证的技术）；GeneratePane 为 R5 退役对象（R4 已验收，但 R5 前仍保留，不得提前移除）。实施中的选型演化以 DECISIONS D26 起条目为准。
- Tailwind v4 不扫描 node_modules：第三方组件（e.g. streamdown）的效用类不会生成 CSS——用 @source 显式纳入或写作用域 CSS（曾致复制按钮 pointer-events 失效、物理不可点）。
- 交互验证必须用真实事件：合成 el.click() 绕过浏览器命中测试会产生假阳性；原生弹层（select/confirm/dialog）在嵌入环境定位不可控，统一用 shadcn 组件（ui/select、Dialog 等，Select 弹层 alignItemWithTrigger=false 标准下方展开）。
- 装节点（菜单/弹层类）必须真实点开验收：base-ui 的 DropdownMenuLabel 必须包在 DropdownMenuGroup 内（否则展开即白屏 MenuGroupContext is missing，快照看不出）；Playwright 定位点击不可达时，先用 elementFromPoint 核对命中再走坐标级真实点击（cua.click）。
- cargo 构建/测试前先停 webapi（运行中会锁 target/debug/examples/webapi.exe 致 link 失败）：netstat 找 PID → taskkill → 构建 → 重新拉起。
