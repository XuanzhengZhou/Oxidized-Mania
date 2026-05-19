use std::collections::HashMap;
use std::sync::Arc;
use std::time::Instant;
use winit::window::Window;

use crate::audio::bass::BassAudio;
use crate::game::judgment::{judge_hold_release, judge_tap, JudgmentResult, JudgmentWindows};
use crate::game::notes::{
    calc_lanes, draw_hit_burst, draw_key_pads, note_y, process_notes,
    stage_bounds, set_screen_w, screen_w, LEAD_IN, SCREEN_H,
};
use crate::game::scoring::Score;
use crate::game::{NoteRT, NoteType};
use crate::replay::{ReplayData, ReplayEvent};
use crate::render::context::RenderCtx;
use crate::skin::AtlasRegion;

pub struct ReplayEngine {
    replay: ReplayData,
    events: Vec<ReplayEvent>,
    event_idx: usize,
    audio: BassAudio,
    render: RenderCtx,
    window: Arc<Window>,

    notes: Vec<NoteRT>,
    active_idx: usize,
    total_duration: f64,
    start_time: Instant,
    song_rate: f64,
    map_offset: f64,
    eff_speed: f64,
    windows: JudgmentWindows,

    score: Score,
    keys_pressed: [bool; 4],
    judgment_type: Option<JudgmentResult>,
    burst_start: Option<Instant>,
    lanes: [f32; 4],
    screen_w: f32,
    hit_y: f64,
    note_w: f32,
    skin_regions: Option<HashMap<String, AtlasRegion>>,
    show_line: bool,
    fps_times: Vec<Instant>,

    pre_judgments: Vec<(u32, JudgmentResult)>,
    paused: bool,
    pause_start: Option<Instant>,
    pub finished: bool,
    finished_at: Option<Instant>,
    countdown_text: Option<String>,
    pub show_fps: bool,
    kps_start_idx: usize,
}

impl ReplayEngine {
    pub fn new(
        replay: ReplayData,
        audio_path: &str,
        beatmap_path: &str,
        skin_regions: Option<HashMap<String, AtlasRegion>>,
        skin_config: Option<HashMap<String, String>>,
        window: Arc<Window>,
        render: RenderCtx,
    ) -> Result<Self, String> {
        let mut audio = BassAudio::init().map_err(|e| format!("audio: {e}"))?;
        audio.load(audio_path).map_err(|e| format!("audio load: {e}"))?;

        let (meta, bnotes) =
            crate::beatmap::load_beatmap_rox(beatmap_path).map_err(|e| format!("beatmap: {e}"))?;

        let song_rate = replay.song_rate;
        let od = replay.od;
        let scroll_speed = if replay.scroll_speed < 5.0 {
            replay.scroll_speed * 30.0
        } else {
            replay.scroll_speed
        };

        set_screen_w(render.logical_w as f32);
        let sw = screen_w();
        let hit_y = replay.hit_position;
        let note_w = (80.0 * replay.stage_scale) as f32;
        let lanes = calc_lanes(sw, replay.stage_spacing, replay.stage_scale);
        let eff_speed = scroll_speed / 24.0 / song_rate;

        let mut notes: Vec<NoteRT> = bnotes
            .into_iter()
            .map(|n| NoteRT {
                time: n.time, end_time: n.end_time, lane: n.lane,
                note_type: n.note_type, hit: false, missed: false,
                holding: false, ghost: false, stuck_y: None, release_time: None,
            })
            .collect();

        if replay.mirror_mode {
            let m = [3usize, 2, 1, 0];
            for n in &mut notes { n.lane = m[n.lane]; }
        }

        let total_duration = notes.iter().map(|n| n.end_time.max(n.time)).fold(1.0, f64::max);
        let total_notes: u32 = notes.iter()
            .map(|n| if n.note_type == NoteType::Hold { 2 } else { 1 })
            .sum();
        let windows = JudgmentWindows::new(od, song_rate);

        let show_line = skin_config
            .as_ref()
            .map_or(true, |c| c.get("JudgementLine").map_or(true, |v| v != "0"));

        let mut events = replay.events.clone();
        events.sort_by_key(|e| e.time_ms);

        log::info!("[Replay] Loaded: {} events, {} notes, od={} rate={}",
            events.len(), notes.len(), od, song_rate);

        let pre_judgments = Self::pre_compute_judgments(&events, &notes, &windows);

        Ok(Self {
            events, event_idx: 0, audio, render, window,
            notes, active_idx: 0, total_duration, replay,
            start_time: Instant::now(), song_rate,
            map_offset: meta.offset, eff_speed, windows,
            score: Score { total_notes, ..Default::default() },
            keys_pressed: [false; 4],
            judgment_type: None, burst_start: None,
            lanes, screen_w: sw, hit_y, note_w,
            skin_regions, show_line, fps_times: Vec::new(),
            pre_judgments,
            paused: false, pause_start: None, finished: false, finished_at: None, countdown_text: None, show_fps: true, kps_start_idx: 0,
        })
    }

    /// 预计算判定列表 — 事件驱动，精确到每事件时刻检测 miss/auto-release
    fn pre_compute_judgments(
        events: &[ReplayEvent],
        notes: &[NoteRT],
        windows: &JudgmentWindows,
    ) -> Vec<(u32, JudgmentResult)> {
        let mut jlist = Vec::new();
        let mut ncopy: Vec<NoteRT> = notes.to_vec();
        let mut active = 0usize;
        let mut last_ct = 0.0f64;

        // 按时间排序的事件迭代
        let mut sorted_events = events.to_vec();
        sorted_events.sort_by_key(|e| e.time_ms);
        let mut ev_idx = 0usize;

        while ev_idx < sorted_events.len() {
            let ct = sorted_events[ev_idx].time_ms as f64;

            // ── miss / auto-release 检测：在 last_ct..ct 区间内到期的 ──
            let mut i = active;
            while i < ncopy.len() {
                if ncopy[i].hit || ncopy[i].missed { i += 1; continue; }
                match ncopy[i].note_type {
                    NoteType::Tap => {
                        let miss_at = ncopy[i].time + windows.miss;
                        if miss_at > last_ct && miss_at <= ct {
                            ncopy[i].missed = true;
                            jlist.push(((miss_at + 1.0) as u32, JudgmentResult::Miss));
                        }
                    }
                    NoteType::Hold => {
                        if ncopy[i].holding {
                            // 正在 hold → 检查 end_time 是否落入区间
                            if ncopy[i].end_time > last_ct && ncopy[i].end_time <= ct {
                                ncopy[i].hit = true;
                                ncopy[i].holding = false;
                                jlist.push((ncopy[i].end_time as u32, JudgmentResult::Perfect));
                            }
                        } else {
                            // 未被 hit → 检查 miss 窗口
                            let miss_at = ncopy[i].time + windows.miss;
                            if miss_at > last_ct && miss_at <= ct {
                                ncopy[i].missed = true;
                                jlist.push(((miss_at + 1.0) as u32, JudgmentResult::Miss));
                                // hold 尾部也计为 miss（与 Score.total_notes 对齐：hold=2）
                                let tail_t = ncopy[i].end_time.max(miss_at + 2.0);
                                jlist.push((tail_t as u32, JudgmentResult::Miss));
                            }
                        }
                    }
                }
                i += 1;
            }

            // advance active
            while active < ncopy.len() && (ncopy[active].hit || ncopy[active].missed) {
                active += 1;
            }

            // ── 处理当前时间的所有事件 ──
            while ev_idx < sorted_events.len() && (sorted_events[ev_idx].time_ms as f64 - ct).abs() < 0.5 {
                let ev = &sorted_events[ev_idx];
                let lane = ev.lane as usize;
                if lane < 4 {
                    if ev.pressed {
                        let mut best_i = None;
                        let mut best_d = f64::MAX;
                        for i in active..ncopy.len() {
                            let n = &ncopy[i];
                            if n.lane != lane || n.hit || n.missed || n.holding { continue; }
                            let d = (n.time - ct).abs();
                            if d <= windows.miss && d < best_d { best_d = d; best_i = Some(i); }
                        }
                        if let Some(i) = best_i {
                            let n = &mut ncopy[i];
                            let r = judge_tap(n.time, ct, windows);
                            jlist.push((ev.time_ms, r));
                            if r != JudgmentResult::Miss {
                                match n.note_type {
                                    NoteType::Tap => n.hit = true,
                                    NoteType::Hold => n.holding = true,
                                }
                            } else { n.missed = true; }
                        }
                    } else {
                        for i in active..ncopy.len() {
                            let n = &mut ncopy[i];
                            if n.lane != lane || !n.holding || n.hit || n.missed { continue; }
                            n.holding = false;
                            let r = judge_hold_release(n.end_time, ct, windows);
                            jlist.push((ev.time_ms, r));
                            if r == JudgmentResult::Miss { n.missed = true; }
                            else { n.hit = true; }
                            break;
                        }
                    }
                }
                ev_idx += 1;
            }

            last_ct = ct;
        }

        // ── 所有事件结束后，处理剩余音符 ──
        for i in active..ncopy.len() {
            if ncopy[i].hit || ncopy[i].missed { continue; }
            match ncopy[i].note_type {
                NoteType::Tap => {
                    let miss_at = ncopy[i].time + windows.miss;
                    ncopy[i].missed = true;
                    jlist.push(((miss_at + 1.0) as u32, JudgmentResult::Miss));
                }
                NoteType::Hold => {
                    if ncopy[i].holding {
                        // 仍按住 → auto-release as Perfect
                        ncopy[i].hit = true;
                        jlist.push((ncopy[i].end_time as u32, JudgmentResult::Perfect));
                    } else {
                        // 从未被 hit → 两次 miss
                        let miss_at = ncopy[i].time + windows.miss;
                        ncopy[i].missed = true;
                        jlist.push(((miss_at + 1.0) as u32, JudgmentResult::Miss));
                        let tail_t = ncopy[i].end_time.max(miss_at + 2.0);
                        jlist.push((tail_t as u32, JudgmentResult::Miss));
                    }
                }
            }
        }

        jlist.sort_by_key(|(t, _)| *t);
        jlist
    }

    /// 从预计算判定列表查询 ct 时刻的 (combo, acc, score, max_combo)
    fn compute_stats_at(&self, ct: f64) -> (u32, f64, u32, u32) {
        let mut total_score: u32 = 0;
        let mut combo: u32 = 0;
        let mut max_combo: u32 = 0;
        let mut judged: u32 = 0;
        for &(t, ref r) in &self.pre_judgments {
            if t as f64 > ct { break; }
            let pts = r.score_value();
            total_score += pts;
            judged += 1;
            if *r == JudgmentResult::Miss { combo = 0; }
            else { combo += 1; }
            max_combo = max_combo.max(combo);
        }
        let acc = if judged > 0 { total_score as f64 / (judged as f64 * 305.0) * 100.0 } else { 100.0 };
        (combo, acc, total_score, max_combo)
    }

    pub fn current_time(&self) -> f64 {
        if self.paused {
            if let Some(p) = self.pause_start {
                p.duration_since(self.start_time).as_secs_f64() * 1000.0 * self.song_rate
                    - self.map_offset - LEAD_IN
            } else { 0.0 }
        } else {
            self.start_time.elapsed().as_secs_f64() * 1000.0 * self.song_rate
                - self.map_offset - LEAD_IN
        }
    }

    pub fn paused(&self) -> bool { self.paused }

    pub fn total_objects(&self) -> u32 {
        self.notes.len() as u32
    }

    pub fn map_path(&self) -> &str { &self.replay.map_path }

    pub fn toggle_pause(&mut self) {
        if self.paused {
            let paused_dur = self.pause_start.unwrap().elapsed();
            self.start_time += paused_dur;
            self.pause_start = None;
            self.paused = false;
            let _ = self.audio.play();
        } else {
            self.pause_start = Some(Instant::now());
            self.paused = true;
            let _ = self.audio.pause();
        }
    }

    pub fn seek(&mut self, target_ct: f64) {
        let target_ct = target_ct.max(0.0);
        for n in &mut self.notes {
            n.hit = false; n.missed = false; n.holding = false;
            n.ghost = false; n.stuck_y = None; n.release_time = None;
        }
        self.active_idx = 0;
        self.event_idx = 0;
        self.kps_start_idx = 0;
        self.keys_pressed = [false; 4];
        self.score = Score { total_notes: self.score.total_notes, ..Default::default() };
        self.process_events(target_ct);
        let new_start = target_ct + self.map_offset + LEAD_IN;
        self.start_time = Instant::now()
            - std::time::Duration::from_secs_f64(new_start / 1000.0 / self.song_rate);
        let audio_pos = target_ct.max(0.0);
        self.audio.seek_ms(audio_pos);
        let _ = self.audio.play();
        self.advance_active_idx(target_ct);
    }

    fn process_events(&mut self, ct: f64) {
        let ct_ms = ct as i64;
        while self.event_idx < self.events.len() && (self.events[self.event_idx].time_ms as i64) <= ct_ms {
            let ev = &self.events[self.event_idx];
            let lane = ev.lane as usize;
            if lane < 4 {
                let old = self.keys_pressed[lane];
                self.keys_pressed[lane] = ev.pressed;
                if ev.pressed && !old {
                    self.simulate_keydown(lane, ct);
                } else if !ev.pressed && old {
                    self.simulate_keyup(lane, ct);
                }
            }
            self.event_idx += 1;
        }
    }

    fn simulate_keydown(&mut self, lane: usize, ct: f64) {
        let mut best_idx: Option<usize> = None;
        let mut best_dist = f64::MAX;
        for i in self.active_idx..self.notes.len() {
            let n = &self.notes[i];
            if n.lane != lane || n.hit || n.missed || n.holding || n.ghost { continue; }
            let dist = (n.time - ct).abs();
            if dist <= self.windows.miss && dist < best_dist {
                best_dist = dist; best_idx = Some(i);
            }
        }
        if let Some(idx) = best_idx {
            let n = &mut self.notes[idx];
            let result = judge_tap(n.time, ct, &self.windows);
            self.score.add_judgment(result);
            if result == JudgmentResult::Miss {
                n.missed = true;
            } else {
                match n.note_type {
                    NoteType::Tap => { n.hit = true; }
                    NoteType::Hold => { n.holding = true; n.stuck_y = Some(note_y(n.time, ct, self.hit_y, self.eff_speed)); }
                }
            }
            self.judgment_type = Some(result);
            self.burst_start = Some(Instant::now());
        }
    }

    fn simulate_keyup(&mut self, lane: usize, ct: f64) {
        for i in self.active_idx..self.notes.len() {
            let n = &mut self.notes[i];
            if n.lane != lane || !n.holding || n.hit || n.missed { continue; }
            n.holding = false;
            let result = judge_hold_release(n.end_time, ct, &self.windows);
            self.score.add_judgment(result);
            if result == JudgmentResult::Miss {
                n.missed = true; n.release_time = Some(ct);
            } else { n.hit = true; }
            self.judgment_type = Some(result);
            self.burst_start = Some(Instant::now());
            return;
        }
    }

    fn advance_active_idx(&mut self, ct: f64) {
        while self.active_idx < self.notes.len() {
            let n = &self.notes[self.active_idx];
            let nt = n.end_time.max(n.time);
            if (n.hit || n.missed) && (ct - nt) * self.eff_speed > 300.0 { self.active_idx += 1; }
            else { break; }
        }
    }

    pub fn render_frame(&mut self) {
        let ct = self.current_time();
        self.process_events(ct);

        // 提前计算倒计时（避免后续 &mut self.render 冲突）
        self.countdown_text = None;

        self.render.quad.clear();
        self.render.text.clear();

        // 1. 曲绘背景
        if let Some(ref regions) = self.skin_regions {
            if let Some(r) = regions.get("bg_cover") {
                self.render.quad.push_textured_rect(0.0, 0.0, self.screen_w, SCREEN_H, r.uv_x, r.uv_y, r.uv_w, r.uv_h, [255;4]);
            }
        }

        // 2. 舞台区域
        let (stage_l, stage_r) = stage_bounds(self.screen_w, &self.lanes, self.note_w);
        self.render.quad.push_rect(stage_l, 0.0, stage_r - stage_l, SCREEN_H, [0,0,0,255]);
        let sa: u8 = 160;
        if stage_l > 0.0 { self.render.quad.push_rect(0.0, 0.0, stage_l, SCREEN_H, [0,0,0,sa]); }
        if stage_r < self.screen_w { self.render.quad.push_rect(stage_r, 0.0, self.screen_w - stage_r, SCREEN_H, [0,0,0,sa]); }

        // 3. 舞台背景
        if let Some(ref regions) = self.skin_regions {
            if let Some(r) = regions.get("stage_bottom").or_else(|| regions.get("mania-stage-bottom")) {
                let asp = r.height as f32 / r.width as f32;
                self.render.quad.push_textured_rect(0.0, 500.0 - self.screen_w * asp, self.screen_w, self.screen_w * asp, r.uv_x, r.uv_y, r.uv_w, r.uv_h, [255;4]);
            }
        }

        // 4. 判定线
        if self.show_line {
            self.render.quad.push_rect(0.0, self.hit_y as f32 - 2.5, self.screen_w, 5.0, [255,0,0,255]);
        }

        // 5. 音符处理 + 渲染
        {
            let result = process_notes(
                &mut self.notes, &mut self.active_idx, ct, self.eff_speed,
                self.windows.miss, self.hit_y, self.note_w,
                &self.lanes, &mut self.render.quad,
                self.skin_regions.as_ref(), &mut self.score,
            );
            if let Some(j) = result.latest_judgment {
                self.judgment_type = Some(j);
                self.burst_start = Some(Instant::now());
            }
        }

        // 6. 按键底板
        if let Some(ref regions) = self.skin_regions {
            draw_key_pads(&self.lanes, self.note_w, &self.keys_pressed, &mut self.render.quad, regions);
        }

        // 7. 判定特效
        draw_hit_burst(&mut self.render.quad, self.skin_regions.as_ref(), &mut self.judgment_type, &mut self.burst_start, self.screen_w);

        // 8. HUD
        self.draw_hud(ct);

        self.submit();

        // 10. 音频 + 结束检测
        self.advance_active_idx(ct);
        if ct >= 0.0 && ct < 100.0 { let _ = self.audio.play(); }
        if self.active_idx >= self.notes.len() && !self.notes.is_empty() && ct > self.total_duration + 1500.0 {
            let _ = self.audio.stop();
            if !self.finished {
                self.finished_at = Some(Instant::now());
            }
            self.finished = true;
        }
    }

    pub fn should_auto_advance(&self) -> bool {
        self.finished_at.map_or(false, |t| t.elapsed().as_secs_f64() >= 3.0)
    }

    pub fn auto_advance_remaining(&self) -> f64 {
        self.finished_at.map_or(3.0, |t| (3.0 - t.elapsed().as_secs_f64()).max(0.0))
    }

    pub fn cancel_auto_advance(&mut self) {
        self.finished_at = None;
    }

    fn draw_hud(&mut self, ct: f64) {
        let (combo, acc, score, _max_cb) = self.compute_stats_at(ct);
        let q = &mut self.render.quad;
        let t = &mut self.render.text;

        q.push_rect(0.0, 0.0, self.screen_w, 44.0, [0,0,0,255]);
        q.push_rect(0.0, 44.0, self.screen_w, 2.0, [200,200,200,255]);

        t.queue_text(&format!("Combo: {}", combo), 10.0, 12.0, 15.0, [255,255,0,255]);
        t.queue_text(&format!("Score: {}", score), 10.0, 32.0, 11.0, [150,255,150,255]);
        t.queue_text(&format!("ACC: {:.2}%", acc), self.screen_w - 170.0, 12.0, 15.0, [0,255,255,255]);

        // 单调游标 KPS — 推进左边界（3秒窗口外）+ 右边界已有 event_idx
        while self.kps_start_idx < self.events.len() && (self.events[self.kps_start_idx].time_ms as i64) <= ct as i64 - 3000 {
            self.kps_start_idx += 1;
        }
        let mut kps_cnt = 0u32;
        for i in self.kps_start_idx..self.event_idx.min(self.events.len()) {
            if self.events[i].pressed { kps_cnt += 1; }
        }
        t.queue_text(&format!("KPS: {:.1}", kps_cnt as f64 / 3.0), self.screen_w - 170.0, 32.0, 11.0, [150,200,255,255]);
        t.queue_text("[REPLAY]", self.screen_w / 2.0 - 40.0, 12.0, 15.0, [255,100,100,255]);

        let mut mods = vec![format!("OD:{:.1}", self.replay.od)];
        if (self.song_rate - 1.0).abs() > 0.001 { mods.push(format!("{:.1}x", self.song_rate)); }
        if self.replay.mirror_mode { mods.push("Mirror".into()); }
        let mods_str = mods.join(" | ");
        t.queue_text(&mods_str, self.screen_w / 2.0 - (mods_str.len() as f32 * 3.5), 32.0, 11.0, [200,255,200,255]);

        let progress = (ct / self.total_duration).clamp(0.0, 1.0) as f32;
        q.push_rect(0.0, 0.0, self.screen_w, 4.0, [50,50,50,255]);
        q.push_rect(0.0, 0.0, self.screen_w * progress, 4.0, [255,100,100,255]);

        // 回放结束倒计时
        if let Some(ref txt) = self.countdown_text {
            t.queue_text(txt, self.screen_w / 2.0 - txt.len() as f32 * 5.0, 55.0, 14.0, [255, 220, 100, 255]);
        }

        if ct < 0.0 {
            let cn = ((ct.abs() / 1000.0).ceil()) as i32;
            if cn > 0 { t.queue_text(&format!("{}", cn), self.screen_w / 2.0 - 15.0, 300.0, 36.0, [255;4]); }
        }

        let now = Instant::now();
        self.fps_times.retain(|tm| tm.elapsed().as_secs_f32() < 1.0);
        self.fps_times.push(now);
        t.queue_text(&format!("FPS: {:.0}", self.fps_times.len() as f64), 10.0, SCREEN_H - 25.0, 12.0, [255;4]);
    }

    fn submit(&mut self) {
        let gpu = self.render.gpu.clone();
        let quad_count = self.render.quad.upload(&gpu.queue);
        let quad_buf_idx = self.render.quad.last_buffer();
        let glyph_count = self.render.text.upload(&gpu.queue);

        let output = match self.render.begin_frame() {
            Ok(o) => o,
            Err(wgpu::SurfaceError::Outdated | wgpu::SurfaceError::Lost) => {
                self.render.resize(self.render.config.width, self.render.config.height);
                return;
            }
            Err(_) => return,
        };

        let view = output.texture.create_view(&wgpu::TextureViewDescriptor::default());
        let mut encoder = gpu.device.create_command_encoder(&wgpu::CommandEncoderDescriptor::default());
        {
            let mut rpass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("replay"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &view, resolve_target: None,
                    ops: wgpu::Operations { load: wgpu::LoadOp::Clear(wgpu::Color::BLACK), store: wgpu::StoreOp::Store },
                })],
                depth_stencil_attachment: None, timestamp_writes: None, occlusion_query_set: None,
            });
            self.render.quad.draw(&mut rpass, quad_buf_idx, quad_count);
            self.render.text.draw(&mut rpass, glyph_count);
        }
        gpu.queue.submit([encoder.finish()]);
        self.render.end_frame(output);
    }
}
