//! Agent 内核（D5）：spec / kernel / tools / skills / session / events。
//! 一切 LLM 编排只经过这里；新增 Agent 类型 = 声明 Spec +（可选）写 Skill。

pub mod approval;
pub mod bash;
pub mod events;
pub mod goal;
pub mod kernel;
pub mod session;
pub mod skills;
pub mod spec;
pub mod subtask;
pub mod tools;
