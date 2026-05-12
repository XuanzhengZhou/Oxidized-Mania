use flate2::read::GzDecoder;
use flate2::write::GzEncoder;
use flate2::Compression;
use serde::{Deserialize, Serialize};
use std::io::{Read, Write};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReplayEvent {
    pub time_ms: u32,
    pub lane: u8,
    pub pressed: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JudgmentCounts {
    pub perfect: u32, pub great: u32, pub good: u32,
    pub ok: u32, pub meh: u32, pub miss: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReplayData {
    pub map_path: String,
    pub song_rate: f64, pub od: f64,
    pub scroll_speed: f64, pub hit_position: f64,
    pub mirror_mode: bool, pub global_offset: f64,
    #[serde(default = "default_spacing")]
    pub stage_spacing: f64,
    #[serde(default = "default_scale")]
    pub stage_scale: f64,
    pub player_name: String, pub date: String,
    pub events: Vec<ReplayEvent>,
    pub score: u32, pub acc: f64, pub max_combo: u32,
    pub counts: JudgmentCounts, pub total_notes: u32,
}
fn default_spacing() -> f64 { 100.0 }
fn default_scale() -> f64 { 1.0 }

pub struct ReplayRecorder {
    map_path: String, song_rate: f64, od: f64,
    scroll_speed: f64, hit_position: f64,
    mirror_mode: bool, global_offset: f64,
    stage_spacing: f64, stage_scale: f64,
    player_name: String, events: Vec<ReplayEvent>,
}

impl ReplayRecorder {
    pub fn new(
        map_path: &str, song_rate: f64, od: f64,
        scroll_speed: f64, hit_position: f64,
        mirror_mode: bool, global_offset: f64,
        stage_spacing: f64, stage_scale: f64,
        player_name: &str,
    ) -> Self {
        Self { map_path: map_path.to_string(), song_rate, od,
            scroll_speed, hit_position, mirror_mode, global_offset,
            stage_spacing, stage_scale,
            player_name: player_name.to_string(), events: Vec::new() }
    }

    pub fn record_event(&mut self, time_ms: f64, lane: usize, pressed: bool) {
        self.events.push(ReplayEvent { time_ms: time_ms as u32, lane: lane as u8, pressed });
    }

    pub fn finalize(self, score: u32, acc: f64, max_combo: u32,
        counts: JudgmentCounts, total_notes: u32) -> ReplayData
    {
        let now = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0);
        let secs = now + 8 * 3600; let days = secs / 86400; let rem = secs % 86400;
        let h = rem / 3600; let m = (rem % 3600) / 60;
        let (y, mo, d) = unix_to_date(days as i64);
        ReplayData { map_path: self.map_path, song_rate: self.song_rate, od: self.od,
            scroll_speed: self.scroll_speed, hit_position: self.hit_position,
            mirror_mode: self.mirror_mode, global_offset: self.global_offset,
            stage_spacing: self.stage_spacing, stage_scale: self.stage_scale,
            player_name: self.player_name,
            date: format!("{:04}-{:02}-{:02} {:02}:{:02}", y, mo, d, h, m),
            events: self.events, score, acc, max_combo, counts, total_notes }
    }
}

fn unix_to_date(days: i64) -> (i64, u32, u32) {
    let mut y = 1970i64; let mut r = days;
    loop { let dy = if is_leap(y) { 366 } else { 365 }; if r < dy { break; } r -= dy; y += 1; }
    let md = if is_leap(y) { [31,29,31,30,31,30,31,31,30,31,30,31] } else { [31,28,31,30,31,30,31,31,30,31,30,31] };
    let mut mo = 1u32; for &m in &md { if r < m as i64 { break; } r -= m as i64; mo += 1; }
    (y, mo, (r + 1) as u32)
}
fn is_leap(y: i64) -> bool { (y % 4 == 0 && y % 100 != 0) || y % 400 == 0 }

impl ReplayData {
    pub fn save(&self, path: &str) -> std::io::Result<()> {
        let json = serde_json::to_string(self)?;
        let mut e = GzEncoder::new(Vec::new(), Compression::default());
        e.write_all(json.as_bytes())?;
        std::fs::write(path, e.finish()?)?;
        log::info!("[Replay] saved {}", path);
        Ok(())
    }
    pub fn load(path: &str) -> std::io::Result<Self> {
        let data = std::fs::read(path)?;
        let mut d = GzDecoder::new(&data[..]);
        let mut s = String::new(); d.read_to_string(&mut s)?;
        // Python 格式: 有 "frames" 字段 → 需要转换
        if s.contains("\"frames\"") {
            return Self::from_python_json(&s);
        }
        // Rust 原生格式 (events)
        serde_json::from_str(&s).map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, format!("json: {e}")))
    }

    fn from_python_json(json: &str) -> std::io::Result<Self> {
        #[derive(Deserialize)]
        struct PyCounts {
            perfect: u32, great: u32, good: u32,
            ok: u32, meh: u32, miss: u32,
        }
        #[derive(Deserialize)]
        struct PyFrame { t: i32, k: u8 }
        #[derive(Deserialize)]
        struct PyReplay {
            map_path: String,
            #[serde(default)] song_rate: f64,
            #[serde(default)] od: f64,
            #[serde(default)] scroll_speed: f64,
            #[serde(default)] hit_position: f64,
            #[serde(default)] mirror_mode: bool,
            #[serde(default)] global_offset: f64,
            #[serde(default)] player_name: String,
            #[serde(default)] date: String,
            #[serde(default)] frames: Vec<PyFrame>,
            #[serde(default)] score: u32,
            #[serde(default)] acc: f64,
            #[serde(default)] max_combo: u32,
            #[serde(default)] counts: Option<PyCounts>,
            #[serde(default)] total_notes: u32,
            #[serde(default = "default_spacing")] stage_spacing: f64,
            #[serde(default = "default_scale")] stage_scale: f64,
        }

        let py: PyReplay = serde_json::from_str(json)
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, format!("json: {e}")))?;

        // frames (bitmask) → events (per-lane)
        let mut events = Vec::new();
        let mut prev: u8 = 0;
        for f in &py.frames {
            let changed = prev ^ f.k;
            for lane in 0..4u8 {
                if (changed >> lane) & 1 != 0 {
                    // 过滤 lead-in 负帧 (t<0, always k=0)
                    if f.t >= 0 {
                        events.push(ReplayEvent {
                            time_ms: f.t as u32,
                            lane,
                            pressed: (f.k >> lane) & 1 != 0,
                        });
                    }
                }
            }
            prev = f.k;
        }

        let counts = py.counts.map(|c| JudgmentCounts {
            perfect: c.perfect, great: c.great, good: c.good,
            ok: c.ok, meh: c.meh, miss: c.miss,
        }).unwrap_or(JudgmentCounts {
            perfect: 0, great: 0, good: 0, ok: 0, meh: 0, miss: 0,
        });

        Ok(ReplayData {
            map_path: py.map_path,
            song_rate: py.song_rate,
            od: py.od,
            scroll_speed: py.scroll_speed,
            hit_position: py.hit_position,
            mirror_mode: py.mirror_mode,
            global_offset: py.global_offset,
            stage_spacing: py.stage_spacing,
            stage_scale: py.stage_scale,
            player_name: py.player_name,
            date: py.date,
            events,
            score: py.score,
            acc: py.acc,
            max_combo: py.max_combo,
            counts,
            total_notes: py.total_notes,
        })
    }
}

pub fn list_replays(map_path: &str) -> Vec<String> {
    let mut v = Vec::new();

    // 1. Rust 回放: replays/*.json.gz
    let rdir = std::path::Path::new("replays");
    if let Ok(es) = std::fs::read_dir(rdir) {
        for e in es.flatten() {
            let p = e.path();
            if p.extension().map_or(false, |x| x == "gz") {
                if let Ok(d) = ReplayData::load(&p.to_string_lossy()) {
                    if d.map_path == map_path { v.push(p.to_string_lossy().to_string()); }
                }
            }
        }
    }

    // 2. Python 回放: 谱面目录下 *.osr
    if let Some(parent) = std::path::Path::new(map_path).parent() {
        if let Ok(es) = std::fs::read_dir(parent) {
            for e in es.flatten() {
                let p = e.path();
                if p.extension().map_or(false, |x| x == "osr") {
                    if let Ok(d) = ReplayData::load(&p.to_string_lossy()) {
                        if d.map_path == map_path { v.push(p.to_string_lossy().to_string()); }
                    }
                }
            }
        }
    }

    v.sort_by(|a, b| b.cmp(a));
    v
}
