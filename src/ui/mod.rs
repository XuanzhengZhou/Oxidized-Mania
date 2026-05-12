pub mod primitives;
pub mod theme;

use crate::game::notes::{SCREEN_H, screen_w};
use crate::render::quad::QuadRenderer;
use crate::render::text::TextRenderer;
use crate::skin::AtlasRegion;
use theme::{BG_DARK, PINK_LOGO, WHITE};

/// 模糊曲绘背景 + 暗色遮罩
pub fn draw_menu_background(quad: &mut QuadRenderer, cover_region: Option<&AtlasRegion>) {
    let w = screen_w();
    quad.push_rect(0.0, 0.0, w, SCREEN_H, BG_DARK);

    if let Some(region) = cover_region {
        let cover_w = w;
        let cover_h = cover_w * region.height as f32 / region.width as f32;
        let cover_y = SCREEN_H / 2.0 - cover_h / 2.0;
        quad.push_textured_rect(0.0, cover_y, cover_w, cover_h,
            region.uv_x, region.uv_y, region.uv_w, region.uv_h,
            [160, 160, 170, 200]);
    }

    quad.push_rect(0.0, 0.0, w, SCREEN_H, [0, 0, 0, 40]);
}

/// osu! 圆圈：白色粗边 + 实心粉色填充 + 文字
pub fn draw_osu_circle(quad: &mut QuadRenderer, text: &mut TextRenderer, cx: f32, cy: f32, r: f32, label: Option<&str>, font_size: f32, logo: Option<&AtlasRegion>) {
    if let Some(lg) = logo {
        let aspect = lg.width as f32 / lg.height as f32;
        let h = r * 2.0;
        let w = h * aspect;
        quad.push_textured_rect(cx - w / 2.0, cy - h / 2.0, w, h,
            lg.uv_x, lg.uv_y, lg.uv_w, lg.uv_h, [255, 255, 255, 255]);
    } else {
        let edge = r * 0.1;
        primitives::draw_circle(quad, cx, cy, r + edge, WHITE, (r * 1.5) as u32);
        primitives::draw_circle(quad, cx, cy, r, PINK_LOGO, (r * 1.5) as u32);
        if let Some(s) = label {
            let tw = s.len() as f32 * font_size * 0.45;
            text.queue_text(s, cx - tw / 2.0, cy - font_size * 0.35, font_size, WHITE);
        }
    }
}

/// 5 个不等宽选项卡：Settings=circle_left, 右侧3个=(w-circle_right)/4, Play填中间
pub fn draw_menu_tabs(
    quad: &mut QuadRenderer,
    text: &mut TextRenderer,
    tabs: &[(&str, [u8; 4])],
    selected: usize,
    hovered: Option<usize>,
    circle_cx: f32,
    circle_r: f32,
) {
    let w = screen_w();
    let tab_h = SCREEN_H / 5.0;
    let tab_y = SCREEN_H * 2.0 / 5.0;
    let circle_left = circle_cx - circle_r;
    let circle_right = circle_cx + circle_r;

    // 计算宽度：Settings=circle_cx (圆圈居中覆盖交界)，右侧3个都=(w-circle_right)/4, Play=剩余
    let settings_w = circle_cx;
    let right_each = (w - circle_right) / 4.0;
    let play_w = w - settings_w - right_each * 3.0;
    let widths: [f32; 5] = [settings_w, play_w, right_each, right_each, right_each];

    let mut x = 0.0f32;
    for (i, &(label, base_color)) in tabs.iter().enumerate() {
        let tw = widths[i];
        let color = if Some(i) == hovered {
            let f = 1.3;
            [(base_color[0] as f32 * f).min(255.0) as u8,
             (base_color[1] as f32 * f).min(255.0) as u8,
             (base_color[2] as f32 * f).min(255.0) as u8, 255]
        } else if i == selected {
            base_color
        } else {
            [base_color[0], base_color[1], base_color[2], 220]
        };
        quad.push_rect(x, tab_y, tw + 1.0, tab_h, color);

        if !label.is_empty() {
            let label_w = label.len() as f32 * 11.0;
            // Play/Solo tab 文字放到圆圈右边；Settings 文字靠左
            let text_x = if i == 1 {
                // Play/Solo: 文字放在圆圈右侧可见区域，留足够间距
                (circle_right + 22.0).max(x + 8.0)
            } else if x + tw < circle_left + 10.0 {
                x + tw - label_w - 8.0  // 左边 tab 文字靠右
            } else {
                x + 8.0  // 右边 tabs 文字靠左
            };
            text.queue_text(label, text_x, tab_y + tab_h / 2.0 - 8.0, 16.0, WHITE);
        }
        x += tw;
    }
}

/// 判断鼠标悬停在哪个 tab 上（考虑不等宽）
pub fn hovered_tab(mouse_x: f32, _num_tabs: usize, circle_cx: f32, circle_r: f32) -> Option<usize> {
    let w = screen_w();
    let circle_right = circle_cx + circle_r;
    let settings_w = circle_cx;
    let right_each = (w - circle_right) / 4.0;
    let play_w = w - settings_w - right_each * 3.0;
    let widths: [f32; 5] = [settings_w, play_w, right_each, right_each, right_each];

    let mut acc = 0.0f32;
    for i in 0..5 {
        acc += widths[i];
        if mouse_x < acc { return Some(i); }
    }
    None
}
