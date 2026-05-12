use std::collections::HashMap;
use std::sync::atomic::{AtomicU32, Ordering};
use crate::render::quad::QuadRenderer;
use crate::skin::AtlasRegion;
use super::judgment::JudgmentResult;
use super::scoring::Score;
use super::{NoteRT, NoteType};

const NOTE_D: [&str; 4] = ["note_0D", "note_1D", "note_2D", "note_3D"];
const NOTE_: [&str; 4] = ["note_0", "note_1", "note_2", "note_3"];
const HH_D: [&str; 4] = ["hold_head_0D", "hold_head_1D", "hold_head_2D", "hold_head_3D"];
const HH_: [&str; 4] = ["hold_head_0", "hold_head_1", "hold_head_2", "hold_head_3"];
const HB_D: [&str; 4] = ["hold_body_0D", "hold_body_1D", "hold_body_2D", "hold_body_3D"];
const HB_: [&str; 4] = ["hold_body_0", "hold_body_1", "hold_body_2", "hold_body_3"];
const HT_D: [&str; 4] = ["hold_tail_0D", "hold_tail_1D", "hold_tail_2D", "hold_tail_3D"];
const HT_: [&str; 4] = ["hold_tail_0", "hold_tail_1", "hold_tail_2", "hold_tail_3"];
const KEY_D: [&str; 4] = ["key_0D", "key_1D", "key_2D", "key_3D"];
const KEY_: [&str; 4] = ["key_0", "key_1", "key_2", "key_3"];

pub const SCREEN_H: f32 = 600.0;
pub const LEAD_IN: f64 = 3000.0;

static SCREEN_W_VAL: AtomicU32 = AtomicU32::new(800);
pub fn set_screen_w(w: f32) { SCREEN_W_VAL.store(w as u32, Ordering::Relaxed); }
pub fn screen_w() -> f32 { SCREEN_W_VAL.load(Ordering::Relaxed) as f32 }

pub fn calc_lanes(screen_w: f32, spacing: f64, scale: f64) -> [f32; 4] {
    let cx = screen_w as f64 / 2.0;
    let gap = spacing * scale;
    [(cx - gap * 1.5) as f32, (cx - gap * 0.5) as f32, (cx + gap * 0.5) as f32, (cx + gap * 1.5) as f32]
}

pub fn stage_bounds(screen_w: f32, lanes: &[f32; 4], note_w: f32) -> (f32, f32) {
    ((lanes[0] - note_w / 2.0 - 10.0).max(0.0), (lanes[3] + note_w / 2.0 + 10.0).min(screen_w))
}

pub fn note_y(time: f64, current_time: f64, hit_y: f64, eff_speed: f64) -> f64 {
    hit_y - (time - current_time) * eff_speed
}

// ─── 核心音符处理 (对标 Python: 远跳+底漏+渲染 三合一) ───

pub struct NoteProcessResult {
    pub visible: usize,
    pub latest_judgment: Option<JudgmentResult>,
}

/// 完全对标 Python 的单个 for 循环：远跳、底漏、渲染
pub fn process_notes(
    notes: &mut [NoteRT],
    active_idx: &mut usize,
    current_time: f64,
    eff_speed: f64,
    miss_window: f64,
    hit_y: f64,
    note_w: f32,
    lanes: &[f32; 4],
    quad: &mut QuadRenderer,
    skin_regions: Option<&HashMap<String, AtlasRegion>>,
    score: &mut Score,
) -> NoteProcessResult {
    // 1. 清理陈旧音符 (对标 Python cleanup)
    while *active_idx < notes.len() {
        let n = &notes[*active_idx];
        let nt = if n.note_type == NoteType::Hold { n.end_time } else { n.time };
        if (n.hit || n.missed) && (current_time - nt) * eff_speed > 300.0 {
            *active_idx += 1;
        } else { break; }
    }

    // 2. 远跳+底漏+渲染 单循环 (对标 Python note_render)
    let mut visible = 0;
    let mut latest = None;

    for i in *active_idx..notes.len() {
        // 远跳中断 (对标 Python: if (note.time - current_time) * eff_speed > SCREEN_H + 100)
        if (notes[i].time - current_time) * eff_speed > SCREEN_H as f64 + 100.0 { break; }

        // 底漏检测 (对标 Python 1064-1093)
        match notes[i].note_type {
            NoteType::Tap => {
                if !notes[i].hit && !notes[i].missed && current_time - notes[i].time > miss_window {
                    notes[i].missed = true;
                    score.add_judgment(JudgmentResult::Miss);
                    latest = Some(JudgmentResult::Miss);
                }
            }
            NoteType::Hold => {
                if !notes[i].hit {
                    // 头部漏判
                    if !notes[i].missed && !notes[i].holding && current_time - notes[i].time > miss_window {
                        notes[i].missed = true;
                        score.add_judgment(JudgmentResult::Miss);
                        latest = Some(JudgmentResult::Miss);
                    }
                    // 按穿判定
                    if notes[i].holding && current_time >= notes[i].end_time {
                        notes[i].hit = true;
                        notes[i].holding = false;
                        score.add_judgment(JudgmentResult::Perfect);
                        latest = Some(JudgmentResult::Perfect);
                    }
                }
            }
        }

        // 渲染 (对标 Python: 跳过 hit 和 ghost)
        if notes[i].hit || notes[i].ghost { continue; }

        let lane = notes[i].lane;
        let lx = lanes[lane];

        match notes[i].note_type {
            NoteType::Tap => {
                let y = note_y(notes[i].time, current_time, hit_y, eff_speed) as f32;
                if y > -50.0 && y < SCREEN_H {
                    let alpha: u8 = if notes[i].missed { 80 } else { 255 };
                    let mut used = draw_skin_element(quad, skin_regions, NOTE_D[lane], NOTE_[lane], lx, y, note_w, alpha);
                    if !used && notes[i].lane != 0 {
                        used = draw_skin_element(quad, skin_regions, "note_0D", "note_0", lx, y, note_w, alpha);
                    }
                    if !used {
                        let color = if notes[i].missed { [100, 100, 100, 255] } else { [0, 200, 255, 255] };
                        quad.push_rect(lx - note_w/2.0, y - 10.0, note_w, 20.0, color);
                    }
                    visible += 1;
                }
            }
            NoteType::Hold => {
                let head_y = calc_hold_head_y(&notes[i], current_time, hit_y, eff_speed);
                let tail_y = note_y(notes[i].end_time, current_time, hit_y, eff_speed) as f32;
                let rect_h = head_y as i32 - tail_y as i32;
                if rect_h > 0 && tail_y < SCREEN_H && head_y > -50.0 {
                    let alpha: u8 = if notes[i].holding { 255 } else if notes[i].missed { 80 } else { 200 };
                    let mut drawn = draw_hold_skin(quad, skin_regions, notes[i].lane, lx, head_y, tail_y, note_w, alpha);
                    if !drawn && notes[i].lane != 0 {
                        drawn = draw_hold_skin(quad, skin_regions, 0, lx, head_y, tail_y, note_w, alpha);
                    }
                    if !drawn {
                        let color = if notes[i].holding { [150, 255, 150, 255] }
                                    else if notes[i].missed { [80, 100, 80, 255] }
                                    else { [0, 255, 100, 255] };
                        quad.push_rect(lx - note_w/2.0, tail_y, note_w, rect_h as f32, color);
                    }
                    visible += 1;
                }
            }
        }
    }

    NoteProcessResult { visible, latest_judgment: latest }
}

// ─── 辅助函数 ───

fn calc_hold_head_y(note: &NoteRT, current_time: f64, hit_y: f64, eff_speed: f64) -> f32 {
    if let Some(stuck_y) = note.stuck_y {
        if note.holding { stuck_y as f32 }
        else if let Some(rt) = note.release_time {
            (stuck_y + (current_time - rt) * eff_speed) as f32
        } else { note_y(note.time, current_time, hit_y, eff_speed) as f32 }
    } else { note_y(note.time, current_time, hit_y, eff_speed) as f32 }
}

fn draw_skin_element(
    quad: &mut QuadRenderer, regions: Option<&HashMap<String, AtlasRegion>>,
    primary: &str, fallback: &str,
    cx: f32, cy: f32, target_w: f32, alpha: u8,
) -> bool {
    let atlas_map = match regions { Some(a) => a, None => return false };
    let r = match atlas_map.get(primary).or_else(|| atlas_map.get(fallback)) {
        Some(r) => r, None => return false,
    };
    let aspect = r.height as f32 / r.width as f32;
    let draw_h = target_w * aspect;
    let color = [255, 255, 255, alpha];
    quad.push_textured_rect(cx - target_w/2.0, cy - draw_h/2.0, target_w, draw_h, r.uv_x, r.uv_y, r.uv_w, r.uv_h, color);
    true
}

fn draw_hold_skin(
    quad: &mut QuadRenderer, regions: Option<&HashMap<String, AtlasRegion>>,
    lane: usize, lx: f32, head_y: f32, tail_y: f32, note_w: f32, alpha: u8,
) -> bool {
    let atlas_map = match regions { Some(a) => a, None => return false };
    let head_r = atlas_map.get(HH_D[lane]).or_else(|| atlas_map.get(HH_[lane]));
    let body_r = atlas_map.get(HB_D[lane]).or_else(|| atlas_map.get(HB_[lane]));
    let tail_r = atlas_map.get(HT_D[lane]).or_else(|| atlas_map.get(HT_[lane]));
    if head_r.is_none() { return false; }

    let hr = head_r.unwrap();
    let color = [255, 255, 255, alpha];
    let body_color = [255, 255, 255, 255u8]; // 身体始终全不透明 (对标 Python)
    let head_h = note_w * hr.height as f32 / hr.width as f32;
    let tail_h = tail_r.map(|tr| note_w * tr.height as f32 / tr.width as f32).unwrap_or(0.0);

    // 对标 Python: body_top = head_y - head_h/2 (头部中心), body_bottom = tail_y - tail_h (尾部顶部)
    let body_top = head_y - head_h / 2.0;
    let body_bot = tail_y;  // 身体延伸到尾部底部，尾部覆盖在上层无缝衔接

    // 1. 身体 (最底层): 头部中心 → 尾部底部，每格重叠1.5px消除黑线
    if let Some(br) = body_r {
        let bh = note_w * br.height as f32 / br.width as f32;
        if body_top > body_bot && bh > 0.0 {
            let mut ty = body_top;
            while ty > body_bot {
                // draw_h 比需要的高度多 1.5px，向上偏移 0.75px 确保覆盖
                let needed = (ty - body_bot).min(bh);
                let draw_h = needed + 1.5;
                let y = ty - draw_h + 0.75;
                quad.push_textured_rect(lx - note_w/2.0, y, note_w, draw_h,
                    br.uv_x, br.uv_y, br.uv_w, br.uv_h, body_color);
                ty -= bh;
            }
        }
    }

    // 2. 尾巴 (中层): 180° UV翻转, 底部对齐 tail_y
    if let Some(tr) = tail_r {
        quad.push_textured_rect(lx - note_w/2.0, tail_y - tail_h, note_w, tail_h,
            tr.uv_x, tr.uv_y + tr.uv_h, tr.uv_w, -tr.uv_h, color);
    }

    // 3. 头部 (最上层): 底部对齐 head_y
    quad.push_textured_rect(lx - note_w/2.0, head_y - head_h, note_w, head_h,
        hr.uv_x, hr.uv_y, hr.uv_w, hr.uv_h, color);

    true
}

// ─── 按键底板 + 判定特效 ───

pub fn draw_key_pads(lanes: &[f32; 4], note_w: f32, pressed: &[bool; 4], quad: &mut QuadRenderer, atlas: &HashMap<String, AtlasRegion>) {
    let key_y = SCREEN_H - 105.0;
    for lane in 0..4 {
        let region = atlas.get(KEY_D[lane]).or_else(|| atlas.get(KEY_[lane]))
            .or_else(|| atlas.get(KEY_D[0])).or_else(|| atlas.get(KEY_[0]));
        if let Some(r) = region {
            let kw = (note_w - 5.0).max(50.0);
            let kh = kw * r.height as f32 / r.width as f32;
            let draw_h = kh.min(100.0);
            let v_crop = if kh > 100.0 { (kh - 100.0) / kh } else { 0.0 };
            let alpha: u8 = if pressed[lane] { 255 } else { 120 };
            quad.push_textured_rect(lanes[lane] - kw/2.0, key_y, kw, draw_h, r.uv_x, r.uv_y + r.uv_h * v_crop, r.uv_w, r.uv_h * (1.0 - v_crop), [255, 255, 255, alpha]);
        }
    }
}

pub fn draw_hit_burst(quad: &mut QuadRenderer, regions: Option<&HashMap<String, AtlasRegion>>, judgment_type: &mut Option<JudgmentResult>, burst_start: &mut Option<std::time::Instant>, screen_w: f32) {
    let jt = match *judgment_type { Some(j) => j, None => return };
    let regions = match regions { Some(r) => r, None => return };
    let hit_map: &[(JudgmentResult, &[&str])] = &[
        (JudgmentResult::Perfect, &["hit_300g", "hit_300"]),
        (JudgmentResult::Great, &["hit_300", "hit_200"]),
        (JudgmentResult::Good, &["hit_200", "hit_100"]),
        (JudgmentResult::Ok, &["hit_100", "hit_50", "hit_0"]),
        (JudgmentResult::Meh, &["hit_50", "hit_0"]),
        (JudgmentResult::Miss, &["hit_0"]),
    ];
    let mut img_key = None;
    for (j, keys) in hit_map { if jt == *j { for k in *keys { if regions.contains_key(*k) { img_key = Some(*k); break; } } break; } }
    let r = match img_key.and_then(|k| regions.get(k)) { Some(r) => r, None => return };

    // 基于时间渐隐: 持续 ~500ms
    let start = burst_start.unwrap_or(std::time::Instant::now());
    let elapsed_ms = start.elapsed().as_millis() as i32;
    let alpha = (255 - elapsed_ms * 255 / 500).clamp(0, 255) as u8;
    if alpha == 0 { *judgment_type = None; *burst_start = None; return; }
    let draw_w = 180.0f32;
    let draw_h = draw_w * r.height as f32 / r.width as f32;
    let x = screen_w / 2.0 - draw_w / 2.0;
    let y = 250.0 - draw_h / 2.0;
    quad.push_textured_rect(x, y, draw_w, draw_h, r.uv_x, r.uv_y, r.uv_w, r.uv_h, [255, 255, 255, alpha]);
}
