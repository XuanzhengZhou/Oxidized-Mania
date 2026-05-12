use crate::render::quad::QuadRenderer;
use crate::render::text::TextRenderer;
use super::notes::{SCREEN_H, screen_w};
use super::NoteRT;

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum PauseAction {
    Continue,
    Restart,
    Exit,
}

const LANES: [f32; 4] = [50.0, 150.0, 250.0, 350.0];

pub fn pause_menu(
    _notes: &[NoteRT],
    _active_idx: usize,
    current_time: f64,
    _eff_speed: f64,
    total_duration: f64,
    show_fps: bool,
    fps: f64,
    quad: &mut QuadRenderer,
    text: &mut TextRenderer,
) -> PauseAction {
    // 渲染暂停时的静态画面（对标 Python simple_ver 暂停画面）
    quad.push_rect(0.0, 0.0, screen_w(), SCREEN_H, [30, 30, 30, 255]); // 背景

    // 轨道线
    for lx in &LANES {
        quad.push_rect(*lx - 1.0, 0.0, 2.0, SCREEN_H, [100, 100, 100, 255]);
    }
    // 判定线
    quad.push_rect(0.0, 500.0 - 2.5, screen_w(), 5.0, [255, 0, 0, 255]);

    // 渲染可见音符
    // paused — no miss detection needed; rendered in engine inline

    // 顶部黑条 + 进度条
    quad.push_rect(0.0, 0.0, screen_w(), 40.0, [0, 0, 0, 255]);
    quad.push_rect(0.0, 40.0, screen_w(), 2.0, [200, 200, 200, 255]);
    let progress = (current_time / total_duration).clamp(0.0, 1.0) as f32;
    quad.push_rect(0.0, 0.0, screen_w(), 4.0, [50, 50, 50, 255]);
    quad.push_rect(0.0, 0.0, screen_w() * progress, 4.0, [100, 200, 255, 255]);

    // 暂停标题
    text.queue_text(
        "=== PAUSED ===",
        screen_w() / 2.0 - 70.0,
        100.0,
        20.0,
        [255, 255, 255, 255],
    );

    // FPS
    if show_fps {
        text.queue_text(
            &format!("FPS: {:.0}", fps),
            10.0,
            SCREEN_H - 30.0,
            16.0,
            [255, 100, 100, 255],
        );
    }

    PauseAction::Continue // 实际选择由 engine 的事件循环处理
}

pub fn pause_options_text(text: &mut TextRenderer, selected: usize) {
    let options = ["Continue", "Restart", "Exit"];
    for (i, opt) in options.iter().enumerate() {
        let prefix = if i == selected { ">> " } else { "   " };
        let color = if i == selected {
            [0, 255, 100, 255]
        } else {
            [150, 150, 150, 255]
        };
        text.queue_text(
            &format!("{}{}", prefix, opt),
            screen_w() / 2.0 - 60.0,
            200.0 + i as f32 * 50.0,
            18.0,
            color,
        );
    }
}
