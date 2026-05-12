use rosu_pp::model::mode::GameMode;
use rosu_pp::{Beatmap, Difficulty, Performance};
use std::collections::HashMap;
use std::path::Path;
use std::sync::Mutex;

/// 从 JSON 谱面路径推导对应 .osu 文件路径
fn json_to_osu_path(json_path: &str) -> Option<String> {
    let p = Path::new(json_path).with_extension("osu");
    if p.exists() {
        Some(p.to_string_lossy().to_string())
    } else {
        None
    }
}

static STARS_CACHE: std::sync::LazyLock<Mutex<HashMap<(String, u64), f64>>> =
    std::sync::LazyLock::new(|| Mutex::new(HashMap::new()));

/// 使用 rosu-pp 从 .osu 文件计算星数（带缓存）
pub fn calculate_stars(json_path: &str, song_rate: f64) -> f64 {
    // 量化 song_rate 到 0.01 精度作为缓存 key（避免浮点精度问题）
    let rate_key = (song_rate * 100.0).round() as u64;
    let cache_key = (json_path.to_string(), rate_key);
    if let Some(cache) = STARS_CACHE.lock().ok() {
        if let Some(&stars) = cache.get(&cache_key) { return stars; }
    }
    let osu_path = match json_to_osu_path(json_path) {
        Some(p) => p,
        None => return 0.0,
    };
    let map = match Beatmap::from_path(&osu_path) {
        Ok(m) => m,
        Err(e) => { log::warn!("rosu-pp parse '{osu_path}': {e:?}"); return 0.0; }
    };
    let mania = match map.convert(GameMode::Mania, &rosu_pp::GameMods::default()) {
        Ok(m) => m,
        Err(e) => { log::warn!("rosu-pp convert '{osu_path}': {e:?}"); return 0.0; }
    };
    let attrs = Difficulty::new().clock_rate(song_rate).calculate(&mania);
    let stars = attrs.stars() as f64;
    if let Some(mut cache) = STARS_CACHE.lock().ok() {
        cache.insert(cache_key, stars);
    }
    stars
}

/// 使用 rosu-pp 计算 PP
pub fn calculate_pp(json_path: &str, song_rate: f64, accuracy: f64, misses: u32, max_combo: u32) -> f64 {
    let osu_path = match json_to_osu_path(json_path) {
        Some(p) => p,
        None => return 0.0,
    };
    let map = match Beatmap::from_path(&osu_path) {
        Ok(m) => m,
        Err(e) => {
            log::warn!("rosu-pp parse '{osu_path}': {e:?}");
            return 0.0;
        }
    };
    let mania = match map.convert(GameMode::Mania, &rosu_pp::GameMods::default()) {
        Ok(m) => m,
        Err(e) => {
            log::warn!("rosu-pp convert '{osu_path}': {e:?}");
            return 0.0;
        }
    };
    let diff_attrs = Difficulty::new()
        .clock_rate(song_rate)
        .calculate(&mania);
    let perf_attrs = Performance::new(diff_attrs)
        .accuracy(accuracy)
        .misses(misses)
        .combo(max_combo)
        .calculate();
    perf_attrs.pp() as f64
}
