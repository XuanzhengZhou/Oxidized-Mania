pub mod analysis;
pub mod audio;
pub mod config;
pub mod input;
pub mod render;

use crate::beatmap::{BeatmapFile, BeatmapMeta, NoteDef, NoteType};
use crate::config::GameConfig;
use audio::EditorAudio;
use config::SpectrogramConfig;
use std::sync::{Arc, Mutex};
use std::thread;

type Note = (f64, f64, usize, NoteType);

#[derive(Clone, PartialEq)]
pub enum SpectStatus { Hidden, Computing, Loading, Shown }

pub struct EditorState {
    pub notes: Vec<Note>, pub audio_path: String, pub audio: EditorAudio,
    pub cursor_ms: f64, pub snap_divisor: u32, pub bpm: f64, pub offset_ms: f64,
    pub playing: bool, pub song_rate: f64, pub undo_stack: Vec<Vec<Note>>,
    pub config: GameConfig, pub song_folder: String, pub bg_path: Option<String>,
    pub dirty: bool, pub open_holds: [Option<f64>; 4],
    pub spect_config: SpectrogramConfig, pub bpm_result: Option<analysis::BpmResult>,
    pub show_spectrogram: bool, pub spect_status: SpectStatus,
    pub spectrogram_time_first: f64, pub spectrogram_time_last: f64,
    // u8 矩阵 (GPU 纹理的 CPU 副本)
    pub spectrogram_matrix: Option<Vec<u8>>,
    pub spectrum_w: u32, pub spectrum_h: u32,
    pub spectrum_needs_init: bool,       // 本帧需创建 GPU 纹理
    pub spectrum_needs_upload: bool,     // 本帧需全量上传
    spectrogram_result: Option<Arc<Mutex<Option<BgResult>>>>,
    bg_handle: Option<thread::JoinHandle<()>>,
}

/// 后台线程计算结果
struct BgResult {
    bpm_result: Option<analysis::BpmResult>,
    bpm: f64,
    spectrogram: Option<analysis::FullSpectrogram>,
}

impl EditorState {
    pub fn new_blank(config: GameConfig) -> Result<Self, String> {
        let spect_config = SpectrogramConfig::load();
        let show_spec = spect_config.show_spectrogram;
        Ok(Self { notes: vec![], audio_path: String::new(), audio: EditorAudio::new()?,
            cursor_ms: 0.0, snap_divisor: 4, bpm: 120.0, offset_ms: 0.0, playing: false,
            song_rate: 1.0, undo_stack: vec![], config, song_folder: "New Map".into(),
            bg_path: None, dirty: false, open_holds: [None; 4],
            spect_config, bpm_result: None,
            show_spectrogram: show_spec, spect_status: SpectStatus::Hidden,
            spectrogram_time_first: 0.0, spectrogram_time_last: 0.0,
            spectrogram_matrix: None, spectrum_w: 0, spectrum_h: 0,
            spectrum_needs_init: false, spectrum_needs_upload: false,
            spectrogram_result: None, bg_handle: None })
    }

    fn compute_analysis(&mut self) {
        log::info!("[Editor] analysis for {}", self.audio_path);
        self.spect_status = SpectStatus::Computing;

        let audio_path = self.audio_path.clone();
        let spect_config = self.spect_config.clone();
        let result_holder: Arc<Mutex<Option<BgResult>>> = Arc::new(Mutex::new(None));
        let rh = Arc::clone(&result_holder);
        self.spectrogram_result = Some(result_holder);

        self.bg_handle = Some(thread::spawn(move || {
            // BPM 检测 (stratum_dsp)
            let bpm_result = analysis::detect_bpm(&audio_path).ok();
            let bpm = bpm_result.as_ref().map(|r| r.bpm).unwrap_or(120.0);

            // 检查 zstd 缓存
            if let Some(full) = analysis::load_full_cache(&audio_path, &spect_config) {
                log::info!("[Editor] cache hit: {}x{} u8={}KB time=[{:.0}..{:.0}]ms",
                    full.w, full.h, full.matrix.len() / 1024, full.time_first_ms, full.time_last_ms);
                *rh.lock().unwrap() = Some(BgResult { bpm_result, bpm, spectrogram: Some(full) });
                return;
            }

            // PCM 解码 + 频谱生成
            let (pcm, sr) = match analysis::decode_to_pcm(&audio_path) {
                Ok(v) => v,
                Err(e) => { log::error!("[Editor] decode: {e}"); return; }
            };
            let full = analysis::generate_full_spectrogram(&pcm, sr, &spect_config);
            analysis::save_full_cache(&audio_path, &spect_config, &full);
            *rh.lock().unwrap() = Some(BgResult { bpm_result, bpm, spectrogram: Some(full) });
        }));
    }

    pub fn update(&mut self) -> bool {
        // 检查后台线程完成
        if let Some(ref h) = self.bg_handle {
            if h.is_finished() {
                let _ = self.bg_handle.take().unwrap().join();
                if let Some(ref holder) = self.spectrogram_result {
                    if let Some(result) = holder.lock().unwrap().take() {
                        self.bpm_result = result.bpm_result;
                        if let Some(ref r) = self.bpm_result { self.bpm = r.bpm; }
                        else { self.bpm = result.bpm; }
                        if let Some(full) = result.spectrogram {
                            log::info!("[Editor] spectrogram ready: {}x{} time=[{:.0}..{:.0}]ms",
                                full.w, full.h, full.time_first_ms, full.time_last_ms);
                            self.spectrogram_matrix = Some(full.matrix);
                            self.spectrum_w = full.w; self.spectrum_h = full.h;
                            self.spectrogram_time_first = full.time_first_ms;
                            self.spectrogram_time_last = full.time_last_ms;
                            self.spectrum_needs_init = true;
                            self.spectrum_needs_upload = true;
                        }
                    }
                }
                self.spectrogram_result = None;
                self.spect_status = SpectStatus::Hidden;
            }
        }
        if self.spect_status == SpectStatus::Loading { self.spect_status = SpectStatus::Shown; }
        self.spectrum_needs_init || self.spectrum_needs_upload
    }

    pub fn re_render_png(&mut self) {
        self.join_bg(); self.spectrogram_matrix = None;
        self.spect_status = SpectStatus::Hidden;
        self.spectrogram_result = None;
        self.compute_analysis();
    }

    fn join_bg(&mut self) { if let Some(h) = self.bg_handle.take() { let _ = h.join(); } }

    /// 退出编辑器前必须调用, 确保后台线程资源释放
    pub fn cleanup(&mut self) { self.join_bg(); }

    pub fn load_audio(&mut self, path: &str) -> Result<(), String> {
        self.audio.load(path)?; self.audio_path = path.to_string();
        self.song_folder = std::path::Path::new(path).parent().and_then(|p| p.file_name())
            .map(|n| n.to_string_lossy().to_string()).unwrap_or_else(|| "New Map".into());
        self.cursor_ms = 0.0; self.playing = false;
        self.join_bg(); self.compute_analysis(); Ok(())
    }

    pub fn from_existing(json_path: &str, config: GameConfig) -> Result<Self, String> {
        let c = std::fs::read_to_string(json_path).map_err(|e| format!("read: {e}"))?;
        let bf: BeatmapFile = serde_json::from_str(&c).map_err(|e| format!("parse: {e}"))?;
        let ap = std::path::Path::new(json_path).parent().unwrap().join(&bf.meta.song).to_string_lossy().to_string();
        let mut audio = EditorAudio::new()?; audio.load(&ap)?;
        let folder = std::path::Path::new(json_path).parent().and_then(|p| p.file_name())
            .map(|n| n.to_string_lossy().to_string()).unwrap_or_default();
        let notes: Vec<Note> = bf.notes.into_iter().map(|n| match n {
            NoteDef::Tap { time, lane } => (time, time, lane, NoteType::Tap),
            NoteDef::Hold { time, end_time, lane } => (time, end_time, lane, NoteType::Hold),
        }).collect();
        let spect_config = SpectrogramConfig::load();
        let show_spec = spect_config.show_spectrogram;
        let mut es = Self { notes, audio_path: ap, audio, cursor_ms: 0.0, snap_divisor: 4,
            bpm: bf.meta.bpm.max(1.0), offset_ms: bf.meta.offset, playing: false,
            song_rate: 1.0, undo_stack: vec![], config, song_folder: folder,
            bg_path: bf.meta.bg, dirty: false, open_holds: [None; 4],
            spect_config, bpm_result: None,
            show_spectrogram: show_spec, spect_status: SpectStatus::Hidden,
            spectrogram_time_first: 0.0, spectrogram_time_last: 0.0,
            spectrogram_matrix: None, spectrum_w: 0, spectrum_h: 0,
            spectrum_needs_init: false, spectrum_needs_upload: false,
            spectrogram_result: None, bg_handle: None };
        es.compute_analysis(); Ok(es)
    }

    pub fn note_action(&mut self, time_ms: f64, lane: usize) {
        self.push_undo(); let snapped = self.snap_time(time_ms);
        if let Some(head) = self.open_holds[lane].take() {
            self.notes.push((head, snapped.max(head + 100.0), lane, NoteType::Hold));
        } else if let Some(p) = self.notes.iter().position(|n| (n.0 - snapped).abs() < 0.5 && n.2 == lane) {
            self.notes.remove(p); } else { self.notes.push((snapped, snapped, lane, NoteType::Tap)); }
        self.notes.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap()); self.dirty = true;
    }
    pub fn start_hold(&mut self, t: f64, lane: usize) { self.open_holds[lane] = Some(self.snap_time(t)); }
    fn push_undo(&mut self) { self.undo_stack.push(self.notes.clone()); if self.undo_stack.len() > 50 { self.undo_stack.remove(0); } }
    pub fn delete_at_cursor(&mut self) { self.push_undo(); self.notes.retain(|n| (n.0 - self.cursor_ms).abs() > 0.5); self.dirty = true; }
    pub fn undo(&mut self) { if let Some(prev) = self.undo_stack.pop() { self.notes = prev; self.dirty = true; } }
    pub fn snap_time(&self, t: f64) -> f64 { (t / (60_000.0 / self.bpm / self.snap_divisor as f64)).round() * (60_000.0 / self.bpm / self.snap_divisor as f64) }
    pub fn move_cursor(&mut self, d: i32) { self.cursor_ms = (self.cursor_ms + d as f64 * 60_000.0 / self.bpm / self.snap_divisor as f64).max(0.0); }
    pub fn save(&self, jp: &str) -> Result<(), String> {
        let ndefs: Vec<NoteDef> = self.notes.iter().map(|n| match n.3 {
            NoteType::Tap => NoteDef::Tap { time: n.0, lane: n.2 },
            NoteType::Hold => NoteDef::Hold { time: n.0, end_time: n.1, lane: n.2 },
        }).collect();
        let bf = BeatmapFile { meta: BeatmapMeta { song: self.audio_path.clone(), offset: self.offset_ms,
            bg: self.bg_path.clone(), bpm: self.bpm }, notes: ndefs };
        std::fs::write(jp, serde_json::to_string_pretty(&bf).map_err(|e| format!("serialize: {e}"))?)
            .map_err(|e| format!("write: {e}"))
    }
}
