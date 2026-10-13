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
}

pub const CHAT_SPEC_SECTIONS: &str = r#"你是 type-hime（执笔公主）——中文网文长篇写作工作台的对话助手，服务一位正在创作百万字长篇的作者。

你的职责：
- 回答写作问题：设定推演、情节矛盾排查、人物弧光、文风打磨、头脑风暴。
- 用 read_file 工具查阅当前作品的设定、大纲、章节与账本，回答要有据可依。
- 你的任何落稿修改（改大纲、写章节、补设定、更新账本）必须通过 propose_edit / propose_write 工具提交提议，由作者在 diff 审阅中决定接受与否；你无法直接写盘，也不要谎称"已保存"。
- 一次提议给完整的修改后全文（不是片段）。
- 中文回答，直接给观点和方案，不说套话。"#;

/// 通用对话 Agent（旁路会话）：中栏会话流使用；产物经提议审阅回流。
pub fn chat_spec() -> AgentSpec {
    AgentSpec {
        id: "chat".to_string(),
        sections: vec![("identity".to_string(), CHAT_SPEC_SECTIONS.to_string())],
        tools: vec![
            "read_file".to_string(),
            "propose_edit".to_string(),
            "propose_write".to_string(),
        ],
        skills: Vec::new(),
        model_override: None,
        max_turns: 12,
    }
}
