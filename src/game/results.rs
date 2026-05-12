use crate::render::quad::QuadRenderer;
use crate::render::text::TextRenderer;
use crate::ui::theme;
use super::judgment::{JudgmentWindows, JudgmentResult};
use super::notes::{screen_w, SCREEN_H};

// ─── 数据结构 ───

#[derive(Debug, Clone)]
pub struct GameResult {
    pub score: u32,
    pub standardized_score: u32,
    pub acc: f64,
    pub max_combo: u32,

    pub perfect_count: u32,
    pub great_count: u32,
    pub good_count: u32,
    pub ok_count: u32,
    pub meh_count: u32,
    pub miss_count: u32,

    pub total_notes: u32,
    pub total_objects: u32,
    pub song_name: String,
    pub map_path: String,
    pub song_rate: f64,
    pub od: f64,
    pub mirror_mode: bool,

    pub rank: String,
    pub stars: f64,
    pub pp: f64,
    pub cover_path: Option<String>,
}

// ─── 辅助函数 ───

/// Port from Python `calc_standardized_score` (gameplay.py:402-405)
pub fn standardized_score(accuracy: f64, combo_progress: f64) -> u32 {
    if accuracy <= 0.0 { return 0; }
    let acc_pow = accuracy.powf(2.0 + 2.0 * accuracy);
    let cp = combo_progress.min(1.0).max(0.0);
    (150_000.0 * cp + 850_000.0 * acc_pow * accuracy.min(1.0)) as u32
}

/// Format integer with comma separators (e.g., 1,000,000)
pub fn format_score_commas(score: u32) -> String {
    let s = score.to_string();
    let len = s.len();
    let mut out = String::with_capacity(len + len / 3);
    for (i, c) in s.chars().enumerate() {
        if i > 0 && (len - i) % 3 == 0 { out.push(','); }
        out.push(c);
    }
    out
}

fn judgment_weight(name: &str) -> u32 {
    match name { "PERFECT" => 305, "GREAT" => 300, "GOOD" => 200, "OK" => 100, "MEH" => 50, _ => 0 }
}

// ─── 圆环几何常量 ───

const RING_CY: f32 = 145.0;
const RING_INNER_R: f32 = 100.0;           // 直径 = SCREEN_H/3 = 200
const RING_INNER_WIDTH: f32 = 6.67;        // outer_width / 5
const RING_GAP: f32 = 3.0;
const RING_OUTER_WIDTH: f32 = 33.33;       // RING_INNER_R / 3

fn ring_outer_inner() -> f32 { RING_INNER_R + RING_INNER_WIDTH + RING_GAP }
fn ring_outer_outer() -> f32 { ring_outer_inner() + RING_OUTER_WIDTH }
fn capsule_radius() -> f32 { ring_outer_outer() + 16.0 }

// Rank 阈值角度 (从顶部顺时针): (start_deg, end_deg, color)
const RANK_ARCS: &[(f32, f32, [u8; 4])] = &[
    (0.0,   252.0, theme::RANK_D),
    (252.0, 288.0, theme::RANK_C),
    (288.0, 324.0, theme::RANK_B),
    (324.0, 342.0, theme::RANK_A),
    (342.0, 357.0, theme::RANK_S),
    (357.0, 360.0, theme::RANK_SS),
];

// 胶囊标签角度
const CAPSULES: &[(f32, &str)] = &[
    (126.0,  "D"),
    (270.0,  "C"),
    (306.0,  "B"),
    (333.0,  "A"),
    (349.5,  "S"),
    (0.0,    "SS"),
];

// ─── 圆环绘制 ───

/// Draw a filled ring arc from start_deg to end_deg clockwise from top.
/// Uses thin radial rectangles (~1 per degree) to approximate the arc.
fn draw_ring_arc(
    quad: &mut QuadRenderer,
    cx: f32, cy: f32,
    inner_r: f32, outer_r: f32,
    start_deg: f32, end_deg: f32,
    color: [u8; 4],
) {
    if end_deg <= start_deg { return; }
    let mid_r = (inner_r + outer_r) / 2.0;
    let h = outer_r - inner_r;
    let steps = (end_deg - start_deg).ceil() as usize;
    for i in 0..steps {
        let deg = start_deg + i as f32 + 0.5;
        let a = ((deg - 90.0) as f64).to_radians() as f32;
        let x = cx + mid_r * a.cos();
        let y = cy + mid_r * a.sin();
        quad.push_rect(x - 1.5, y - h / 2.0, 3.0, h, color);
    }
}

/// Draw accuracy fill on the outer ring — stacks very thin concentric rings for a flat look.
fn draw_accuracy_fill(
    quad: &mut QuadRenderer,
    cx: f32, cy: f32,
    accuracy: f64,
) {
    let fill_deg = (accuracy / 100.0 * 360.0).min(360.0) as f32;
    if fill_deg <= 0.0 { return; }
    let inner_r = ring_outer_inner();
    let outer_r = ring_outer_outer();
    let color: [u8; 4] = [113, 222, 250, 255];

    // ~16 concentric ultra-thin rings (≈2px each) → no radial stepping artefacts
    let num_rings = 16;
    let ring_w = (outer_r - inner_r) / num_rings as f32;
    for i in 0..num_rings {
        let r0 = inner_r + i as f32 * ring_w;
        let r1 = r0 + ring_w;
        draw_ring_arc(quad, cx, cy, r0, r1, 0.0, fill_deg, color);
    }
}

/// Draw a capsule (pill shape with rounded ends) at the given polar position.
fn draw_capsule_label(
    quad: &mut QuadRenderer,
    text: &mut TextRenderer,
    cx: f32, cy: f32,
    angle_deg: f32, radius: f32,
    label: &str, bg_color: [u8; 4],
) {
    let a = ((angle_deg - 90.0) as f64).to_radians() as f32;
    let px = cx + radius * a.cos();
    let py = cy + radius * a.sin();
    let tw = label.len() as f32 * 7.0;
    draw_pill(quad, px, py, tw + 10.0, 16.0, bg_color);
    text.queue_text(label, px - tw / 2.0, py - 5.0, 11.0, [0, 0, 0, 255]);
}

/// Draw a pill/capsule shape centered at (cx, cy) using stacked rects for rounded ends.
fn draw_pill(quad: &mut QuadRenderer, cx: f32, cy: f32, body_w: f32, h: f32, color: [u8; 4]) {
    let r = h / 2.0;  // radius of semicircular ends
    let n_strips = 8;
    // Left semicircle
    let lx = cx - body_w / 2.0;
    for i in 0..n_strips {
        let t = (i as f32 + 0.5) / n_strips as f32 - 0.5; // -0.5 .. 0.5
        let strip_h = h / n_strips as f32;
        let strip_w = (r * 2.0 * (1.0 - t * t * 4.0).max(0.0).sqrt()).max(1.0);
        let sy = cy + t * h; // t*h ranges from -h/2 to +h/2
        quad.push_rect(lx - strip_w / 2.0, sy, strip_w, strip_h, color);
    }
    // Right semicircle
    let rx = cx + body_w / 2.0;
    for i in 0..n_strips {
        let t = (i as f32 + 0.5) / n_strips as f32 - 0.5;
        let strip_h = h / n_strips as f32;
        let strip_w = (r * 2.0 * (1.0 - t * t * 4.0).max(0.0).sqrt()).max(1.0);
        let sy = cy + t * h;
        quad.push_rect(rx - strip_w / 2.0, sy, strip_w, strip_h, color);
    }
    // Center body
    quad.push_rect(lx, cy - r, body_w, h, color);
}

/// Draw a pill at a specific rect position (x, y = top-left of body area).
fn draw_pill_at(quad: &mut QuadRenderer, x: f32, y: f32, body_w: f32, h: f32, color: [u8; 4]) {
    let cx = x + body_w / 2.0;
    let cy = y + h / 2.0;
    draw_pill(quad, cx, cy, body_w, h, color);
}

// ─── 第1页：左侧 1/3 圆环+数据（带左边距），右侧 2/3 偏移直方图 ───

fn draw_page1(
    quad: &mut QuadRenderer,
    text: &mut TextRenderer,
    result: &GameResult,
    offsets: &[HitOffset],
) {
    let sw = screen_w();
    let left_margin = sw / 12.0;           // 左边留空 1/12 屏宽
    let left_w = sw / 3.0;                // 左侧面板宽度
    let cx = left_margin + left_w / 2.0;  // 圆环中心
    let shift_y = SCREEN_H / 6.0;         // 整体下移 1/6 屏高
    let text_down = 75.0;   // 分数及以下组件下移量
    let cy = RING_CY + shift_y;

    // ── 左侧面板：圆环 + 数据 ──

    // 内层圆环 — 用多个极薄同心环避免 3D 纹理
    let inner_num = 3;
    let inner_ring_w = RING_INNER_WIDTH / inner_num as f32;
    for &(start, end, color) in RANK_ARCS {
        for j in 0..inner_num {
            let r0 = RING_INNER_R + j as f32 * inner_ring_w;
            let r1 = r0 + inner_ring_w;
            draw_ring_arc(quad, cx, cy, r0, r1, start, end, color);
        }
    }
    // 外层圆环
    draw_accuracy_fill(quad, cx, cy, result.acc);

    // Rank 字母
    let rank_str = &result.rank;
    let rank_fs: f32 = if rank_str == "SS" { 38.0 } else { 48.0 };
    let rank_w = rank_str.len() as f32 * rank_fs * 0.55;
    text.queue_text(rank_str, cx - rank_w / 2.0, cy - rank_fs / 2.0, rank_fs, [206, 228, 251, 255]);

    // 胶囊标签
    let cap_r = capsule_radius();
    for &(angle, label) in CAPSULES {
        draw_capsule_label(quad, text, cx, cy, angle, cap_r, label, theme::rank_color(label));
    }

    // 标准化分数
    let score_str = format_score_commas(result.standardized_score);
    let score_w = score_str.len() as f32 * 24.0 * 0.55;
    text.queue_text(&score_str, cx - score_w / 2.0, 210.0 + shift_y + text_down, 22.0, [206, 228, 251, 255]);

    // 星数胶囊 + mods
    let star_text = format!("{:.2}★", result.stars);
    let star_tw = star_text.len() as f32 * 11.0 * 0.55;
    let star_cap_x = cx - star_tw / 2.0 - 6.0;
    let star_cap_y = 242.0 + shift_y + text_down;
    // 星数胶囊（真圆角）
    let sc = theme::star_color(result.stars);
    draw_pill_at(quad, star_cap_x, star_cap_y, star_tw + 12.0, 16.0, sc);
    text.queue_text(&star_text, star_cap_x + 6.0, star_cap_y + 1.0, 11.0, [0, 0, 0, 255]);

    let mut mods_str = format!("OD:{:.1}", result.od);
    if (result.song_rate - 1.0).abs() > 0.01 { mods_str.push_str(&format!(" {:.1}x", result.song_rate)); }
    if result.mirror_mode { mods_str.push_str(" Mirror"); }
    text.queue_text(&mods_str, star_cap_x + star_tw + 20.0, star_cap_y + 2.0, 10.0, [180, 180, 200, 255]);

    // 准确率 / 最大连击 / PP (3列)
    let col_w = left_w / 3.0;
    let content_left = left_margin;
    let info_y = 272.0 + shift_y + text_down;
    text.queue_text("准确率", content_left + col_w * 0.5 - 21.0, info_y - 3.0, 11.0, [160, 160, 170, 255]);
    text.queue_text(&format!("{:.2}%", result.acc), content_left + col_w * 0.5 - 20.0, info_y + 12.0, 14.0, [206, 228, 251, 255]);
    text.queue_text("最大连击", content_left + col_w * 1.5 - 24.0, info_y - 3.0, 11.0, [160, 160, 170, 255]);
    text.queue_text(&format!("{}/{}", result.max_combo, result.total_objects), content_left + col_w * 1.5 - 24.0, info_y + 12.0, 14.0, [206, 228, 251, 255]);
    text.queue_text("PP", content_left + col_w * 2.5 - 8.0, info_y - 3.0, 11.0, [160, 160, 170, 255]);
    text.queue_text(&format!("{:.0}", result.pp), content_left + col_w * 2.5 - 12.0, info_y + 12.0, 14.0, [206, 228, 251, 255]);

    // 文件夹名 + 难度名
    let folder_name = std::path::Path::new(&result.map_path)
        .parent().and_then(|p| p.file_name())
        .map(|n| n.to_string_lossy().to_string()).unwrap_or_default();
    let diff_name = std::path::Path::new(&result.map_path)
        .file_stem().map(|n| n.to_string_lossy().to_string()).unwrap_or_default();
    let meta_y = info_y + 28.0;
    text.queue_text(&folder_name, content_left + 2.0, meta_y, 9.0, [130, 140, 160, 255]);
    text.queue_text(&diff_name, content_left + 2.0, meta_y + 14.0, 9.0, [130, 140, 160, 255]);

    // 判定统计 (6列)
    let judgments: &[(&str, u32, [u8; 4])] = &[
        ("PERFECT", result.perfect_count, theme::RANK_SS),
        ("GREAT",   result.great_count,   theme::RANK_S),
        ("GOOD",    result.good_count,    theme::RANK_A),
        ("OK",      result.ok_count,      theme::RANK_B),
        ("MEH",     result.meh_count,     theme::RANK_C),
        ("MISS",    result.miss_count,    theme::RANK_D),
    ];
    let jy = 355.0 + shift_y + text_down;
    let jcol_w = left_w / 6.0;
    for (i, (name, count, color)) in judgments.iter().enumerate() {
        let jcx = content_left + jcol_w * i as f32 + jcol_w / 2.0;
        text.queue_text(name, jcx - name.len() as f32 * 5.0, jy, 10.0, *color);
        let cnt_s = count.to_string();
        text.queue_text(&cnt_s, jcx - cnt_s.len() as f32 * 7.0, jy + 14.0, 13.0, [206, 228, 251, 255]);
    }

    // ── 右侧面板：偏移直方图 ──
    let right_margin = sw / 12.0;
    let hist_x = left_margin + left_w + 5.0;
    let hist_w = sw - hist_x - right_margin;
    let hist_y = SCREEN_H / 8.0;
    let hist_h = SCREEN_H - SCREEN_H / 4.0;
    draw_histogram(quad, text, offsets, result, hist_x, hist_y, hist_w, hist_h);

    // ── 底部提示 ──
    let hint = "[S] 保存回放  [R] 重试  [ENTER] 返回  [←→] 翻页";
    let hw = hint.len() as f32 * 6.0;
    text.queue_text(hint, sw / 2.0 - hw / 2.0, SCREEN_H - 25.0, 11.0, [160, 160, 170, 255]);
}

// ─── 命中偏移数据 ───

#[derive(Debug, Clone)]
pub struct HitOffset {
    pub time_ms: u32,
    pub offset_ms: f64,
    pub judgment: JudgmentResult,
}

/// Compute hit offsets by matching replay events to beatmap notes.
pub fn compute_hit_offsets(
    replay: &crate::replay::ReplayData,
    beatmap_path: &str,
) -> Vec<HitOffset> {
    use crate::beatmap::NoteDef;

    let bf: crate::beatmap::BeatmapFile = match std::fs::read_to_string(beatmap_path)
        .ok().and_then(|s| serde_json::from_str(&s).ok())
    {
        Some(v) => v,
        None => return Vec::new(),
    };

    let mut notes: Vec<(f64, f64, usize)> = Vec::new();
    for n in &bf.notes {
        match n {
            NoteDef::Tap { time, lane } => notes.push((*time, *time, *lane)),
            NoteDef::Hold { time, end_time, lane } => notes.push((*time, *end_time, *lane)),
        }
    }
    notes.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap_or(std::cmp::Ordering::Equal));

    if replay.mirror_mode {
        let mirror = [3usize, 2, 1, 0];
        for n in &mut notes { n.2 = mirror[n.2]; }
    }

    let windows = JudgmentWindows::new(replay.od, replay.song_rate);

    let mut matched: Vec<bool> = vec![false; notes.len()];
    let mut offsets: Vec<HitOffset> = Vec::new();

    for ev in &replay.events {
        if !ev.pressed { continue; }
        let et = ev.time_ms as f64;
        let lane = ev.lane as usize;

        let mut best_idx: Option<usize> = None;
        let mut best_dist = f64::MAX;
        for (i, (nt, _end_t, nl)) in notes.iter().enumerate() {
            if matched[i] || *nl != lane { continue; }
            let dist = (et - *nt).abs();
            if dist < windows.miss && dist < best_dist {
                best_dist = dist;
                best_idx = Some(i);
            }
        }

        if let Some(idx) = best_idx {
            matched[idx] = true;
            let offset = et - notes[idx].0;
            let abs_off = offset.abs();
            let jt = if abs_off <= windows.perfect { JudgmentResult::Perfect }
                else if abs_off <= windows.great { JudgmentResult::Great }
                else if abs_off <= windows.good { JudgmentResult::Good }
                else if abs_off <= windows.ok { JudgmentResult::Ok }
                else if abs_off <= windows.meh { JudgmentResult::Meh }
                else { JudgmentResult::Miss };
            offsets.push(HitOffset { time_ms: ev.time_ms, offset_ms: offset, judgment: jt });
        }
    }

    // 未匹配的音符 → MISS
    for (i, (nt, _et, _nl)) in notes.iter().enumerate() {
        if !matched[i] {
            offsets.push(HitOffset { time_ms: *nt as u32, offset_ms: 0.0, judgment: JudgmentResult::Miss });
        }
    }
    offsets.sort_by(|a, b| a.time_ms.cmp(&b.time_ms));

    offsets
}

// ─── 第2页：统计图表 (上下排列) ───

fn draw_page2(
    quad: &mut QuadRenderer,
    text: &mut TextRenderer,
    result: &GameResult,
    offsets: &[HitOffset],
    view_start: f64,
    view_end: f64,
    n_sec: f64,
) {
    let sw = screen_w();
    let margin = 10.0;
    let chart_w = sw - margin * 2.0;
    let top_y = 40.0;
    let bottom_hint_h = 30.0;
    let gap = 6.0;
    let available_h = SCREEN_H - top_y - bottom_hint_h;
    let chart_h = (available_h - gap) / 2.0;

    draw_acc_curve(quad, text, offsets, result, margin, top_y, chart_w, chart_h, view_start, view_end);
    draw_nsec_loss(quad, text, offsets, result, margin, top_y + chart_h + gap, chart_w, chart_h,
        view_start, view_end, n_sec);

    // Bottom hint
    let cx = sw / 2.0;
    let hint = "[A/D] 平移  [Z/C] 缩放  [X] 调节N  [Shift] 加速  [←→] 翻页  [ENTER] 返回";
    let hw = hint.len() as f32 * 6.0;
    text.queue_text(hint, cx - hw / 2.0, SCREEN_H - 25.0, 11.0, [160, 160, 170, 255]);
}

// ─── ACC-时间曲线 ───

fn draw_acc_curve(
    quad: &mut QuadRenderer,
    text: &mut TextRenderer,
    offsets: &[HitOffset],
    _result: &GameResult,
    x: f32, y: f32, w: f32, h: f32,
    view_start: f64, view_end: f64,
) {
    quad.push_rect(x, y, w, h, [25, 25, 35, 255]);
    quad.push_rect(x, y, w, 1.0, [60, 60, 80, 255]);
    quad.push_rect(x, y + h, w, 1.0, [60, 60, 80, 255]);
    quad.push_rect(x, y, 1.0, h, [60, 60, 80, 255]);
    quad.push_rect(x + w, y, 1.0, h, [60, 60, 80, 255]);

    let title = "ACC 曲线";
    text.queue_text(title, x + w / 2.0 - title.len() as f32 * 5.0, y - 8.0, 10.0, [200, 200, 200, 255]);

    if offsets.is_empty() {
        text.queue_text("无回放数据", x + w / 2.0 - 30.0, y + h / 2.0, 11.0, [120, 120, 120, 255]);
        return;
    }

    let total_duration = offsets.last().map(|o| o.time_ms as f64).unwrap_or(1000.0);
    let mut cum_score: u64 = 0;
    let mut cum_total: u64 = 0;
    let mut pts: Vec<(f64, f64)> = Vec::new();

    for o in offsets {
        let w = judgment_weight(o.judgment.name()) as u64;
        cum_score += w;
        cum_total += 305;
        let acc = cum_score as f64 / cum_total as f64 * 100.0;
        pts.push((o.time_ms as f64, acc));
    }

    for &pct in &[100.0, 90.0, 80.0] {
        let ly = y + h - ((pct - 80.0) / 20.0 * h as f64) as f32;
        quad.push_rect(x, ly, w, 1.0, [50, 50, 65, 255]);
        let lbl = format!("{:.0}%", pct);
        text.queue_text(&lbl, x + 2.0, ly - 2.0, 8.0, [120, 120, 120, 255]);
    }

    let ve = view_end.max(view_start + 0.01);
    let vs = view_start;
    for i in 1..pts.len() {
        let (t0, a0) = pts[i - 1];
        let (t1, a1) = pts[i];
        let sx = (t0 / total_duration - vs) / (ve - vs);
        let ex = (t1 / total_duration - vs) / (ve - vs);
        if sx > 1.0 || ex < 0.0 { continue; }
        let sx_c = sx.max(0.0).min(1.0);
        let ex_c = ex.max(0.0).min(1.0);
        let px0 = x + (sx_c * w as f64) as f32;
        let px1 = x + (ex_c * w as f64) as f32;
        let py0 = y + h - (((a0 - 80.0) / 20.0).min(1.0).max(0.0) * h as f64) as f32;
        let py1 = y + h - (((a1 - 80.0) / 20.0).min(1.0).max(0.0) * h as f64) as f32;
        quad.push_rect(px0, py0.min(py1) - 1.0, (px1 - px0).max(1.0), (py1 - py0).abs().max(2.0), [0, 200, 200, 255]);
    }

    let info = format!("[{:.0}%~{:.0}%]", vs * 100.0, ve * 100.0);
    text.queue_text(&info, x + 2.0, y + h + 14.0, 9.0, [150, 150, 150, 255]);
}

// ─── 每N秒ACC损失 ───

fn draw_nsec_loss(
    quad: &mut QuadRenderer,
    text: &mut TextRenderer,
    offsets: &[HitOffset],
    result: &GameResult,
    x: f32, y: f32, w: f32, h: f32,
    view_start: f64, view_end: f64,
    n_sec: f64,
) {
    quad.push_rect(x, y, w, h, [25, 25, 35, 255]);
    quad.push_rect(x, y, w, 1.0, [60, 60, 80, 255]);
    quad.push_rect(x, y + h, w, 1.0, [60, 60, 80, 255]);

    let title = format!("{}s ACC损失", n_sec);
    text.queue_text(&title, x + w / 2.0 - title.len() as f32 * 5.0, y - 8.0, 10.0, [200, 200, 200, 255]);

    if offsets.is_empty() {
        text.queue_text("无回放数据", x + w / 2.0 - 30.0, y + h / 2.0, 11.0, [120, 120, 120, 255]);
        return;
    }

    let total_duration = offsets.last().map(|o| o.time_ms as f64).unwrap_or(1000.0);
    let n_ms = n_sec * 1000.0;
    if n_ms <= 0.0 || total_duration <= 0.0 { return; }

    let num_buckets = ((total_duration / n_ms).ceil() as usize).max(1);
    let mut bucket_score: Vec<u64> = vec![0; num_buckets];
    let mut bucket_total: Vec<u64> = vec![0; num_buckets];
    let mut bucket_has_hits: Vec<bool> = vec![false; num_buckets];

    for o in offsets {
        let bi = ((o.time_ms as f64 / n_ms) as usize).min(num_buckets - 1);
        let w = judgment_weight(o.judgment.name()) as u64;
        bucket_score[bi] += w;
        bucket_total[bi] += 305;
        bucket_has_hits[bi] = true;
    }

    let mut losses: Vec<f64> = Vec::with_capacity(num_buckets);
    let mut max_loss: f64 = 1.0;
    for bi in 0..num_buckets {
        let acc = if bucket_total[bi] > 0 {
            bucket_score[bi] as f64 / bucket_total[bi] as f64 * 100.0
        } else { 100.0 };
        let loss = 100.0 - acc;
        max_loss = max_loss.max(loss);
        losses.push(loss);
    }
    if max_loss < 0.1 { max_loss = 20.0; }

    let bar_w = (w / num_buckets as f32).max(2.0);
    let ve = view_end.max(view_start + 0.01);
    let vs = view_start;
    let vis_start = (vs * num_buckets as f64) as usize;
    let vis_end = ((ve * num_buckets as f64).ceil() as usize).min(num_buckets);

    let sky_blue: [u8; 4] = [100, 180, 240, 255];
    for bi in vis_start..vis_end {
        let b_rel = bi as f64 / num_buckets as f64;
        if b_rel < vs || b_rel > ve { continue; }
        let bx = x + ((b_rel - vs) / (ve - vs) * w as f64) as f32;
        if !bucket_has_hits[bi] {
            // 无操作 → 天蓝色虚线柱
            let dash_h = 5.0;
            let gap_h = 4.0;
            let mut dy = 0.0f32;
            while dy < h {
                quad.push_rect(bx, y + h - dy - dash_h, bar_w, dash_h.min(h - dy), sky_blue);
                dy += dash_h + gap_h;
            }
        } else {
            let loss = losses[bi];
            let bar_h = (loss / max_loss * h as f64) as f32;
            let color: [u8; 4] = if loss < 5.0 { [80, 200, 80, 255] }
                else if loss < 15.0 { [220, 200, 50, 255] }
                else { [240, 80, 60, 255] };
            quad.push_rect(bx, y + h - bar_h, bar_w, bar_h, color);
        }
    }

    text.queue_text("绿<5% 黄<15% 红>15%", x + 2.0, y + h + 14.0, 8.0, [150, 150, 150, 255]);
}

// ─── 命中偏移直方图 ───

fn draw_histogram(
    quad: &mut QuadRenderer,
    text: &mut TextRenderer,
    offsets: &[HitOffset],
    result: &GameResult,
    x: f32, y: f32, w: f32, h: f32,
) {
    quad.push_rect(x, y, w, h, [25, 25, 35, 255]);
    quad.push_rect(x, y, w, 1.0, [60, 60, 80, 255]);
    quad.push_rect(x, y + h, w, 1.0, [60, 60, 80, 255]);

    let title = "击打误差";
    text.queue_text(title, x + w / 2.0 - title.len() as f32 * 5.0, y - 8.0, 10.0, [200, 200, 200, 255]);

    if offsets.is_empty() {
        let msg = "无偏移数据";
        text.queue_text(msg, x + w / 2.0 - msg.len() as f32 * 5.5, y + h / 2.0, 11.0, [120, 120, 120, 255]);
        return;
    }

    let od = result.od;
    let rate = result.song_rate;
    let windows = JudgmentWindows::new(od, rate);

    let bin_w_ms: f64 = 2.5;   // halved step size
    let range_ms: f64 = 150.0;
    let num_bins = ((range_ms * 2.0) / bin_w_ms) as usize;
    let mut bins = vec![0u32; num_bins];

    for o in offsets {
        if matches!(o.judgment, JudgmentResult::Miss) { continue; }  // 排除 MISS
        let clamped = o.offset_ms.clamp(-range_ms, range_ms - bin_w_ms);
        let bi = ((clamped + range_ms) / bin_w_ms) as usize;
        if bi < num_bins { bins[bi] += 1; }
    }

    let max_cnt = *bins.iter().max().unwrap_or(&1).max(&1);
    let bar_w = (w / num_bins as f32).max(0.8);  // thinner bars
    let stats_h = 30.0;
    let draw_h = h - stats_h;

    let judge_colors: [(f64, [u8; 4]); 6] = [
        (windows.perfect, theme::RANK_SS),
        (windows.great,   theme::RANK_S),
        (windows.good,    theme::RANK_A),
        (windows.ok,      theme::RANK_B),
        (windows.meh,     theme::RANK_C),
        (f64::MAX,        theme::RANK_D),
    ];

    for bi in 0..num_bins {
        let cnt = bins[bi];
        if cnt == 0 { continue; }
        let bar_h = cnt as f32 / max_cnt as f32 * draw_h;
        let bx = x + bi as f32 * bar_w;
        let center_ms = -range_ms + (bi as f64 + 0.5) * bin_w_ms;
        let abs_ms = center_ms.abs();
        let mut color = theme::RANK_D;
        for &(threshold, c) in &judge_colors {
            if abs_ms <= threshold { color = c; break; }
        }
        quad.push_rect(bx, y + draw_h - bar_h, bar_w, bar_h, color);
    }

    let zero_x = x + w / 2.0;
    quad.push_rect(zero_x, y, 1.5, draw_h, [255, 60, 60, 200]);
    quad.push_rect(x, y + draw_h / 2.0, w, 1.0, [50, 50, 65, 255]);

    text.queue_text("-150ms", x, y + draw_h + 2.0, 8.0, [120, 120, 120, 255]);
    text.queue_text("0ms", zero_x - 10.0, y + draw_h + 2.0, 8.0, [120, 120, 120, 255]);
    text.queue_text("+150ms", x + w - 28.0, y + draw_h + 2.0, 8.0, [120, 120, 120, 255]);

    let n = offsets.len() as f64;
    let mean = offsets.iter().map(|o| o.offset_ms).sum::<f64>() / n;
    let variance = offsets.iter().map(|o| (o.offset_ms - mean).powi(2)).sum::<f64>() / n;
    let sigma = variance.sqrt();
    let ur = 10.0 * sigma;
    let stats = format!("UR:{:.1}  σ:{:.1}ms  μ:{:+.1}ms  n:{}", ur, sigma, mean, offsets.len());
    text.queue_text(&stats, x + 2.0, y + h - 2.0, 9.0, [180, 180, 180, 255]);
}

// ─── 主入口 ───

pub fn render_results(
    result: &GameResult,
    offsets: &[HitOffset],
    page: u32,
    chart_view_start: f64,
    chart_view_end: f64,
    chart_n_sec: f64,
    quad: &mut QuadRenderer,
    text: &mut TextRenderer,
    cover_region: Option<&crate::skin::AtlasRegion>,
) {
    if let Some(cr) = cover_region {
        quad.push_textured_rect(0.0, 0.0, screen_w(), SCREEN_H,
            cr.uv_x, cr.uv_y, cr.uv_w, cr.uv_h, [255, 255, 255, 255]);
    } else {
        quad.push_rect(0.0, 0.0, screen_w(), SCREEN_H, [18, 18, 28, 255]);
    }
    quad.push_rect(0.0, 0.0, screen_w(), SCREEN_H, [18, 18, 28, 160]);

    if page == 0 {
        draw_page1(quad, text, result, offsets);
    } else {
        draw_page2(quad, text, result, offsets, chart_view_start, chart_view_end, chart_n_sec);
    }
}
