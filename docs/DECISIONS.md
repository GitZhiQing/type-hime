# DECISIONS

轻量 ADR，一行式，只追加。决策被推翻时追加新条目并注明"取代 Dx"，不删旧条目。重构或选型前必读；与生效条目冲突的改动先向用户提出。

- D1 · 2026-10-13 · 包管理器用 pnpm（v10） · 为什么：本机就绪、磁盘高效；npm registry 已配 npmmirror 镜像。
- D2 · 2026-10-13 · 作品仓库的 git 操作走命令行 git（std::process 调用） · 为什么：机器已装 git 2.55；diff/commit 行为所见即所得；避免 git2-rs 的 libgit2 C 编译依赖与自构造 diff。前置条件：使用本应用需安装 git。取代备选：git2-rs。
- D3 · 2026-10-13 · 模型接入用 rig facade 0.44（含 agent 运行时，features 默认带 derive/rustls/reqwest） · 为什么：需要 AgentBuilder + 工具调用 + 多轮流式；rig-core 0.43 起大重构（旧 API 全废），0.44 为当前最新（2026-10-07 发布）。接任意 OpenAI 兼容服务：`OpenAIConfig::new(key).with_base_url(..).with_route(Route::Chat).client().chat(model)`——兼容服务（DeepSeek 等）只有 /chat/completions，必须 Route::Chat。注意：rig GitHub README 版本号 stale；OpenAIConfig 无 with_api_key（换 key 重建 config）。
- D4 · 2026-10-13 · 作品目录：每作品 = 一个 git 仓库；产物全部 md + YAML frontmatter；作品状态存 project.json；会话历史存 .sessions/（gitignore） · 为什么：git 可回溯一切落盘产物；状态机状态非叙事产物，用 json；会话历史不是产物，不进版本库。
- D5 · 2026-10-13 · Agent 内核：一切 LLM 编排经 agent/ 模块（AgentSpec 声明式定义 preamble/tools/skills/model + 工具注册表 + skills 加载器 + 会话管理 + 统一流式事件）；新增 Agent 类型 = 声明 Spec +（可选）写 Skill，不写编排代码 · 为什么：用户拍板"内核先行，后续基于内核扩展其他类型 Agent"；参考 deepseek-harness "Everything is a Plugin" 的内核/插件分层。
- D6 · 2026-10-13 · 编辑工具模式：agent 的 ProposeEdit/ProposeWrite 工具不写盘，生成待审提议入队 → 前端 diff 审阅 → 接受=写盘+git commit，拒绝=丢弃 · 为什么：落实 SPEC D0-6"AI 永远不静默改稿"；工具返回"已入审阅队列"给模型，模型可继续工作。
- D7 · 2026-10-13 · API key 只存 Rust 侧（app data 目录 settings.json），前端仅见打码值 · 为什么：BYO key 定位（SPEC D0-1）+ 切片 2 验收"key 不进渲染层"。
- D8 · 2026-10-13 · 流式输出用 Tauri 2 ipc::Channel 逐块推送 · 为什么：Channel 是 Tauri 2 为流式设计的单向通道，比全局 event emit 干净（不广播）。
- D9 · 2026-10-13 · token 预算用本地近似估算（CJK 字符≈1 token，其他≈4 字符/token），UI 标注"估算" · 为什么：rig 无本地 tokenizer；装配预览需在发送前估数，近似法对预算条足够。事后真实用量可从 rig Usage 读取对账。
- D10 · 2026-10-13 · MSVC 工具链：本机 VS2019 BuildTools (v142) 可编译 rustc 1.96 / rig 0.44；遇新版依赖要求 v143 再升级 · 为什么：当前够用，不预支环境变更。
