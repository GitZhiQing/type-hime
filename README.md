# type-hime

AI 辅助中文网文长篇写作工作台。单机自用，本地优先，BYO API key。

- **Agent 为核心**：与一个完整的创作 Agent 对话完成一切——`/` 调技能（章纲/正文/回写/一致性/去AI味/复检…）、`@` 引用素材、自然语言提需求；输入框上方 PLAN 条展示六阶段进度（设定→总纲→卷纲→章纲→正文→审校，阶段门保留为后台约束）
- **一致性工程**保证不吃书：连续性账本（含时间线）+ 规则装配（出场过滤/档位/上下文面板可控）+ 可见性控制 + 修为代码预检 + 一致性审校（问题清单/裁决/复检闭环）
- **AI 永不静默改稿**：一切落稿走 提议 → diff 人审 → git commit（提议就地出现在聊天流与右栏审阅，拒绝可附评论反馈给 AI；账本回写可半自动）
- **上下文可观测**：会话真实用量 + 装配估算（窗口占用按最近一轮输入计），接近窗口上限时提示新建会话；会话列表支持标题/搜索/重命名/删除

技术栈：Tauri 2 + React/TS + Tailwind v4 + shadcn/ui（base-ui）；AI 界面组件改编自 Vercel AI Elements（Message/Conversation/Tool）+ Streamdown 流式 Markdown；Rust 侧 rig 0.44 Agent 内核（供应商/工具/技能可插拔）。

范围契约见 [docs/SPEC.md](docs/SPEC.md)，选型记录见 [docs/DECISIONS.md](docs/DECISIONS.md)，产品重设计与重构执行见 [docs/REDESIGN.md](docs/REDESIGN.md) / [docs/REFACTOR.md](docs/REFACTOR.md)。

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

首次使用：设置页添加供应商（可拉取模型列表、测试连通；可配置模型上下文窗口）→ 新建作品 → 在项目页聊天中推进流水线（PLAN 条确认阶段）。

## 验收脚本（真实模型）

```bash
cd src-tauri
cargo run --example mvp_flow        # 一卷 3 章从新建到审校的全流程验收
cargo run --example regression      # 修复回归：2 章 + 8 项机制检查（装配/时间线/预检/usage 等）
cargo run --example webapi          # Web 测试通道（127.0.0.1:18360）+ pnpm dev 可在浏览器全流程验证
```
