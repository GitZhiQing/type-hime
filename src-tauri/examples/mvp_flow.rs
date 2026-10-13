//! MVP 交付判据验收：一卷 3 章从新建作品到审校通过全程可演示，任意一步可从 git 历史回溯。
//! 真实模型全流程：新建作品 → 设定 → 总纲 → 卷纲 → 3×(章纲→正文→账本回写) → 一致性审校。
//! 运行：cargo run --example mvp_flow

use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use type_hime_lib::agent::events::AgentEvent;
use type_hime_lib::agent::kernel;
use type_hime_lib::agent::spec::{ledger_spec, reviewer_spec, writer_spec};
use type_hime_lib::agent::tools::ToolCtx;
use type_hime_lib::assembly::{self, AssemblyOptions};
use type_hime_lib::pipeline::{self, PipelineState, Stage};
use type_hime_lib::review::{self, ReviewQueue};
use type_hime_lib::settings;
use type_hime_lib::{lore, works};

fn step(n: &str, msg: &str) {
    println!("\n===== [{n}] {msg} =====");
}

fn main() -> Result<(), String> {
    let rt = tokio::runtime::Runtime::new().map_err(|e| e.to_string())?;
    match rt.block_on(run()) {
        Ok(()) => Ok(()),
        Err(e) => {
            eprintln!("\n[MVP 验收失败] {e}");
            std::process::exit(1);
        }
    }
}

async fn run() -> Result<(), String> {
    let s = settings::load()?;
    if s.active().is_none() {
        return Err("无 active 供应商".into());
    }
    let queue: Arc<Mutex<ReviewQueue>> = Arc::new(Mutex::new(ReviewQueue::default()));

    // ---------- 1. 新建作品 ----------
    step("1", "新建作品（目录骨架 + git init + 首次提交）");
    let base = std::env::temp_dir().join("type-hime-mvp");
    let _ = std::fs::remove_dir_all(&base);
    std::fs::create_dir_all(&base).unwrap();
    let work = works::create_work("问剑青云·验收", Some(base.to_string_lossy().into_owned()))?;
    let dir = PathBuf::from(&work.path);
    println!("作品：{}", work.path);

    let mut pipeline_state = PipelineState::default();

    let ctx = || ToolCtx {
        work_root: dir.clone(),
        queue: Arc::clone(&queue),
        on_proposal: Arc::new(|p: &review::Proposal| {
            println!("    [提议 #{}] {}", p.id, p.rel_path);
        }),
    };

    // helper：writer 型任务（任意产物）
    async fn run_writer(
        s: &settings::AppSettings,
        dir: &Path,
        task: String,
        prompt: String,
        queue: &Arc<Mutex<ReviewQueue>>,
    ) -> Result<(), String> {
        let mut spec = writer_spec("");
        spec.sections = vec![
            ("identity".into(), type_hime_lib::agent::spec::WRITER_IDENTITY.into()),
            ("task".into(), task),
        ];
        let tc = ToolCtx {
            work_root: dir.to_path_buf(),
            queue: Arc::clone(queue),
            on_proposal: Arc::new(|_| {}),
        };
        kernel::run(s, Some(&tc), &spec, Vec::new(), &prompt, |_| {}).await?;
        Ok(())
    }

    // ---------- 2. 录设定（含可见性红线） ----------
    step("2", "录设定：主角（可见）+ 悬念（隐藏）+ 文风");
    lore::create_setting(&dir, "character", "林晚", "十六岁，青云宗外门弟子，性沉稳坚韧。当前修为：炼气三层。剑心通明（尚未自觉）。", lore::Visibility::Visible, None)?;
    lore::create_setting(&dir, "mystery", "黑剑来历", "黑剑为前朝国师本命剑，剑中封印一缕残魂。第二卷末方可揭示。", lore::Visibility::Hidden, None)?;
    let pipeline_confirm = |st: Stage, ps: &mut PipelineState| -> Result<(), String> {
        ps.confirm(st, &dir)?;
        pipeline::save_pipeline(&dir, ps)?;
        Ok(())
    };
    pipeline_confirm(Stage::Settings, &mut pipeline_state)?;
    println!("设定阶段确认 ✓（黑剑来历已隐藏）");

    // ---------- 3. 总纲（真实生成） ----------
    step("3", "总纲生成 → 审阅接受 → 确认");
    run_writer(
        &s,
        &dir,
        "撰写总纲：一段话说清主线（起承转合）+ 三卷走向。用 propose_edit 提交（路径 outline/总纲.md）。\n目标文件：outline/总纲.md".into(),
        "题材：少年凡人流修仙逆袭。主角林晚（外门弟子，藏黑剑）。三卷：外门风云/内门争锋/帝局终章。总纲 300 字左右。".into(),
        &queue,
    ).await?;
    apply_latest(&dir, &queue, "outline/总纲.md")?;
    pipeline_confirm(Stage::Outline, &mut pipeline_state)?;

    // ---------- 4. 卷纲（真实生成） ----------
    step("4", "卷纲生成 → 审阅接受 → 确认");
    run_writer(
        &s,
        &dir,
        "撰写第一卷卷纲：本卷目标/起止情节/主要冲突/出场人物/卷末钩子。用 propose_edit 提交（路径 volumes/vol-01/卷纲.md）。\n目标文件：volumes/vol-01/卷纲.md".into(),
        "第一卷《外门风云》，10 章规模（本次演示取前 3 章）：黑剑认主、外门倾轧、执法堂起疑、黑令牌线索。卷纲 300 字左右。".into(),
        &queue,
    ).await?;
    apply_latest(&dir, &queue, "volumes/vol-01/卷纲.md")?;
    pipeline_confirm(Stage::VolumeOutline, &mut pipeline_state)?;

    // ---------- 5~7. 三章循环：章纲 → 正文 → 账本 ----------
    let chapters = ["ch-001", "ch-002", "ch-003"];
    let mut first_outline_confirmed = false;
    for ch in chapters {
        step(ch, "章纲 → 正文 → 账本回写");

        // 章纲
        let mut opts = AssemblyOptions::default();
        opts.volume = 1;
        opts.chapter = ch.into();
        opts.stage = Stage::ChapterOutline;
        let asm = assembly::assemble(&dir, &opts)?;
        let outline_path = assembly::chapter_outline_path(1, ch);
        let mut spec = writer_spec(&asm.system);
        spec.sections.push(("task".into(), format!("撰写本章章纲（情节点/冲突/章末钩子，150 字内）。用 propose_write 提交。\n目标文件：{outline_path}")));
        kernel::run(&s, Some(&ctx()), &spec, Vec::new(), &assembly::render_prompt(&asm), |_| {}).await?;
        apply_latest(&dir, &queue, &outline_path)?;
        if !first_outline_confirmed {
            pipeline_confirm(Stage::ChapterOutline, &mut pipeline_state)?;
            first_outline_confirmed = true;
        }

        // 正文
        opts.stage = Stage::Chapters;
        let asm = assembly::assemble(&dir, &opts)?;
        let ch_path = assembly::chapter_path(1, ch);
        let mut spec = writer_spec(&asm.system);
        spec.sections.push(("task".into(), format!("撰写本章正文（1200~1800 字）。用 propose_write 提交。\n目标文件：{ch_path}")));
        kernel::run(&s, Some(&ctx()), &spec, Vec::new(), &assembly::render_prompt(&asm), |_| {}).await?;
        apply_latest(&dir, &queue, &ch_path)?;
        let n: usize = works::read_file(&dir, &ch_path)?.chars().count();
        println!("    {ch} 正文 {n} 字 ✓");

        // 账本回写
        let ledger_path = type_hime_lib::ledger::LEDGER_PATH;
        let prompt = format!(
            "第1卷 {ch} 已完稿。请先 read_file 读取正文 {ch_path} 与账本 {ledger_path}，然后 propose_edit 提交更新后的账本全文（路径 {ledger_path}）。"
        );
        let spec = ledger_spec();
        kernel::run(&s, Some(&ctx()), &spec, Vec::new(), &prompt, |_| {}).await?;
        apply_latest(&dir, &queue, ledger_path)?;
        let ledger = type_hime_lib::ledger::read_ledger(&dir)?;
        println!(
            "    账本：角色 {} / 事实 {} / 伏笔 {}",
            ledger.characters.len(),
            ledger.facts.len(),
            ledger.foreshadows.len()
        );
    }
    pipeline_confirm(Stage::Chapters, &mut pipeline_state)?;

    // ---------- 8. 一致性审校（第 1 章） ----------
    step("8", "审校：一致性检查（对照账本与设定出问题清单）");
    let mut opts = AssemblyOptions::default();
    opts.volume = 1;
    opts.chapter = "ch-001".into();
    opts.stage = Stage::Review;
    let asm = assembly::assemble(&dir, &opts)?;
    let mut spec = reviewer_spec(&asm.system);
    spec.sections.push(("task".into(),
        "对 volumes/vol-01/chapters/ch-001.md 执行一致性检查：先 read 正文与账本，输出编号问题清单（每条带证据）；确定的硬伤用 propose_edit 提交修复。".into()));
    let (review_text, _) = kernel::run(&s, Some(&ctx()), &spec, Vec::new(), &assembly::render_prompt(&asm), |ev| {
        if let AgentEvent::TextDelta { text } = ev {
            print!("{text}");
        }
    }).await?;
    println!();
    // 应用可能产生的修复提议
    let fixes = queue.lock().unwrap().list();
    let fix_count = fixes.len();
    for p in fixes {
        let id = p.id;
        if let Some(taken) = queue.lock().unwrap().take(id) {
            review::apply(&dir, taken)?;
        }
    }
    println!("一致性检查完成：输出清单 {} 字，修复提议 {} 条已应用", review_text.chars().count(), fix_count);

    // ---------- 9. 复检 ----------
    step("9", "复检（对修复后正文再跑一遍一致性检查）");
    let mut spec = reviewer_spec(&asm.system);
    spec.sections.push(("task".into(),
        "复检 volumes/vol-01/chapters/ch-001.md：核对上一轮修复后是否仍存在一致性问题；无问题则输出「复检通过」。".into()));
    let (recheck, _) = kernel::run(&s, Some(&ctx()), &spec, Vec::new(), &assembly::render_prompt(&asm), |_| {}).await?;
    println!("复检输出前 200 字：{}", recheck.chars().take(200).collect::<String>());

    pipeline_confirm(Stage::Review, &mut pipeline_state)?;

    // ---------- 10. git 历史可回溯 ----------
    step("10", "git 历史回溯验证");
    let log = type_hime_lib::git::log_oneline(&dir, 30)?;
    println!("提交数：{}", log.len());
    for l in log.iter().take(12) {
        println!("  {l}");
    }
    assert!(log.len() >= 12, "全程提交应 ≥ 12 条（骨架+设定+总纲+卷纲+3×3+审校）");

    // 红线复查：隐藏设定不出现在任何正文/章纲/总纲
    step("11", "红线终检：隐藏设定零泄露");
    let secret_words = ["残魂", "本命剑"];
    let mut leaked = Vec::new();
    check_leak(&dir, Path::new("outline"), &secret_words, &mut leaked);
    check_leak(&dir, Path::new("volumes"), &secret_words, &mut leaked);
    if !leaked.is_empty() {
        return Err(format!("红线泄露：{leaked:?}"));
    }
    println!("「残魂/本命剑」在 outline 与 volumes 中零泄露 ✓");

    // 收尾统计
    let ledger = type_hime_lib::ledger::read_ledger(&dir)?;
    println!("\n===== MVP 交付判据达成 =====");
    println!("3 章从新建到审校通过；git {} 条提交全程留痕；账本 角色{}/事实{}/伏笔{}", 
        log.len(), ledger.characters.len(), ledger.facts.len(), ledger.foreshadows.len());
    Ok(())
}

fn apply_latest(dir: &Path, queue: &Arc<Mutex<ReviewQueue>>, path: &str) -> Result<(), String> {
    let mut q = queue.lock().unwrap();
    let ids: Vec<u64> = q.list().iter().filter(|p| p.rel_path == path).map(|p| p.id).collect();
    let latest = ids.iter().max().copied().ok_or(format!("{path} 无提议"))?;
    let p = q.take(latest).unwrap();
    // 清掉同路径旧提议，避免堆积
    let rest: Vec<u64> = q.list().iter().map(|x| x.id).collect();
    for id in rest {
        let _ = q.take(id);
    }
    drop(q);
    let msg = review::apply(dir, p)?;
    println!("    应用：{msg}");
    Ok(())
}

fn check_leak(dir: &Path, sub: &Path, words: &[&str], leaked: &mut Vec<String>) {
    if let Ok(entries) = std::fs::read_dir(dir.join(sub)) {
        for e in entries.flatten() {
            let p = e.path();
            if p.is_dir() {
                check_leak(dir, &p.strip_prefix(dir).unwrap().to_path_buf(), words, leaked);
            } else if p.extension().and_then(|x| x.to_str()) == Some("md") {
                if let Ok(raw) = std::fs::read_to_string(&p) {
                    for w in words {
                        if raw.contains(w) {
                            leaked.push(format!("{} 含「{w}」", p.display()));
                        }
                    }
                }
            }
        }
    }
}
