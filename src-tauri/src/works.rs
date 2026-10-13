//! 作品管理：作品库配置、作品目录骨架、资源树、文件读写。
//! 每个作品 = 一个 git 仓库（D4）；一切落盘修改接受即自动 commit。

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::git;

// ---------------------------------------------------------------------------
// 应用配置（存 app data；库目录可改，默认 Documents/type-hime/works）

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct AppConfig {
    pub works_root: String,
    /// 记住上次打开的作品
    pub last_work: Option<String>,
}

fn app_config_path() -> Result<PathBuf, String> {
    let base = dirs::config_dir().ok_or("无法定位用户配置目录")?;
    let dir = base.join("type-hime");
    std::fs::create_dir_all(&dir).map_err(|e| format!("创建配置目录失败：{e}"))?;
    Ok(dir.join("config.json"))
}

fn default_works_root() -> Result<PathBuf, String> {
    let docs = dirs::document_dir().ok_or("无法定位文档目录")?;
    Ok(docs.join("type-hime").join("works"))
}

pub fn load_config() -> Result<AppConfig, String> {
    let path = app_config_path()?;
    if !path.exists() {
        let config = AppConfig {
            works_root: default_works_root()?.to_string_lossy().into_owned(),
            last_work: None,
        };
        save_config(&config)?;
        // 确保库根目录存在
        std::fs::create_dir_all(&config.works_root)
            .map_err(|e| format!("创建作品库目录失败：{e}"))?;
        return Ok(config);
    }
    let raw = std::fs::read_to_string(&path).map_err(|e| format!("读取配置失败：{e}"))?;
    serde_json::from_str(&raw).map_err(|e| format!("解析配置失败：{e}"))
}

pub fn save_config(config: &AppConfig) -> Result<(), String> {
    let path = app_config_path()?;
    let raw = serde_json::to_string_pretty(config).map_err(|e| e.to_string())?;
    std::fs::write(&path, raw).map_err(|e| format!("写入配置失败：{e}"))
}

// ---------------------------------------------------------------------------
// 作品

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct WorkSummary {
    /// 作品名
    pub name: String,
    /// 作品目录绝对路径
    pub path: String,
    pub created_at: String,
}

#[derive(Debug, Serialize, Deserialize)]
struct ProjectMeta {
    name: String,
    created_at: String,
}

/// 列出作品库下所有作品（含 project.json 的目录）。
pub fn list_works() -> Result<Vec<WorkSummary>, String> {
    let config = load_config()?;
    let root = PathBuf::from(&config.works_root);
    if !root.exists() {
        return Ok(Vec::new());
    }
    let mut works = Vec::new();
    let entries = std::fs::read_dir(&root).map_err(|e| format!("读取作品库失败：{e}"))?;
    for entry in entries.flatten() {
        let meta_path = entry.path().join("project.json");
        if !meta_path.is_file() {
            continue;
        }
        if let Ok(raw) = std::fs::read_to_string(&meta_path) {
            if let Ok(meta) = serde_json::from_str::<ProjectMeta>(&raw) {
                works.push(WorkSummary {
                    name: meta.name,
                    path: entry.path().to_string_lossy().into_owned(),
                    created_at: meta.created_at,
                });
            }
        }
    }
    works.sort_by(|a, b| b.created_at.cmp(&a.created_at));
    Ok(works)
}

/// 新建作品：目录骨架 + git init + 首次提交。
/// base 为 None 时落在默认库目录下；目录名取作品名（非法字符替换）。
pub fn create_work(name: &str, base: Option<String>) -> Result<WorkSummary, String> {
    let name = name.trim();
    if name.is_empty() {
        return Err("作品名不能为空".into());
    }
    let root = match &base {
        Some(b) => PathBuf::from(b),
        None => PathBuf::from(&load_config()?.works_root),
    };
    std::fs::create_dir_all(&root).map_err(|e| format!("创建库目录失败：{e}"))?;

    let dir_name: String = name
        .chars()
        .map(|c| match c {
            '/' | '\\' | ':' | '*' | '?' | '"' | '<' | '>' | '|' => '_',
            _ => c,
        })
        .collect();
    let work_dir = unique_dir(&root, &dir_name)?;

    write_skeleton(&work_dir, name)?;

    git::init_repo(&work_dir)?;
    git::commit_all(&work_dir, "初始化作品")?;

    let summary = WorkSummary {
        name: name.to_string(),
        path: work_dir.to_string_lossy().into_owned(),
        created_at: now_iso(),
    };
    Ok(summary)
}

/// 生成不冲突的目录名（作品名重复时追加 -2、-3 …）。
fn unique_dir(root: &Path, dir_name: &str) -> Result<PathBuf, String> {
    let mut candidate = root.join(dir_name);
    let mut n = 2;
    while candidate.exists() {
        candidate = root.join(format!("{dir_name}-{n}"));
        n += 1;
        if n > 999 {
            return Err("目录名冲突过多".into());
        }
    }
    std::fs::create_dir_all(&candidate).map_err(|e| format!("创建作品目录失败：{e}"))?;
    Ok(candidate)
}

/// 写作品骨架（md + frontmatter，D4）。
fn write_skeleton(work_dir: &Path, name: &str) -> Result<(), String> {
    let w = |rel: &str, content: &str| -> Result<(), String> {
        let path = work_dir.join(rel);
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).map_err(|e| format!("创建 {rel} 失败：{e}"))?;
        }
        std::fs::write(&path, content).map_err(|e| format!("写入 {rel} 失败：{e}"))
    };

    let meta = ProjectMeta {
        name: name.to_string(),
        created_at: now_iso(),
    };
    std::fs::write(
        work_dir.join("project.json"),
        serde_json::to_string_pretty(&meta).unwrap(),
    )
    .map_err(|e| format!("写入 project.json 失败：{e}"))?;

    w(
        ".gitignore",
        "# 会话历史不是叙事产物，不进版本库\n.sessions/\n",
    )?;

    w(
        "settings/characters/.gitkeep",
        "",
    )?;
    w("settings/worldview/.gitkeep", "")?;
    w("settings/mysteries/.gitkeep", "")?;

    w(
        "style/文风指令.md",
        "---\ntype: style\n---\n\n# 文风指令\n\n<!-- 这里的话会作为系统提示词的一部分注入正文生成。可自由改写。 -->\n\n- 语言：简体中文\n- 体裁：网络小说长篇\n- 叙事：第三人称限知为主，画面感强，多用具体动作与感官细节，少抽象形容\n- 节奏：每章 2000~4000 字，章末留钩子\n- 对话：口语化、有性格区分度，避免书面腔\n",
    )?;

    w(
        "outline/总纲.md",
        "---\ntype: master-outline\n---\n\n# 总纲\n\n<!-- 全书主线：一句话故事 + 主题 + 预期卷数与每卷走向。确认后进入卷纲阶段。 -->\n",
    )?;

    w(
        "volumes/vol-01/卷纲.md",
        "---\ntype: volume-outline\nvolume: 1\n---\n\n# 第一卷 卷纲\n\n<!-- 本卷目标、起止情节点、主要冲突与出场人物。 -->\n",
    )?;
    w("volumes/vol-01/chapters/.gitkeep", "")?;

    w(
        "ledger/账本.md",
        "---\ntype: ledger\n---\n\n# 连续性账本\n\n<!-- 完稿一章后由 agent 提议回写；下一章装配时注入摘要。 -->\n\n## 角色状态\n\n<!-- 每个主要角色：位置 / 修为等级 / 关系 / 持有物 / 当前目标（条目末尾以（ch-00N）标注最近更新章） -->\n\n## 已交代事实\n\n<!-- 已在正文明确交代、不可与之矛盾的事实 -->\n\n## 伏笔登记\n\n<!-- 伏笔内容 / 埋设章节 / 计划回收点 / 状态（未回收|已回收） -->\n\n## 时间线\n\n<!-- 每章一条：### 第N章 → 距开篇天数 + 关键事件。正文的时间表述必须与之相容 -->\n",
    )?;

    Ok(())
}

// ---------------------------------------------------------------------------
// 资源树与文件操作

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct TreeNode {
    pub name: String,
    /// 相对作品根的路径（/ 分隔）
    pub path: String,
    pub is_dir: bool,
    pub children: Vec<TreeNode>,
}

/// 读取作品目录树（排除 .git / .sessions）。
pub fn read_tree(work_dir: &Path) -> Result<TreeNode, String> {
    fn build(dir: &Path, rel: &str, name: &str) -> Result<TreeNode, String> {
        let mut children = Vec::new();
        let mut entries: Vec<_> = std::fs::read_dir(dir)
            .map_err(|e| format!("读取目录失败：{e}"))?
            .flatten()
            .collect();
        entries.sort_by_key(|e| e.file_name());
        // 目录在前、文件在后，视觉更稳定
        entries.sort_by_key(|e| !e.path().is_dir());
        for entry in entries {
            let entry_name = entry.file_name().to_string_lossy().into_owned();
            if entry_name.starts_with('.') {
                continue; // .git / .sessions / .gitkeep 的父级照常显示
            }
            if entry_name == ".gitkeep" {
                continue;
            }
            let child_rel = if rel.is_empty() {
                entry_name.clone()
            } else {
                format!("{rel}/{entry_name}")
            };
            let path = entry.path();
            if path.is_dir() {
                children.push(build(&path, &child_rel, &entry_name)?);
            } else {
                children.push(TreeNode {
                    name: entry_name,
                    path: child_rel,
                    is_dir: false,
                    children: Vec::new(),
                });
            }
        }
        Ok(TreeNode {
            name: name.to_string(),
            path: rel.to_string(),
            is_dir: true,
            children,
        })
    }
    let root_name = work_dir
        .file_name()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_else(|| "作品".into());
    build(work_dir, "", &root_name)
}

/// 校验相对路径合法（防穿越、防触及 .git）。
fn safe_rel(work_dir: &Path, rel: &str) -> Result<PathBuf, String> {
    let normalized = rel.replace('\\', "/");
    if normalized.contains("..") || normalized.starts_with('/') {
        return Err(format!("非法路径：{rel}"));
    }
    if normalized.starts_with(".git/") || normalized == ".git" {
        return Err("不允许操作 .git 目录".into());
    }
    // 以 canonicalize 过的根为基准 join：目标文件不存在也安全，
    // 且不含 .. 时拼出的路径必在根内。
    let root_canon = dunce_canonical(work_dir)?;
    Ok(root_canon.join(&normalized))
}

/// Windows 上 canonicalize 返回 UNC 前缀，dunce 转回普通路径。
fn dunce_canonical(path: &Path) -> Result<PathBuf, String> {
    dunce::canonicalize(path).map_err(|e| format!("解析路径失败：{e}"))
}

/// 校验路径并返回绝对目标（供外部模块写盘复用；不写盘本身）。
pub fn ensure_writable(work_dir: &Path, rel: &str) -> Result<PathBuf, String> {
    safe_rel(work_dir, rel)
}

pub fn read_file(work_dir: &Path, rel: &str) -> Result<String, String> {
    let path = safe_rel(work_dir, rel)?;
    std::fs::read_to_string(&path).map_err(|e| format!("读取 {rel} 失败：{e}"))
}

/// 手动编辑保存：直接写盘并自动 commit（人自己的修改无需 diff 审阅，D0-6）。
pub fn write_file(work_dir: &Path, rel: &str, content: &str) -> Result<(), String> {
    let path = safe_rel(work_dir, rel)?;
    std::fs::write(&path, content).map_err(|e| format!("写入 {rel} 失败：{e}"))?;
    git::commit_all(work_dir, &format!("手动编辑：{rel}"))
}

/// 新建文件（含章节）。父目录自动创建。
pub fn create_file(work_dir: &Path, rel: &str, content: &str) -> Result<(), String> {
    if rel.is_empty() {
        return Err("文件名不能为空".into());
    }
    let path = safe_rel(work_dir, rel)?;
    if path.exists() {
        return Err(format!("{rel} 已存在"));
    }
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| format!("创建目录失败：{e}"))?;
    }
    std::fs::write(&path, content).map_err(|e| format!("创建 {rel} 失败：{e}"))?;
    git::commit_all(work_dir, &format!("新建文件：{rel}"))
}

/// 删除文件并 commit（可从 git 历史恢复）。
pub fn delete_file(work_dir: &Path, rel: &str) -> Result<(), String> {
    let path = safe_rel(work_dir, rel)?;
    std::fs::remove_file(&path).map_err(|e| format!("删除 {rel} 失败：{e}"))?;
    git::commit_all(work_dir, &format!("删除文件：{rel}"))
}

/// 从 git HEAD 恢复文件。
pub fn restore_file(work_dir: &Path, rel: &str) -> Result<(), String> {
    safe_rel(work_dir, rel)?;
    git::restore_file(work_dir, rel)
}

pub fn now_iso() -> String {
    let secs = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
        + 8 * 3600; // 固定 UTC+8，单机自用足够
    let days = secs / 86400;
    let rem = secs % 86400;
    let (h, mi, s) = ((rem / 3600) as u32, ((rem % 3600) / 60) as u32, (rem % 60) as u32);
    let (y, mo, d) = epoch_to_ymd(days as i64);
    format!("{y:04}-{mo:02}-{d:02} {h:02}:{mi:02}:{s:02}")
}

/// 自 1970-01-01 的天数 → 公历年月日。
fn epoch_to_ymd(days: i64) -> (i64, u32, u32) {
    let z = days + 719468;
    let era = z.div_euclid(146097);
    let doe = z.rem_euclid(146097);
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = (doy - (153 * mp + 2) / 5 + 1) as u32;
    let m = if mp < 10 { mp + 3 } else { mp - 9 } as u32;
    let y = if m <= 2 { y + 1 } else { y };
    (y, m, d)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn create_list_tree_roundtrip() {
        let root = std::env::temp_dir().join(format!("type-hime-works-test-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root).unwrap();

        let work = create_work("测试作品·壹", Some(root.to_string_lossy().into_owned())).unwrap();
        let work_dir = PathBuf::from(&work.path);

        // 骨架就位
        assert!(work_dir.join("project.json").is_file());
        assert!(work_dir.join("outline/总纲.md").is_file());
        assert!(work_dir.join("ledger/账本.md").is_file());
        assert!(work_dir.join(".git").is_dir());

        // git 历史可回溯
        assert!(!git::log_oneline(&work_dir, 10).unwrap().is_empty());

        // 建章 → 删 → 恢复
        create_file(&work_dir, "volumes/vol-01/chapters/ch-001.md", "# 第一章\n正文").unwrap();
        delete_file(&work_dir, "volumes/vol-01/chapters/ch-001.md").unwrap();
        assert!(!work_dir.join("volumes/vol-01/chapters/ch-001.md").exists());
        restore_file(&work_dir, "volumes/vol-01/chapters/ch-001.md").unwrap();
        assert!(work_dir.join("volumes/vol-01/chapters/ch-001.md").is_file());

        // 资源树不含 .git
        let tree = read_tree(&work_dir).unwrap();
        let names: Vec<&str> = tree.children.iter().map(|c| c.name.as_str()).collect();
        assert!(!names.contains(&".git"));
        assert!(names.contains(&"outline"));

        // 路径穿越被拦截
        assert!(read_file(&work_dir, "../project.json").is_err());
        assert!(read_file(&work_dir, ".git/config").is_err());

        let _ = std::fs::remove_dir_all(&root);
    }
}
