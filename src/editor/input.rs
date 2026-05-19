use winit::keyboard::KeyCode;
use super::EditorState;

pub fn handle_editor_key(
    state: &mut EditorState, key: KeyCode, shift: bool, ctrl: bool,
) -> Option<EditorAction> {
    let mut refresh = false;
    match key {
        // ── 音符: Shift=开始hold, 普通=结束hold或toggle tap ──
        KeyCode::KeyZ if ctrl => state.undo(),
        KeyCode::KeyZ => if shift { state.start_hold(state.cursor_ms, 0); }
                         else { state.note_action(state.cursor_ms, 0); },
        KeyCode::KeyX => if shift { state.start_hold(state.cursor_ms, 1); }
                         else { state.note_action(state.cursor_ms, 1); },
        KeyCode::KeyC => if shift { state.start_hold(state.cursor_ms, 2); }
                         else { state.note_action(state.cursor_ms, 2); },
        KeyCode::KeyV => if shift { state.start_hold(state.cursor_ms, 3); }
                         else { state.note_action(state.cursor_ms, 3); },

        // ── 时间导航 ──
        KeyCode::KeyI => state.move_cursor(1),
        KeyCode::KeyK => state.move_cursor(-1),

        // ── 进度 ← → ──
        KeyCode::ArrowLeft => {
            if state.playing { state.cursor_ms = (state.cursor_ms - 5000.0).max(0.0); state.audio.set_position_ms(state.cursor_ms); }
            else { state.move_cursor(-4); }
        }
        KeyCode::ArrowRight => {
            if state.playing { state.cursor_ms += 5000.0; state.audio.set_position_ms(state.cursor_ms); }
            else { state.move_cursor(4); }
        }

        // ── Snap ──
        KeyCode::KeyQ => state.snap_divisor = (state.snap_divisor / 2).max(1),
        KeyCode::KeyW => state.snap_divisor = (state.snap_divisor * 2).min(32),

        // ── 播放 ──
        KeyCode::Space => {
            if state.audio_path.is_empty() { return None; }
            state.playing = !state.playing;
            if state.playing { state.audio.set_position_ms(state.cursor_ms); state.audio.play(); }
            else { state.audio.pause(); }
        }

        // ── 调速 ──
        KeyCode::KeyE => { state.song_rate = (state.song_rate - 0.1).max(0.5); state.audio.set_rate(state.song_rate as f32, &state.audio_path); }
        KeyCode::KeyR => { state.song_rate = (state.song_rate + 0.1).min(2.0); state.audio.set_rate(state.song_rate as f32, &state.audio_path); }

        // ── BPM ──
        KeyCode::BracketLeft => { let s = if shift { 0.1 } else { 1.0 }; state.bpm = (state.bpm - s).max(1.0); }
        KeyCode::BracketRight => { let s = if shift { 0.1 } else { 1.0 }; state.bpm = (state.bpm + s).min(999.0); }

        // ── 频谱参数快捷键 (数字 1-9 保留) ──
        KeyCode::Digit1 => { state.spect_config.n_mels = (state.spect_config.n_mels as i32 - 64).max(64) as usize; refresh = true; }
        KeyCode::Digit2 => { state.spect_config.n_mels = (state.spect_config.n_mels + 64).min(1024); refresh = true; }
        KeyCode::Digit3 => { state.spect_config.hop_length = (state.spect_config.hop_length as i32 - 64).max(64) as usize; refresh = true; }
        KeyCode::Digit4 => { state.spect_config.hop_length = (state.spect_config.hop_length + 64).min(1024); refresh = true; }
        KeyCode::Digit5 => { cycle_colormap(&mut state.spect_config.colormap, -1); refresh = true; }
        KeyCode::Digit6 => { cycle_colormap(&mut state.spect_config.colormap, 1); refresh = true; }
        KeyCode::Digit7 => { state.spect_config.freq_max = (state.spect_config.freq_max - 1000.0).max(1000.0); refresh = true; }
        KeyCode::Digit8 => { state.spect_config.freq_max = (state.spect_config.freq_max + 1000.0).min(20000.0); refresh = true; }
        KeyCode::Digit9 => { state.spect_config.freq_min = (state.spect_config.freq_min - 100.0).max(10.0); refresh = true; }
        KeyCode::Digit0 => { state.spect_config.freq_min = (state.spect_config.freq_min + 100.0).min(5000.0); refresh = true; }
        KeyCode::Minus => { state.spect_config.noise_gate = (state.spect_config.noise_gate - 0.05).max(0.0); refresh = true; }
        KeyCode::Equal => { state.spect_config.noise_gate = (state.spect_config.noise_gate + 0.05).min(0.5); refresh = true; }

        // ── BPM / 对音 ──
        KeyCode::KeyB => {
            if !state.audio_path.is_empty() {
                if let Ok(r) = super::analysis::detect_bpm(&state.audio_path) {
                    state.bpm = r.bpm;
                    state.bpm_result = Some(r);
                }
            }
        }
        KeyCode::KeyG => {
            if let Some(ref r) = state.bpm_result {
                if let Some(&first_beat) = r.beat_times.first() {
                    let beat_int = 60_000.0 / r.bpm;
                    let residual = first_beat % beat_int;
                    let offset = if residual > beat_int / 2.0 { residual - beat_int } else { residual };
                    state.offset_ms = -offset;
                }
            }
        }

        // ── 保存 (Ctrl+S) / 设置 (S) / 退出 ──
        KeyCode::KeyS if ctrl => {
            let dir = std::path::Path::new("songs").join(&state.song_folder);
            let _ = std::fs::create_dir_all(&dir);
            let jp = dir.join(format!("{}.json", state.song_folder));
            if state.save(&jp.to_string_lossy()).is_ok() { state.dirty = false; }
        }
        KeyCode::KeyS => return Some(EditorAction::OpenSettings),
        KeyCode::Delete => state.delete_at_cursor(),
        KeyCode::Escape => {
            state.audio.stop(); state.playing = false;
            return Some(if state.dirty { EditorAction::ExitDirty } else { EditorAction::Exit });
        }
        _ => {}
    }
    if refresh {
        let _ = state.spect_config.save();
        state.re_render_png();
        return Some(EditorAction::RefreshTexture);
    }
    None
}

/// 鼠标滚轮
pub fn handle_editor_wheel(state: &mut EditorState, delta: f32, shift: bool) {
    let snaps = if shift { (delta * 4.0) as i32 } else { delta as i32 };
    state.move_cursor(snaps);
}

fn cycle_colormap(cm: &mut String, dir: i32) {
    const MAPS: &[&str] = &["viridis", "magma", "inferno", "plasma", "gray"];
    let pos = MAPS.iter().position(|&m| m == cm.as_str()).unwrap_or(0);
    let idx = ((pos as i32 + dir + MAPS.len() as i32) % MAPS.len() as i32) as usize;
    *cm = MAPS[idx].to_string();
}

pub enum EditorAction { Exit, ExitDirty, RefreshTexture, OpenSettings }
