use serde::{Deserialize, Serialize};
use std::path::Path;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BeatmapMeta {
    pub song: String,
    #[serde(default)]
    pub offset: f64,
    #[serde(default)]
    pub bg: Option<String>,
    #[serde(default)]
    pub bpm: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum NoteDef {
    #[serde(rename = "tap")]
    Tap { time: f64, lane: usize },
    #[serde(rename = "hold")]
    Hold {
        time: f64,
        end_time: f64,
        lane: usize,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BeatmapFile {
    pub meta: BeatmapMeta,
    pub notes: Vec<NoteDef>,
}

// ─── 运行时音符 ───

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum NoteType {
    Tap,
    Hold,
}

#[derive(Debug, Clone)]
pub struct Note {
    pub time: f64,
    pub end_time: f64,
    pub lane: usize,
    pub note_type: NoteType,

    // 运行时状态
    pub hit: bool,
    pub missed: bool,
    pub holding: bool,
    pub stuck_y: Option<f64>,
    pub release_time: Option<f64>,
}

pub fn load_beatmap(json_path: &str) -> Result<(BeatmapMeta, Vec<Note>), String> {
    let (meta, mut notes) = {
        let content = std::fs::read_to_string(json_path).map_err(|e| format!("read error: {}", e))?;
        let bf: BeatmapFile = serde_json::from_str(&content).map_err(|e| format!("parse error: {}", e))?;
        let meta = bf.meta;
        let notes: Vec<Note> = bf.notes.into_iter().map(|def| match def {
            NoteDef::Tap { time, lane } => Note {
                time,
                end_time: time,
                lane,
                note_type: NoteType::Tap,
                hit: false,
                missed: false,
                holding: false,
                stuck_y: None,
                release_time: None,
            },
            NoteDef::Hold {
                time,
                end_time,
                lane,
            } => Note {
                time,
                end_time,
                lane,
                note_type: NoteType::Hold,
                hit: false,
                missed: false,
                holding: false,
                stuck_y: None,
                release_time: None,
            },
        })
        .collect();
        // 音频路径
        let json_dir = Path::new(json_path).parent().unwrap_or(Path::new("."));
        let mut meta = meta;
        meta.song = json_dir.join(&meta.song).to_string_lossy().to_string();
        (meta, notes)
    }; // content & BeatmapFile dropped here

    notes.sort_by(|a, b| a.time.partial_cmp(&b.time).unwrap());
    Ok((meta, notes))
}

/// Load beatmap via ROX (supports .osu / .mc / .sm / .qua / .json).
/// Uses transparent `.rox` cache for instant reloads.
pub fn load_beatmap_rox(path: &str) -> Result<(BeatmapMeta, Vec<Note>), String> {
    use rhythm_open_exchange::model::NoteType as RoxNoteType;

    let chart = crate::beatmap_cache::load_chart_cached(path)?;
    let parent_dir = Path::new(path).parent().unwrap_or(Path::new("."));

    let resolve = |file: &str| -> String {
        let p = Path::new(file);
        if p.is_absolute() || p.exists() { return file.to_string(); }
        parent_dir.join(file).to_string_lossy().to_string()
    };

    let meta = BeatmapMeta {
        song: resolve(&chart.metadata.audio_file),
        offset: (chart.metadata.audio_offset_us / 1000) as f64,
        bg: chart.metadata.background_file.as_ref().map(|s| resolve(s)),
        bpm: chart.timing_points.iter().find(|t| !t.is_inherited).map(|t| t.bpm as f64).unwrap_or(0.0),
    };

    let notes: Vec<Note> = chart.notes.iter().map(|n| {
        let (note_type, et) = match n.note_type {
            RoxNoteType::Hold { duration_us } => (NoteType::Hold, (n.time_us + duration_us) as f64 / 1000.0),
            _ => (NoteType::Tap, n.time_us as f64 / 1000.0),
        };
        Note { time: n.time_us as f64 / 1000.0, end_time: et, lane: n.column as usize, note_type,
            hit: false, missed: false, holding: false, stuck_y: None, release_time: None }
    }).collect();

    Ok((meta, notes))
}
