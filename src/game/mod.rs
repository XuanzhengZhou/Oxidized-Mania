pub mod engine;
pub mod hud;
pub mod judgment;
pub mod notes;
pub mod pause;
pub mod results;
pub mod scoring;

// 从 beatmap 重导出共享类型
pub use crate::beatmap::NoteType;

#[derive(Debug, Clone)]
pub struct NoteRT {
    pub time: f64,
    pub end_time: f64,
    pub lane: usize,
    pub note_type: NoteType,

    pub hit: bool,
    pub missed: bool,
    pub holding: bool,
    pub ghost: bool,
    pub stuck_y: Option<f64>,
    pub release_time: Option<f64>,
}
