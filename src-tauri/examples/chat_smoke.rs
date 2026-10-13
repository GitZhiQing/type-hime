//! 切片 2/5 端到端冒烟：真实供应商上验证 模型列表 + 流式对话 + 工具调用 + 会话落盘 + 提议审阅。
//! 运行：cargo run --example chat_smoke -- <work_dir>

use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use type_hime_lib::agent::kernel;
use type_hime_lib::agent::spec::chat_spec;
use type_hime_lib::agent::tools::ToolCtx;
use type_hime_lib::review::{self, ReviewQueue};
use type_hime_lib::settings;

fn main() -> Result<(), String> {
    let rt = tokio::runtime::Runtime::new().map_err(|e| e.to_string())?;
    match rt.block_on(run()) {
        Ok(()) => Ok(()),
        Err(e) => {
            eprintln!("[冒烟失败] {e}");
            std::process::exit(1);
        }
    }
}

async fn run() -> Result<(), String> {
    let settings = settings::load()?;
    let provider = settings.active().ok_or("无 active 供应商")?;
    println!("[1] 供应商：{} @ {}", provider.name, provider.base_url);

    // 模型列表
    let models = kernel::list_models(&provider.base_url, &provider.api_key).await?;
    println!("[2] 模型列表（{} 个）：{}", models.len(), models.join(", "));

    // 工具上下文：共享提议队列
    let queue: Arc<Mutex<ReviewQueue>> = Arc::new(Mutex::new(ReviewQueue::default()));
    let tool_ctx = ToolCtx {
        work_root: std::env::args()
            .nth(1)
            .map(PathBuf::from)
            .unwrap_or_else(|| PathBuf::from(".")),
        queue: Arc::clone(&queue),
        on_proposal: Arc::new(|p| println!("    [新提议 #{}] {}：{}", p.id, p.rel_path, p.reason)),
        expect: None,
    };
    let work_dir = tool_ctx.work_root.clone();

    let spec = chat_spec();
    let prompt = "读取 outline/总纲.md，然后调用 propose_edit 工具，在总纲末尾追加一行\"## 冒烟测试\"（给出修改后全文），理由写\"冒烟测试修改\"。";

    let outcome_final_text = kernel::run(
        &settings,
        Some(&tool_ctx),
        &spec,
        Vec::new(),
        prompt,
        |ev| {
            if let Ok(s) = serde_json::to_string(&ev) {
                let brief: String = s.chars().take(160).collect();
                println!("    事件 {brief}");
            }
        },
    )
    .await?;
let final_text = outcome_final_text.text;
let new_messages = outcome_final_text.messages;

    println!("[3] 最终输出（{} 字）：", final_text.chars().count());
    println!("{}", final_text);
    println!("[4] 本轮消息数（含工具轮次）：{}", new_messages.len());

    // 会话落盘验证
    let sess = type_hime_lib::agent::session::Session {
        id: "smoke-test".into(),
        created_at: type_hime_lib::works::now_iso(),
        messages: new_messages,
        usage: None,
    };
    type_hime_lib::agent::session::save_session(&work_dir, &sess)?;
    let back = type_hime_lib::agent::session::load_session(&work_dir, "smoke-test")?;
    println!("[5] 会话落盘回读：{} 条消息 ✓", back.messages.len());

    // 提议审阅：接受后落盘 + git 留痕
    let proposals = queue.lock().unwrap().list();
    println!("[6] 提议队列：{} 条", proposals.len());
    for p in &proposals {
        println!("    #{} {}（diff {} 行）", p.id, p.rel_path, p.diff.lines().count());
    }
    if let Some(p) = proposals.first() {
        let id = p.id;
        let taken = queue.lock().unwrap().take(id).unwrap();
        let msg = review::apply(&work_dir, taken)?;
        println!("[7] 应用提议：{msg}");
        let log = type_hime_lib::git::log_oneline(&work_dir, 5)?;
        println!("[8] git 历史：{}", log.first().map(|s| s.as_str()).unwrap_or("-"));
        let content = type_hime_lib::works::read_file(&work_dir, "outline/总纲.md")?;
        assert!(content.contains("冒烟测试"), "提议应用后内容应含新行");
        println!("[9] 落盘内容校验 ✓");
    } else {
        return Err("agent 未产生提议".into());
    }

    if final_text.trim().is_empty() {
        return Err("最终输出为空".into());
    }
    println!("\n=== 冒烟通过：模型列表 + 流式 + 工具 + 会话 + 提议审阅落盘 ===");
    Ok(())
}
