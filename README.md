# type-hime

AI 辅助中文网文长篇写作工作台（单机自用，本地优先，BYO API key）。
固定流水线（设定→总纲→卷纲→章纲→正文→审校）保证不烂尾；连续性账本 + 规则装配 + 可见性控制保证不吃书；AI 的一切修改经 patch→diff 人审→git commit，永不静默改稿。

范围契约见 [docs/SPEC.md](docs/SPEC.md)，选型记录见 [docs/DECISIONS.md](docs/DECISIONS.md)。

## 前置条件

- Rust 1.95+（MSVC 工具链）与 [Node.js](https://nodejs.org) 20+、pnpm
- git（作品仓库依赖命令行 git）
- Windows 需 WebView2 运行时（一般已内置）

## 运行

```bash
pnpm install
pnpm tauri dev     # 开发模式
pnpm tauri build   # 打包
```
