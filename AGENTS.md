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
cargo test              # Rust 单测（25 项：git/works/pipeline/lore/ledger/assembly/settings/issues/preflight）
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
docs/            SPEC.md（范围契约）+ DECISIONS.md（选型账）
src/             前端（React + TS + Tailwind v4 + shadcn/ui，base-ui 基座）
  components/ui/      shadcn 组件（CLI 生成）
  components/*.tsx    资源树/编辑器/会话/生成/审阅/装配/阶段条
  pages/              Launcher（作品库）/ Workbench / Settings
src-tauri/src/
  lib.rs             Tauri 命令注册（唯一入口）
  agent/             Agent 内核（D5）：spec/kernel/tools/skills/session/events
    skills/          内置技能 md（writer-basics/ledger-update/review-consistency/ledger-tidy）
  works.rs           作品/文件/资源树（路径穿越防护）
  git.rs             命令行 git 封装
  pipeline.rs        六阶段状态机 + 阶段门
  lore.rs            设定库 + 可见性（is_prompt_visible 是红线唯一裁决点）+ 4-gram 泄漏检查
  ledger.rs          账本解析（时间线小节/宽容格式）+ 分层摘要（活跃窗口）
  assembly.rs        规则装配引擎（出场过滤/档位/预算自动降档/上一章结尾）
  preflight.rs       修为层级代码预检（零 token 必查）
  issues.rs          审校问题清单（结构化落盘/裁决/复检档案）
  review.rs          提议队列 + unified diff + 应用 + 持久化
  settings.rs        多供应商管理（key 只存本地）
examples/        chat_smoke / full_flow / mvp_flow / write_book / regression
```

## 硬约束

- 全程 UTF-8；作品目录/文件名可含中文，所有 IO 显式 UTF-8。
- API key 只存 Rust 侧 app data；任何命令返回值不得包含 key 明文。
- 可见性过滤是红线：被隐藏/未到开放卷的设定条目，无论装配档位怎么设都不进 prompt（lore::is_prompt_visible 在装配出口强制执行）。
- agent 对作品文件的一切修改必须走提议→diff 审阅→应用通道（propose_edit/propose_write 不写盘），禁止工具直接写盘。
- rig 0.44 注意：AgentBuilder 是 typestate（第一个 tool() 转换状态）；ToolCall 用 `.function.name`；OpenAI 兼容服务必须 `Route::Chat`；OpenAIConfig 无 with_api_key。
- shadcn 项目基座是 base-ui（不是 radix）：无 asChild，用 `render={<Component/>}`。
- 单测中的临时目录要按测试名区分后缀，避免并行测试竞态（曾犯）。
- cargo check 警告保持 0（cargo fix --lib 可自动清）。
- 开发纪律：一片一提交（中文提交信息），切片验收通过才进下一片；先 cargo check/test 绿再动前端。
- 文档纪律：产物按现状书写，演进过程交给提交说明与 DECISIONS.md。
