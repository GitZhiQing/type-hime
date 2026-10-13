//! 切片 7/8 端到端冒烟：真实模型上走通 章纲生成→审阅应用→正文生成→审阅应用→账本回写→审阅应用。
//! 运行：cargo run --example full_flow -- <work_dir>

use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use type_hime_lib::agent::kernel;
use type_hime_lib::agent::spec::{ledger_spec, writer_spec};
use type_hime_lib::agent::tools::ToolCtx;
use type_hime_lib::assembly;
use type_hime_lib::pipeline::Stage;
use type_hime_lib::review::{self, ReviewQueue};
use type_hime_lib::settings;

fn main() -> Result<(), String> {
    let rt = tokio::runtime::Runtime::new().map_err(|e| e.to_string())?;
    match rt.block_on(run()) {
        Ok(()) => Ok(()),
        Err(e) => {
            eprintln!("[全流程冒烟失败] {e}");
            std::process::exit(1);
        }
    }
}

fn step(n: &str, msg: &str) {
    println!("\n===== [{n}] {msg} =====");
}

async fn run() -> Result<(), String> {
    let work_dir: PathBuf = std::env::args()
        .nth(1)
        .map(PathBuf::from)
        .ok_or("用法：full_flow <work_dir>（需为已含设定与卷纲的作品目录）")?;
    let settings = settings::load()?;
    if settings.active().is_none() {
        return Err("无 active 供应商".into());
    }
    let queue: Arc<Mutex<ReviewQueue>> = Arc::new(Mutex::new(ReviewQueue::default()));
    let vol = 1u32;
    let chapter = "ch-001".to_string();

    let tool_ctx = |queue: &Arc<Mutex<ReviewQueue>>| ToolCtx {
        work_root: work_dir.clone(),
        queue: Arc::clone(queue),
        on_proposal: Arc::new(|p: &review::Proposal| {
            println!("    [提议 #{}] {}（{}）", p.id, p.rel_path, p.reason);
        }),
    };

    // ---------- 1. 章纲生成 ----------
    step("1", "装配 + writer agent 生成章纲");
    let mut opts = assembly::AssemblyOptions::default();
    opts.volume = vol;
    opts.chapter = chapter.clone();
    opts.stage = Stage::ChapterOutline;
    let assembled = assembly::assemble(&work_dir, &opts)?;
    println!("装配 token 估算：{}", assembled.total_tokens);
    let outline_path = assembly::chapter_outline_path(vol, &chapter);
    let mut spec = writer_spec(&assembled.system);
    spec.sections.push((
        "task".into(),
        format!("撰写本章章纲（用 propose_write 提交，路径见下）\n目标文件：{outline_path}"),
    ));
    let prompt = format!(
        "{}\n\n# 输出文件\n{outline_path}\n\n请开始工作：完成后用 propose_write 提交全文。",
        assembly::render_prompt(&assembled)
    );
    let mut _text = String::new();
    let (text, _) = kernel::run(&settings, Some(&tool_ctx(&queue)), &spec, Vec::new(), &prompt, |_| {}).await?;
    _text = text;
    let (proposals_before, ) = (queue.lock().unwrap().list().len(), );
    let p = take_latest(&queue, &outline_path)?;
    let msg = review::apply(&work_dir, p)?;
    println!("章纲应用：{msg}（本轮新增提议 {proposals_before} 条）");
    assert!(Path::new(&work_dir).join(&outline_path).is_file(), "章纲未落盘");

    // ---------- 2. 正文生成 ----------
    step("2", "装配（含章纲）+ writer agent 生成正文");
    opts.stage = Stage::Chapters;
    let assembled = assembly::assemble(&work_dir, &opts)?;
    println!("装配 token 估算：{}", assembled.total_tokens);
    let ch_path = assembly::chapter_path(vol, &chapter);
    let mut spec = writer_spec(&assembled.system);
    spec.sections.push((
        "task".into(),
        format!("撰写本章正文全文（用 propose_write 提交，路径见下）\n目标文件：{ch_path}"),
    ));
    let prompt = format!(
        "{}\n\n# 输出文件\n{ch_path}\n\n请开始工作：完成后用 propose_write 提交全文（1500 字左右）。",
        assembly::render_prompt(&assembled)
    );
    kernel::run(&settings, Some(&tool_ctx(&queue)), &spec, Vec::new(), &prompt, |ev| {
        if let type_hime_lib::agent::events::AgentEvent::ToolCall { name, .. } = ev {
            println!("    工具：{name}");
        }
    })
    .await?;
    let p = take_latest(&queue, &ch_path)?;
    let msg = review::apply(&work_dir, p)?;
    println!("正文应用：{msg}");
    let content = type_hime_lib::works::read_file(&work_dir, &ch_path)?;
    let chars = content.chars().count();
    println!("正文字数：{chars}");
    assert!(chars > 500, "正文过短：{chars}");

    // ---------- 3. 账本回写 ----------
    step("3", "ledger agent 提议账本回写");
    let ledger_path = type_hime_lib::ledger::LEDGER_PATH;
    let prompt = format!(
        "第{vol}卷 {chapter} 已完稿。请先 read_file 读取正文 {ch_path} 与账本 {ledger_path}，\
         然后 propose_edit 提交更新后的账本全文（路径 {ledger_path}）。"
    );
    let spec = ledger_spec();
    kernel::run(&settings, Some(&tool_ctx(&queue)), &spec, Vec::new(), &prompt, |ev| {
        if let type_hime_lib::agent::events::AgentEvent::ToolCall { name, .. } = ev {
            println!("    工具：{name}");
        }
    })
    .await?;
    let p = take_latest(&queue, ledger_path)?;
    let msg = review::apply(&work_dir, p)?;
    println!("账本应用：{msg}");
    let ledger_raw = type_hime_lib::works::read_file(&work_dir, ledger_path)?;
    let ledger = type_hime_lib::ledger::parse(&ledger_raw);
    println!(
        "账本条目：角色 {} / 事实 {} / 伏笔 {}",
        ledger.characters.len(),
        ledger.facts.len(),
        ledger.foreshadows.len()
    );
    assert!(
        !ledger.characters.is_empty(),
        "账本回写后应有角色状态条目"
    );

    // ---------- 4. 下一章装配引用新账本 ----------
    step("4", "下一章装配应引用新账本状态");
    opts.chapter = "ch-002".into();
    let assembled2 = assembly::assemble(&work_dir, &opts)?;
    let prompt2 = assembly::render_prompt(&assembled2);
    let ledger_summary = type_hime_lib::ledger::render_summary(&ledger);
    let sample = ledger.characters[0].title.clone();
    assert!(
        prompt2.contains(&sample),
        "ch-002 的装配应包含账本条目「{sample}」"
    );
    let _ = ledger_summary;
    println!("ch-002 装配已包含账本条目「{sample}」 ✓（token {}）", assembled2.total_tokens);

    // ---------- 5. git 历史 ----------
    let log = type_hime_lib::git::log_oneline(&work_dir, 10)?;
    println!("\n===== git 历史 =====");
    for l in &log {
        println!("  {l}");
    }
    assert!(log.iter().any(|l| l.contains("提议")), "git 应有提议提交");

    println!("\n=== 全流程冒烟通过：章纲→正文→账本回写→下一章引用新状态 ===");
    Ok(())
}

fn take_latest(queue: &Arc<Mutex<ReviewQueue>>, path: &str) -> Result<review::Proposal, String> {
    let mut q = queue.lock().unwrap();
    let id = q
        .list()
        .iter()
        .filter(|p| p.rel_path == path)
        .map(|p| p.id)
        .max()
        .ok_or(format!("没有 {path} 的提议（agent 未提交？）"))?;
    // take 指定 id，顺带清掉同路径旧提议
    let target = q.take(id).unwrap();
    let stale: Vec<u64> = q.list().iter().map(|p| p.id).collect();
    for sid in stale {
        let _ = q.take(sid);
    }
    Ok(target)
}
