# type-hime

AI 辅助中文网文长篇写作工作台。单机自用，本地优先，BYO API key。

- **固定流水线**（设定→总纲→卷纲→章纲→正文→审校，阶段门写死）保证不烂尾
- **一致性工程**（连续性账本 + 规则装配 + 可见性控制）保证不吃书
- **AI 永不静默改稿**：一切修改走 提议 → diff 人审 → git commit

技术栈：Tauri 2 + React/TS + Tailwind v4 + shadcn/ui（base-ui）；Rust 侧 rig 0.44 Agent 内核（供应商/工具/技能可插拔）。

范围契约见 [docs/SPEC.md](docs/SPEC.md)，选型记录见 [docs/DECISIONS.md](docs/DECISIONS.md)。

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

首次使用：设置页添加供应商（可拉取模型列表、测试连通）→ 新建作品 → 按阶段条推进。
AI 的产出在中栏「生成」（章纲/正文/账本回写/一致性检查/去 AI 味），一律以提议出现在右栏「审阅」，接受才落盘并 git 提交。

## 验收脚本（真实模型）

```bash
cd src-tauri
cargo run --example mvp_flow        # 一卷 3 章从新建到审校的全流程验收
```
