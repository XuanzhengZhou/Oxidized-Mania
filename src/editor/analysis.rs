use super::config::SpectrogramConfig;
use spectrs::spectrogram::stft::{par_compute_spectrogram, SpectrogramType};
use spectrs::spectrogram::mel::{par_convert_to_mel, MelScale};

// ─── BPM 结果 ───

#[derive(Clone)]
pub struct BpmResult {
    pub bpm: f64,
    pub beat_times: Vec<f64>,
    pub confidence: f64,
}

// ─── Colormap: matplotlib 8 锚点插值 (CPU 端, 用于构建 256×1 GPU 纹理) ───

pub(crate) fn get_colormap_anchors(name: &str) -> &'static [(f32, f32, f32, f32)] {
    match name {
        "magma" => &[
            (0.00, 0.00, 0.00, 0.04), (0.12, 0.08, 0.00, 0.25),
            (0.25, 0.25, 0.00, 0.45), (0.38, 0.48, 0.06, 0.45),
            (0.55, 0.73, 0.20, 0.20), (0.70, 0.92, 0.40, 0.05),
            (0.85, 0.99, 0.65, 0.10), (1.00, 1.00, 1.00, 0.80),
        ],
        "inferno" => &[
            (0.00, 0.00, 0.00, 0.04), (0.13, 0.12, 0.00, 0.18),
            (0.25, 0.35, 0.02, 0.20), (0.38, 0.60, 0.10, 0.05),
            (0.55, 0.85, 0.27, 0.00), (0.70, 0.98, 0.55, 0.05),
            (0.85, 1.00, 0.82, 0.25), (1.00, 1.00, 1.00, 1.00),
        ],
        "plasma" => &[
            (0.00, 0.05, 0.00, 0.25), (0.17, 0.30, 0.00, 0.60),
            (0.33, 0.60, 0.00, 0.70), (0.50, 0.85, 0.10, 0.50),
            (0.67, 0.95, 0.30, 0.15), (0.83, 1.00, 0.60, 0.00),
            (0.95, 1.00, 0.90, 0.20), (1.00, 1.00, 1.00, 0.80),
        ],
        "gray" => &[(0.0, 0.0, 0.0, 0.0), (1.0, 1.0, 1.0, 1.0)],
        _ => &[  // viridis (default)
            (0.00, 0.27, 0.00, 0.33), (0.14, 0.22, 0.16, 0.52),
            (0.29, 0.13, 0.35, 0.55), (0.43, 0.07, 0.49, 0.44),
            (0.57, 0.15, 0.62, 0.25), (0.71, 0.35, 0.73, 0.09),
            (0.86, 0.65, 0.82, 0.00), (1.00, 0.99, 0.90, 0.13),
        ],
    }
}

#[allow(dead_code)]
fn colormap_rgb(v: f32, name: &str) -> [u8; 3] {
    colormap_rgb_precomputed(v, get_colormap_anchors(name))
}

pub(crate) fn colormap_rgb_precomputed(v: f32, anchors: &[(f32, f32, f32, f32)]) -> [u8; 3] {
    let n = anchors.len();
    let v = v.clamp(0.0, 1.0);
    let (r, g, b) = if n == 2 {
        let a0 = &anchors[0]; let a1 = &anchors[1];
        let t = v;
        (a0.1 + t * (a1.1 - a0.1), a0.2 + t * (a1.2 - a0.2), a0.3 + t * (a1.3 - a0.3))
    } else if v <= anchors[0].0 {
        (anchors[0].1, anchors[0].2, anchors[0].3)
    } else if v >= anchors[n - 1].0 {
        (anchors[n - 1].1, anchors[n - 1].2, anchors[n - 1].3)
    } else {
        let mut lo = &anchors[0]; let mut hi = &anchors[1];
        for w in anchors.windows(2) {
            if v >= w[0].0 && v <= w[1].0 { lo = &w[0]; hi = &w[1]; break; }
        }
        let t = (v - lo.0) / (hi.0 - lo.0);
        (lo.1 + t * (hi.1 - lo.1), lo.2 + t * (hi.2 - lo.2), lo.3 + t * (hi.3 - lo.3))
    };
    [(r * 255.0) as u8, (g * 255.0) as u8, (b * 255.0) as u8]
}

/// 构建 colormap 256×1 RGBA8 像素数据 (用于 GPU 1D 纹理)
pub(crate) fn build_colormap_pixels(name: &str) -> [u8; 256 * 4] {
    let anchors = get_colormap_anchors(name);
    let mut pixels = [0u8; 1024];
    for i in 0..256u16 {
        let v = i as f32 / 255.0;
        let [r, g, b] = colormap_rgb_precomputed(v, anchors);
        let off = i as usize * 4;
        pixels[off] = r;
        pixels[off + 1] = g;
        pixels[off + 2] = b;
        pixels[off + 3] = 255;
    }
    pixels
}

// ─── u8 矩阵计算: Mel 降采样 → 归一化 u8 ───

fn compute_spectrum_matrix(mel: &[Vec<f32>], config: &SpectrogramConfig) -> (Vec<u8>, u32, u32) {
    use rayon::prelude::*;
    let n_mels = mel.len();
    let n_frames = mel.first().map(|r| r.len()).unwrap_or(0);
    if n_mels == 0 || n_frames == 0 { return (vec![], 0, 0); }

    // ── 并行扫描: min / max / total ──
    let (sample_lo, sample_hi, total) = mel.par_iter()
        .flat_map(|row| row.par_iter())
        .fold(|| (f32::MAX, f32::MIN, 0usize), |(lo, hi, cnt), &v| {
            let lv = (v + 1.0).ln();
            (lo.min(lv), hi.max(lv), cnt + 1)
        })
        .reduce(|| (f32::MAX, f32::MIN, 0usize), |a, b| {
            (a.0.min(b.0), a.1.max(b.1), a.2 + b.2)
        });

    // ── 并行直方图 (1000 桶) ──
    let bins = 1000usize;
    let bin_w = (sample_hi - sample_lo).max(1e-6) / bins as f32;
    let hist: Vec<u32> = mel.par_iter()
        .flat_map(|row| row.par_iter())
        .fold(|| vec![0u32; bins], |mut acc, &v| {
            let lv = (v + 1.0).ln();
            let b = (((lv - sample_lo) / bin_w) as usize).min(bins - 1);
            acc[b] += 1;
            acc
        })
        .reduce(|| vec![0u32; bins], |mut a, b| {
            for i in 0..bins { a[i] += b[i]; }
            a
        });

    let lo_idx = (total as f64 * (config.noise_gate as f64 * 0.5)) as usize;
    let hi_idx = (total as f64 * 0.995) as usize;
    let (mut lo, mut hi) = (sample_lo, sample_hi);
    let mut cum = 0usize;
    for (i, &c) in hist.iter().enumerate() {
        let before = cum; cum += c as usize;
        if before <= lo_idx && cum > lo_idx { lo = sample_lo + i as f32 * bin_w; }
        if before <= hi_idx && cum > hi_idx { hi = sample_lo + i as f32 * bin_w; break; }
    }
    let rng = (hi - lo).max(1e-6);
    drop(hist);

    let w = n_mels.min(2048) as u32;
    let h = n_frames.min(8192) as u32;
    let mel_ratio = n_mels as f32 / w as f32;
    let time_ratio = n_frames as f32 / h as f32;

    // ── 并行降采样 → 归一化 u8 ──
    let n_pixels = (w * h) as usize;
    let matrix: Vec<u8> = (0..n_pixels)
        .into_par_iter()
        .map(|idx| {
            let fx = idx as u32 % w;
            let ty = idx as u32 / w;
            let m0 = (fx as f32 * mel_ratio) as usize;
            let m1 = (((fx + 1) as f32 * mel_ratio).ceil() as usize).min(n_mels);
            let t0 = (ty as f32 * time_ratio) as usize;
            let t1 = (((ty + 1) as f32 * time_ratio).ceil() as usize).min(n_frames);
            let mut sum = 0.0f32;
            let mut cnt = 0usize;
            for mi in m0..m1 {
                for ti in t0..t1 {
                    let lv = (mel[mi][ti] + 1.0).ln();
                    sum += ((lv - lo) / rng).clamp(0.0, 1.0);
                    cnt += 1;
                }
            }
            let avg = if cnt > 0 { sum / cnt as f32 } else { 0.0 };
            (avg * 255.0) as u8
        })
        .collect();

    (matrix, w, h)
}

/// 全曲频谱结果: u8 矩阵 + 精确时间范围
pub struct FullSpectrogram {
    pub matrix: Vec<u8>,
    pub w: u32,
    pub h: u32,
    pub time_first_ms: f64,
    pub time_last_ms: f64,
}

/// 全曲频谱 (一次性生成)
pub fn generate_full_spectrogram(pcm: &[i16], sr: u32, config: &SpectrogramConfig) -> FullSpectrogram {
    let mono: Vec<f32> = pcm.chunks(2).map(|c| {
        (c[0] as f32 + c.get(1).copied().unwrap_or(c[0]) as f32) / 2.0 / 32768.0
    }).collect();
    let duration_s = mono.len() as f64 / sr as f64;

    log::info!("[Editor] full spectrogram: {:.1}s, computing STFT...", duration_s);
    let t0 = std::time::Instant::now();

    let spec = par_compute_spectrogram(&mono, config.n_fft, config.hop_length, config.n_fft, true, SpectrogramType::Magnitude);
    let n_frames = spec.first().map(|r| r.len()).unwrap_or(0);
    let mel = par_convert_to_mel(&spec, sr, config.n_fft, config.n_mels,
        Some(config.freq_min as f32), Some(config.freq_max as f32), MelScale::HTK);

    let center_offset = config.n_fft as f64 / 2.0 / sr as f64 * 1000.0;
    let hop_ms = config.hop_length as f64 / sr as f64 * 1000.0;
    let time_first = center_offset;
    let time_last = center_offset + (n_frames as f64 - 1.0) * hop_ms;

    let (matrix, w, h) = compute_spectrum_matrix(&mel, config);
    log::info!("[Editor] full spectrogram: {:.1}s, mel={}x{}, frames={}, time=[{:.0}..{:.0}]ms, matrix={}x{} u8={:.1}MB, took {:.1}s",
        duration_s, mel.len(), n_frames, n_frames,
        time_first, time_last, w, h, matrix.len() as f64 / 1_048_576.0, t0.elapsed().as_secs_f64());
    FullSpectrogram { matrix, w, h, time_first_ms: time_first, time_last_ms: time_last }
}

// ─── BASS decode → i16 PCM ───

pub fn decode_to_pcm(audio_path: &str) -> Result<(Vec<i16>, u32), String> {
    use std::ffi::CString;
    extern "C" {
        fn BASS_StreamCreateFile(mem: u32, file: *const std::ffi::c_void, offset: u64, length: u64, flags: u32) -> u32;
        fn BASS_ChannelGetData(handle: u32, buf: *mut std::ffi::c_void, length: u32) -> u32;
        fn BASS_ChannelGetLength(handle: u32, mode: u32) -> u64;
        fn BASS_StreamFree(handle: u32) -> i32;
        fn BASS_ErrorGetCode() -> i32;
    }
    #[repr(C)] struct Info { freq: u32, chans: u32, _f: u32, _c: u32, _o: u32, _p: u32, _s: u32, _n: *const i8 }
    extern "C" { fn BASS_ChannelGetInfo(handle: u32, info: *mut Info) -> i32; }
    const DECODE: u32 = 0x200000;

    let c = CString::new(audio_path).map_err(|_| "bad path")?;
    let h = unsafe { BASS_StreamCreateFile(0, c.as_ptr() as *const _, 0, 0, DECODE) };
    if h == 0 { return Err(format!("BASS open: {}", unsafe { BASS_ErrorGetCode() })); }
    let mut info = Info { freq: 0, chans: 0, _f: 0, _c: 0, _o: 0, _p: 0, _s: 0, _n: std::ptr::null() };
    unsafe { BASS_ChannelGetInfo(h, &mut info); }
    let len = unsafe { BASS_ChannelGetLength(h, 0) };
    let mut buf = vec![0u8; len as usize];
    let read = unsafe { BASS_ChannelGetData(h, buf.as_mut_ptr() as *mut _, len as u32) };
    unsafe { BASS_StreamFree(h); }
    if read == 0 { return Err("BASS read failed".into()); }
    buf.truncate(read as usize);
    let samples: Vec<i16> = buf.chunks_exact(2).map(|c| i16::from_le_bytes([c[0], c[1]])).collect();
    Ok((samples, info.freq))
}

// ─── BPM (保留 stratum_dsp) ───

pub fn detect_bpm(audio_path: &str) -> Result<BpmResult, String> {
    let (pcm, sr) = decode_to_pcm(audio_path)?;
    let samples: Vec<f32> = pcm.iter().map(|&s| s as f32 / 32768.0).collect();
    let r = stratum_dsp::analyze_audio(&samples, sr, stratum_dsp::AnalysisConfig::default())
        .map_err(|e| format!("BPM: {e}"))?;
    let beats: Vec<f64> = r.beat_grid.beats.iter().map(|&s| s as f64 * 1000.0).collect();
    Ok(BpmResult { bpm: r.bpm as f64, beat_times: beats, confidence: r.bpm_confidence as f64 })
}

// ─── 磁盘缓存 (zstd) ───

fn full_cache_path(audio_path: &str, cfg: &SpectrogramConfig) -> String {
    use std::hash::{Hash, Hasher};
    let stem = std::path::Path::new(audio_path).file_stem().map(|s| s.to_string_lossy().to_string()).unwrap_or_default();
    let mut h = std::collections::hash_map::DefaultHasher::new();
    cfg.n_mels.hash(&mut h); cfg.n_fft.hash(&mut h); cfg.hop_length.hash(&mut h);
    cfg.freq_min.to_bits().hash(&mut h); cfg.freq_max.to_bits().hash(&mut h);
    cfg.colormap.hash(&mut h); cfg.noise_gate.to_bits().hash(&mut h);
    format!(".spectrogram_cache/{}_full_{:016x}.zst", stem, h.finish())
}

pub(crate) fn load_full_cache(audio_path: &str, cfg: &SpectrogramConfig) -> Option<FullSpectrogram> {
    let compressed = std::fs::read(full_cache_path(audio_path, cfg)).ok().filter(|p| p.len() > 32)?;
    let data = zstd::stream::decode_all(&compressed[..]).ok()?;
    if data.len() < 24 { return None; }
    let w = u32::from_le_bytes([data[0], data[1], data[2], data[3]]);
    let h = u32::from_le_bytes([data[4], data[5], data[6], data[7]]);
    let t0 = f64::from_le_bytes(data[8..16].try_into().unwrap());
    let t1 = f64::from_le_bytes(data[16..24].try_into().unwrap());
    let matrix = data[24..].to_vec();
    let expected = (w * h) as usize;
    if matrix.len() < expected { return None; }
    Some(FullSpectrogram { matrix, w, h, time_first_ms: t0, time_last_ms: t1 })
}

pub(crate) fn save_full_cache(audio_path: &str, cfg: &SpectrogramConfig, full: &FullSpectrogram) {
    let _ = std::fs::create_dir_all(".spectrogram_cache");
    let mut raw = Vec::with_capacity(24 + full.matrix.len());
    raw.extend_from_slice(&full.w.to_le_bytes());
    raw.extend_from_slice(&full.h.to_le_bytes());
    raw.extend_from_slice(&full.time_first_ms.to_le_bytes());
    raw.extend_from_slice(&full.time_last_ms.to_le_bytes());
    raw.extend_from_slice(&full.matrix);
    if let Ok(compressed) = zstd::stream::encode_all(&raw[..], 3) {
        let path = full_cache_path(audio_path, cfg);
        let _ = std::fs::write(&path, compressed);
        log::info!("[Editor] cache saved: {} ({}KB raw → {}KB zst)",
            path, raw.len() / 1024,
            std::fs::metadata(&path).map(|m| m.len() as usize / 1024).unwrap_or(0));
    }
}
