use std::ffi::c_void;

// FFI bindings to sonic.c
extern "C" {
    fn sonicCreateStream(sample_rate: i32, channels: i32) -> *mut c_void;
    fn sonicSetSpeed(stream: *mut c_void, speed: f32);
    fn sonicWriteShortToStream(stream: *mut c_void, samples: *const i16, num_samples: i32) -> i32;
    fn sonicReadShortFromStream(stream: *mut c_void, samples: *mut i16, max_samples: i32) -> i32;
    fn sonicFlushStream(stream: *mut c_void) -> i32;
    fn sonicDestroyStream(stream: *mut c_void);
}

/// 对标 Python process_audio: 变速不变调
pub fn process_audio(raw_bytes: &[u8], speed: f32, sample_rate: i32, channels: i32) -> Vec<u8> {
    if (speed - 1.0).abs() < 0.001 {
        return raw_bytes.to_vec();
    }

    let num_samples = raw_bytes.len() / (2 * channels as usize);
    let mut in_buffer: Vec<i16> = Vec::with_capacity(num_samples);
    for chunk in raw_bytes.chunks_exact(2) {
        in_buffer.push(i16::from_le_bytes([chunk[0], chunk[1]]));
    }

    unsafe {
        let stream = sonicCreateStream(sample_rate, channels);
        sonicSetSpeed(stream, speed);
        sonicWriteShortToStream(stream, in_buffer.as_ptr(), num_samples as i32);
        sonicFlushStream(stream);

        let out_frames = (num_samples as f32 / speed * 1.5) as usize + 4096;
        let mut out_buffer: Vec<i16> = vec![0i16; out_frames * channels as usize];
        let read = sonicReadShortFromStream(stream, out_buffer.as_mut_ptr(), out_frames as i32);
        sonicDestroyStream(stream);

        let byte_count = read as usize * channels as usize * 2;
        let mut result = Vec::with_capacity(byte_count);
        for i in 0..(read as usize * channels as usize) {
            result.extend_from_slice(&out_buffer[i].to_le_bytes());
        }
        result
    }
}

/// 写入 WAV 文件
pub fn write_wav(path: &str, data: &[u8], sample_rate: u32, channels: u16) -> std::io::Result<()> {
    use std::io::Write;
    let mut f = std::fs::File::create(path)?;
    let data_len = data.len() as u32;
    // RIFF header
    f.write_all(b"RIFF")?;
    f.write_all(&(36 + data_len).to_le_bytes())?;
    f.write_all(b"WAVE")?;
    // fmt chunk
    f.write_all(b"fmt ")?;
    f.write_all(&16u32.to_le_bytes())?;
    f.write_all(&1u16.to_le_bytes())?; // PCM
    f.write_all(&channels.to_le_bytes())?;
    f.write_all(&sample_rate.to_le_bytes())?;
    f.write_all(&(sample_rate * channels as u32 * 2).to_le_bytes())?; // byte rate
    f.write_all(&(channels * 2).to_le_bytes())?; // block align
    f.write_all(&16u16.to_le_bytes())?; // bits per sample
    // data chunk
    f.write_all(b"data")?;
    f.write_all(&data_len.to_le_bytes())?;
    f.write_all(data)?;
    Ok(())
}

/// 对标 Python generate_stretched_audio: 变速生成 WAV 文件
/// 使用 BASS 解码，sonic 变速，写入 WAV
pub fn generate_stretched_audio(
    in_file: &str,
    out_wav_file: &str,
    speed: f32,
    bass_lib: Option<&crate::audio::bass::BassAudio>,
) -> bool {
    if (speed - 1.0).abs() < 0.001 { return false; }

    // 尝试用 BASS 解码
    if let Some(_bass) = bass_lib {
        if let Ok((raw, freq, chans)) = read_audio_bass_decode(in_file) {
            let stretched = process_audio(&raw, speed, freq as i32, chans as i32);
            if write_wav(out_wav_file, &stretched, freq, chans as u16).is_ok() {
                return true;
            }
        }
    }
    false
}

/// 使用 BASS 解码音频为原始 PCM (对标 Python _read_audio_bass)
fn read_audio_bass_decode(file_path: &str) -> Result<(Vec<u8>, u32, u32), String> {
    use std::ffi::CString;

    extern "C" {
        fn BASS_StreamCreateFile(mem: u32, file: *const c_void, offset: u64, length: u64, flags: u32) -> u32;
        fn BASS_ChannelGetData(handle: u32, buf: *mut c_void, length: u32) -> u32;
        fn BASS_ChannelGetLength(handle: u32, mode: u32) -> u64;
        fn BASS_StreamFree(handle: u32) -> i32;
        fn BASS_ErrorGetCode() -> i32;
    }

    const BASS_STREAM_DECODE: u32 = 0x200000;
    const BASS_POS_BYTE: u32 = 0;

    let c_path = CString::new(file_path).map_err(|_| "invalid path")?;
    let handle = unsafe {
        BASS_StreamCreateFile(0, c_path.as_ptr() as *const c_void, 0, 0, BASS_STREAM_DECODE)
    };
    if handle == 0 {
        let err = unsafe { BASS_ErrorGetCode() };
        return Err(format!("BASS decode open failed: {}", err));
    }

    // 获取流信息 (通过 BASS_ChannelGetInfo)
    #[repr(C)]
    struct BassChannelInfo {
        freq: u32, chans: u32, flags: u32, ctype: u32, origres: u32, plugin: u32, sample: u32, filename: *const i8,
    }
    extern "C" { fn BASS_ChannelGetInfo(handle: u32, info: *mut BassChannelInfo) -> i32; }

    let mut info = BassChannelInfo { freq: 0, chans: 0, flags: 0, ctype: 0, origres: 0, plugin: 0, sample: 0, filename: std::ptr::null() };
    unsafe { BASS_ChannelGetInfo(handle, &mut info); }

    let length = unsafe { BASS_ChannelGetLength(handle, BASS_POS_BYTE) };
    let mut buf = vec![0u8; length as usize];
    let read = unsafe { BASS_ChannelGetData(handle, buf.as_mut_ptr() as *mut c_void, length as u32) };
    unsafe { BASS_StreamFree(handle); }

    if read > 0 {
        buf.truncate(read as usize);
        Ok((buf, info.freq, info.chans))
    } else {
        Err("BASS decode read failed".into())
    }
}
