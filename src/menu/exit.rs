use crate::render::quad::QuadRenderer;
use crate::render::text::TextRenderer;
use crate::game::notes::{SCREEN_H, screen_w};

pub fn render(quad: &mut QuadRenderer, text: &mut TextRenderer) {
    let w = screen_w();
    quad.push_rect(0.0, 0.0, w, SCREEN_H, [0, 0, 0, 180]);
    let bx = w/2.0 - 150.0; let by = SCREEN_H/2.0 - 70.0;
    quad.push_rect(bx, by, 300.0, 140.0, [50, 50, 70, 255]);
    text.queue_text("退出游戏?", w/2.0 - 40.0, by + 20.0, 22.0, [255, 255, 255, 255]);
    text.queue_text("[ENTER] 确认  [ESC] 返回", w/2.0 - 85.0, by + 70.0, 16.0, [200, 200, 200, 255]);
}
