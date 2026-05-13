use std::collections::HashMap;
use std::sync::Arc;
use std::time::Instant;
use winit::event::{ElementState, KeyEvent, WindowEvent};
use winit::keyboard::{KeyCode, PhysicalKey};
use winit::window::Window;

use crate::audio::bass::BassAudio;
use crate::config::GameConfig;
use crate::render::context::RenderCtx;
use crate::skin::AtlasRegion;

use super::hud::draw_hud;
use super::judgment::{judge_hold_release, judge_tap, JudgmentResult, JudgmentWindows};
use super::notes::{
    calc_lanes, draw_hit_burst, draw_key_pads, process_notes,
    stage_bounds, LEAD_IN, SCREEN_H,
};
use super::results::{GameResult, standardized_score, render_results};
use super::scoring::Score;
use super::{NoteRT, NoteType};

// ─── 性能分析器 ───

struct FrameProfiler {
    frame_times: Vec<f64>,
    report_interval: usize,
    frame_count: u64,
    section_times: [f64; 7],  // logic, collect, q_upload, t_upload, begin_frame, gpu_submit, present
    section_start: Instant,
    quad_count: usize,
    glyph_count: usize,
}

impl FrameProfiler {
    fn new() -> Self {
        Self {
            frame_times: Vec::with_capacity(120),
            report_interval: 120,
            frame_count: 0,
            section_times: [0.0; 7],
            section_start: Instant::now(),
            quad_count: 0,
            glyph_count: 0,
        }
    }

    fn begin_frame(&mut self) { self.section_start = Instant::now(); }
    fn end_section(&mut self, idx: usize) {
        let elapsed = self.section_start.elapsed().as_secs_f64() * 1000.0;
        self.section_times[idx] = elapsed;
        self.section_start = Instant::now();
    }

    fn end_frame(&mut self) {
        self.frame_count += 1;
        let total = self.section_times.iter().sum::<f64>();
        self.frame_times.push(total);
        if self.frame_times.len() > self.report_interval { self.frame_times.remove(0); }

        // 性能报告
        const PERF_REPORT: bool = false;
        if PERF_REPORT && self.frame_count % self.report_interval as u64 == 0 && !self.frame_times.is_empty() {
            let avg = self.frame_times.iter().sum::<f64>() / self.frame_times.len() as f64;
            let min = self.frame_times.iter().cloned().fold(f64::INFINITY, f64::min);
            let max = self.frame_times.iter().cloned().fold(0.0, f64::max);
            let fps = 1000.0 / avg;
            println!(
                "[Frame {:>5}] avg={:7.3}ms min={:7.3}ms max={:7.3}ms fps={:>5.0} | logic={:7.3}ms q_up={:7.3}ms t_up={:7.3}ms begin={:7.3}ms submit={:7.3}ms pres={:7.3}ms | quad={:>4} glyph={:>3}",
                self.frame_count, avg, min, max, fps,
                self.section_times[0], self.section_times[2],
                self.section_times[3], self.section_times[4], self.section_times[5],
                self.section_times[6],
                self.quad_count, self.glyph_count,
            );
        }
    }
}

// ─── 游戏状态机 ───

#[derive(Debug)]
enum Phase {
    Countdown, // lead-in 3 秒
    Playing,
    Paused {
        selected: usize,
    },
    PauseCountdown {
        selected: usize,
        count_start: Instant,
        pause_start: Instant,
        current_time_snap: f64,
    },
    Results {
        result: GameResult,
        offsets: Vec<super::results::HitOffset>,
    },
}

pub struct GameEngine {
    recorder: Option<crate::replay::ReplayRecorder>,
    pub replay_data: Option<crate::replay::ReplayData>,
    audio: BassAudio,
    config: GameConfig,
    song_name: String,
    map_path: String,
    notes: Vec<NoteRT>,
    total_duration: f64,
    render: RenderCtx,
    window: Arc<Window>,

    // 时间
    start_time: Instant,
    song_rate: f64,
    map_offset: f64,
    mirror_mode: bool,
    global_offset: f64,
    eff_speed: f64,
    stars: f64,
    cover_path: Option<String>,
    pub exit_result: Option<GameResult>,
    windows: JudgmentWindows,
    music_started: bool,
    skip_used: bool,
    pause_start: Option<Instant>,
    last_pause_ct: Option<f64>,  // 防止暂停恢复后时间无限回退
    total_offset: f64,
    total_hits: u32,
    last_hit_offset: f64,
    hit_timestamps: Vec<f64>,

    // 游戏状态
    phase: Phase,
    active_idx: usize,
    score: Score,
    combo: u32,
    max_combo: u32,
    keys_pressed: [bool; 4],
    last_judgment: Option<(JudgmentResult, Instant)>,
    judgment_type: Option<JudgmentResult>,
    burst_start: Option<std::time::Instant>,
    fps_times: Vec<Instant>,
    lanes: [f32; 4],
    screen_w: f32,
    hit_y: f64,
    note_w: f32,
    skin_regions: Option<HashMap<String, AtlasRegion>>,
    skin_config: Option<HashMap<String, String>>,
    profiler: FrameProfiler,
}

impl GameEngine {
    pub fn new(
        audio: BassAudio,
        config: GameConfig,
        song_name: String,
        map_path: String,
        mut notes: Vec<NoteRT>,
        total_duration: f64,
        map_offset: f64,
        _mirror_mode: bool,
        cover_path: Option<String>,
        skin_regions: Option<HashMap<String, AtlasRegion>>,
        skin_config: Option<HashMap<String, String>>,
        window: Arc<Window>,
        render: RenderCtx,
    ) -> Self {
        let song_rate = config.song_rate;
        let global_offset = config.global_offset;
        let od = config.od;
        let mirror_mode = config.mirror_mode;
        let mut scroll_speed = config.scroll_speed;
        if scroll_speed < 5.0 { scroll_speed *= 30.0; }
        let screen_w = render.logical_w as f32;
        let hit_y = config.hit_position;
        let note_w = (80.0 * config.stage_scale) as f32;
        let lanes = calc_lanes(screen_w, config.stage_spacing, config.stage_scale);
        let total_notes: u32 = notes.iter()
            .map(|n| if n.note_type == NoteType::Hold { 2 } else { 1 })
            .sum();
        if mirror_mode {
            let mirror_map = [3usize, 2, 1, 0];
            for n in &mut notes { n.lane = mirror_map[n.lane]; }
        }
        let rec = crate::replay::ReplayRecorder::new(&map_path, song_rate, od, config.scroll_speed, config.hit_position, mirror_mode, config.global_offset, config.stage_spacing, config.stage_scale, "Player");
        let stars = crate::pp::calculate_stars(&map_path, song_rate);
        Self {
            recorder: Some(rec), replay_data: None,
            audio, song_name, map_path, notes, total_duration, render, window, config,
            start_time: Instant::now(), song_rate, map_offset, global_offset,
            eff_speed: scroll_speed / 24.0 / song_rate,
            windows: JudgmentWindows::new(od, song_rate),
            music_started: false, skip_used: false, pause_start: None, last_pause_ct: None,
            total_offset: 0.0, total_hits: 0, last_hit_offset: 0.0, hit_timestamps: Vec::new(),
            phase: Phase::Countdown, active_idx: 0,
            mirror_mode,
            score: Score { total_notes, ..Default::default() },
            combo: 0, max_combo: 0,
            keys_pressed: [false; 4], last_judgment: None,
            judgment_type: None, burst_start: None,
            fps_times: Vec::new(),
            lanes, screen_w, hit_y, note_w,
            skin_regions: skin_regions.clone(),
            skin_config,
            stars,
            cover_path,
            exit_result: None,
            profiler: FrameProfiler::new(),
        }
    }

    pub fn take_result(&mut self) -> Option<GameResult> {
        self.exit_result.take()
    }

    fn current_time(&self) -> f64 {
        let real_elapsed = self.start_time.elapsed().as_secs_f64() * 1000.0;
        real_elapsed * self.song_rate - self.map_offset - self.global_offset - LEAD_IN
    }

    fn ensure_playing(&mut self) {
        if matches!(self.phase, Phase::Countdown) && self.current_time() >= 0.0 {
            self.phase = Phase::Playing;
        }
    }

    fn calc_fps(&mut self) -> f64 {
        let now = Instant::now();
        self.fps_times.push(now);
        self.fps_times
            .retain(|t| now.duration_since(*t).as_secs_f64() < 1.0);
        self.fps_times.len() as f64
    }

    fn handle_keydown(&mut self, key: KeyCode) {
        let Some((key_name, key_code)) = Self::key_info(key) else { return };
        if let Some(lane) = self.config.key_to_lane(key_name, Some(key_code)) {
            self.keys_pressed[lane] = true;
            let ct = self.current_time();
            if let Some(ref mut r) = self.recorder { r.record_event(ct, lane, true); }
            if matches!(self.phase, Phase::Playing) {
                self.handle_note_hit(lane);
            }
        }
    }

    fn handle_note_hit(&mut self, lane: usize) {
        let ct = self.current_time();

        // 收集该轨道有效音符 (对标 Python valid_notes) — 栈数组，最多3个
        let mut candidates: [Option<(usize, f64)>; 3] = [None; 3];
        let mut cand_count = 0;
        for i in self.active_idx..self.notes.len() {
            let n = &self.notes[i];
            if n.lane == lane && !n.hit && !n.missed && !n.holding && !n.ghost {
                candidates[cand_count] = Some((i, (n.time - ct).abs()));
                cand_count += 1;
                if cand_count >= 3 { break; }
            }
        }

        if cand_count == 0 { return; }
        // 找最小 diff
        let mut best: Option<(usize, f64)> = None;
        for i in 0..cand_count {
            let c = candidates[i].unwrap();
            if best.map_or(true, |b| c.1 < b.1) { best = Some(c); }
        }
        let (idx, diff) = best.unwrap();

        // offset 追踪
        self.last_hit_offset = self.notes[idx].time - ct;
        self.total_offset += self.last_hit_offset;
        self.total_hits += 1;
        self.hit_timestamps.push(ct);

        if diff <= self.windows.miss {
            let result = judge_tap(self.notes[idx].time, ct, &self.windows);
            self.score.add_judgment(result);
            if matches!(result, JudgmentResult::Miss) {
                self.notes[idx].missed = true;
                self.combo = 0;
            } else {
                self.combo = self.score.combo;
                if self.notes[idx].note_type == NoteType::Tap {
                    self.notes[idx].hit = true;
                } else if self.notes[idx].note_type == NoteType::Hold {
                    self.notes[idx].holding = true;
                    self.notes[idx].stuck_y =
                        Some(self.hit_y - (self.notes[idx].time - ct) * self.eff_speed);
                }
            }
            self.last_judgment = Some((result, Instant::now()));
            self.judgment_type = Some(result);
            self.burst_start = Some(std::time::Instant::now());
        }

        self.max_combo = self.max_combo.max(self.combo);
    }

    fn key_info(key: KeyCode) -> Option<(&'static str, u32)> {
        let (name, code): (&str, u32) = match key {
            KeyCode::KeyA => ("a", 97), KeyCode::KeyB => ("b", 98), KeyCode::KeyC => ("c", 99),
            KeyCode::KeyD => ("d", 100), KeyCode::KeyE => ("e", 101), KeyCode::KeyF => ("f", 102),
            KeyCode::KeyG => ("g", 103), KeyCode::KeyH => ("h", 104), KeyCode::KeyI => ("i", 105),
            KeyCode::KeyJ => ("j", 106), KeyCode::KeyK => ("k", 107), KeyCode::KeyL => ("l", 108),
            KeyCode::KeyM => ("m", 109), KeyCode::KeyN => ("n", 110), KeyCode::KeyO => ("o", 111),
            KeyCode::KeyP => ("p", 112), KeyCode::KeyQ => ("q", 113), KeyCode::KeyR => ("r", 114),
            KeyCode::KeyS => ("s", 115), KeyCode::KeyT => ("t", 116), KeyCode::KeyU => ("u", 117),
            KeyCode::KeyV => ("v", 118), KeyCode::KeyW => ("w", 119), KeyCode::KeyX => ("x", 120),
            KeyCode::KeyY => ("y", 121), KeyCode::KeyZ => ("z", 122),
            KeyCode::Digit0 => ("0", 48), KeyCode::Digit1 => ("1", 49), KeyCode::Digit2 => ("2", 50),
            KeyCode::Digit3 => ("3", 51), KeyCode::Digit4 => ("4", 52), KeyCode::Digit5 => ("5", 53),
            KeyCode::Digit6 => ("6", 54), KeyCode::Digit7 => ("7", 55), KeyCode::Digit8 => ("8", 56),
            KeyCode::Digit9 => ("9", 57),
            KeyCode::Numpad0 => ("0", 256), KeyCode::Numpad1 => ("1", 257),
            KeyCode::Numpad2 => ("2", 258), KeyCode::Numpad3 => ("3", 259),
            KeyCode::Numpad4 => ("4", 260), KeyCode::Numpad5 => ("5", 261),
            KeyCode::Numpad6 => ("6", 262), KeyCode::Numpad7 => ("7", 263),
            KeyCode::Numpad8 => ("8", 264), KeyCode::Numpad9 => ("9", 265),
            KeyCode::NumpadAdd => ("[+]", 270), KeyCode::NumpadSubtract => ("[-]", 271),
            KeyCode::NumpadMultiply => ("[*]", 272), KeyCode::NumpadDivide => ("[/]", 273),
            KeyCode::NumpadDecimal => ("[.]", 274),
            _ => return None,
        };
        Some((name, code))
    }
    fn handle_keyup(&mut self, key: KeyCode) {
        let Some((key_name, key_code)) = Self::key_info(key) else { return };
        if let Some(lane) = self.config.key_to_lane(key_name, Some(key_code)) {
            self.keys_pressed[lane] = false;
            let ct = self.current_time();
            if let Some(ref mut r) = self.recorder { r.record_event(ct, lane, false); }
            if matches!(self.phase, Phase::Playing) {
                self.handle_note_release(lane);
            }
        }
    }

    fn handle_note_release(&mut self, lane: usize) {
        let ct = self.current_time();

        // 找该轨道 holding 的音符
        for i in self.active_idx..self.notes.len() {
            let n = &self.notes[i];
            if n.holding && n.lane == lane && !n.hit && !n.missed && !n.ghost {
                let result = judge_hold_release(n.end_time, ct, &self.windows);
                self.notes[i].holding = false;
                self.score.add_judgment(result);
                if matches!(result, JudgmentResult::Miss) {
                    self.combo = 0;
                    self.notes[i].missed = true;
                    self.notes[i].release_time = Some(ct);
                } else {
                    self.combo = self.score.combo;
                    self.notes[i].hit = true;
                }
                self.last_judgment = Some((result, Instant::now()));
            self.judgment_type = Some(result);
            self.burst_start = Some(std::time::Instant::now());
                self.max_combo = self.max_combo.max(self.combo);
                break;
            }
        }
    }

    pub fn render_frame(&mut self) {
        self.profiler.begin_frame();
        let _show_fps = true;

        // 暂停时不更新游戏逻辑
        if matches!(self.phase, Phase::Paused { .. }) {
            self.render_pause_overlay();
            self.profiler.end_section(0);
            self.profiler.end_section(1);
            self.profiler.end_section(2);
            self.profiler.end_section(3);
            self.do_gpu_submit();
            self.profiler.end_section(4);
            self.profiler.end_section(5);
            self.profiler.end_section(6);
            self.profiler.end_frame();
            return;
        }

        let ct = self.current_time();
        // 当游戏时间超过首次暂停点+3秒后，清除记录(允许下次暂停重新计时)
        if let Some(lpc) = self.last_pause_ct {
            if ct > lpc + 3000.0 { self.last_pause_ct = None; }
        }
        self.ensure_playing(); // Countdown → Playing when lead-in ends

        // PauseCountdown → Playing: 3 秒后恢复 (unused now, replaced by do_continue)
        if let Phase::PauseCountdown { .. } = self.phase {
            self.phase = Phase::Playing;
        }

        self.render.quad.clear();
        self.render.text.clear();

        match &self.phase {
            Phase::Playing | Phase::Countdown => {
                // 1. 曲绘背景
                // 曲绘背景
                if let Some(ref regions) = self.skin_regions {
                    if let Some(r) = regions.get("bg_cover") {
                        self.render.quad.push_textured_rect(0.0, 0.0, self.screen_w, SCREEN_H, r.uv_x, r.uv_y, r.uv_w, r.uv_h, [255, 255, 255, 255]);
                    }
                }

                // 2. 舞台区域: 纯黑背景 (遮盖曲绘)
                let (stage_l, stage_r) = stage_bounds(self.screen_w, &self.lanes, self.note_w);
                self.render.quad.push_rect(stage_l, 0.0, stage_r - stage_l, SCREEN_H, [0, 0, 0, 255]);

                // 两侧区域: 曲绘 + 可调暗度蒙版 (alpha=160 可改)
                let side_alpha: u8 = 160;
                if stage_l > 0.0 { self.render.quad.push_rect(0.0, 0.0, stage_l, SCREEN_H, [0, 0, 0, side_alpha]); }
                if stage_r < self.screen_w { self.render.quad.push_rect(stage_r, 0.0, self.screen_w - stage_r, SCREEN_H, [0, 0, 0, side_alpha]); }

                // 3. 舞台背景(皮肤)
                if let Some(ref regions) = self.skin_regions {
                    if let Some(r) = regions.get("stage_bottom").or_else(|| regions.get("mania-stage-bottom")) {
                        let asp = r.height as f32 / r.width as f32;
                        self.render.quad.push_textured_rect(0.0, 500.0 - self.screen_w * asp, self.screen_w, self.screen_w * asp, r.uv_x, r.uv_y, r.uv_w, r.uv_h, [255, 255, 255, 255]);
                    }
                }

                // 4. 判定线
                let show_line = self.skin_config.as_ref().map_or(true, |c| c.get("JudgementLine").map_or(true, |v| v != "0"));
                if show_line { self.render.quad.push_rect(0.0, self.hit_y as f32 - 2.5, self.screen_w, 5.0, [255, 0, 0, 255]); }

                // 5. 音符处理 (远跳+底漏+渲染 三合一，对标 Python)
                {
                    let result = process_notes(
                        &mut self.notes, &mut self.active_idx, ct, self.eff_speed,
                        self.windows.miss, self.hit_y, self.note_w,
                        &self.lanes, &mut self.render.quad,
                        self.skin_regions.as_ref(), &mut self.score,
                    );
                    // 新判定立即刷新 burst
                    if let Some(j) = result.latest_judgment {
                        self.judgment_type = Some(j);
                        self.burst_start = Some(std::time::Instant::now());
                    }
                    self.combo = self.score.combo;
                    self.max_combo = self.max_combo.max(self.combo);
                }

                // 6. 按键底板(皮肤)
                if let Some(ref regions) = self.skin_regions {
                    draw_key_pads(&self.lanes, self.note_w, &self.keys_pressed, &mut self.render.quad, regions);
                }

                // 7. 判定特效 (对标 Python _draw_hit_burst)
                draw_hit_burst(&mut self.render.quad, self.skin_regions.as_ref(), &mut self.judgment_type, &mut self.burst_start, self.screen_w);

                // HUD
                let fps = self.calc_fps();
                let show_fps = true /* show_fps override */;
                let rate = self.song_rate;
                draw_hud(
                    &mut self.render.quad,
                    &mut self.render.text,
                    &self.score,
                    self.combo,
                    ct,
                    self.total_duration,
                    fps,
                    show_fps,
                    rate,
                );

                // 音频触发
                if ct >= self.global_offset && !self.music_started {
                    self.audio.play();
                    self.music_started = true;
                }

                // 结束检测
                if self.active_idx >= self.notes.len()
                    && !self.notes.is_empty()
                    && ct > self.total_duration + 1500.0
                {
                    let acc = self.score.accuracy();
                    // 保存历史
                    {
                        let rel = &self.map_path;
                        let mut hist = crate::history::load_history("history.json");
                        let config_od = self.config.od;
                        let rank = crate::ui::theme::rank_from_acc(
                            acc, self.score.good_count, self.score.ok_count,
                            self.score.meh_count, self.score.miss_count);
                        crate::history::add_record(
                            &mut hist, rel,
                            self.score.total_score, acc, self.song_rate,
                            rank, config_od, self.mirror_mode,
                        );
                        crate::history::save_history("history.json", &hist);
                    }
                    if let Some(rec) = self.recorder.take() {
                        self.replay_data = Some(rec.finalize(self.score.total_score, acc, self.max_combo, crate::replay::JudgmentCounts { perfect: self.score.perfect_count, great: self.score.great_count, good: self.score.good_count, ok: self.score.ok_count, meh: self.score.meh_count, miss: self.score.miss_count }, self.score.total_notes));
                    }
                    let total_objects = self.score.total_notes; // judged objects (tap=1, hold=2)
                    let combo_progress = if total_objects > 0 {
                        self.max_combo as f64 / total_objects as f64
                    } else { 0.0 };
                    let standard = standardized_score(acc / 100.0, combo_progress);
                    let rank = crate::ui::theme::rank_from_acc(
                        acc, self.score.good_count, self.score.ok_count,
                        self.score.meh_count, self.score.miss_count).to_string();
                    let pp_val = crate::pp::calculate_pp(&self.map_path, self.song_rate, acc, self.score.miss_count, self.max_combo);
                    let diff_label = crate::difficulty::analyze_path_label(&self.map_path, self.song_rate, self.config.od);
                    self.exit_result = Some(GameResult {
                        score: self.score.total_score,
                        standardized_score: standard,
                        acc,
                        max_combo: self.max_combo,
                        perfect_count: self.score.perfect_count,
                        great_count: self.score.great_count,
                        good_count: self.score.good_count,
                        ok_count: self.score.ok_count,
                        meh_count: self.score.meh_count,
                        miss_count: self.score.miss_count,
                        total_notes: self.score.total_notes,
                        total_objects,
                        song_name: self.song_name.clone(),
                        map_path: self.map_path.clone(),
                        song_rate: self.song_rate,
                        od: self.config.od,
                        mirror_mode: self.mirror_mode,
                        rank,
                        stars: self.stars,
                        pp: pp_val,
                        cover_path: self.cover_path.clone(),
                        difficulty_label: diff_label,
                    });
                    let _ = self.audio.stop();
                    let offsets = self.replay_data.as_ref()
                        .map(|r| super::results::compute_hit_offsets(r, &self.map_path))
                        .unwrap_or_default();
                    self.phase = Phase::Results {
                        result: self.exit_result.clone().unwrap(),
                        offsets,
                    };
                }
            }
            /* dead: Phase::Paused rendering handled by render_pause_overlay + early return */
            #[allow(unreachable_patterns)]
            Phase::Paused { .. } => {}
            /* dead: PauseCountdown rendering — phase transitions at line 389 */
            Phase::PauseCountdown { .. } => {}
            Phase::Results { result, offsets } => {
                let cover = self.render.skin_regions().get("bg_cover").cloned();
                render_results(
                    result,
                    offsets,
                    0,
                    0.0, 0.0, 0.5,
                    &mut self.render.quad,
                    &mut self.render.text,
                    cover.as_ref(),
                );
            }
        }

        // GPU 提交
        self.profiler.end_section(0); // logic
        // collect draws already happened inside the match
        self.profiler.end_section(1); // collect

        let quad_count = self.render.quad.upload(&self.render.queue);
        let quad_buf_idx = self.render.quad.last_buffer();
        self.profiler.end_section(2); // quad upload

        let glyph_count = self.render.text.upload(&self.render.queue);
        self.profiler.end_section(3); // text upload

        self.profiler.quad_count = quad_count;
        self.profiler.glyph_count = glyph_count;

        let output = match self.render.begin_frame() {
            Ok(o) => o,
            Err(wgpu::SurfaceError::Outdated | wgpu::SurfaceError::Lost) => {
                self.render.resize(self.render.config.width, self.render.config.height);
                return;
            }
            Err(e) => {
                log::error!("Surface error: {:?}", e);
                return;
            }
        };
        self.profiler.end_section(4); // begin_frame (get_current_texture)

        let view = output
            .texture
            .create_view(&wgpu::TextureViewDescriptor::default());

        let mut encoder = self
            .render
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor::default());
        {
            let mut rpass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("main"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &view,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color::BLACK),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
            });
            self.render.quad.draw(&mut rpass, quad_buf_idx, quad_count);
            self.render.text.draw(&mut rpass, glyph_count);
        }
        self.render.queue.submit([encoder.finish()]);
        self.profiler.end_section(5); // submit

        self.render.end_frame(output);
        self.profiler.end_section(6); // present
        self.profiler.end_frame();
    }


    /// 简化的按键处理 (供外部 App 调用)
    pub fn handle_key(&mut self, key: winit::keyboard::KeyCode, pressed: bool) -> bool {
        // key release 也走 handle_key_msg（录制 + 松键处理），但菜单操作仅响应 press
        if !pressed {
            self.handle_key_msg(key, false);
            return false;
        }
        // 结算界面任意键退出（确保 replay_data 已生成）
        if matches!(self.phase, Phase::Results { .. }) {
            if self.replay_data.is_none() {
                if let Some(rec) = self.recorder.take() {
                    let acc = self.score.accuracy();
                    self.replay_data = Some(rec.finalize(self.score.total_score, acc, self.max_combo, crate::replay::JudgmentCounts { perfect: self.score.perfect_count, great: self.score.great_count, good: self.score.good_count, ok: self.score.ok_count, meh: self.score.meh_count, miss: self.score.miss_count }, self.score.total_notes));
                }
            }
            return true;
        }
        // 暂停菜单 Exit — 也保存回放
        if key == KeyCode::Enter {
            if let Phase::Paused { selected: 2 } = self.phase {
                if self.replay_data.is_none() {
                    if let Some(rec) = self.recorder.take() {
                        let acc = self.score.accuracy();
                        self.replay_data = Some(rec.finalize(self.score.total_score, acc, self.max_combo, crate::replay::JudgmentCounts { perfect: self.score.perfect_count, great: self.score.great_count, good: self.score.good_count, ok: self.score.ok_count, meh: self.score.meh_count, miss: self.score.miss_count }, self.score.total_notes));
                    }
                }
                return true;
            }
        }
        self.handle_key_msg(key, pressed);
        false
    }

    fn handle_key_msg(&mut self, key: winit::keyboard::KeyCode, pressed: bool) {
        // 菜单操作 (仅 press)
        if pressed {
            // ESC: 暂停/继续
            if key == KeyCode::Escape {
                match &self.phase {
                    Phase::Playing | Phase::Countdown => {
                        self.audio.pause();
                        self.pause_start = Some(Instant::now());
                        let ct = self.current_time();
                        let _old_lpc = self.last_pause_ct;
                        if self.last_pause_ct.map_or(true, |lpc| ct > lpc + 3000.0) {
                            self.last_pause_ct = Some(ct);
                        }
                        self.phase = Phase::Paused { selected: 0 };
                        return;
                    }
                    Phase::Paused { .. } => {
                        self.do_continue();
                        return;
                    }
                    _ => {}
                }
            }

            // SPACE: 快进到第一个音符前3秒 (对标 Python, 仅一次)
            if key == KeyCode::Space && !self.skip_used && !self.notes.is_empty() {
                let ct = self.current_time();
                let first = self.notes[0].time;
                let target = first - 3000.0;
                if target > 0.0 && ct < target {
                    self.skip_used = true;
                    self.start_time -= std::time::Duration::from_secs_f64(((target - ct) / self.song_rate / 1000.0).max(0.0));
                    if target >= self.global_offset && !self.music_started {
                        self.audio.seek_ms((target - self.global_offset).max(0.0));
                        self.audio.play();
                        self.music_started = true;
                    }
                }
                return;
            }

            // 暂停菜单导航
            if let Phase::Paused { selected } = &mut self.phase {
                match key {
                    KeyCode::ArrowUp => *selected = (*selected + 2) % 3,
                    KeyCode::ArrowDown => *selected = (*selected + 1) % 3,
                    KeyCode::Enter => match *selected {
                        0 => self.do_continue(),
                        1 => self.do_restart(),
                        _ => { /* exit — handled by returning true from handle_key */ }
                    },
                    _ => {}
                }
                return;
            }
        }

        // 游玩按键 (press + release 都处理)
        let Some((key_name, key_code)) = Self::key_info(key) else { return };
        if let Some(lane) = self.config.key_to_lane(key_name, Some(key_code)) {
            self.keys_pressed[lane] = pressed;
            let ct = self.current_time();
            if let Some(ref mut r) = self.recorder { r.record_event(ct, lane, pressed); }
            if matches!(self.phase, Phase::Playing) {
                if pressed { self.handle_note_hit(lane); }
                else { self.handle_note_release(lane); }
            }
        }
    }

    fn do_continue(&mut self) {
        for n in &mut self.notes { if n.hit || n.missed { n.ghost = true; } }
        let pause_ct = self.last_pause_ct.unwrap_or_else(|| self.current_time());
        // 不在这里清除 last_pause_ct — 让 render_frame 检测到 ct > lpc+3000 后再清除
        let target_ct = (pause_ct - 3000.0).max(0.0);
        let real_target_ms = (target_ct + self.map_offset + self.global_offset + super::notes::LEAD_IN) / self.song_rate;
        self.start_time = Instant::now() - std::time::Duration::from_secs_f64((real_target_ms / 1000.0).max(0.0));
        self.audio.seek_ms((target_ct - self.global_offset).max(0.0));
        self.audio.play();
        if target_ct >= 0.0 { self.music_started = true; }
        self.pause_start = None;
        self.phase = if target_ct >= 0.0 { Phase::Playing } else { Phase::Countdown };
    }

    fn render_pause_overlay(&mut self) {
        let w = self.screen_w;
        self.render.quad.clear();
        self.render.text.clear();
        // 暗色遮罩
        self.render.quad.push_rect(0.0, 0.0, w, super::notes::SCREEN_H, [0, 0, 0, 160]);
        self.render.text.queue_text("=== PAUSED ===", w/2.0 - 70.0, 180.0, 24.0, [255, 255, 255, 255]);
        let sel = if let Phase::Paused { selected } = self.phase { selected } else { 0 };
        let opts = ["Continue", "Restart", "Exit"];
        for (i, o) in opts.iter().enumerate() {
            let prefix = if i == sel { "▶ " } else { "  " };
            let color = if i == sel { [0xff, 0x66, 0xaa, 255] } else { [180, 180, 180, 255] };
            self.render.text.queue_text(&format!("{}{}", prefix, o), w/2.0 - 60.0, 260.0 + i as f32 * 40.0, 18.0, color);
        }
        self.render.text.queue_text("[↑↓] Navigate  [ENTER] Select  [ESC] Continue", w/2.0 - 160.0, 420.0, 14.0, [150, 150, 150, 255]);
    }

    fn do_gpu_submit(&mut self) {
        self.render.quad.upload(&self.render.queue);
        let gc = self.render.text.upload(&self.render.queue);
        if let Ok(o) = self.render.begin_frame() {
            let v = o.texture.create_view(&wgpu::TextureViewDescriptor::default());
            let mut e = self.render.device.create_command_encoder(&wgpu::CommandEncoderDescriptor::default());
            { let mut rp = e.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: None, color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &v, resolve_target: None,
                    ops: wgpu::Operations { load: wgpu::LoadOp::Clear(wgpu::Color::BLACK), store: wgpu::StoreOp::Store },
                })], depth_stencil_attachment: None, timestamp_writes: None, occlusion_query_set: None,
            });
            let qc = self.render.quad.instances.len();
            rp.set_viewport(0.0, 0.0, self.render.config.width as f32, self.render.config.height as f32, 0.0, 1.0);
            self.render.quad.draw(&mut rp, self.render.quad.last_buffer(), qc);
            self.render.text.draw(&mut rp, gc); }
            self.render.queue.submit([e.finish()]); self.render.end_frame(o);
        }
    }

    fn do_restart(&mut self) {
        for n in &mut self.notes { n.hit = false; n.missed = false; n.holding = false; n.ghost = false; n.stuck_y = None; n.release_time = None; }
        self.active_idx = 0;
        self.score = Score { total_notes: self.score.total_notes, ..Default::default() };
        self.combo = 0; self.max_combo = 0;
        self.music_started = false; self.skip_used = false;
        self.keys_pressed = [false; 4]; self.last_pause_ct = None;
        self.start_time = Instant::now();
        self.audio.stop();
        // 重新加载音频 (变速支持)
        self.audio.load(&self.song_name).ok();
        if self.song_rate != 1.0 { self.audio.set_tempo(self.song_rate as f32); }
        self.phase = Phase::Countdown;
    }

    pub fn handle_event(&mut self, event: WindowEvent) -> bool {
        // 返回 true 表示应该退出
        let mut should_exit = false;

        match event {
            WindowEvent::CloseRequested => {
                should_exit = true;
            }
            WindowEvent::KeyboardInput {
                event:
                    KeyEvent {
                        physical_key: PhysicalKey::Code(key),
                        state,
                        repeat: false,
                        ..
                    },
                ..
            } => {
                match (&self.phase, state) {
                    (Phase::Results { .. }, ElementState::Pressed) => {
                        if key == KeyCode::Enter {
                            should_exit = true;
                        }
                    }
                    (Phase::Paused { selected }, ElementState::Pressed) => match key {
                        KeyCode::Escape => {
                            // 继续
                            let ct = self.current_time();
                            let pause_start = Instant::now();
                            self.audio.pause();
                            self.phase = Phase::PauseCountdown {
                                selected: 0,
                                count_start: Instant::now(),
                                pause_start,
                                current_time_snap: ct,
                            };
                        }
                        KeyCode::ArrowUp => {
                            self.phase = Phase::Paused {
                                selected: (selected + 2) % 3,
                            };
                        }
                        KeyCode::ArrowDown => {
                            self.phase = Phase::Paused {
                                selected: (selected + 1) % 3,
                            };
                        }
                        KeyCode::Enter => match selected {
                            0 => {
                                // Continue
                                let ct = self.current_time();
                                let pause_start = Instant::now();
                                self.audio.pause();
                                self.phase = Phase::PauseCountdown {
                                    selected: 0,
                                    count_start: Instant::now(),
                                    pause_start,
                                    current_time_snap: ct,
                                };
                            }
                            1 => {
                                // Restart — rebuild
                                for n in &mut self.notes {
                                    n.hit = false;
                                    n.missed = false;
                                    n.holding = false;
                                    n.stuck_y = None;
                                    n.release_time = None;
                                }
                                self.active_idx = 0;
                                let total_notes = self.score.total_notes;
                                self.score = Score { total_notes, ..Default::default() };
                                self.combo = 0;
                                self.max_combo = 0;
                                self.keys_pressed = [false; 4];
                                self.music_started = false;
                                self.start_time = Instant::now();
                                self.audio.stop();
                                if self.audio.load(&self.song_name).is_ok() {
                                    if self.song_rate != 1.0 {
                                        self.audio.set_tempo(self.song_rate as f32);
                                    }
                                }
                                self.phase = Phase::Countdown;
                            }
                            2 => {
                                // Exit
                                should_exit = true;
                            }
                            _ => {}
                        },
                        _ => {}
                    },
                    (Phase::PauseCountdown { .. }, _) => {
                        // 倒计时期间不处理输入，只等 3 秒
                    }
                    (Phase::Playing | Phase::Countdown, ElementState::Pressed) => match key {
                        KeyCode::Escape => {
                            self.audio.pause();
                            self.phase = Phase::Paused { selected: 0 };
                        }
                        _ => self.handle_keydown(key),
                    },
                    (Phase::Playing | Phase::Countdown, ElementState::Released) => {
                        self.handle_keyup(key);
                    }
                    _ => {}
                }
            }
            _ => {}
        }

        should_exit
    }
}

