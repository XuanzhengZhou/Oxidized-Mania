use crate::beatmap::NoteType;
use crate::game::notes::{calc_lanes, SCREEN_H};
use crate::render::quad::QuadRenderer;
use crate::render::text::TextRenderer;
use super::EditorState;

fn note_y(time_ms: f64, cursor_ms: f64, hit_y: f32, eff_speed: f64) -> f32 {
    hit_y - (time_ms - cursor_ms) as f32 * eff_speed as f32
}

pub fn render_editor(
    state: &EditorState, screen_w: f32,
    quad: &mut QuadRenderer, text: &mut TextRenderer,
) {
    let hit_y = state.config.hit_position as f32;
    let eff_speed = state.config.scroll_speed / 24.0 / state.song_rate;
    let lanes = calc_lanes(screen_w, state.config.stage_spacing, state.config.stage_scale);
    let note_w = (80.0 * state.config.stage_scale) as f32;
    let left = lanes[0] - note_w - 20.0;
    let right = lanes[3] + note_w + 20.0;

    // ── 背景 ──
    quad.push_rect(0.0, 0.0, screen_w, SCREEN_H, [15, 15, 25, 255]);

    // ── 频谱贴图 (R8Unorm 纹理 + colormap shader, tex_index=2) ──
    if state.show_spectrogram && state.spectrogram_time_last > state.spectrogram_time_first {
        let first_ms = state.spectrogram_time_first;
        let last_ms = state.spectrogram_time_last;
        let span = last_ms - first_ms;
        let time_top = state.cursor_ms + hit_y as f64 / eff_speed;
        let time_bot = state.cursor_ms - (SCREEN_H - hit_y) as f64 / eff_speed;
        // 应用 offset: game_time = audio_time + offset → audio_time = game_time - offset
        let audio_top = time_top - state.offset_ms;
        let audio_bot = time_bot - state.offset_ms;
        let v_top = ((audio_top - first_ms).max(0.0) / span).clamp(0.0, 1.0) as f32;
        let v_bot = ((audio_bot - first_ms).max(0.0) / span).clamp(0.0, 1.0) as f32;
        // uv_h 可为负: 屏幕上方=未来→V=1.0, 下方=过去→V=0.0
        quad.push_textured_rect(
            0.0, 0.0, screen_w, SCREEN_H,
            0.0, v_top, 1.0, v_bot - v_top,
            [255, 255, 255, 160],
        );
        quad.instances.last_mut().unwrap().tex_index = 2;
    }

    // ── Beat 网格线 ──
    let beat_ms = 60_000.0 / state.bpm;
    let mut t = 0.0;
    while t <= state.cursor_ms + 5000.0 {
        if t >= state.cursor_ms - 5000.0 {
            let y = note_y(t, state.cursor_ms, hit_y, eff_speed);
            if y >= 0.0 && y <= SCREEN_H {
                let strong = (t / beat_ms).round() as i64 % 4 == 0;
                let c = if strong { [80, 80, 100, 120] } else { [50, 50, 60, 80] };
                quad.push_rect(left, y, right - left, 1.0, c);
            }
        }
        t += beat_ms / 4.0;
    }

    // ── 节拍线（黄色） ──
    if let Some(ref bpm_r) = state.bpm_result {
        for &bt in &bpm_r.beat_times {
            let y = note_y(bt, state.cursor_ms, hit_y, eff_speed);
            if y >= 0.0 && y <= SCREEN_H {
                quad.push_rect(left, y, right - left, 1.0, [255, 200, 0, 100]);
            }
        }
    }

    // ── Onset 曲线：已由 spectrs Mel 频谱替代 ──

    // ── 判定线 ──
    quad.push_rect(left, hit_y - 1.0, right - left, 3.0, [220, 40, 40, 255]);

    // ── 打开的 hold ──
    let cursor_y = note_y(state.cursor_ms, state.cursor_ms, hit_y, eff_speed);
    for lane in 0..4 {
        if let Some(head) = state.open_holds[lane] {
            let hy = note_y(head, state.cursor_ms, hit_y, eff_speed);
            let nx = lanes[lane] - note_w / 2.0;
            if hy >= 0.0 && cursor_y <= SCREEN_H {
                quad.push_rect(nx, cursor_y, note_w, (hy - cursor_y).max(0.0), [80, 220, 120, 100]);
                quad.push_rect(nx, hy - note_w / 2.0, note_w, note_w, [80, 220, 120, 180]);
            }
        }
    }

    // ── 光标 ──
    quad.push_rect(left, cursor_y - 1.0, right - left, 2.0, [255, 255, 255, 180]);

    // ── 音符 ──
    for &(time_ms, _end_time, lane, note_type) in &state.notes {
        let y = note_y(time_ms, state.cursor_ms, hit_y, eff_speed);
        if y < -50.0 || y > SCREEN_H + 50.0 { continue; }
        let nx = lanes[lane.min(3)] - note_w / 2.0;
        match note_type {
            NoteType::Tap => {
                let th = note_w / 5.0;
                quad.push_rect(nx, y - th / 2.0, note_w, th, [80, 160, 255, 220]);
            }
            NoteType::Hold => {
                let body_end = note_y(time_ms + 200.0, state.cursor_ms, hit_y, eff_speed);
                quad.push_rect(nx, y - note_w / 2.0, note_w, note_w, [80, 220, 120, 220]);
                quad.push_rect(nx + note_w * 0.3, y + note_w / 2.0, note_w * 0.4,
                    (body_end - y - note_w / 2.0).max(0.0), [60, 180, 100, 160]);
            }
        }
    }

    // ── Lane 线 ──
    for &lx in &lanes {
        quad.push_rect(lx - 1.0, 0.0, 2.0, SCREEN_H, [80, 80, 100, 100]);
    }

    // ── 顶部信息栏 ──
    quad.push_rect(0.0, 0.0, screen_w, 24.0, [10, 10, 20, 200]);
    let min = (state.cursor_ms / 60_000.0) as u32;
    let sec = (state.cursor_ms as u32 / 1000) % 60;
    let mut info_buf = String::with_capacity(128);
    use std::fmt::Write;
    if let Some(ref r) = state.bpm_result {
        write!(info_buf, "BPM:{:.1}({:.0}%) ", r.bpm, r.confidence * 100.0).unwrap();
    } else {
        write!(info_buf, "BPM:{:.0} ", state.bpm).unwrap();
    }
    write!(info_buf, "Snap:1/{} {:02}:{:02} N:{} {}",
        state.snap_divisor, min, sec, state.notes.len(),
        if state.playing { "▶" } else { "⏸" }).unwrap();
    text.queue_text(&info_buf, 8.0, 4.0, 12.0, [200, 200, 220, 255]);
    text.queue_text("[Z][X][C][V]Note [I/K]Move [Space]Play [E/R]Spd [Q/W]Snap [P]Spec [S]Save",
        8.0, SCREEN_H - 14.0, 10.0, [140, 140, 160, 255]);

}
