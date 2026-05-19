use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Clone)]
pub struct SpectrogramConfig {
    #[serde(default = "default_freq_min")]
    pub freq_min: f64,
    #[serde(default = "default_freq_max")]
    pub freq_max: f64,
    #[serde(default = "default_n_mels")]
    pub n_mels: usize,
    #[serde(default = "default_n_fft")]
    pub n_fft: usize,
    #[serde(default = "default_hop_length")]
    pub hop_length: usize,
    #[serde(default = "default_colormap")]
    pub colormap: String,
    #[serde(default = "default_noise_gate")]
    pub noise_gate: f32,
    #[serde(default = "default_show")]
    pub show_spectrogram: bool,
}

fn default_show() -> bool { true }

fn default_freq_min() -> f64 { 20.0 }
fn default_freq_max() -> f64 { 8000.0 }
fn default_n_mels() -> usize { 512 }
fn default_n_fft() -> usize { 4096 }
fn default_hop_length() -> usize { 128 }
fn default_colormap() -> String { "magma".into() }
fn default_noise_gate() -> f32 { 0.35 }

impl Default for SpectrogramConfig {
    fn default() -> Self {
        Self { freq_min: 20.0, freq_max: 8000.0, n_mels: 512, n_fft: 4096,
            hop_length: 128, colormap: "magma".into(), noise_gate: 0.35, show_spectrogram: true }
    }
}
impl SpectrogramConfig {
    pub fn load() -> Self {
        std::fs::read_to_string("editor_config.json").ok().and_then(|s| serde_json::from_str(&s).ok())
            .unwrap_or_else(|| { let c = Self::default(); let _ = c.save(); c })
    }
    pub fn save(&self) -> Result<(), String> {
        std::fs::write("editor_config.json", serde_json::to_string_pretty(self)
            .map_err(|e| format!("serialize: {e}"))?).map_err(|e| format!("write: {e}"))
    }
}
