use crate::render::quad::QuadRenderer;
use crate::render::text::TextRenderer;
use crate::skin::AtlasRegion;
use crate::game::notes::{SCREEN_H, screen_w};
use super::SongEntry;
// use std::collections::HashMap;

pub fn render_preview(
    quad: &mut QuadRenderer, text: &mut TextRenderer,
    song: &SongEntry, diff_idx: usize,
    song_name: &str, star_rating: f64, duration_str: &str, total_notes: usize,
    song_rate: f64, cover_region: Option<&AtlasRegion>,
) {
    quad.push_rect(0.0, 0.0, screen_w(), SCREEN_H, [40, 40, 60, 255]);

    text.queue_text("=== 谱面预览 ===", screen_w()/2.0 - 55.0, 30.0, 20.0, [255, 255, 255, 255]);

    let mut y = 80.0;

    // 封面
    if let Some(r) = cover_region {
        let _asp = r.height as f32 / r.width as f32;
        let max_w = 280.0; let max_h = 160.0;
        let (cw, ch) = if r.width as f32 > max_w || r.height as f32 > max_h {
            let s = (max_w / r.width as f32).min(max_h / r.height as f32);
            (r.width as f32 * s, r.height as f32 * s)
        } else { (r.width as f32, r.height as f32) };
        let cx = screen_w()/2.0 - cw/2.0;
        quad.push_textured_rect(cx, y, cw, ch, r.uv_x, r.uv_y, r.uv_w, r.uv_h, [255, 255, 255, 255]);
        quad.push_rect(cx, y, cw, ch, [200, 200, 200, 2]); // border
        y += ch + 15.0;
    } else {
        y += 40.0;
    }

    let diff_name = std::path::Path::new(&song.jsons[diff_idx])
        .file_stem().map(|s| s.to_string_lossy().to_string()).unwrap_or_default();

    let lines: Vec<(&str, String, [u8; 4])> = vec![
        ("", format!("歌曲: {}", song_name), [200, 200, 200, 255]),
        ("", format!("星级: {:.2}★", star_rating), [255, 180, 50, 255]),
        ("", format!("难度: < {}/{}  {} >", diff_idx+1, song.jsons.len(), diff_name), [200, 255, 255, 255]),
        ("", format!("时长: {}", duration_str), [200, 200, 200, 255]),
        ("", format!("倍速: {:.1}x", song_rate), [255, 200, 200, 255]),
        ("", format!("按键总数: {}", total_notes), [200, 200, 200, 255]),
    ];

    for (_, main_str, color) in &lines {
        text.queue_text(main_str, 40.0, y, 15.0, *color);
        y += 24.0;
    }

    y += 10.0;
    text.queue_text("[ENTER] 开始  [← →] 切换难度  [Y] 历史  [ESC] 返回", screen_w()/2.0 - 220.0, y, 14.0, [255, 255, 0, 255]);
}
