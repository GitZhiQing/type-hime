# 码字姬（type-hime）

本地优先的中文网文 AI 写作工作台——聊天写书，改动先过目。

> 本文件写给人类读者；AI Agent 请读 [AGENTS.md](AGENTS.md)。范围与决策见 [docs/SPEC.md](docs/SPEC.md) 与 [docs/DECISIONS.md](docs/DECISIONS.md)。

## 能做什么

- **聊天写书**：新书先做开书访谈——回答几个问题，拿到人设卡、世界观、文风卡、样章和总纲；之后一句「写第 3 章」或点流程条上的「下一步」，章纲、正文、账本回写、审校一路推进
- **过程看得见**：正文实时流式输出；多个子 Agent 可同时开工，任务行带进度、耗时和用量，点标题即开它的完整会话（运行中实时、完成后回看）
- **改动先过目**：AI 改任何文件都先给 diff，你接受才写盘并自动 git 提交；拒绝可附一句意见，它照着重改
- **不怕吃书**：账本记录角色状态、伏笔和时间线，写新章自动带上；审校查一致性、可读性和 AI 味；隐藏与未到开放卷的设定不会进入 AI 上下文
- **批量活交给 /goal**：「把第一卷逐章审一遍」这类长任务自己排轮次做完，独立验证器核对是否真完成，假完成被打回
- **权限随手切**：输入框左下角两档——变更前确认（提议、白名单命令逐条问你）与完全访问（自动落稿）；删文件、改可见性永远要你亲手点
- **制品面板**：文稿、审阅、问题清单、账本、委派会话、成本、git 历史、技能文档，多标签随意开合；左右两栏可收起
- **书架式作品库**：右键重命名/复制路径/删除（删除 = 移入回收站）；作品库文件夹可换

技术栈：Tauri 2 + React/TS + shadcn/ui，Rust 侧经 rig 接模型（DeepSeek 等 OpenAI 兼容服务均可）；每个作品是一个 git 仓库。

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

首次使用：设置页添加供应商（可拉取模型列表、测试连通；可配置上下文窗口）→ 新建作品 → 欢迎页开始开书访谈。

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
