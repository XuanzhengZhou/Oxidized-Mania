use crate::render::quad::QuadRenderer;
use crate::render::text::TextRenderer;
use super::notes::screen_w;
use super::scoring::Score;
use std::fmt::Write;

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
    quad.push_rect(0.0, 0.0, screen_w(), 40.0, [0, 0, 0, 255]);
    quad.push_rect(0.0, 40.0, screen_w(), 2.0, [200, 200, 200, 255]);

    let mut buf = String::with_capacity(64);

    write!(buf, "Combo: {}", combo).unwrap();
    text.queue_text(&buf, 10.0, 10.0, 18.0, [255, 255, 0, 255]);
    buf.clear();

    write!(buf, "ACC: {:.2}%", score.accuracy()).unwrap();
    text.queue_text(&buf, screen_w() - 160.0, 10.0, 18.0, [0, 255, 255, 255]);
    buf.clear();

    let progress = (current_time / total_duration).clamp(0.0, 1.0) as f32;
    quad.push_rect(0.0, 0.0, screen_w(), 4.0, [50, 50, 50, 255]);
    quad.push_rect(0.0, 0.0, screen_w() * progress, 4.0, [100, 200, 255, 255]);

    if current_time < 0.0 {
        let cnum = ((current_time.abs() / 1000.0).ceil()) as i32;
        if cnum > 0 {
            write!(buf, "{}", cnum).unwrap();
            text.queue_text(&buf, screen_w() / 2.0 - 15.0, 300.0, 36.0, [255, 255, 255, 255]);
            buf.clear();
        }
    }

    if show_fps {
        write!(buf, "FPS: {:.0}", fps).unwrap();
        text.queue_text(&buf, 10.0, super::notes::SCREEN_H - 25.0, 12.0, [255, 255, 255, 255]);
        buf.clear();
    }
}
