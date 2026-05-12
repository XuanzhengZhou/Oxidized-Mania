use crate::render::quad::QuadRenderer;
use crate::render::text::TextRenderer;
use super::notes::screen_w;
use super::scoring::Score;

pub fn draw_hud(
    quad: &mut QuadRenderer,
    text: &mut TextRenderer,
    score: &Score,
    combo: u32,
    current_time: f64,
    total_duration: f64,
    fps: f64,
    show_fps: bool,
    _song_rate: f64,
) {
    // 顶部黑条
    quad.push_rect(0.0, 0.0, screen_w(), 40.0, [0, 0, 0, 255]);
    // 分割线
    quad.push_rect(0.0, 40.0, screen_w(), 2.0, [200, 200, 200, 255]);

    // Combo — 左上角
    text.queue_text(
        &format!("Combo: {}", combo),
        10.0,
        10.0,
        18.0,
        [255, 255, 0, 255],
    );

    // ACC — 右上角
    text.queue_text(
        &format!("ACC: {:.2}%", score.accuracy()),
        screen_w() - 160.0,
        10.0,
        18.0,
        [0, 255, 255, 255],
    );

    // 进度条
    let progress = (current_time / total_duration).clamp(0.0, 1.0) as f32;
    quad.push_rect(0.0, 0.0, screen_w(), 4.0, [50, 50, 50, 255]);
    quad.push_rect(0.0, 0.0, screen_w() * progress, 4.0, [100, 200, 255, 255]);

    // 倒计时
    if current_time < 0.0 {
        let cnum = ((current_time.abs() / 1000.0).ceil()) as i32;
        if cnum > 0 {
            text.queue_text(
                &format!("{}", cnum),
                screen_w() / 2.0 - 15.0,
                300.0,
                36.0,
                [255, 255, 255, 255],
            );
        }
    }

    // 判定文字已由 draw_hit_burst 纹理替代

    // FPS
    if show_fps {
        text.queue_text(
            &format!("FPS: {:.0}", fps),
            10.0,
            super::notes::SCREEN_H - 25.0,
            12.0,
            [255, 255, 255, 255],
        );
    }
}
