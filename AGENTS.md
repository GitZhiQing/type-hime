# AGENTS.md

会话级上下文。只在 Agent 犯重复错误时追加纠正，保持 <100 行。

## 命令（全部真实执行过）

```bash
pnpm install            # 装前端依赖（首次）
pnpm dev                # 仅前端 dev server（浏览器预览 UI，无 Tauri 外壳）
pnpm tauri dev          # 完整桌面应用开发模式（Rust + 前端热更）
pnpm build              # 前端类型检查 + 构建（tsc && vite build）
cd src-tauri
cargo check             # Rust 编译检查
cargo test              # Rust 单测
```

## 目录速览

```
docs/        SPEC.md（范围契约）+ DECISIONS.md（选型账）
src/         前端（React + TS + Tailwind v4 + shadcn/ui）
  components/ui/  shadcn 组件（CLI 生成，勿手改样式规范）
  lib/utils.ts    cn() 等
src-tauri/   Rust 后端（tauri 2 + rig 0.44）
  src/lib.rs      Tauri 入口 + 命令注册
  src/agent/      Agent 内核（spec/kernel/tools/skills/session/events）
```

## 硬约束

- 全程 UTF-8；作品目录/文件名可含中文，所有 IO 显式 UTF-8。
- API key 只存 Rust 侧 app data；任何命令返回值不得包含 key 明文。
- 可见性过滤是红线：被隐藏/未到开放卷的设定条目，无论装配档位怎么设都不进 prompt（在装配出口强制执行）。
- agent 对作品文件的一切修改必须走提议→diff 审阅→应用通道，禁止工具直接写盘。
- 开发纪律：一片一提交（中文提交信息），切片验收通过才进下一片；先 cargo check/test 绿再动前端。
- 文档纪律：产物按现状书写，演进过程交给提交说明与 DECISIONS.md。
- shadcn 组件遵循项目 skill 规则（className 只做布局、语义色、gap 不用 space-y、组合优于自造）。
