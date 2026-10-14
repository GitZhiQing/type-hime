//! AgentSpec：声明式 Agent 定义（内核的组装契约）。
//! 新增 Agent 类型 = 声明一个 Spec +（可选）写一个 Skill，不写编排代码（D5）。

use serde::{Deserialize, Serialize};

/// 提示词分节：name → 内容，按插入顺序拼装（借鉴 dsh 的 PromptSection 排序装配）。
pub type Section = (String, String);

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentSpec {
    /// Agent 标识，也用于按 agent 配模型（settings.agent_overrides）
    pub id: String,
    /// 提示词分节（身份 → 文风 → 上下文…），装配引擎产物作为其中一节注入
    pub sections: Vec<Section>,
    /// 工具名列表（引用 tools.rs 注册表）
    pub tools: Vec<String>,
    /// 技能名列表（拼入 preamble 尾部）
    pub skills: Vec<String>,
    /// 模型覆盖（None 回落 default_model）
    pub model_override: Option<String>,
    /// 工具循环上限
    pub max_turns: usize,
    /// 同一消息内多个工具调用的执行并发（rig AgentRunner::tool_concurrency；R8-①，0/缺省=1 顺序）
    #[serde(default)]
    pub tool_concurrency: usize,
    /// 允许模型一回合发起多个工具调用（GenerationOptions；R8-①，默认 false）
    #[serde(default)]
    pub parallel_tool_calls: bool,
}

pub const CHAT_SPEC_SECTIONS: &str = r#"你是 type-hime（执笔公主）——中文网文长篇写作工作台的对话助手，服务一位正在创作百万字长篇的作者。

你的职责：
- 回答写作问题：设定推演、情节矛盾排查、人物弧光、文风打磨、头脑风暴。
- 用 read_file 工具查阅当前作品的设定、大纲、章节与账本，回答要有据可依。
- 结构化查询用专用工具，不要用 read_file 读 json/管理文件：get_ledger（账本结构化视图）、list_issues（审校问题清单与裁决状态）、list_lore（可见设定条目，隐藏/未到卷条目不可见也不可读）、check_visibility_leak / preflight_chapter（零 token 检查，不发模型请求）。
- 需要执行写作流程（生成章纲/正文、账本回写、一致性检查、去 AI 味、复检、卷级批审、账本整理）时，一律用 run_flow 工具触发——它走完整流水线（装配注入、零 token 预检、问题清单落盘、提议入审阅队列），不要用 read_file/propose_* 手工模拟流程。
- 需要查看 git 历史/仓库状态等只读信息时，可用 bash 工具（白名单：git 只读子命令与只读管道；写产物命令会被拦截，执行前作者可能要求确认）。命令保持短小明确。
- 你的任何落稿修改（改大纲、写章节、补设定、更新账本）必须通过 propose_edit / propose_write 工具提交提议，由作者在 diff 审阅中决定接受与否；你无法直接写盘，也不要谎称"已保存"。
- 一次提议给完整的修改后全文（不是片段）。
- 中文回答，直接给观点和方案，不说套话。"#;

/// 通用对话 Agent（旁路会话）：中栏会话流使用；产物经提议审阅回流。
/// 默认挂 writer-basics（REDESIGN §8.2：chat 默认技能），其余技能动态挂载（R4-④）。
pub fn chat_spec() -> AgentSpec {
    AgentSpec {
        id: "chat".to_string(),
        sections: vec![("identity".to_string(), CHAT_SPEC_SECTIONS.to_string())],
        tools: vec![
            "read_file".to_string(),
            "propose_edit".to_string(),
            "propose_write".to_string(),
            "bash".to_string(),
            "run_flow".to_string(),
            "get_ledger".to_string(),
            "list_issues".to_string(),
            "list_lore".to_string(),
            "check_visibility_leak".to_string(),
            "preflight_chapter".to_string(),
        ],
        skills: vec!["writer-basics".to_string()],
        model_override: None,
        max_turns: 12,
        tool_concurrency: 4,
        parallel_tool_calls: true,
    }
}

/// chat spec + 动态技能（slash 强制 / 关键词路由的产物；去重、保序）。
pub fn chat_spec_with_skills(extra: &[String]) -> AgentSpec {
    let mut spec = chat_spec();
    for name in extra {
        if !spec.skills.contains(name) {
            spec.skills.push(name.clone());
        }
    }
    spec
}

/// slash 强制技能映射（R4-④：slash 指定技能 = 强制挂载该技能；REDESIGN §8.2）。
pub fn slash_forced_skill(prompt: &str) -> Option<&'static str> {
    let head = prompt.trim_start();
    if !head.starts_with('/') {
        return None;
    }
    let cmd = head
        .split_whitespace()
        .next()
        .unwrap_or("")
        .trim_start_matches('/');
    Some(match cmd {
        "章纲" | "正文" => "writer-basics",
        "回写" => "ledger-update",
        "一致性" | "去AI味" | "复检" | "审卷" => "review-consistency",
        "整理账本" => "ledger-tidy",
        _ => return None,
    })
}

pub const GLOBAL_CHAT_SECTIONS: &str = r#"你是 type-hime 的通用对话助手（当前未打开任何作品，没有作品文件上下文）。

你的职责：
- 与作者自由交流：找灵感、头脑风暴、人物/情节构思、写作问答。
- 可以讨论任何话题，不限于当前项目；用户粘贴的内容照常分析。
- 没有文件工具，也不要假装能读写文件；涉及具体作品文件时建议用户进入该作品的会话。
- 中文回答，直接给观点和方案，不说套话。"#;

/// 全局对话 Agent（无作品上下文，rail「对话」页）：纯对话，无文件工具。
pub fn global_chat_spec() -> AgentSpec {
    AgentSpec {
        id: "chat".to_string(),
        sections: vec![("identity".to_string(), GLOBAL_CHAT_SECTIONS.to_string())],
        tools: Vec::new(),
        skills: Vec::new(),
        model_override: None,
        max_turns: 12,
        tool_concurrency: 1,
        parallel_tool_calls: false,
    }
}

pub const WRITER_IDENTITY: &str = r#"你是一位成熟的中文网文作者（执笔者），按流水线任务产出稿件。

铁律：
- 你的正式产出必须通过 propose_write（新建文件）或 propose_edit（修改文件）提交——直接写在回复里的正文不会落盘。
- 写正文前先确认你已看到：文风指令、卷纲、本章章纲、可见设定、账本摘要、上一章结尾。缺项要在回复中明确指出，不要凭空编造设定。
- 与账本冲突的既有事实一律以账本为准；不得提前说破未到回收点的伏笔。
- 章节文件只含正文（可带一个 # 章节标题行），不写任何说明性文字。"#;

/// 正文/章纲写作 Agent：装配产物作为 preamble 一节，装配上下文作为 user prompt。
pub fn writer_spec(assembly_system: &str) -> AgentSpec {
    AgentSpec {
        id: "writer".to_string(),
        sections: vec![
            ("identity".to_string(), WRITER_IDENTITY.to_string()),
            ("assembly".to_string(), assembly_system.to_string()),
        ],
        tools: vec![
            "read_file".to_string(),
            "propose_write".to_string(),
            "propose_edit".to_string(),
        ],
        skills: vec!["writer-basics".to_string()],
        model_override: None,
        max_turns: 14,
        tool_concurrency: 1,
        parallel_tool_calls: false,
    }
}

pub const REVIEWER_IDENTITY: &str = r#"你是责编（连续性裁判 + 去味编辑），对照账本与设定审校已完稿章节。

一致性检查规程：
- 逐条核对账本：角色状态（位置/修为/关系/持有物）、已交代事实、伏笔状态，找出正文与账本的矛盾。
- 核对可见设定条目：人设、世界观规则是否被违反。
- 输出编号问题清单：每条给出【问题】【证据（引用正文/账本原句）】【建议修复】。
- 确定无误的硬伤（与账本/设定直接矛盾）用 propose_edit 提交修复；拿不准的只列入清单等作者裁决。
- 全部核对无误时明确输出「未发现一致性问题」。

去 AI 味规程：
- 在文风指令约束下改写：删套话与万能形容、拆长句、去重复句式、把概括性叙述落到具体动作与细节。
- 不改变情节、人设与事实；不引入新设定。
- 改写后用 propose_edit 提交整章全文。"#;

/// 审校 Agent：一致性检查 / 去 AI 味。
pub fn reviewer_spec(assembly_system: &str) -> AgentSpec {
    AgentSpec {
        id: "reviewer".to_string(),
        sections: vec![
            ("identity".to_string(), REVIEWER_IDENTITY.to_string()),
            ("assembly".to_string(), assembly_system.to_string()),
        ],
        tools: vec!["read_file".to_string(), "propose_edit".to_string()],
        skills: vec!["review-consistency".to_string()],
        model_override: None,
        max_turns: 12,
        tool_concurrency: 1,
        parallel_tool_calls: false,
    }
}

pub const LEDGER_IDENTITY: &str = r#"你是连续性账本的记录员。刚完稿一章后，从正文提取状态变化，提议更新账本。

要求：
- 先 read 本章正文与当前账本，再 propose_edit 提交更新后的账本全文。
- 只记录正文中明确发生/交代的变化：角色状态（位置/修为/关系/持有物/目标）、新交代事实、伏笔埋设与回收。
- 不改写既有条目的历史记录，只更新状态或追加条目；被正文推翻的旧状态标注「已被第N章推翻」。
- 账本格式保持：## 角色状态 / ## 已交代事实 / ## 伏笔登记，条目用 ### 小节。"#;

/// 账本回写 Agent：提取状态变化提议更新账本。
pub fn ledger_spec() -> AgentSpec {
    AgentSpec {
        id: "ledger".to_string(),
        sections: vec![("identity".to_string(), LEDGER_IDENTITY.to_string())],
        tools: vec!["read_file".to_string(), "propose_edit".to_string()],
        skills: vec!["ledger-update".to_string()],
        model_override: None,
        max_turns: 12,
        tool_concurrency: 1,
        parallel_tool_calls: false,
    }
}

pub const LEDGER_TIDY_IDENTITY: &str = r#"你是账本管理员，执行定期整理（不改变事实，只改组织方式）。

先 read 当前账本全文，然后按 ledger-tidy 技能整理，最后 propose_edit 提交整理后的账本全文。
整理前后任何一条事实（状态/事实/伏笔内容）不得增删改写，只允许：合并同线索、归档已回收、
压缩休眠条目、补时间线、去噪、规范（ch-00N）标注。"#;

/// 账本整理 Agent：合并/归档/压缩/补时间线。
pub fn ledger_tidy_spec() -> AgentSpec {
    AgentSpec {
        id: "ledger-tidy".to_string(),
        sections: vec![("identity".to_string(), LEDGER_TIDY_IDENTITY.to_string())],
        tools: vec!["read_file".to_string(), "propose_edit".to_string()],
        skills: vec!["ledger-tidy".to_string()],
        model_override: None,
        max_turns: 6,
        tool_concurrency: 1,
        parallel_tool_calls: false,
    }
}
