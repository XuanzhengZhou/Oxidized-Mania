/// Transparent ROX chart cache layer.
use rhythm_open_exchange::codec::{auto_decode, Decoder, Encoder, RoxCodec};
use rhythm_open_exchange::model::RoxChart;
use std::path::Path;

const CACHE_DIR: &str = "cache/roxcache";

/// Load a RoxChart with transparent `.rox` caching.
/// Cache key = sanitized source path stem. Invalidation = source mtime > cache mtime.
pub fn load_chart_cached(path: &str) -> Result<RoxChart, String> {
    let src = Path::new(path);
    if !src.exists() { return Err(format!("File not found: {}", src.display())); }

    let src_mtime = src.metadata().ok()
        .and_then(|m| m.modified().ok())
        .unwrap_or(std::time::SystemTime::UNIX_EPOCH);

    let stem = src.file_stem().unwrap_or_default().to_string_lossy();
    let cache_name = sanitize(&stem);
    let cache_path = Path::new(CACHE_DIR).join(format!("{}.rox", cache_name));

    // Check cache: must exist and be newer than source
    let cache_valid = cache_path.exists() && cache_path.metadata().ok()
        .and_then(|m| m.modified().ok())
        .map(|ct| ct >= src_mtime)
        .unwrap_or(false);

    if cache_valid {
        if let Ok(data) = std::fs::read(&cache_path) {
            if let Ok(chart) = <RoxCodec as Decoder>::decode(&data) {
                log::info!("[Cache] hit: {}", src.file_name().unwrap_or_default().to_string_lossy());
                return Ok(chart);
            }
        }
    }

    log::info!("[Cache] miss: {}", src.file_name().unwrap_or_default().to_string_lossy());
    let chart = auto_decode(src).map_err(|e| format!("ROX decode {}: {e}", src.display()))?;

    let _ = std::fs::create_dir_all(CACHE_DIR);
    match <RoxCodec as Encoder>::encode(&chart) {
        Ok(data) => {
            let _ = std::fs::write(&cache_path, &data);
            log::info!("[Cache] saved: {}.rox", cache_name);
        }
        Err(e) => log::warn!("[Cache] encode failed: {e}"),
    }

    Ok(chart)
}

fn sanitize(name: &str) -> String {
    name.chars().map(|c| if c.is_alphanumeric() || c == '-' || c == '_' { c } else { '_' }).collect()
}
