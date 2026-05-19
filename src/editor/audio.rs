use crate::audio::bass::BassAudio;

pub struct EditorAudio {
    inner: BassAudio,
}

impl EditorAudio {
    pub fn new() -> Result<Self, String> {
        BassAudio::init().map(|inner| Self { inner })
    }
    pub fn load(&mut self, path: &str) -> Result<(), String> {
        self.inner.load_with_tempo(path, 1.0)?;
        Ok(())
    }
    pub fn play(&self) { self.inner.play(); }
    pub fn pause(&self) { self.inner.pause(); }
    pub fn stop(&mut self) { self.inner.stop(); }
    pub fn set_position_ms(&self, ms: f64) { self.inner.set_position_ms(ms); }
    pub fn position_ms(&self) -> f64 { self.inner.position_ms() }
    pub fn set_rate(&mut self, rate: f32, original_path: &str) {
        self.inner.unload();
        let _ = self.inner.load_with_tempo(original_path, rate);
    }
}
