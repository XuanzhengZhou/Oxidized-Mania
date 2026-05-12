use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GameConfig {
    #[serde(default = "default_scroll_speed")]
    pub scroll_speed: f64,
    #[serde(default)]
    pub global_offset: f64,
    #[serde(default = "default_key_bindings")]
    pub key_bindings: Vec<String>,
    #[serde(default = "default_song_rate")]
    pub song_rate: f64,
    #[serde(default)]
    pub show_fps: bool,
    #[serde(default)]
    pub fullscreen: bool,
    #[serde(default = "default_stage_spacing")]
    pub stage_spacing: f64,
    #[serde(default = "default_stage_scale")]
    pub stage_scale: f64,
    #[serde(default = "default_hit_position")]
    pub hit_position: f64,
    #[serde(default = "default_active_skin")]
    pub active_skin: String,
    #[serde(default = "default_od")]
    pub od: f64,
    #[serde(default)]
    pub mirror_mode: bool,
}

fn default_scroll_speed() -> f64 { 0.6 }
fn default_key_bindings() -> Vec<String> { vec!["d".into(), "f".into(), "j".into(), "k".into()] }
fn default_song_rate() -> f64 { 1.0 }
fn default_stage_spacing() -> f64 { 100.0 }
fn default_stage_scale() -> f64 { 1.0 }
fn default_hit_position() -> f64 { 500.0 }
fn default_active_skin() -> String { String::new() }
fn default_od() -> f64 { 5.0 }

impl Default for GameConfig {
    fn default() -> Self {
        Self {
            scroll_speed: 0.6, global_offset: 0.0,
            key_bindings: default_key_bindings(), song_rate: 1.0,
            show_fps: false, fullscreen: true,
            stage_spacing: 100.0, stage_scale: 1.0,
            hit_position: 500.0, active_skin: String::new(),
            od: 5.0, mirror_mode: false,
        }
    }
}

impl GameConfig {
    pub fn load(path: &str) -> Self {
        std::fs::read_to_string(path)
            .ok()
            .and_then(|s| serde_json::from_str(&s).ok())
            .unwrap_or_default()
    }

    /// 将按键映射到轨道。支持字符串键名("d")和整数键码("100")。
    pub fn key_to_lane(&self, key_name: &str, key_code: Option<u32>) -> Option<usize> {
        // 先尝试整数键码匹配
        if let Some(code) = key_code {
            for (i, binding) in self.key_bindings.iter().enumerate() {
                if let Ok(binding_code) = binding.parse::<u32>() {
                    if binding_code == code { return Some(i); }
                }
            }
        }
        // 再尝试字符串键名匹配
        self.key_bindings.iter().position(|k| k.eq_ignore_ascii_case(key_name))
    }

    pub fn save(&self, path: &str) {
        if let Ok(json) = serde_json::to_string_pretty(self) {
            let _ = std::fs::write(path, json);
        }
    }

}