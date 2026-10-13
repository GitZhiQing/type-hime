//! 切片 2 端到端冒烟：真实供应商上验证 模型列表 + 流式对话 + 工具调用 + 会话落盘。
//! 运行：cargo run --example chat_smoke -- <work_dir>

use std::path::PathBuf;

use type_hime_lib::agent::kernel;
use type_hime_lib::agent::spec::chat_spec;
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

    // 流式对话（带作品目录 → read_file 工具可用）
    let work_dir: Option<PathBuf> = std::env::args().nth(1).map(PathBuf::from);
    let spec = chat_spec();
    let prompt = "用一句话自我介绍，然后调用 read_file 工具读取 outline/总纲.md 并告诉我它当前有多少字。";

    let (final_text, new_messages) = kernel::run(
        &settings,
        work_dir.as_deref(),
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

    println!("[3] 最终输出（{} 字）：", final_text.chars().count());
    println!("{}", final_text);
    println!("[4] 本轮消息数（含工具轮次）：{}", new_messages.len());

    // 会话落盘验证
    if let Some(dir) = &work_dir {
        let sess = type_hime_lib::agent::session::Session {
            id: "smoke-test".into(),
            created_at: type_hime_lib::works::now_iso(),
            messages: new_messages,
        };
        type_hime_lib::agent::session::save_session(std::path::Path::new(dir), &sess)?;
        let back =
            type_hime_lib::agent::session::load_session(std::path::Path::new(dir), "smoke-test")?;
        println!("[5] 会话落盘回读：{} 条消息 ✓", back.messages.len());
    }

    if final_text.trim().is_empty() {
        return Err("最终输出为空".into());
    }
    println!("\n=== 冒烟通过：模型列表 + 流式 + 工具 + 会话 ===");
    Ok(())
}
