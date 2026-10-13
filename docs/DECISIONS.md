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

---（以下为写书测试暴露问题的修复批次，2026-10-14）---

- D11 · 2026-10-14 · 装配裁剪三机制：出场过滤（章纲命中条目名；世界观不参与过滤——全书背景；可过滤条目零命中回退全量）+ 账本分层休眠（条目 body 带 ch-00N 标注，最近 N=3 章活跃全文、休眠仅标题、伏笔标题常驻防说破；未标注视为活跃）+ 预算自动降档（Full→NameOnly→Compact，Compact 同时压账本窗口至 0；降后仍超才拦截） · 为什么：写书测试装配 token 每章+900 线性增长，外推 15 章触顶；根因是全量注入，SPEC 的"出场角色卡"此前未真正落地。
- D12 · 2026-10-14 · 账本加「## 时间线」小节（### 第N章 → 距开篇天数 + 事件），装配注入最近 5 个锚点并标注硬约束；解析宽容列表前缀（"- ### 第N章"）· 为什么：时间无条目承载导致"快一个月 vs 十夜"类漂移只能靠审校事后捞；账本从正文提取又被当事实源，连续量错误会自举固化。
- D13 · 2026-10-14 · 一致性检查部分下沉为代码必查：preflight.rs 修为层级预检（正文句中"角色名+炼气N层"与账本比对，中文/阿拉伯数字、筑基/金丹），零 token；审校自动预检并注入 reviewer · 为什么：一部分一致性不该依赖模型眼力。
- D14 · 2026-10-14 · 审校问题清单结构化：报告按编号拆条落盘 review/issues.json（进 git），裁决（fixed/dismissed/needs_ruling）持久化；复检注入上轮清单+裁决逐项核验；支持卷级批审 · 为什么：清单原为自由文本无状态无裁决落点，"审校通过"永远无法达成。
- D15 · 2026-10-14 · 提议队列持久化于 app data/proposals.json（Proposal 带 work_path 跨作品过滤），应用启动恢复 · 为什么：长正文生成 2-3 分钟，重启丢提议=token 白花。
- D16 · 2026-10-14 · 写入轻校验即工具校验器：propose_write/propose_edit 按期望（字数区间+中文语境半角标点）校验，不合规返回错误文本让模型当场自改 · 为什么：质量前置拦截比审校兜底便宜（一次工具往返 vs 一次审校 pass）。
- D17 · 2026-10-14 · 关联泄漏检查：隐藏条目与可见条目正文 4-gram 重叠 ≥4（≈7字连续复述）报警，提示性非阻断，剥离 frontmatter 防元数据假重叠 · 为什么：可见性是条目级粒度，管不住可见条目内容复述隐藏条目（实测"林晚"条目泄露"黑剑反哺"秘密）。
- D18 · 2026-10-14 · Web 测试通道：业务层双传输入口——flows.rs 承载全部流式命令体（回调参数化），Tauri 走 IPC/Channel/emit，浏览器走 axum HTTP+SSE（examples/webapi.rs，仅绑 127.0.0.1:18360，vite /api 代理）；前端 tauri.ts 传输分流（invoke↔fetch、Channel↔SSE 解析、listen↔EventSource），上层组件零感知 · 为什么：桌面端无法被 Web 工具自动化测试，此前 UI 验证只有构建层+骨架走查；通道测的是真实后端（真 git/真装配/真模型流式）而非 mock。已用它完成浏览器内全闭环验证（建作品→SSE 生成→工具轨迹→轻校验打回重提→diff 审阅→接受落盘→git 留痕→成本账刷新）。
