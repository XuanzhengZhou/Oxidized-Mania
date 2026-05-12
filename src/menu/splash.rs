use crate::render::quad::QuadRenderer;
use crate::render::text::TextRenderer;
use crate::game::notes::{SCREEN_H, screen_w};
use crate::skin::AtlasRegion;
use crate::ui::{draw_menu_background, draw_osu_circle};
// use crate::ui::theme::WHITE;
use std::time::Instant;

pub fn render(
    quad: &mut QuadRenderer,
    text: &mut TextRenderer,
    cover_region: Option<&AtlasRegion>,
    logo: Option<&AtlasRegion>,
) {
    let w = screen_w();
    draw_menu_background(quad, cover_region);

    let cx = w / 2.0;
    let cy = SCREEN_H / 2.0;
    let r = SCREEN_H * 0.45;
    draw_osu_circle(quad, text, cx, cy, r, Some("Oxidized Mania"), 192.0, logo);

    let now = Instant::now();
    let alpha = ((now.elapsed().as_millis() % 1400) as f32 / 700.0 - 1.0).abs().clamp(0.25, 1.0);
    let hint_alpha = (alpha * 255.0) as u8;
    text.queue_text("Click or press any key to start", cx - 110.0, cy + r + 50.0, 15.0, [255, 255, 80, hint_alpha]);

    text.queue_text("[ESC] to exit", 20.0, SCREEN_H - 18.0, 10.0, [120, 120, 120, 255]);
    text.queue_text("v0.1.0", w - 70.0, SCREEN_H - 18.0, 10.0, [120, 120, 120, 255]);
}
