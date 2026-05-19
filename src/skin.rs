use std::collections::HashMap;
use std::path::Path;
use std::sync::Mutex;
use wgpu::util::DeviceExt as _;

struct CachedSkin {
    images: Vec<(String, Vec<u8>, u32, u32)>,
    config: HashMap<String, String>,
}
static SKIN_CACHE: std::sync::LazyLock<Mutex<HashMap<String, std::sync::Arc<CachedSkin>>>> =
    std::sync::LazyLock::new(|| Mutex::new(HashMap::new()));

// ─── 纹理图集 ───

pub struct TextureAtlas {
    pub texture: wgpu::Texture,
    pub view: wgpu::TextureView,
    pub sampler: wgpu::Sampler,
    pub regions: HashMap<String, AtlasRegion>,
    pub size: u32,
}

#[derive(Debug, Clone)]
pub struct AtlasRegion {
    pub uv_x: f32, pub uv_y: f32, pub uv_w: f32, pub uv_h: f32,
    pub width: u32, pub height: u32,
}

// ─── CPU 侧数据 ───

pub struct CpuSkin {
    data: std::sync::Arc<CachedSkin>,
}
impl CpuSkin {
    pub fn images(&self) -> &[(String, Vec<u8>, u32, u32)] { &self.data.images }
    pub fn config(&self) -> &HashMap<String, String> { &self.data.config }
}

// ─── skin.ini 解析 ───

fn parse_skin_ini(content: &str) -> (HashMap<String, String>, HashMap<String, String>) {
    let mut general = HashMap::new();
    let mut mania_4k = HashMap::new();
    let mut current_section = String::new();
    let mut in_mania = false;
    let mut mania_keys: Option<i32> = None;
    let mut temp_config = HashMap::new();

    for raw_line in content.lines() {
        let mut line = raw_line.trim().to_string();
        if line.is_empty() || line.starts_with("//") || line.starts_with('#') { continue; }
        // 去掉行内 //
        if let Some(idx) = line.find("//") { line = line[..idx].trim().to_string(); }
        if line.is_empty() { continue; }

        if line.starts_with('[') && line.ends_with(']') {
            // 保存之前的 Mania 段
            if in_mania {
                if let Some(keys) = mania_keys {
                    if keys == 4 && mania_4k.is_empty() {
                        mania_4k = temp_config.clone();
                    }
                }
            }
            current_section = line[1..line.len()-1].to_string();
            in_mania = current_section == "Mania";
            if in_mania {
                temp_config = HashMap::new();
                mania_keys = None;
            }
            continue;
        }

        let sep = if line.contains(':') { ':' } else if line.contains('=') { '=' } else { continue };
        let (key, value) = line.split_once(sep).unwrap();
        let key = key.trim().to_string();
        let value = value.trim().trim_matches('"').trim_matches('\'').to_string();

        if current_section == "General" {
            general.insert(key, value);
        } else if in_mania {
            if key == "Keys" {
                mania_keys = value.parse().ok();
            }
            temp_config.insert(key, value);
        }
    }
    // 最后一个 Mania 段
    if in_mania {
        if let Some(keys) = mania_keys {
            if keys == 4 && mania_4k.is_empty() {
                mania_4k = temp_config;
            }
        }
    }

    (general, mania_4k)
}

// ─── 图片路径解析 (对标 _resolve_image_path) ───

fn resolve_image_path(skin_root: &Path, image_ref: &str) -> Option<std::path::PathBuf> {
    if image_ref.is_empty() { return None; }
    let image_ref = image_ref.replace('\\', "/");
    let base_ref = if image_ref.to_lowercase().ends_with(".png") || image_ref.to_lowercase().ends_with(".jpg") {
        image_ref[..image_ref.rfind('.').unwrap()].to_string()
    } else { image_ref };

    let candidates: Vec<std::path::PathBuf> = vec![
        skin_root.join(format!("{}.png", base_ref)),
        skin_root.join(format!("{}.jpg", base_ref)),
        skin_root.join(format!("{}.png", Path::new(&base_ref).file_name()?.to_str()?)),
        skin_root.join(format!("{}.jpg", Path::new(&base_ref).file_name()?.to_str()?)),
    ];

    for c in candidates {
        if c.exists() { return Some(c); }
    }
    None
}

pub(crate) fn resize_image(data: &[u8], w: u32, h: u32, target_w: u32, target_h: u32) -> (Vec<u8>, u32, u32) {
    if w == target_w && h == target_h { return (data.to_vec(), w, h); }
    // data 是原始 RGBA8 像素，直接用 ImageBuffer 构造
    let buf = image::ImageBuffer::<image::Rgba<u8>, _>::from_raw(w, h, data.to_vec());
    match buf {
        Some(img) => {
            let resized = image::imageops::resize(&img, target_w, target_h, image::imageops::FilterType::Lanczos3);
            let (nw, nh) = resized.dimensions();
            (resized.into_raw(), nw, nh)
        }
        None => (data.to_vec(), w, h),
    }
}

pub(crate) fn load_png(path: &Path) -> Option<(Vec<u8>, u32, u32)> {
    let data = std::fs::read(path).ok()?;
    let img = image::load_from_memory(&data).ok()?;
    let rgba = img.to_rgba8();
    let (w, h) = rgba.dimensions();
    Some((rgba.into_raw(), w, h))
}

// ─── 主加载函数 ───

impl CpuSkin {
    pub fn load(skin_name: &str, cover_path: Option<&Path>) -> Self {
        inc_skin_load();
        // 缓存 key 只按皮肤名。封面每次独立加载（约 2MB），皮肤纹理（10-30MB）
        // 被同名的所有歌曲共享，不再因封面路径不同而重复缓存。
        let cache_key = format!("skin:{skin_name}");
        let (mut images, mut config) = if let Some(cached) = SKIN_CACHE.lock().ok()
            .and_then(|c| c.get(&cache_key).cloned())
        {
            (cached.images.clone(), cached.config.clone())
        } else {
            let mut imgs = Vec::new();
            let mut cfg = HashMap::new();

            if let Some((data, w, h)) = load_png(std::path::Path::new("assets/logo.png")) {
                let target_h = 512u32;
                let target_w = (w as f64 * target_h as f64 / h as f64) as u32;
                let resized = resize_image(&data, w, h, target_w, target_h);
                imgs.push(("osu_logo".into(), resized.0, resized.1, resized.2));
            }

            if !skin_name.is_empty() {
                let skin_dir = Path::new("skins").join(skin_name);
                let dir = if skin_dir.exists() { &skin_dir }
                    else { &Path::new("../skins").join(skin_name) };

                if dir.exists() {
                    let skin_config_path = dir.join("skin_config.json");
                    let mania_4k: HashMap<String, String> = if skin_config_path.exists() {
                        if let Ok(content) = std::fs::read_to_string(&skin_config_path) {
                            if let Ok(parsed) = serde_json::from_str::<serde_json::Value>(&content) {
                                let mut c = HashMap::new();
                                if let Some(mc) = parsed.get("mania_configs") {
                                    let k4 = mc.get("4").or_else(|| mc.get("4".to_string().as_str()));
                                    if let Some(obj) = k4.and_then(|v| v.as_object()) {
                                        for (k, v) in obj {
                                            if let Some(s) = v.as_str() { c.insert(k.clone(), s.to_string()); }
                                        }
                                    }
                                }
                                c
                            } else { HashMap::new() }
                        } else { HashMap::new() }
                    } else {
                        let ini_path = dir.join("skin.ini");
                        if ini_path.exists() {
                            if let Ok(content) = std::fs::read_to_string(&ini_path) {
                                let (_, m4k) = parse_skin_ini(&content);
                                m4k
                            } else { HashMap::new() }
                        } else { HashMap::new() }
                    };

                    let cfg_fn = |key: &str, default: &str| -> String {
                        mania_4k.get(key).cloned().unwrap_or_else(|| default.to_string())
                    };
                    log::info!("Skin 4K config: {} items from {:?}", mania_4k.len(), dir);

                    let load_img = |image_ref: &str| -> Option<(Vec<u8>, u32, u32)> {
                        resolve_image_path(dir, image_ref).and_then(|p| load_png(&p))
                    };

                    for lane in 0..4 {
                        let ln = lane + 1;
                        let note_ref = cfg_fn(&format!("NoteImage{}", lane), &format!("mania-note{}", ln));
                        let mut note_data = load_img(&note_ref);
                        if note_data.is_none() && lane > 0 {
                            note_data = load_img(&cfg_fn("NoteImage0", "mania-note1"));
                        }
                        if let Some(data) = note_data {
                            imgs.push((format!("note_{}", lane), data.0, data.1, data.2));
                        }

                        let head_ref = cfg_fn(&format!("NoteImage{}H", lane), &format!("mania-note{}H", ln));
                        let mut head_data = load_img(&head_ref);
                        if head_data.is_none() {
                            head_data = imgs.iter().find(|(n,_,_,_)| n == &format!("note_{}", lane)).map(|(_,d,w,h)| (d.clone(), *w, *h));
                        }
                        if let Some(data) = head_data {
                            imgs.push((format!("hold_head_{}", lane), data.0, data.1, data.2));
                        }

                        let body_ref = cfg_fn(&format!("NoteImage{}L", lane), &format!("mania-note{}L", ln));
                        let body_data = load_img(&body_ref).or_else(|| {
                            if lane > 0 { load_img(&cfg_fn("NoteImage0L", "mania-note1L")) } else { None }
                        });
                        if let Some(data) = body_data {
                            imgs.push((format!("hold_body_{}", lane), data.0, data.1, data.2));
                        }

                        let tail_ref = cfg_fn(&format!("NoteImage{}T", lane), &format!("mania-note{}T", ln));
                        let mut tail_data = load_img(&tail_ref);
                        if tail_data.is_none() {
                            tail_data = imgs.iter().find(|(n,_,_,_)| n == &format!("hold_head_{}", lane)).map(|(_,d,w,h)| (d.clone(), *w, *h));
                        }
                        if let Some(data) = tail_data {
                            imgs.push((format!("hold_tail_{}", lane), data.0, data.1, data.2));
                        }

                        let key_ref = cfg_fn(&format!("KeyImage{}", lane), &format!("mania-key{}", ln));
                        if let Some(data) = load_img(&key_ref) {
                            imgs.push((format!("key_{}", lane), data.0, data.1, data.2));
                        }

                        let key_d_ref = cfg_fn(&format!("KeyImage{}D", lane), &format!("mania-key{}D", ln));
                        let key_d_data = load_img(&key_d_ref).or_else(|| {
                            imgs.iter().find(|(n,_,_,_)| n == &format!("key_{}", lane)).map(|(_,d,w,h)| (d.clone(), *w, *h))
                        });
                        if let Some(data) = key_d_data {
                            imgs.push((format!("key_{}D", lane), data.0, data.1, data.2));
                        }
                    }

                    for osu_name in &[
                        cfg_fn("StageBottom", "mania-stage-bottom"),
                        "mania-stage-bottom".to_string(),
                    ] {
                        if let Some(data) = load_img(osu_name) {
                            imgs.push(("stage_bottom".into(), data.0, data.1, data.2));
                            break;
                        }
                    }

                    let hit_defaults = [
                        ("hit_300g", "Hit300gImage", "mania-hit300g"),
                        ("hit_300", "Hit300Image", "mania-hit300"),
                        ("hit_200", "Hit200Image", "mania-hit200"),
                        ("hit_100", "Hit100Image", "mania-hit100"),
                        ("hit_50", "Hit50Image", "mania-hit50"),
                        ("hit_0", "Hit0Image", "mania-hit0"),
                    ];
                    for (internal, cfg_key, default) in &hit_defaults {
                        let ref_name = cfg_fn(cfg_key, default);
                        if let Some(data) = load_img(&ref_name) {
                            imgs.push((internal.to_string(), data.0, data.1, data.2));
                        } else if let Some(data) = load_img(default) {
                            imgs.push((internal.to_string(), data.0, data.1, data.2));
                        }
                    }

                    for (k, v) in &mania_4k { cfg.insert(k.clone(), v.clone()); }
                    log::info!("Skin: {} images collected from {:?}", imgs.len(), dir);
                }
            }

            if imgs.is_empty() {
                imgs.push(("_placeholder".into(), vec![255u8;4], 1, 1));
            }

            // 缓存皮肤纹理（不含封面）
            let cache_arc = std::sync::Arc::new(CachedSkin { images: imgs.clone(), config: cfg.clone() });
            if let Some(mut cache) = SKIN_CACHE.lock().ok() {
                const MAX_CACHE: usize = 8;
                if cache.len() >= MAX_CACHE {
                    if let Some(k) = cache.keys().next().cloned() { cache.remove(&k); }
                }
                let total_mb: usize = cache.values().flat_map(|c| c.images.iter().map(|(_,d,_,_)| d.len())).sum::<usize>() / 1024 / 1024;
                log::info!("[Skin] cache: {} entries, ~{}MB", cache.len() + 1, total_mb);
                cache.insert(cache_key, cache_arc);
            }

            (imgs, cfg)
        };

        // 封面每次独立加载（不同歌曲封面不同，约 2MB）
        if let Some(cp) = cover_path {
            if let Some((data, w, h)) = load_png(cp) {
                let resized = resize_image(&data, w, h, 800, 600);
                // 移除缓存中可能残留的旧封面，替换为新封面
                images.retain(|(name, _, _, _)| name != "bg_cover");
                images.push(("bg_cover".into(), resized.0, resized.1, resized.2));
            }
        }

        Self { data: std::sync::Arc::new(CachedSkin { images, config }) }
    }

    /// 加载单张全分辨率封面（800x600），质量完美
    pub fn load_menu_bg(cover_path: Option<&std::path::Path>) -> Self {
        inc_skin_load();
        let cache_key = format!("menu_bg:{}", cover_path.map(|p| p.to_string_lossy()).unwrap_or_default());
        if let Some(cache) = SKIN_CACHE.lock().ok() {
            if let Some(c) = cache.get(&cache_key) {
                return Self { data: c.clone() };
            }
        }
        let mut images = Vec::new();

        // osu! logo
        if let Some((data, w, h)) = load_png(std::path::Path::new("assets/logo.png")) {
            let target_h = 512u32;
            let target_w = (w as f64 * target_h as f64 / h as f64) as u32;
            let resized = resize_image(&data, w, h, target_w, target_h);
            images.push(("osu_logo".into(), resized.0, resized.1, resized.2));
        }

        // 单张全分辨率封面
        if let Some(cp) = cover_path {
            if let Some((data, w, h)) = load_png(cp) {
                let resized = resize_image(&data, w, h, 800, 600);
                images.push(("bg_cover".into(), resized.0, resized.1, resized.2));
            }
        }

        if images.len() <= 1 {
            images.push(("_placeholder".into(), vec![255u8;4], 1, 1));
        }

        let arc = std::sync::Arc::new(CachedSkin { images, config: HashMap::new() });
        if let Some(mut cache) = SKIN_CACHE.lock().ok() {
            const MAX_CACHE: usize = 8;
            if cache.len() >= MAX_CACHE {
                if let Some(k) = cache.keys().next().cloned() { cache.remove(&k); }
            }
            cache.insert(cache_key, arc.clone());
        }
        Self { data: arc }
    }

    pub fn build_atlas(self, device: &wgpu::Device, queue: &wgpu::Queue) -> TextureAtlas {
        build_atlas(device, queue, self.images())
    }
}

// ─── 图集构建 ───

fn build_atlas(
    device: &wgpu::Device, queue: &wgpu::Queue,
    images: &[(String, Vec<u8>, u32, u32)],
) -> TextureAtlas {
    // 按图片总面积计算最小二次幂尺寸，避免菜单等轻量场景浪费 36MB
    let size = {
        let mut max_w: u32 = 0;
        let mut pack_cx: u32 = 2;
        let mut pack_cy: u32 = 2;
        let mut pack_row_h: u32 = 0;
        for (_, _, w, h) in images.iter().filter(|(_, _, w, h)| *w > 0 && *h > 0) {
            if pack_cx + w + 2 > 3072 { pack_cx = 2; pack_cy += pack_row_h + 2; pack_row_h = 0; }
            max_w = max_w.max(pack_cx + w + 2);
            pack_cx += w + 2;
            pack_row_h = pack_row_h.max(*h);
        }
        let total_h = pack_cy + pack_row_h + 2;
        max_w.max(total_h).next_power_of_two().max(256).min(3072)
    };
    let mut pixels = vec![0u8; (size * size * 4) as usize];
    let mut regions = HashMap::new();
    let mut cx: u32 = 2; let mut cy: u32 = 2; let mut row_h: u32 = 0;

    for (name, data, w, h) in images {
        if *w == 0 || *h == 0 { continue; }
        if cx + w + 2 > size { cx = 2; cy += row_h + 2; row_h = 0; }
        if cy + h + 2 > size { log::warn!("Atlas overflow at {}", name); continue; }
        for row in 0..*h {
            let src = (row * w * 4) as usize;
            let dst = ((cy + row) * size + cx) as usize * 4;
            let len = (*w * 4) as usize;
            pixels[dst..dst + len].copy_from_slice(&data[src..src + len]);
        }
        regions.insert(name.clone(), AtlasRegion {
            uv_x: cx as f32 / size as f32, uv_y: cy as f32 / size as f32,
            uv_w: *w as f32 / size as f32, uv_h: *h as f32 / size as f32,
            width: *w, height: *h,
        });
        cx += w + 2; row_h = row_h.max(*h);
    }

    track_atlas_mem(size as u64 * size as u64 * 4);
    let texture = device.create_texture_with_data(
        queue,
        &wgpu::TextureDescriptor {
            label: Some("skin_atlas"), size: wgpu::Extent3d { width: size, height: size, depth_or_array_layers: 1 },
            mip_level_count: 1, sample_count: 1, dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba8UnormSrgb,
            usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
            view_formats: &[],
        },
        wgpu::util::TextureDataOrder::LayerMajor, &pixels,
    );
    let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
    let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
        address_mode_u: wgpu::AddressMode::ClampToEdge, address_mode_v: wgpu::AddressMode::ClampToEdge,
        address_mode_w: wgpu::AddressMode::ClampToEdge, mag_filter: wgpu::FilterMode::Linear,
        min_filter: wgpu::FilterMode::Linear, ..Default::default()
    });
    TextureAtlas { texture, view, sampler, regions, size }
}

/// 扫描 skins/ 和 ../skins/ 目录，返回所有可用皮肤名（文件夹名）
pub fn list_skins() -> Vec<String> {
    let mut names: Vec<String> = Vec::new();
    for base in &[std::path::Path::new("skins"), std::path::Path::new("../skins")] {
        if let Ok(entries) = std::fs::read_dir(base) {
            for e in entries.flatten() {
                let p = e.path();
                if p.is_dir() {
                    if let Some(name) = p.file_name().and_then(|n| n.to_str()) {
                        if !name.starts_with('.') && !names.contains(&name.to_string()) {
                            names.push(name.to_string());
                        }
                    }
                }
            }
        }
    }
    names.sort();
    names
}

pub fn estimated_memory_mb() -> f64 {
    ATLAS_MEM_MB.load(std::sync::atomic::Ordering::Relaxed) as f64
}
pub fn skin_load_count() -> usize {
    SKIN_LOAD_COUNT.load(std::sync::atomic::Ordering::Relaxed)
}
pub(crate) fn inc_skin_load() {
    SKIN_LOAD_COUNT.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
}
static SKIN_LOAD_COUNT: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
pub(crate) fn track_atlas_mem(size_bytes: u64) {
    ATLAS_MEM_MB.store((size_bytes / 1024 / 1024) as usize, std::sync::atomic::Ordering::Relaxed);
}
static ATLAS_MEM_MB: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
