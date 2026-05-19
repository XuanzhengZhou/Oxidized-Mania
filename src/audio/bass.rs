use std::ffi::{c_void, CString};

// ─── BASS 常量 ───
const BASS_POS_BYTE: u32 = 0;
const BASS_STREAM_AUTOFREE: u32 = 0x40000;
const BASS_STREAM_DECODE: u32 = 0x200000;
const BASS_ATTRIB_TEMPO: u32 = 0x10000;
const BASS_FX_FREESOURCE: u32 = 0x10000;
const BASS_ACTIVE_PLAYING: u32 = 1;

// ─── BASS FFI ───
#[link(name = "bass")]
extern "C" {
    fn BASS_Init(device: i32, freq: u32, flags: u32, win: *mut c_void, dsguid: *mut c_void) -> i32;
    fn BASS_Free() -> i32;
    fn BASS_StreamCreateFile(mem: u32, file: *const c_void, offset: u64, length: u64, flags: u32) -> u32;
    fn BASS_StreamFree(handle: u32) -> i32;
    fn BASS_ChannelPlay(handle: u32, restart: i32) -> i32;
    fn BASS_ChannelStop(handle: u32) -> i32;
    fn BASS_ChannelPause(handle: u32) -> i32;
    fn BASS_ChannelIsActive(handle: u32) -> u32;
    fn BASS_ChannelSetPosition(handle: u32, pos: u64, mode: u32) -> i32;
    fn BASS_ChannelGetPosition(handle: u32, mode: u32) -> u64;
    fn BASS_ChannelSetAttribute(handle: u32, attrib: u32, value: f32) -> i32;
    fn BASS_ChannelGetInfo(handle: u32, info: *mut BASS_CHANNELINFO) -> i32;
    fn BASS_ErrorGetCode() -> i32;
}

// ─── BASS_FX FFI ───
#[link(name = "bass_fx")]
extern "C" {
    fn BASS_FX_TempoCreate(chan: u32, flags: u32) -> u32;
}

#[repr(C)]
struct BASS_CHANNELINFO {
    freq: u32, chans: u32, flags: u32, ctype: u32,
    origres: u32, plugin: u32, sample: u32, filename: *const i8,
}

pub struct BassAudio {
    stream: u32,
    decode: u32,
    freq: u32,
    chans: u32,
    has_fx: bool,
}

impl BassAudio {
    pub fn init() -> Result<Self, String> {
        let result = unsafe { BASS_Init(-1, 44100, 0, std::ptr::null_mut(), std::ptr::null_mut()) };
        if result == 0 {
            let err = unsafe { BASS_ErrorGetCode() };
            if err != 14 { return Err(format!("BASS_Init failed: error {}", err)); }
        }
        Ok(Self { stream: 0, decode: 0, freq: 44100, chans: 2, has_fx: false })
    }

    /// 加载音频。若速率≠1.0 且 BASS_FX 可用则实时变速，否则回退 sonic WAV。
    /// 返回 `Ok(true)` = 需要调用者生成 sonic 变速 WAV。
    pub fn load_with_tempo(&mut self, path: &str, rate: f32) -> Result<bool, String> {
        self.unload();
        let c_path = CString::new(path).map_err(|_| "invalid path".to_string())?;
        let use_tempo = (rate - 1.0).abs() > 0.001;

        if use_tempo {
            if let Ok(()) = self.try_load_tempo(&c_path, rate) {
                self.has_fx = true;
                return Ok(false);
            }
            log::warn!("BASS_FX tempo unavailable, fallback to sonic");
        }

        // 直接播放 (rate=1.0 或 BASS_FX 不可用)
        let handle = unsafe {
            BASS_StreamCreateFile(0, c_path.as_ptr() as *const c_void, 0, 0, BASS_STREAM_AUTOFREE)
        };
        if handle == 0 {
            let err = unsafe { BASS_ErrorGetCode() };
            return Err(format!("BASS_StreamCreateFile failed: error {} for {}", err, path));
        }
        let mut info = BASS_CHANNELINFO { freq: 0, chans: 0, flags: 0, ctype: 0, origres: 0, plugin: 0, sample: 0, filename: std::ptr::null() };
        unsafe { BASS_ChannelGetInfo(handle, &mut info); }
        self.stream = handle;
        self.decode = 0;
        self.freq = info.freq;
        self.chans = info.chans;
        Ok(use_tempo) // true 表示需要 sonic
    }

    fn try_load_tempo(&mut self, c_path: &CString, rate: f32) -> Result<(), String> {
        let decode = unsafe {
            BASS_StreamCreateFile(0, c_path.as_ptr() as *const c_void, 0, 0, BASS_STREAM_DECODE)
        };
        if decode == 0 { return Err(format!("decode failed: {}", unsafe { BASS_ErrorGetCode() })); }

        let tempo = unsafe { BASS_FX_TempoCreate(decode, BASS_STREAM_AUTOFREE | BASS_FX_FREESOURCE) };
        if tempo == 0 {
            unsafe { BASS_StreamFree(decode); }
            return Err("BASS_FX_TempoCreate failed".into());
        }
        let tempo_pct = (rate - 1.0) * 100.0;
        unsafe { BASS_ChannelSetAttribute(tempo, BASS_ATTRIB_TEMPO, tempo_pct); }

        let mut info = BASS_CHANNELINFO { freq: 0, chans: 0, flags: 0, ctype: 0, origres: 0, plugin: 0, sample: 0, filename: std::ptr::null() };
        unsafe { BASS_ChannelGetInfo(decode, &mut info); }
        self.stream = tempo;
        self.decode = decode;
        self.freq = info.freq;
        self.chans = info.chans;
        self.has_fx = true;
        Ok(())
    }

    pub fn load(&mut self, path: &str) -> Result<(), String> { self.load_with_tempo(path, 1.0).map(|_| ()) }

    pub fn unload(&mut self) {
        if self.stream != 0 { unsafe { BASS_StreamFree(self.stream); } self.stream = 0; }
        self.decode = 0; self.has_fx = false;
    }

    pub fn play(&self) { if self.stream != 0 { unsafe { BASS_ChannelPlay(self.stream, 0); } } }
    pub fn pause(&self) { if self.stream != 0 { unsafe { BASS_ChannelPause(self.stream); } } }
    pub fn stop(&mut self) { if self.stream != 0 { unsafe { BASS_ChannelStop(self.stream); } } self.stream = 0; self.decode = 0; }

    pub fn set_position_ms(&self, ms: f64) {
        if self.stream == 0 { return; }
        let byte_pos = (ms / 1000.0 * self.freq as f64 * self.chans as f64 * 2.0) as u64;
        unsafe { BASS_ChannelSetPosition(self.stream, byte_pos, BASS_POS_BYTE); }
    }
    pub fn seek_ms(&self, ms: f64) { self.set_position_ms(ms); }

    pub fn position_ms(&self) -> f64 {
        if self.stream == 0 { return 0.0; }
        let byte_pos = unsafe { BASS_ChannelGetPosition(self.stream, BASS_POS_BYTE) };
        byte_pos as f64 / (self.freq as f64 * self.chans as f64 * 2.0) * 1000.0
    }

    pub fn is_playing(&self) -> bool { self.stream != 0 && unsafe { BASS_ChannelIsActive(self.stream) } == BASS_ACTIVE_PLAYING }

    pub fn set_tempo(&self, rate: f32) {
        if self.stream == 0 { return; }
        let tempo_pct = (rate - 1.0) * 100.0;
        unsafe { BASS_ChannelSetAttribute(self.stream, BASS_ATTRIB_TEMPO, tempo_pct); }
    }
}

impl Drop for BassAudio {
    fn drop(&mut self) { self.unload(); unsafe { BASS_Free(); } }
}
