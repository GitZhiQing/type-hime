# 码字姬（type-hime）

AI 辅助中文网文长篇写作工作台。本地优先、单机自用、BYO API key；一本小说 = 一个本地 git 仓库。

> **本文件写给人类读者**；AI Agent 请读 [AGENTS.md](AGENTS.md)。范围契约见 [docs/SPEC.md](docs/SPEC.md)，选型与决策记录见 [docs/DECISIONS.md](docs/DECISIONS.md)。

## 核心能力

- **Agent 为核心**：与一个完整的创作 Agent 对话完成一切——`/` 调技能（章纲/正文/回写/一致性/去AI味/复检…）、`@` 引用素材、自然语言提需求；标题栏下的流程条展示六阶段进度（设定→总纲→卷纲→章纲→正文→审校，阶段门为后台约束），带可点「下一步」按钮与建议 chips（命令是快捷方式，不是必经路径）；新建作品从欢迎页一键开始开书访谈（问答式产出人设卡/世界观/文风卡/样章/总纲）
- **多 Agent 团队**：根 Agent 调度专职子 Agent——成篇创作/系统审校委派给专职 Agent（独立会话 + 专职提示词），多个任务可在同一条回复里并行委派；任务行点标题即开子 Agent 完整会话（运行中实时、完成后可回看）；子 Agent 一切落稿仍走提议审阅
- **长时目标 /goal**：跨多轮子任务的批量目标（如「把第一卷逐章一致性检查一遍」）迭代执行，轮末独立完成验证器判定真伪完成（假完成被拦续跑）；目标横幅可暂停/继续/清除
- **制品面板**（VS Code 式多标签）：文稿（编辑/预览）、审阅 diff、问题清单（逐条裁决）、账本、委派会话、成本、git 历史、技能文档；左右两栏可完全收起
- **一致性工程**：连续性账本（含时间线）+ 规则装配（出场过滤/档位/预算软提示）+ 可见性控制（被隐藏或未到开放卷的设定绝不进 prompt）+ 修为代码预检 + 一致性审校（含可读性第四查；问题清单/裁决/复检闭环）
- **AI 永不静默改稿**：一切落稿走 提议 → diff 人审 → git commit；拒绝可附评论反馈给 AI 驱动重提
- **权限在会话中定**：输入框左下角两档——变更前确认（提议逐条审阅、白名单命令逐条确认）与完全访问（提议自动应用、命令直通）；可见性变更、删除/恢复文件等留人操作任何档位都需人工
- **作品库**：书架式管理，右键重命名/复制路径/删除（删除 = 移入工作区 `_回收站`）；工作区（作品库文件夹）可配置，含应用内文件夹选择器

技术栈：Tauri 2 + React/TS + Tailwind v4 + shadcn/ui（base-ui）；AI 界面组件改编自 Vercel AI Elements（Message/Conversation/Tool/Task 同构最小版）+ Streamdown 流式 Markdown；Rust 侧 rig 0.44 Agent 内核（供应商/工具/技能可插拔，工具经 DynamicTool 动态注册，bash 无 shell 直执行，审批挂起用 oneshot）。

## 前置条件

- Rust 1.95+（MSVC 工具链）、Node.js 20+、pnpm
- git（每个作品是一个 git 仓库）
- Windows 需 WebView2 运行时（一般已内置）
- 一个 OpenAI 兼容服务的 API key（DeepSeek / GLM / Kimi / OpenAI / OpenRouter / Ollama 均可）

## 运行

```bash
pnpm install
pnpm tauri dev     # 开发模式
pnpm tauri build   # 打包安装程序
```

首次使用：设置页添加供应商（可拉取模型列表、测试连通；可配置模型上下文窗口）→ 新建作品 → 欢迎页开始开书访谈，之后在聊天中推进流水线。

## 配置（进阶）

- **bash 白名单** `permissions.json`（两层叠加：全局 `%APPDATA%\type-hime\permissions.json` + 作品目录同名文件；字段均可省略）：`allow_commands`（额外只读命令，如 `rg`）、`deny_patterns`（额外拦截的子串模式）、`git_readonly`（额外 git 只读子命令）、`pipe_filters`（额外管道过滤命令）。写产物/改历史的黑名单不可豁免——作品文件修改一律走提议通道。
- **权限例外**：会话内权限下拉的「权限例外设置」按工具名 + 模式覆盖档位（写入作品 `project.json` 的 `permission.overrides`）。
- **装配偏好**：作品 `project.json` 的 `assembly_prefs`（档位/预算/组件排除），一般无需手改。

## 开发与验证

```bash
cd src-tauri
cargo check && cargo test        # 编译检查（警告保持 0）+ 单测
cargo run --example webapi       # Web 测试通道（127.0.0.1:18360）+ pnpm dev 可在浏览器验证真实后端
cargo run --example regression   # 真实模型回归（装配/时间线/预检/usage 等 8 项）
cargo run --example mvp_flow     # 交付判据验收（一卷 3 章全流程）
```
