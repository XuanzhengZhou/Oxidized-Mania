use std::ffi::{c_void, CString};
// use std::path::Path;

// ─── BASS 常量 ───

const BASS_POS_BYTE: u32 = 0;
const BASS_STREAM_AUTOFREE: u32 = 0x40000;
const BASS_ATTRIB_TEMPO: u32 = 0x10000;

// ─── FFI 声明 ───

#[link(name = "bass")]
extern "C" {
    fn BASS_Init(device: i32, freq: u32, flags: u32, win: *mut c_void, dsguid: *mut c_void) -> i32;
    fn BASS_Free() -> i32;
    fn BASS_StreamCreateFile(
        mem: u32,
        file: *const c_void,
        offset: u64,
        length: u64,
        flags: u32,
    ) -> u32;
    fn BASS_StreamFree(handle: u32) -> i32;
    fn BASS_ChannelPlay(handle: u32, restart: i32) -> i32;
    fn BASS_ChannelStop(handle: u32) -> i32;
    fn BASS_ChannelPause(handle: u32) -> i32;
    fn BASS_ChannelIsActive(handle: u32) -> u32;
    fn BASS_ChannelSetPosition(handle: u32, pos: u64, mode: u32) -> i32;
    fn BASS_ChannelGetPosition(handle: u32, mode: u32) -> u64;
    fn BASS_ChannelSetAttribute(handle: u32, attrib: u32, value: f32) -> i32;
    fn BASS_ChannelGetAttribute(handle: u32, attrib: u32, value: *mut f32) -> i32;
    fn BASS_ChannelGetInfo(handle: u32, info: *mut BASS_CHANNELINFO) -> i32;
    fn BASS_ErrorGetCode() -> i32;
}

#[repr(C)]
struct BASS_CHANNELINFO {
    freq: u32,
    chans: u32,
    flags: u32,
    ctype: u32,
    origres: u32,
    plugin: u32,
    sample: u32,
    filename: *const i8,
}

// ─── 安全封装 ───

pub struct BassAudio {
    stream: u32,
    freq: u32,
    chans: u32,
}

impl BassAudio {
    /// 查找 BASS 动态库并初始化
    pub fn init() -> Result<Self, String> {
        // BASS 在 #[link] 时已经链接，直接调用 BASS_Init
        let result = unsafe { BASS_Init(-1, 44100, 0, std::ptr::null_mut(), std::ptr::null_mut()) };
        if result == 0 {
            let err = unsafe { BASS_ErrorGetCode() };
            // BASS_ERROR_ALREADY (14): 已初始化，视为成功
            if err != 14 {
                return Err(format!("BASS_Init failed: error {}", err));
            }
        }
        log::info!("BASS audio initialized");

        Ok(Self {
            stream: 0,
            freq: 44100,
            chans: 2,
        })
    }

    pub fn load(&mut self, path: &str) -> Result<(), String> {
        self.unload();

        let c_path = CString::new(path).map_err(|_| "invalid path".to_string())?;
        let handle = unsafe {
            BASS_StreamCreateFile(
                0,
                c_path.as_ptr() as *const c_void,
                0,
                0,
                BASS_STREAM_AUTOFREE,
            )
        };

        if handle == 0 {
            let err = unsafe { BASS_ErrorGetCode() };
            return Err(format!("BASS_StreamCreateFile failed: error {} for {}", err, path));
        }

        // 获取采样率+声道数 (用于 ms↔byte 转换)
        let mut info = BASS_CHANNELINFO {
            freq: 0,
            chans: 0,
            flags: 0,
            ctype: 0,
            origres: 0,
            plugin: 0,
            sample: 0,
            filename: std::ptr::null(),
        };
        unsafe {
            BASS_ChannelGetInfo(handle, &mut info);
        }

        self.stream = handle;
        self.freq = info.freq;
        self.chans = info.chans;
        Ok(())
    }

    pub fn unload(&mut self) {
        if self.stream != 0 {
            unsafe {
                BASS_StreamFree(self.stream);
            }
            self.stream = 0;
        }
    }

    pub fn play(&self) {
        if self.stream != 0 {
            unsafe {
                BASS_ChannelPlay(self.stream, 0);
            }
        }
    }

    pub fn pause(&self) {
        if self.stream != 0 {
            unsafe {
                BASS_ChannelPause(self.stream);
            }
        }
    }

    pub fn stop(&mut self) {
        if self.stream != 0 {
            unsafe {
                BASS_ChannelStop(self.stream);
            }
            self.stream = 0;
        }
    }

    pub fn is_playing(&self) -> bool {
        if self.stream == 0 {
            return false;
        }
        unsafe { BASS_ChannelIsActive(self.stream) == 1 } // BASS_ACTIVE_PLAYING = 1
    }

    pub fn set_position_ms(&self, ms: f64) {
        if self.stream == 0 { return; }
        let byte_pos = (ms / 1000.0 * self.freq as f64 * self.chans as f64 * 2.0) as u64;
        unsafe { BASS_ChannelSetPosition(self.stream, byte_pos, BASS_POS_BYTE); }
    }

    pub fn seek_ms(&self, ms: f64) { self.set_position_ms(ms); }

    pub fn position_ms(&self) -> f64 {
        if self.stream == 0 {
            return 0.0;
        }
        let byte_pos = unsafe { BASS_ChannelGetPosition(self.stream, BASS_POS_BYTE) };
        byte_pos as f64 / (self.freq as f64 * self.chans as f64 * 2.0) * 1000.0
    }

    /// 设置倍速 (使用 BASS_ATTRIB_TEMPO，实时变速不变调)
    pub fn set_tempo(&self, rate: f32) {
        if self.stream == 0 {
            return;
        }
        // TEMPO 范围: -95% ~ +5000%，百分比单位
        let tempo_pct = (rate - 1.0) * 100.0;
        unsafe {
            BASS_ChannelSetAttribute(self.stream, BASS_ATTRIB_TEMPO, tempo_pct);
        }
    }
}

impl Drop for BassAudio {
    fn drop(&mut self) {
        self.unload();
        unsafe {
            BASS_Free();
        }
    }
}
