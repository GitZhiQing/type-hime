//! 写书测试：在用户手动创建的作品上完成一卷 6 章的真实写作全流程。
//!
//! 流程：录设定（含隐藏伏笔）→ 总纲 → 卷纲 → 6×(章纲→正文→账本回写)
//! → 中途（ch-003 后）模拟作者补充新角色设定
//! → ch-005 后故意埋入与账本矛盾的段落 → 一致性审校抓错 → 修复 → 复检。
//!
//! 运行：cargo run --example write_book -- <work_dir>

use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use type_hime_lib::agent::kernel;
use type_hime_lib::agent::spec::{ledger_spec, reviewer_spec, writer_spec, WRITER_IDENTITY};
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
            eprintln!("\n[写书测试失败] {e}");
            std::process::exit(1);
        }
    }
}

async fn run() -> Result<(), String> {
    let dir: PathBuf = std::env::args()
        .nth(1)
        .map(PathBuf::from)
        .ok_or("用法：write_book <work_dir>")?;
    if !dir.join("project.json").exists() {
        return Err(format!("{} 不是作品目录", dir.display()));
    }
    let s = settings::load()?;
    if s.active().is_none() {
        return Err("无 active 供应商（先在应用设置页配置）".into());
    }
    let queue: Arc<Mutex<ReviewQueue>> = Arc::new(Mutex::new(ReviewQueue::in_memory()));
    let mut pipeline_state = pipeline::load_pipeline(&dir)?;
    let mut token_curve: Vec<(String, u64)> = Vec::new();

    let ctx = || {
        ToolCtx::new(
            dir.clone(),
            Arc::clone(&queue),
            Arc::new(|p: &review::Proposal| {
                println!("    [提议 #{}] {}", p.id, p.rel_path);
            }),
        )
    };

    let confirm = |st: Stage, ps: &mut PipelineState| -> Result<(), String> {
        ps.confirm(st, &dir)?;
        pipeline::save_pipeline(&dir, ps)?;
        Ok(())
    };

    // ---------- 1. 录设定与文风（自洽种子） ----------
    step("1", "录设定：3 角色 + 世界观 + 2 伏笔（1 隐藏）+ 文风指令");
    works::write_file(
        &dir,
        "style/文风指令.md",
        "---\ntype: style\n---\n# 文风指令\n\n- 语言：简体中文，网文快节奏，画面感强\n- 叙事：第三人称限知（林晚视角为主），多用具体动作与感官细节，少抽象形容\n- 节奏：每章 2000~3000 字，章末必留钩子\n- 对话：口语化、有性格区分度；师姐沈青梧说话简短直接，师兄赵铁柱粗中有细\n- 禁忌：不写说教式旁白；伏笔不得提前说破\n",
    )?;
    lore::create_setting(&dir, "character", "林晚",
        "男主，16 岁，青云宗外门杂役。无灵根，靠黑剑反哺的丝丝凉气维持修行，当前炼气三层。性沉稳坚韧，藏拙。目标：查明黑剑来历，在宗门站稳。",
        lore::Visibility::Visible, None)?;
    lore::create_setting(&dir, "character", "赵铁柱",
        "林晚同屋杂役，虎背熊腰，炼气二层。粗中有细，重义气。因剑坯克扣事件与林晚结为兄弟。弱点：性子急，容易被当枪使。",
        lore::Visibility::Visible, None)?;
    lore::create_setting(&dir, "character", "谢无咎",
        "执法堂副手，炼气七层。观察力极强，早对外门异动起疑。不贪不躁，是林晚前期最大的暗处威胁。口头禅：'账要对得上。'",
        lore::Visibility::Visible, None)?;
    lore::create_setting(&dir, "worldview", "修行体系",
        "炼气（一至九层）→筑基→金丹。无灵根者不能引气入体，是宗门底层。剑坯、丹药、差事都是资源，资源分配即权力。执法堂掌刑罚与稽查。",
        lore::Visibility::Visible, None)?;
    lore::create_setting(&dir, "mystery", "黑剑来历",
        "黑剑为前朝国师本命剑，剑中封印一缕残魂，认主后可反哺丝丝凉气助无灵根者修行。此秘密全书第二卷末方可揭示。",
        lore::Visibility::Hidden, None)?;
    lore::create_setting(&dir, "mystery", "半截黑令牌",
        "黑剑剑柄旧绳中藏半截黑令牌，与前朝国师一脉有关，是执法堂暗中追查的旧案信物。",
        lore::Visibility::Visible, Some(1))?;
    println!("设定录入完成（黑剑来历已隐藏；黑令牌第 1 卷开放）");

    // ---------- 2. 总纲 ----------
    step("2", "总纲生成 → 应用 → 确认");
    run_writer_task(
        &s, &dir, &queue,
        "撰写总纲：一句话主线 + 主题 + 三卷走向（每卷 2~3 句）。用 propose_edit 提交。\n目标文件：outline/总纲.md",
        "书名《问剑青云》。题材：凡人流修仙逆袭。已有设定：林晚（无灵根杂役，得黑剑反哺）、赵铁柱、谢无咎（执法堂）、半截黑令牌。第一卷：外门风云（黑剑认主→剑坯风波→执法堂起疑→月末大比崭露→黑令牌现踪）；第二卷：内门争锋；第三卷：帝局终章。总纲 300~400 字。",
    ).await?;
    apply_latest(&dir, &queue, "outline/总纲.md")?;
    confirm(Stage::Settings, &mut pipeline_state)?;
    confirm(Stage::Outline, &mut pipeline_state)?;

    // ---------- 3. 卷纲 ----------
    step("3", "卷纲生成 → 应用 → 确认");
    run_writer_task(
        &s, &dir, &queue,
        "撰写第一卷卷纲：本卷目标 / 前 6 章逐章一句话细纲 / 主要冲突 / 卷末钩子。用 propose_edit 提交。\n目标文件：volumes/vol-01/卷纲.md",
        "第一卷《外门风云》共 10 章（本次先写前 6 章）：黑剑认主 → 剑坯克扣风波 → 青影窥视 → 谢无咎登门试探 → 月末大比报名与暗流 → 大比初试锋芒。卷纲 400~500 字。",
    ).await?;
    apply_latest(&dir, &queue, "volumes/vol-01/卷纲.md")?;
    confirm(Stage::VolumeOutline, &mut pipeline_state)?;

    // ---------- 4. 六章循环 ----------
    let chapters = ["ch-001", "ch-002", "ch-003", "ch-004", "ch-005", "ch-006"];
    let mut first_outline_confirmed = false;
    let mut total_chars = 0usize;

    for ch in chapters {
        step(ch, "章纲 → 正文 → 账本回写");

        // 章纲
        let mut opts = AssemblyOptions::default();
        opts.volume = 1;
        opts.chapter = ch.into();
        opts.stage = Stage::ChapterOutline;
        let asm = assembly::assemble(&dir, &opts)?;
        token_curve.push((format!("{ch}-章纲"), asm.total_tokens));
        let outline_path = assembly::chapter_outline_path(1, ch);
        let mut spec = writer_spec(&asm.system);
        spec.sections.push(("task".into(), format!(
            "撰写本章章纲（本章目标/情节点 3~4 个/冲突/章末钩子，200 字内）。用 propose_write 提交。\n目标文件：{outline_path}")));
        kernel::run(&s, Some(&ctx()), &spec, Vec::new(), &assembly::render_prompt(&asm), |_| {}).await?;
        apply_latest(&dir, &queue, &outline_path)?;
        if !first_outline_confirmed {
            confirm(Stage::ChapterOutline, &mut pipeline_state)?;
            first_outline_confirmed = true;
        }

        // 正文
        opts.stage = Stage::Chapters;
        let asm = assembly::assemble(&dir, &opts)?;
        token_curve.push((format!("{ch}-正文"), asm.total_tokens));
        let ch_path = assembly::chapter_path(1, ch);
        let mut spec = writer_spec(&asm.system);
        spec.sections.push(("task".into(), format!(
            "撰写本章正文（2200~2800 字）。用 propose_write 提交。\n目标文件：{ch_path}")));
        kernel::run(&s, Some(&ctx()), &spec, Vec::new(), &assembly::render_prompt(&asm), |_| {}).await?;
        apply_latest(&dir, &queue, &ch_path)?;
        let n = works::read_file(&dir, &ch_path)?.chars().count();
        total_chars += n;
        println!("    {ch} 正文 {n} 字（累计 {total_chars}）");

        // 账本回写
        run_ledger(&s, &dir, &queue, ch).await?;
        let ledger = type_hime_lib::ledger::read_ledger(&dir)?;
        println!(
            "    账本：角色 {} / 事实 {} / 伏笔 {}",
            ledger.characters.len(), ledger.facts.len(), ledger.foreshadows.len()
        );

        // ch-003 完成后：模拟作者补充新角色（后续装配应自动包含）
        if ch == "ch-003" {
            step("插入", "作者行为模拟：补充新角色设定（沈青梧）");
            lore::create_setting(&dir, "character", "沈青梧",
                "内门弟子，剑道天才，炼气九层巅峰。冷淡寡言，唯对剑之外的事不感兴趣。曾在藏经阁外远远看过林晚一眼，原因不明。将在月末大比与林晚有交集。",
                lore::Visibility::Visible, None)?;
            println!("沈青梧已录入——观察 ch-004 装配是否自动包含");
        }

        // ch-005 完成后：埋入与账本矛盾的段落（审校抓错测试）
        if ch == "ch-005" {
            step("插入", "埋错测试：在正文末尾追加与账本矛盾的段落");
            let path5 = assembly::chapter_path(1, "ch-005");
            let raw = works::read_file(&dir, &path5)?;
            // 矛盾 1：黑剑应为贴身藏着 → 写成留在住处；矛盾 2：林晚无灵根不能引气 → 写"引气入体"
            let planted = format!(
                "{raw}\n\n夜深了，他把黑剑留在柴房的旧木箱里没有带走，两手空空走向演武场。盘膝坐下时，他缓缓引气入体，灵气顺着经脉游走一周，炼气四层的修为稳固了几分。\n"
            );
            works::write_file(&dir, &path5, &planted)?;
            println!("已埋两处矛盾：①黑剑未贴身（账本：贴身藏匿）②引气入体（设定：无灵根）");
        }
    }
    confirm(Stage::Chapters, &mut pipeline_state)?;

    // ---------- 5. 审校抓错（ch-005） ----------
    step("5", "一致性审校 ch-005（期待抓出两处埋错）");
    let mut opts = AssemblyOptions::default();
    opts.volume = 1;
    opts.chapter = "ch-005".into();
    opts.stage = Stage::Review;
    let asm = assembly::assemble(&dir, &opts)?;
    let mut spec = reviewer_spec(&asm.system);
    spec.sections.push(("task".into(),
        "对 volumes/vol-01/chapters/ch-005.md 执行一致性检查：先 read 正文与账本、设定，输出编号问题清单（每条带证据）；确定的硬伤用 propose_edit 提交修复（直接删改矛盾句，保持前后文通顺）。".into()));
    let outcome_review_text = kernel::run(&s, Some(&ctx()), &spec, Vec::new(), &assembly::render_prompt(&asm), |ev| {
        if let type_hime_lib::agent::events::AgentEvent::TextDelta { text } = ev {
            print!("{text}");
        }
    }).await?;
let review_text = outcome_review_text.text;
    println!("\n    [审校输出 {} 字]", review_text.chars().count());

    let caught_1 = review_text.contains("黑剑") && (review_text.contains("柴房") || review_text.contains("木箱") || review_text.contains("两手空空"));
    let caught_2 = review_text.contains("引气") || review_text.contains("灵气") || review_text.contains("无灵根");
    println!("    抓错判定：矛盾①(黑剑未贴身) {} / 矛盾②(无灵根却引气) {}",
        if caught_1 { "已发现 ✓" } else { "未发现 ✗" },
        if caught_2 { "已发现 ✓" } else { "未发现 ✗" });

    // 应用修复提议
    let fixes: Vec<u64> = queue.lock().unwrap().list().iter().map(|p| p.id).collect();
    for id in fixes {
        if let Some(taken) = queue.lock().unwrap().take(id) {
            let msg = review::apply(&dir, taken)?;
            println!("    应用修复：{msg}");
        }
    }
    // 修复后复查正文是否还含埋错句
    let raw5 = works::read_file(&dir, &assembly::chapter_path(1, "ch-005"))?;
    let still_bad = raw5.contains("两手空空走向演武场") || raw5.contains("引气入体，灵气顺着经脉");
    println!("    修复后埋错句残留：{}", if still_bad { "仍在 ✗" } else { "已清除 ✓" });

    // ---------- 6. 复检 ----------
    step("6", "复检 ch-005");
    let mut spec = reviewer_spec(&asm.system);
    spec.sections.push(("task".into(),
        "复检 volumes/vol-01/chapters/ch-005.md：修复后是否仍存在一致性问题？无问题输出「复检通过」，有问题继续列出。".into()));
    let outcome_recheck = kernel::run(&s, Some(&ctx()), &spec, Vec::new(), &assembly::render_prompt(&asm), |_| {}).await?;
let recheck = outcome_recheck.text;
    let brief: String = recheck.chars().take(150).collect();
    println!("    复检结论（前 150 字）：{brief}");
    confirm(Stage::Review, &mut pipeline_state)?;

    // ---------- 7. 报告 ----------
    step("7", "写书测试报告");
    let ledger = type_hime_lib::ledger::read_ledger(&dir)?;
    let log = type_hime_lib::git::log_oneline(&dir, 40)?;
    // 红线检查：只查隐藏条目独有的秘密词（反哺/国师等在可见设定里本就存在，不算泄露）
    let secret_words = ["残魂", "本命剑"];
    let mut leaked = Vec::new();
    for sub in ["outline", "volumes"] {
        check_leak(&dir, Path::new(sub), &secret_words, &mut leaked);
    }
    // 装配曲线（ch-004 起应含沈青梧）
    println!("\n—— 装配 token 曲线 ——");
    for (k, v) in &token_curve {
        println!("    {k}: {v}");
    }
    let ch4_prompt = {
        let mut o = AssemblyOptions::default();
        o.volume = 1; o.chapter = "ch-004".into(); o.stage = Stage::Chapters;
        let a = assembly::assemble(&dir, &o)?;
        assembly::render_prompt(&a)
    };
    let has_shen = ch4_prompt.contains("沈青梧");
    println!("—— 沈青梧进入 ch-004 装配：{} ——", if has_shen { "是 ✓" } else { "否 ✗" });
    println!("—— 红线（残魂/本命剑/反哺 泄露）：{} ——", if leaked.is_empty() { "零泄露 ✓".to_string() } else { format!("{:?}", leaked) });
    println!("—— 统计 ——");
    println!("    正文总字数：{total_chars}（6 章）");
    println!("    账本：角色 {} / 事实 {} / 伏笔 {}", ledger.characters.len(), ledger.facts.len(), ledger.foreshadows.len());
    println!("    git 提交：{} 条", log.len());
    println!("    流水线：审校阶段已确认（走通全程）");
    println!("\n=== 写书测试完成：作品位于 {} ===", dir.display());
    Ok(())
}

async fn run_writer_task(
    s: &settings::AppSettings,
    dir: &Path,
    queue: &Arc<Mutex<ReviewQueue>>,
    task: &str,
    prompt: &str,
) -> Result<(), String> {
    let mut spec = writer_spec("");
    spec.sections = vec![
        ("identity".into(), WRITER_IDENTITY.into()),
        ("task".into(), task.to_string()),
    ];
    let tc = ToolCtx::new(dir.to_path_buf(), Arc::clone(queue), Arc::new(|_| {}));
    kernel::run(s, Some(&tc), &spec, Vec::new(), prompt, |_| {}).await?;
    Ok(())
}

async fn run_ledger(
    s: &settings::AppSettings,
    dir: &Path,
    queue: &Arc<Mutex<ReviewQueue>>,
    chapter: &str,
) -> Result<(), String> {
    let ch_path = assembly::chapter_path(1, chapter);
    let ledger_path = type_hime_lib::ledger::LEDGER_PATH;
    let prompt = format!(
        "第1卷 {chapter} 已完稿。请先 read_file 读取正文 {ch_path} 与账本 {ledger_path}，然后 propose_edit 提交更新后的账本全文（路径 {ledger_path}）。"
    );
    let spec = ledger_spec();
    let tc = ToolCtx::new(dir.to_path_buf(), Arc::clone(queue), Arc::new(|_| {}));
    kernel::run(s, Some(&tc), &spec, Vec::new(), &prompt, |_| {}).await?;
    apply_latest(dir, queue, ledger_path)
}

fn apply_latest(dir: &Path, queue: &Arc<Mutex<ReviewQueue>>, path: &str) -> Result<(), String> {
    let mut q = queue.lock().unwrap();
    let ids: Vec<u64> = q.list().iter().filter(|p| p.rel_path == path).map(|p| p.id).collect();
    let latest = ids.iter().max().copied().ok_or(format!("{path} 无提议"))?;
    let p = q.take(latest).unwrap();
    let rest: Vec<u64> = q.list().iter().map(|x| x.id).collect();
    for id in rest {
        let _ = q.take(id);
    }
    drop(q);
    let msg = review::apply(dir, p)?;
    println!("    应用：{}", &msg.chars().take(80).collect::<String>());
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
