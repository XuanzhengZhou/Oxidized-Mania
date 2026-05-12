use crate::render::quad::QuadRenderer;
use crate::render::text::TextRenderer;
use crate::game::notes::{SCREEN_H, screen_w};

pub fn render_exit_confirm(quad: &mut QuadRenderer, text: &mut TextRenderer) {
    // 半透明遮罩（保留当前画面）
    quad.push_rect(0.0, 0.0, screen_w(), SCREEN_H, [0, 0, 0, 180]);
    // 对话框
    let bx = screen_w()/2.0 - 150.0;
    let by = SCREEN_H/2.0 - 70.0;
    quad.push_rect(bx, by, 300.0, 140.0, [50, 50, 70, 255]);
    quad.push_rect(bx, by, 300.0, 140.0, [255, 255, 255, 2]); // border via rect

    text.queue_text("退出游戏?", screen_w()/2.0 - 40.0, by + 20.0, 22.0, [255, 255, 255, 255]);
    text.queue_text("[ENTER] 确认  [ESC] 返回", screen_w()/2.0 - 85.0, by + 70.0, 16.0, [200, 200, 200, 255]);
}
