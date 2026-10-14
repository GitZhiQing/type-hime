# type-hime

AI 辅助中文网文长篇写作工作台。单机自用，本地优先，BYO API key。

- **Agent 为核心**：与一个完整的创作 Agent 对话完成一切——`/` 调技能（章纲/正文/回写/一致性/去AI味/复检…）、`@` 引用素材、自然语言提需求；输入框上方 PLAN 条展示六阶段进度（设定→总纲→卷纲→章纲→正文→审校，阶段门保留为后台约束）
- **多 Agent 团队**（R8 · 形态来源 ZCode 子 Agent 体系）：根 Agent 与你交互、装配上下文、调度子 Agent 干活——成篇创作/系统审校委派给专职子 Agent（独立会话+专职提示词），**多个独立任务可在同一条回复里并行委派**（并发执行、子任务卡各带身份色）；子 Agent 一切落稿仍走提议审阅（红线不放松）。rail「Agents」页创建/查看/配置子 Agent（md+frontmatter：描述/模型/工具白名单/技能/提示词），内置 general 通用模板，可复制出写手、编辑等任意专职 Agent
- **长时目标 /goal**（R4.5 · 形态来源 Codex/ZCode）：跨多轮子任务的批量目标（如「把第一卷逐章一致性检查一遍」）迭代执行、轮末独立完成验证器判定真伪完成（假完成/部分覆盖被拦续跑）、进度常驻 PLAN 条上方的目标横幅（迭代行=上一轮 nextAction，可暂停/继续/清除，预算上限保护）
- **制品面板**（R5 · VS Code 式多标签）：文稿（资源树+md 编辑/预览）、审阅（提议 diff）、问题清单（逐条裁决）、账本（结构化）、装配检查器、成本、git 历史（点击提交看 diff）、技能文档（查看/编辑，作品级覆盖内置）；标签组按作品记忆，浏览器式标签栏（滚轮/箭头/列表下拉/右键菜单/拖拽排序）
- **Agent 有手有眼**：结构化查询（账本/问题清单/可见设定/流水线）、零 token 检查（泄漏/修为预检）直连；bash 只读命令（git 只读 + 只读管道，白名单可用 permissions.json 两层扩充，写产物一律拦截并引导提议）；`/提交` 让 Agent 提交 git 变更（用户主动驱动，确认档逐条审批）
- **权限在会话中定**：输入框左下角档位下拉——「变更前确认」（提议逐条审阅、白名单命令逐条确认）与「完全访问」（提议自动应用、命令直通）；审批卡与「允许并本会话完全访问」升档按钮；可见性变更、删/恢复文件、跳阶段、装配切全量四类操作任何档位都永远人工
- **一致性工程**保证不吃书：连续性账本（含时间线）+ 规则装配（出场过滤/档位/上下文面板可控）+ 可见性控制 + 修为代码预检 + 一致性审校（问题清单/裁决/复检闭环）
- **AI 永不静默改稿**：一切落稿走 提议 → diff 人审 → git commit（提议就地出现在聊天流与制品面板「审阅」标签，拒绝可附评论反馈给 AI）
- **上下文可观测**：会话真实用量 + 装配估算（窗口占用按最近一轮输入计），接近窗口上限时提示新建会话；会话列表支持标题/搜索/重命名/删除

技术栈：Tauri 2 + React/TS + Tailwind v4 + shadcn/ui（base-ui）；AI 界面组件改编自 Vercel AI Elements（Message/Conversation/Tool + Task/Confirmation 同构最小版）+ Streamdown 流式 Markdown；Rust 侧 rig 0.44 Agent 内核（供应商/工具/技能可插拔，工具经 DynamicTool 动态注册，bash 无 shell 直执行，审批挂起用 oneshot）。

范围契约见 [docs/SPEC.md](docs/SPEC.md)，选型记录见 [docs/DECISIONS.md](docs/DECISIONS.md)，产品重设计与重构执行见 [docs/REDESIGN.md](docs/REDESIGN.md) / [docs/REFACTOR.md](docs/REFACTOR.md)，用户可编辑配置示例见 [docs/config.example/](docs/config.example/README.md)。

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
cargo run --example goal_spike -- <作品目录>  # 长时目标完成验证器 8 场景（真实模型）
cargo run --example webapi          # Web 测试通道（127.0.0.1:18360）+ pnpm dev 可在浏览器全流程验证
```
