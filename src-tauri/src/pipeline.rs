//! 主流程状态机（D0-4/D0-5）：六阶段固定流水线 + 阶段门。
//! 阶段顺序写死；每阶段进入条件 = 上游产物已确认；门不满足则拦截。
//! 阶段可重做（重做会连带取消其后所有确认）；跳过需显式确认（同确认语义，UI 区分文案）。

use std::path::Path;

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Stage {
    /// 设定
    Settings,
    /// 总纲
    Outline,
    /// 卷纲
    VolumeOutline,
    /// 章纲
    ChapterOutline,
    /// 正文
    Chapters,
    /// 审校
    Review,
}

impl Default for Stage {
    fn default() -> Self {
        Stage::Settings
    }
}

impl Stage {
    pub const ALL: [Stage; 6] = [
        Stage::Settings,
        Stage::Outline,
        Stage::VolumeOutline,
        Stage::ChapterOutline,
        Stage::Chapters,
        Stage::Review,
    ];

    pub fn index(self) -> usize {
        Self::ALL.iter().position(|s| *s == self).unwrap_or(0)
    }

    pub fn label(self) -> &'static str {
        match self {
            Stage::Settings => "设定",
            Stage::Outline => "总纲",
            Stage::VolumeOutline => "卷纲",
            Stage::ChapterOutline => "章纲",
            Stage::Chapters => "正文",
            Stage::Review => "审校",
        }
    }

    /// 阶段的代表性产物（用于确认时的存在性校验）；None 表示无硬性文件要求。
    fn artifact(self) -> Option<&'static str> {
        match self {
            Stage::Settings => None,
            Stage::Outline => Some("outline/总纲.md"),
            Stage::VolumeOutline => Some("volumes/vol-01/卷纲.md"),
            // 章纲/正文共用章节目录，由内容区分；MVP 只要求目录下有 md 文件
            Stage::ChapterOutline | Stage::Chapters => Some("volumes/vol-01/chapters"),
            Stage::Review => None,
        }
    }

    /// 直接上游阶段
    fn prev(self) -> Option<Stage> {
        match self {
            Stage::Settings => None,
            other => Some(Stage::ALL[other.index() - 1]),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PipelineState {
    /// 当前所在阶段
    #[serde(default)]
    pub stage: Stage,
    /// 已确认（通过阶段门）的阶段
    #[serde(default)]
    pub confirmed: Vec<Stage>,
}

impl Default for PipelineState {
    fn default() -> Self {
        Self {
            stage: Stage::Settings,
            confirmed: Vec::new(),
        }
    }
}

/// 前端视图：阶段状态 + 可用动作 + 拦截原因。
#[derive(Debug, Serialize)]
pub struct PipelineView {
    pub stage: Stage,
    pub stage_label: String,
    pub confirmed: Vec<Stage>,
    /// 每阶段：可否进入 + 原因
    pub gates: Vec<GateInfo>,
}

#[derive(Debug, Serialize)]
pub struct GateInfo {
    pub stage: Stage,
    pub label: String,
    pub enterable: bool,
    /// 不可进入时缺的门
    pub blocked_by: Option<String>,
    pub confirmed: bool,
    pub current: bool,
}

impl PipelineState {
    /// 阶段门：上游全部已确认才可进入。
    pub fn gate_check(&self, target: Stage, work_dir: &Path) -> Result<(), String> {
        // 收集全部上游
        let mut upstream = Vec::new();
        let mut cur = target.prev();
        while let Some(s) = cur {
            upstream.push(s);
            cur = s.prev();
        }
        for s in upstream {
            if !self.confirmed.contains(&s) {
                return Err(format!(
                    "阶段门拦截：「{}」需要先确认「{}」",
                    target.label(),
                    s.label()
                ));
            }
        }
        // 产物存在性（已确认过的阶段不再重复校验）
        if let Some(artifact) = target.artifact() {
            if !self.confirmed.contains(&target) {
                let path = work_dir.join(artifact);
                let ok = if path.is_dir() {
                    std::fs::read_dir(&path)
                        .map(|entries| {
                            entries.flatten().any(|e| {
                                e.path().extension().and_then(|x| x.to_str()) == Some("md")
                            })
                        })
                        .unwrap_or(false)
                } else {
                    path.is_file()
                };
                if !ok {
                    return Err(format!(
                        "阶段门拦截：「{}」的产物 {} 还不存在",
                        target.label(),
                        artifact
                    ));
                }
            }
        }
        Ok(())
    }

    pub fn enter(&mut self, target: Stage, work_dir: &Path) -> Result<(), String> {
        self.gate_check(target, work_dir)?;
        self.stage = target;
        Ok(())
    }

    /// 确认当前/指定阶段产物，放行下游。
    pub fn confirm(&mut self, stage: Stage, work_dir: &Path) -> Result<(), String> {
        // 确认前自身需可进入（上游已确认）
        self.gate_check(stage, work_dir)?;
        if !self.confirmed.contains(&stage) {
            self.confirmed.push(stage);
        }
        self.confirmed.sort_by_key(|s| s.index());
        // 自动推进到下一未确认阶段
        if let Some(next) = Stage::ALL.get(stage.index() + 1) {
            self.stage = *next;
        } else {
            self.stage = stage; // 全部完成，停在最后
        }
        Ok(())
    }

    /// 显式跳过（承认产物空缺/直接放行）：与 confirm 同语义，UI 文案区分。
    pub fn skip_confirmed(&mut self, stage: Stage) {
        if !self.confirmed.contains(&stage) {
            self.confirmed.push(stage);
            self.confirmed.sort_by_key(|s| s.index());
        }
        if let Some(next) = Stage::ALL.get(stage.index() + 1) {
            self.stage = *next;
        }
    }

    /// 重做某阶段：取消其及之后所有确认，回到该阶段。
    pub fn redo(&mut self, stage: Stage) {
        self.confirmed.retain(|s| s.index() < stage.index());
        self.stage = stage;
    }

    pub fn to_view(&self, work_dir: &Path) -> PipelineView {
        let gates = Stage::ALL
            .iter()
            .map(|s| {
                let blocked = self.gate_check(*s, work_dir).err();
                GateInfo {
                    stage: *s,
                    label: s.label().to_string(),
                    enterable: blocked.is_none(),
                    blocked_by: blocked,
                    confirmed: self.confirmed.contains(s),
                    current: self.stage == *s,
                }
            })
            .collect();
        PipelineView {
            stage: self.stage,
            stage_label: self.stage.label().to_string(),
            confirmed: self.confirmed.clone(),
            gates,
        }
    }
}

// ---------------------------------------------------------------------------
// project.json 读写（作品元数据 + 流水线状态）

#[derive(Debug, Serialize, Deserialize)]
struct ProjectMeta {
    name: String,
    created_at: String,
    #[serde(default)]
    pipeline: PipelineState,
}

pub fn load_pipeline(work_dir: &Path) -> Result<PipelineState, String> {
    let path = work_dir.join("project.json");
    let raw = std::fs::read_to_string(&path).map_err(|e| format!("读取 project.json 失败：{e}"))?;
    let meta: ProjectMeta =
        serde_json::from_str(&raw).map_err(|e| format!("解析 project.json 失败：{e}"))?;
    Ok(meta.pipeline)
}

pub fn save_pipeline(work_dir: &Path, pipeline: &PipelineState) -> Result<(), String> {
    let path = work_dir.join("project.json");
    let raw = std::fs::read_to_string(&path).map_err(|e| format!("读取 project.json 失败：{e}"))?;
    let mut meta: ProjectMeta =
        serde_json::from_str(&raw).map_err(|e| format!("解析 project.json 失败：{e}"))?;
    meta.pipeline = pipeline.clone();
    let out = serde_json::to_string_pretty(&meta).map_err(|e| e.to_string())?;
    std::fs::write(&path, out).map_err(|e| format!("写入 project.json 失败：{e}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn work_dir() -> std::path::PathBuf {
        let dir = std::env::temp_dir().join(format!("type-hime-pipeline-test-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        // 骨架
        for p in ["outline", "volumes/vol-01/chapters", "ledger"] {
            std::fs::create_dir_all(dir.join(p)).unwrap();
        }
        std::fs::write(dir.join("outline/总纲.md"), "# 总纲\n内容").unwrap();
        std::fs::write(
            dir.join("project.json"),
            r#"{"name":"t","created_at":"2026-01-01"}"#,
        )
        .unwrap();
        dir
    }

    #[test]
    fn gate_blocks_and_confirms() {
        let dir = work_dir();
        let mut p = PipelineState {
            stage: Stage::Settings,
            confirmed: vec![],
        };

        // 卷纲被总纲门拦截
        let err = p.enter(Stage::VolumeOutline, &dir).unwrap_err();
        assert!(err.contains("总纲"), "{err}");

        // 未确认设定时进总纲也被拦
        assert!(p.enter(Stage::Outline, &dir).is_err());

        // 设定无产物要求，确认后进入总纲
        assert!(p.confirm(Stage::Settings, &dir).is_ok());
        assert_eq!(p.stage, Stage::Outline);
        assert!(p.confirm(Stage::Outline, &dir).is_ok());
        let err = p.confirm(Stage::VolumeOutline, &dir).unwrap_err();
        assert!(err.contains("卷纲.md"));

        // 补产物后确认放行
        std::fs::write(dir.join("volumes/vol-01/卷纲.md"), "# 卷纲").unwrap();
        assert!(p.confirm(Stage::VolumeOutline, &dir).is_ok());
        assert_eq!(p.stage, Stage::ChapterOutline);

        // 章纲目录空被拦
        assert!(p.confirm(Stage::ChapterOutline, &dir).is_err());
        std::fs::write(dir.join("volumes/vol-01/chapters/ch-001.md"), "纲").unwrap();
        assert!(p.confirm(Stage::ChapterOutline, &dir).is_ok());

        // 重做：重做总纲 → 卷纲/章纲确认被连带取消
        p.redo(Stage::Outline);
        assert!(!p.confirmed.contains(&Stage::VolumeOutline));
        assert_eq!(p.stage, Stage::Outline);

        // 显式跳过
        p.skip_confirmed(Stage::Outline);
        assert!(p.confirmed.contains(&Stage::Outline));

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn persistence_roundtrip() {
        let dir = work_dir();
        let mut p = PipelineState {
            stage: Stage::Settings,
            confirmed: vec![],
        };
        p.confirm(Stage::Settings, &dir).unwrap();
        save_pipeline(&dir, &p).unwrap();
        let back = load_pipeline(&dir).unwrap();
        assert_eq!(back.stage, Stage::Outline);
        assert!(back.confirmed.contains(&Stage::Settings));
        let _ = std::fs::remove_dir_all(&dir);
    }
}
