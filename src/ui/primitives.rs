// UI 形状近似函数库 — 全部基于 QuadRenderer 的轴对齐矩形

use crate::render::quad::QuadRenderer;

/// 实心圆 (水平矩形堆叠，条带自动重叠确保无间隙)
pub fn draw_circle(quad: &mut QuadRenderer, cx: f32, cy: f32, r: f32, color: [u8; 4], strips: u32) {
    if strips < 2 { return; }
    let strip_h = 2.0 * r / strips as f32 * 1.2;
    for i in 0..strips {
        let t = i as f32 / (strips as f32 - 1.0);
        let dy = (t - 0.5) * 2.0 * r;
        let half_w = (r * r - dy * dy).sqrt().max(0.0);
        if half_w > 0.0 {
            quad.push_rect(cx - half_w, cy + dy - strip_h / 2.0, half_w * 2.0, strip_h, color);
        }
    }
}

/// 渐变圆: 中心色 → 边缘色 (同心环)
pub fn draw_circle_gradient(quad: &mut QuadRenderer, cx: f32, cy: f32, r: f32, inner: [u8; 4], outer: [u8; 4], strips: u32) {
    for i in 0..strips {
        let t = i as f32 / (strips as f32 - 1.0);
        let cur_r = r * (1.0 - t);
        let color = lerp_color(inner, outer, t);
        draw_circle(quad, cx, cy, cur_r, color, (strips / 3).max(10));
    }
}

/// 带光晕的圆
pub fn draw_circle_glow(quad: &mut QuadRenderer, cx: f32, cy: f32, r: f32, inner: [u8; 4], glow_color: [u8; 4], strips: u32) {
    // 光晕外环
    let glow_r = r * 1.2;
    let glow_strips = strips / 2;
    for i in 0..glow_strips {
        let t = i as f32 / glow_strips as f32;
        let cur_r = glow_r * (1.0 - t * 0.4);
        let alpha = ((1.0 - t) * 0.3).clamp(0.0, 1.0);
        let c = [glow_color[0], glow_color[1], glow_color[2], (alpha * 255.0) as u8];
        draw_circle(quad, cx, cy, cur_r, c, 20);
    }
    // 主圆
    draw_circle_gradient(quad, cx, cy, r, inner, [inner[0], inner[1], inner[2], 200], strips);
}

/// 梯形 (水平矩形堆叠): 上边宽度→下边宽度, 左上角 anchor
pub fn draw_trapezoid(quad: &mut QuadRenderer, x: f32, y: f32, top_w: f32, bot_w: f32, h: f32, color: [u8; 4], strips: u32) {
    for i in 0..strips {
        let t = i as f32 / strips as f32;
        let w = top_w + (bot_w - top_w) * t;
        let strip_h = h / strips as f32;
        quad.push_rect(x, y + t * h, w, strip_h + 0.5, color);
    }
}

/// 梯形 + 右下圆角 (用裁剪矩形近似圆角)
pub fn draw_trapezoid_rounded_br(quad: &mut QuadRenderer, x: f32, y: f32, top_w: f32, bot_w: f32, h: f32, color: [u8; 4], corner_r: f32, strips: u32) {
    draw_trapezoid(quad, x, y, top_w, bot_w, h, color, strips);
    // 右下圆角: 在右下角画一个 1/4 圆遮盖
    let dy_off = h - corner_r;
    for i in 0..20 {
        let t = i as f32 / 19.0;
        let cy_off = corner_r * (1.0 - t);
        let half_w = (corner_r * corner_r - cy_off * cy_off).sqrt();
        let strip_w = bot_w - corner_r + half_w;
        let sy = y + dy_off + cy_off;
        let bg = [0, 0, 0, 0]; // 用 background 色遮盖 → 实际用 context 背景色
        quad.push_rect(x + strip_w, sy, corner_r + 2.0, 1.5, bg);
    }
}

/// 胶囊型 (中间矩形 + 两端半圆)
pub fn draw_capsule(quad: &mut QuadRenderer, x: f32, y: f32, w: f32, h: f32, color: [u8; 4]) {
    let r = h / 2.0;
    // 中段矩形
    quad.push_rect(x + r, y, w - 2.0 * r, h, color);
    // 左端半圆
    let h_strips: u32 = 20;
    for i in 0..h_strips {
        let dy = (i as f32 - (h_strips as f32 - 1.0) / 2.0) / (h_strips as f32 - 1.0) * h;
        let half_w = (r * r - dy * dy).sqrt().max(0.0);
        if half_w > 0.0 {
            quad.push_rect(x + r - half_w, y + h/2.0 + dy - 0.5, half_w * 2.0, 1.2, color);
        }
    }
    // 右端半圆
    for i in 0..h_strips {
        let dy = (i as f32 - (h_strips as f32 - 1.0) / 2.0) / (h_strips as f32 - 1.0) * h;
        let half_w = (r * r - dy * dy).sqrt().max(0.0);
        if half_w > 0.0 {
            quad.push_rect(x + w - r - half_w, y + h/2.0 + dy - 0.5, half_w * 2.0, 1.2, color);
        }
    }
}

/// 进度条: 填充区 + 背景
pub fn draw_bar(quad: &mut QuadRenderer, x: f32, y: f32, w: f32, h: f32, fill_pct: f32, fill_color: [u8; 4], bg_color: [u8; 4]) {
    quad.push_rect(x, y, w, h, bg_color);
    if fill_pct > 0.0 {
        quad.push_rect(x, y, w * fill_pct.clamp(0.0, 1.0), h, fill_color);
    }
}

/// 矩形裁剪区 (用背景色覆盖实现圆角遮罩)
pub fn draw_rounded_rect(quad: &mut QuadRenderer, x: f32, y: f32, w: f32, h: f32, r: f32, color: [u8; 4], bg: [u8; 4]) {
    // 主体
    quad.push_rect(x, y, w, h, color);
    // 四角遮盖 (用背景色)
    let corner_strips = (r as u32).min(20);
    for i in 0..corner_strips {
        let t = i as f32 / corner_strips as f32;
        let cx_off = r * (1.0 - (1.0 - t * t).sqrt()); // quarter-circle profile
        // Top-left
        let strip_w = r - cx_off;
        quad.push_rect(x, y + t * r / corner_strips as f32, strip_w.max(0.0), r / corner_strips as f32 + 0.5, bg);
        // Top-right
        quad.push_rect(x + w - strip_w.max(0.0), y + t * r / corner_strips as f32, strip_w.max(0.0), r / corner_strips as f32 + 0.5, bg);
        // Bottom-left
        quad.push_rect(x, y + h - r + t * r / corner_strips as f32, strip_w.max(0.0), r / corner_strips as f32 + 0.5, bg);
        // Bottom-right
        quad.push_rect(x + w - strip_w.max(0.0), y + h - r + t * r / corner_strips as f32, strip_w.max(0.0), r / corner_strips as f32 + 0.5, bg);
    }
}

// ─── 辅助 ───

fn lerp_color(a: [u8; 4], b: [u8; 4], t: f32) -> [u8; 4] {
    [
        (a[0] as f32 + (b[0] as f32 - a[0] as f32) * t) as u8,
        (a[1] as f32 + (b[1] as f32 - a[1] as f32) * t) as u8,
        (a[2] as f32 + (b[2] as f32 - a[2] as f32) * t) as u8,
        (a[3] as f32 + (b[3] as f32 - a[3] as f32) * t) as u8,
    ]
}
