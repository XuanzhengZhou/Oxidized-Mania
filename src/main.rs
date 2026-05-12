use std::path::Path;
use std::sync::Arc;
use winit::event::{ElementState, KeyEvent, MouseButton, WindowEvent};
use winit::event_loop::{ActiveEventLoop, ControlFlow, EventLoop};
use winit::keyboard::{KeyCode, PhysicalKey};
use winit::window::{Window, WindowAttributes};

#[cfg(feature = "dhat-heap")]
#[global_allocator]
static ALLOC: dhat::Alloc = dhat::Alloc;

mod app; mod audio; mod beatmap; mod beatmap_cache; mod config; mod game; mod history;
mod menu; mod pp; mod render; mod replay; mod replay_viewer; mod skin; mod sonic; mod ui;

// ─── 鼠标输入状态 ───

#[derive(Debug, Clone, Default)]
pub struct MouseState {
    pub x: f32, pub y: f32,         // 逻辑坐标
    pub left: bool,                  // 持续按住
    pub left_just: bool,             // 本帧刚按下
    pub left_released: bool,         // 本帧刚松开
    pub wheel: f32,                  // 本帧滚轮增量
}

#[derive(Debug, Clone, Default)]
pub struct InputState {
    pub mouse: MouseState,
    pub keys_just: Vec<KeyCode>,     // 本帧刚按下的键
    pub screen_w: f32, pub screen_h: f32,  // 逻辑屏幕尺寸
}

impl InputState {
    fn begin_frame(&mut self) {
        self.mouse.left_just = false;
        self.mouse.left_released = false;
        self.mouse.wheel = 0.0;
        self.keys_just.clear();
    }
    pub fn hover(&self, x: f32, y: f32, w: f32, h: f32) -> bool {
        self.mouse.x >= x && self.mouse.x <= x + w && self.mouse.y >= y && self.mouse.y <= y + h
    }
    pub fn clicked(&self, x: f32, y: f32, w: f32, h: f32) -> bool {
        self.mouse.left_just && self.hover(x, y, w, h)
    }
}

use audio::bass::BassAudio;
use beatmap::load_beatmap_rox;
use config::GameConfig;
use game::engine::GameEngine;
use game::notes::{SCREEN_H, screen_w};
use game::results::{render_results, GameResult, standardized_score};
use game::NoteRT;
use menu::{load_songs, SongEntry};
use menu::song_select::{self, SongSelectState, FolderMeta, build_folder_meta};
use render::context::RenderCtx;
use skin::CpuSkin;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum MainMenuTab { Play = 0, Settings = 1, Edit = 2, Browse = 3, Exit = 4 }
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum PlayModeTab { Back = 0, Solo = 1, Empty2 = 2, Empty3 = 3, Empty4 = 4 }

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum SettingsFrom { MainMenu, SongSelect }

type AdjustFn = fn(&mut GameConfig, f64);

struct Adjuster {
    label: &'static str,
    value: f64,
    min: f64,
    max: f64,
    step: f64,
    setter: AdjustFn,
}

enum AppState {
    Splash { cover_regions: Vec<skin::AtlasRegion>, cover_idx: usize, cycle_start: std::time::Instant },
    MainMenu { tab: MainMenuTab, cover_regions: Vec<skin::AtlasRegion>, cover_idx: usize, cycle_start: std::time::Instant, config: GameConfig },
    PlayMode { tab: PlayModeTab, cover_regions: Vec<skin::AtlasRegion>, cover_idx: usize, cycle_start: std::time::Instant, config: GameConfig },
    SongSelect { state: SongSelectState, config: GameConfig },
    Settings { primary: usize, secondary: usize, binding_idx: Option<usize>, adjuster: Option<Adjuster>, cover_regions: Vec<skin::AtlasRegion>, cover_idx: usize, cycle_start: std::time::Instant, config: GameConfig, from: SettingsFrom },
    Preview { song: SongEntry, diff: usize, name: String, dur: String, notes: usize, stars: f64, config: GameConfig },
    Gameplay { engine: GameEngine, replay_data: Option<crate::replay::ReplayData>, config: GameConfig },
    ReplayList { state: menu::replay_list::ReplayListState },
    ReplayPlayback { engine: crate::replay_viewer::ReplayEngine, replay: crate::replay::ReplayData, config: GameConfig },
    Results {
        result: GameResult,
        replay: Option<crate::replay::ReplayData>,
        config: GameConfig,
        cover_region: Option<skin::AtlasRegion>,
        offsets: Vec<crate::game::results::HitOffset>,
        page: u32,
        chart_view_start: f64,
        chart_view_end: f64,
        chart_n_sec: f64,
        chart_adjust_n: bool,
    },
    ExitConfirm,
}

fn main() {
    #[cfg(feature = "dhat-heap")]
    let _profiler = dhat::Profiler::new_heap(); // 程序退出时自动写入 dhat-heap.json
    env_logger::init();
    let event_loop = EventLoop::new().unwrap();
    event_loop.set_control_flow(ControlFlow::Poll);
    let config = GameConfig::load("config.json");
    let attrs = WindowAttributes::default().with_title("Oxidized Mania")
        .with_fullscreen(Some(winit::window::Fullscreen::Borderless(None)));
    let window = Arc::new(event_loop.create_window(attrs).unwrap());
    let songs = load_songs();
    let folders_cache = build_folder_meta(&songs, &config);
    // 从缓存提取曲绘路径（避免重复解析 87 个 JSON）
    let mut cover_paths: Vec<String> = Vec::new();
    for f in &folders_cache {
        for d in &f.diffs {
            if let Some(ref cp) = d.cover_path {
                if !cover_paths.contains(cp) { cover_paths.push(cp.clone()); }
            }
        }
    }
    let extra_chars = menu::collect_ui_chars(&songs);
    event_loop.run_app(&mut App { state: AppState::Splash { cover_regions: vec![], cover_idx: 0, cycle_start: std::time::Instant::now() - std::time::Duration::from_secs(10) }, window, render: None, config, input: InputState::default(), extra_chars, pending_cover: None, cover_paths, folders_cache, shift_held: false, prev_folder_idx: 0, prev_diff_idx: 0 }).unwrap();
}

struct App { state: AppState, window: Arc<Window>, render: Option<RenderCtx>, config: GameConfig, input: InputState, extra_chars: Vec<char>, pending_cover: Option<String>, cover_paths: Vec<String>, folders_cache: Vec<FolderMeta>, shift_held: bool, prev_folder_idx: usize, prev_diff_idx: usize }

impl App {
    fn init_render(&mut self) {
        if self.render.is_none() {
            match &self.state {
                AppState::Splash { .. } | AppState::MainMenu { .. } | AppState::PlayMode { .. } | AppState::Settings { .. } => {
                    let rng = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_micros() as usize).unwrap_or(0);
                    let path = self.cover_paths.get(rng % self.cover_paths.len().max(1)).map(|p| std::path::Path::new(p.as_str()));
                    let cpu = CpuSkin::load_menu_bg(path);
                    let render = pollster::block_on(RenderCtx::new(self.window.clone(), cpu, &self.extra_chars));
                    let region = render.skin_regions().get("bg_cover").cloned();
                    match &mut self.state {
                        AppState::Splash { ref mut cover_regions, .. }
                        | AppState::MainMenu { ref mut cover_regions, .. }
                        | AppState::PlayMode { ref mut cover_regions, .. }
                        | AppState::Settings { ref mut cover_regions, .. } => {
                            *cover_regions = region.into_iter().collect();
                        }
                        _ => {}
                    }
                    self.render = Some(render);
                }
                AppState::SongSelect { ref state, .. } => {
                    let cover = state.current_diff().cover_path.as_deref();
                    log::info!("[Cover] init_render cover={:?}", cover);
                    let cpu = CpuSkin::load(&self.config.active_skin, cover.map(std::path::Path::new));
                    let render = pollster::block_on(RenderCtx::new(self.window.clone(), cpu, &self.extra_chars));
                    if let AppState::SongSelect { ref mut state, .. } = self.state {
                        if state.cover_region.is_none() {
                            state.cover_region = render.skin_regions().get("bg_cover").cloned();
                        }
                    }
                    self.render = Some(render);
                }
                AppState::Results { ref result, .. } => {
                    let cp = result.cover_path.as_deref();
                    let cpu = CpuSkin::load(&self.config.active_skin, cp.map(std::path::Path::new));
                    self.render = Some(pollster::block_on(RenderCtx::new(self.window.clone(), cpu, &self.extra_chars)));
                }
                _ => {
                    let cpu = CpuSkin::load(&self.config.active_skin, None);
                    self.render = Some(pollster::block_on(RenderCtx::new(self.window.clone(), cpu, &self.extra_chars)));
                }
            }
            // 提取 Results 的曲绘区域
            if let AppState::Results { ref mut cover_region, .. } = self.state {
                if cover_region.is_none() {
                    if let Some(ref r) = self.render {
                        *cover_region = r.skin_regions().get("bg_cover").cloned();
                    }
                }
            }
        }
    }
    fn update_cover(&mut self, cover_path: Option<&str>) {
        let cpu = CpuSkin::load(&self.config.active_skin, cover_path.map(|p| std::path::Path::new(p)));
        self.render = Some(pollster::block_on(RenderCtx::new(self.window.clone(), cpu, &self.extra_chars)));
        // 提取新 atlas 中的 bg_cover 区域
        if let Some(ref r) = self.render {
            let region = r.skin_regions().get("bg_cover").cloned();
            if let AppState::SongSelect { ref mut state, .. } = self.state {
                state.cover_region = region;
            }
        }
    }
    fn submit(&mut self) {
        let r = self.render.as_mut().unwrap();
        r.quad.upload(&r.queue); let gc = r.text.upload(&r.queue);
        if let Ok(o) = r.begin_frame() {
            let v = o.texture.create_view(&wgpu::TextureViewDescriptor::default());
            let mut e = r.device.create_command_encoder(&wgpu::CommandEncoderDescriptor::default());
            { let mut rp = e.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: None, color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &v, resolve_target: None,
                    ops: wgpu::Operations { load: wgpu::LoadOp::Clear(wgpu::Color::BLACK), store: wgpu::StoreOp::Store },
                })], depth_stencil_attachment: None, timestamp_writes: None, occlusion_query_set: None,
            });
            r.quad.draw(&mut rp, r.quad.last_buffer(), r.quad.instances.len());
            r.text.draw(&mut rp, gc); }
            r.queue.submit([e.finish()]); r.end_frame(o);
        }
    }
}

impl winit::application::ApplicationHandler for App {
    fn resumed(&mut self, _: &ActiveEventLoop) {}
    fn window_event(&mut self, el: &ActiveEventLoop, _: winit::window::WindowId, ev: WindowEvent) {
        match ev {
            WindowEvent::CloseRequested => el.exit(),
            WindowEvent::KeyboardInput { event: KeyEvent { physical_key: PhysicalKey::Code(k), state, repeat: false, .. }, .. } => {
                if k == KeyCode::ShiftLeft || k == KeyCode::ShiftRight {
                    self.shift_held = state == ElementState::Pressed;
                }
                if state == ElementState::Pressed { self.input.keys_just.push(k); }
                self.on_key(el, k, state == ElementState::Pressed);
            }
            WindowEvent::CursorMoved { position, .. } => {
                let sz = self.window.inner_size();
                let lw = self.render.as_ref().map_or(800.0, |r| r.logical_w as f32);
                let lh = self.render.as_ref().map_or(600.0, |r| r.logical_h as f32);
                self.input.mouse.x = (position.x / sz.width as f64) as f32 * lw;
                self.input.mouse.y = (position.y / sz.height as f64) as f32 * lh;
            }
            WindowEvent::MouseInput { state, button: MouseButton::Left, .. } => {
                self.input.mouse.left = state == ElementState::Pressed;
                if state == ElementState::Pressed { self.input.mouse.left_just = true; }
                else { self.input.mouse.left_released = true; }
            }
            WindowEvent::MouseWheel { delta, .. } => {
                self.input.mouse.wheel = match delta {
                    winit::event::MouseScrollDelta::LineDelta(_, y) => y * 30.0,
                    winit::event::MouseScrollDelta::PixelDelta(p) => p.y as f32,
                };
            }
            WindowEvent::RedrawRequested => self.on_redraw(),
            _ => {}
        }
    }
    fn about_to_wait(&mut self, _: &ActiveEventLoop) { self.window.request_redraw(); }
}

impl App {
    fn on_redraw(&mut self) {
        self.input.begin_frame();
        if let AppState::Gameplay { engine, .. } = &mut self.state { engine.render_frame(); return; }
        // ReplayPlayback: render + 3s auto-advance
        let mut replay_transition = false;
        if let AppState::ReplayPlayback { engine, .. } = &mut self.state {
            engine.render_frame();
            replay_transition = engine.finished;
        }
        if replay_transition {
            if let AppState::ReplayPlayback { replay, config, .. } = &mut self.state {
                let r = replay.clone();
                let acc = r.acc;
                let stars_val = crate::pp::calculate_stars(&r.map_path, r.song_rate);
                let rank = crate::ui::theme::rank_from_acc(acc, r.counts.good, r.counts.ok, r.counts.meh, r.counts.miss).to_string();
                let tobj = r.total_notes;
                let cp = if tobj > 0 { r.max_combo as f64 / tobj as f64 } else { 0.0 };
                let standard = standardized_score(acc / 100.0, cp);
                let pp_val = crate::pp::calculate_pp(&r.map_path, r.song_rate, acc, r.counts.miss, r.max_combo);
                let cover_path = std::fs::read_to_string(&r.map_path).ok()
                    .and_then(|s| serde_json::from_str::<serde_json::Value>(&s).ok())
                    .and_then(|v| v["meta"]["bg"].as_str().map(|bg| {
                        std::path::Path::new(&r.map_path).parent().unwrap().join(bg).to_string_lossy().to_string()
                    }));
                let result = GameResult {
                    score: r.score, standardized_score: standard, acc, max_combo: r.max_combo,
                    perfect_count: r.counts.perfect, great_count: r.counts.great,
                    good_count: r.counts.good, ok_count: r.counts.ok,
                    meh_count: r.counts.meh, miss_count: r.counts.miss,
                    total_notes: r.total_notes, total_objects: tobj,
                    song_name: r.map_path.clone(), map_path: r.map_path.clone(),
                    song_rate: r.song_rate, od: r.od, mirror_mode: r.mirror_mode,
                    rank, stars: stars_val, pp: pp_val, cover_path,
                };
                self.render = None;
                let offsets = crate::game::results::compute_hit_offsets(&r, &r.map_path);
                self.state = AppState::Results {
                    result, replay: Some(r), config: config.clone(), cover_region: None, offsets,
                    page: 0, chart_view_start: 0.0, chart_view_end: 1.0,
                    chart_n_sec: 0.5, chart_adjust_n: false,
                };
            }
            return;
        }
        if matches!(self.state, AppState::ReplayPlayback { .. }) { return; }
        // 封面循环：仅 Splash 每30秒换封面（菜单不循环以减少皮肤重载）
        if let AppState::Splash { ref mut cycle_start, .. } = &mut self.state
        {
            if cycle_start.elapsed().as_secs() >= 30 {
                let r = std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .map(|d| d.as_micros() as usize)
                    .unwrap_or(0);
                if let Some(cp) = self.cover_paths.get(r % self.cover_paths.len().max(1)) {
                    self.pending_cover = Some(cp.clone());
                }
                *cycle_start = std::time::Instant::now();
            }
        }
        // 鼠标点击处理
        if self.input.mouse.left_just {
            let mx = self.input.mouse.x;
            let my = self.input.mouse.y;
            let w = crate::game::notes::screen_w();
            let tab_y0 = SCREEN_H * 2.0 / 5.0;
            let tab_y1 = SCREEN_H * 3.0 / 5.0;
            let in_tabs = my >= tab_y0 && my <= tab_y1;
            match &mut self.state {
                AppState::Splash { .. } => {
                    let empty = vec![];
                    self.render = None;
                    self.state = AppState::MainMenu { tab: MainMenuTab::Play, cover_regions: empty, cover_idx: 0, cycle_start: std::time::Instant::now() - std::time::Duration::from_secs(10), config: self.config.clone() };
                    return;
                }
                AppState::MainMenu { config, .. } => {
                    let circle_r = w / 3.0 / 2.0;
                    let circle_cx = circle_r + w * 0.1;
                    let circle_cy = SCREEN_H / 2.0;
                    let dist = ((mx - circle_cx).powi(2) + (my - circle_cy).powi(2)).sqrt();
                    let clicked_circle = dist <= circle_r + 3.0;
                    let hov = if in_tabs { crate::ui::hovered_tab(mx, 5, circle_cx, circle_r) } else { None };
                    let clicked_tab = if clicked_circle { Some(1) } else { hov };
                    if let Some(idx) = clicked_tab {
                        match idx {
                            0 => { self.render = None; self.state = AppState::Settings { primary: 0, secondary: 0, binding_idx: None, adjuster: None, cover_regions: vec![], cover_idx: 0, cycle_start: std::time::Instant::now() - std::time::Duration::from_secs(10), config: config.clone(), from: SettingsFrom::MainMenu }; return; }
                            1 => { let empty = vec![]; self.render = None; self.state = AppState::PlayMode { tab: PlayModeTab::Solo, cover_regions: empty, cover_idx: 0, cycle_start: std::time::Instant::now() - std::time::Duration::from_secs(10), config: config.clone() }; return; }
                            4 => { self.state = AppState::ExitConfirm; return; }
                            _ => {}
                        }
                    }
                }
                AppState::PlayMode { config, .. } => {
                    let circle_r = w / 3.0 / 2.0;
                    let circle_cx = circle_r + w * 0.1;
                    let circle_cy = SCREEN_H / 2.0;
                    let dist = ((mx - circle_cx).powi(2) + (my - circle_cy).powi(2)).sqrt();
                    let clicked_circle = dist <= circle_r + 3.0;
                    let hov = if in_tabs { crate::ui::hovered_tab(mx, 5, circle_cx, circle_r) } else { None };
                    let clicked_tab = if clicked_circle { Some(1) } else { hov };
                    if let Some(idx) = clicked_tab {
                        match idx {
                            0 => { let empty = vec![]; self.render = None; self.state = AppState::MainMenu { tab: MainMenuTab::Play, cover_regions: empty, cover_idx: 0, cycle_start: std::time::Instant::now() - std::time::Duration::from_secs(10), config: config.clone() }; return; }
                            1 => { let state = SongSelectState::new(self.folders_cache.clone(), config.clone()); self.render = None; self.state = AppState::SongSelect { state, config: config.clone() }; return; }
                            _ => {}
                        }
                    }
                }
                _ => {}
            }
        }

        self.init_render();
        let r = self.render.as_mut().unwrap();

        // 提取 osu! logo 区域 (必须在 mut borrow 之前)
        let logo_region = r.skin_regions().get("osu_logo").cloned();
        // 结算页面曲绘：直接从 render context 获取（避免首帧延迟）
        let results_cover: Option<skin::AtlasRegion> =
            r.skin_regions().get("bg_cover").cloned();

        r.quad.clear(); r.text.clear();
        let (q, t) = (&mut r.quad, &mut r.text);

        // 鼠标悬停检测
        let mx = self.input.mouse.x;
        let w2 = screen_w();
        let circle_r_h = w2 / 3.0 / 2.0;
        let circle_cx_h = circle_r_h + w2 * 0.1;
        let hovered = crate::ui::hovered_tab(mx, 5, circle_cx_h, circle_r_h);

        let logo = logo_region.as_ref();

        match &self.state {
            AppState::Splash { ref cover_regions, ref cover_idx, .. } => {
                let region = cover_regions.get(*cover_idx);
                menu::splash::render(q, t, region, logo);
            }
            AppState::MainMenu { ref tab, ref cover_regions, ref cover_idx, .. } => {
                let region = cover_regions.get(*cover_idx);
                menu::main_menu::render(q, t, region, *tab as usize, hovered, logo);
            }
            AppState::PlayMode { ref tab, ref cover_regions, ref cover_idx, .. } => {
                let region = cover_regions.get(*cover_idx);
                menu::play_mode::render(q, t, region, *tab as usize, hovered, logo);
            }
            AppState::SongSelect { state, config } => {
                song_select::render(q, t, state, config);
            }
            AppState::Settings { primary, secondary, binding_idx, adjuster, ref cover_regions, ref cover_idx, config, .. } => {
                let region = cover_regions.get(*cover_idx);
                menu::settings::render_settings(q, t, region, *primary, *secondary, *binding_idx, adjuster.as_ref(), config);
            }
            AppState::Preview { song, diff, name, dur, notes, stars, config } => menu::preview::render_preview(q, t, song, *diff, name, *stars, dur, *notes, config.song_rate, None),
            AppState::ReplayList { state } => menu::replay_list::render(state, q, t),
            AppState::ExitConfirm => menu::exit::render(q, t),
            AppState::Results { result, offsets, page, chart_view_start, chart_view_end, chart_n_sec, chart_adjust_n, .. } => {
                render_results(result, offsets, *page, *chart_view_start, *chart_view_end, *chart_n_sec, q, t, results_cover.as_ref());
                // N 调节弹窗
                if *chart_adjust_n {
                    let sw = screen_w();
                    let mw = 260.0; let mh = 120.0;
                    let mx = sw / 2.0 - mw / 2.0;
                    let my = SCREEN_H / 2.0 - mh / 2.0;
                    q.push_rect(mx, my, mw, mh, [30, 30, 50, 240]);
                    q.push_rect(mx, my, mw, 2.0, [100, 100, 180, 255]);
                    q.push_rect(mx, my + mh - 2.0, mw, 2.0, [100, 100, 180, 255]);
                    let title = format!("N 步长: {:.1}s", chart_n_sec);
                    t.queue_text(&title, mx + mw/2.0 - title.len() as f32*5.0, my + 28.0, 14.0, [255,255,255,255]);
                    let hint1 = "[A/D] 调节步长  [Shift] 加速";
                    t.queue_text(hint1, mx + mw/2.0 - hint1.len() as f32*5.0, my + 55.0, 10.0, [180,180,200,255]);
                    let hint2 = "[X/Enter/Esc] 确认";
                    t.queue_text(hint2, mx + mw/2.0 - hint2.len() as f32*5.0, my + 75.0, 10.0, [180,180,200,255]);
                }
            }
            _ => {}
        }
        self.submit();
        // 延迟加载曲绘
        if let Some(ref cp) = self.pending_cover.take() {
            log::info!("[Cover] loading cover from: {}", cp);
            let is_menu = matches!(self.state, AppState::Splash { .. } | AppState::MainMenu { .. } | AppState::PlayMode { .. } | AppState::Settings { .. });
            let cpu = if is_menu {
                CpuSkin::load_menu_bg(Some(std::path::Path::new(cp)))
            } else {
                CpuSkin::load(&self.config.active_skin, Some(std::path::Path::new(cp)))
            };
            let render = pollster::block_on(RenderCtx::new(self.window.clone(), cpu, &self.extra_chars));
            let region = render.skin_regions().get("bg_cover").cloned();
            match &mut self.state {
                AppState::Splash { ref mut cover_regions, .. }
                | AppState::MainMenu { ref mut cover_regions, .. }
                | AppState::PlayMode { ref mut cover_regions, .. }
                | AppState::Settings { ref mut cover_regions, .. } => {
                    *cover_regions = region.into_iter().collect();
                }
                AppState::SongSelect { ref mut state, .. } => {
                    state.cover_region = region;
                }
                _ => {}
            }
            self.render = Some(render);
        }
    }

    fn on_key(&mut self, el: &ActiveEventLoop, key: KeyCode, pressed: bool) {
        let mut next: Option<AppState> = None;
        if let AppState::Gameplay { engine, replay_data, config } = &mut self.state {
            let exit = engine.handle_key(key, pressed);
            // 同步引擎内的 replay_data 到 AppState
            if replay_data.is_none() { *replay_data = engine.replay_data.take(); }
            if exit {
                self.render = None;
                if replay_data.is_none() { *replay_data = engine.replay_data.take(); }
                let rd = replay_data.take();
                // 正常结束（有 GameResult）→ 结算；暂停退出 → 返回选歌
                if let Some(result) = engine.take_result() {
                    let offsets = rd.as_ref().map(|r| crate::game::results::compute_hit_offsets(r, &result.map_path)).unwrap_or_default();
                    next = Some(AppState::Results {
                        result, replay: rd, config: config.clone(), cover_region: None, offsets,
                        page: 0, chart_view_start: 0.0, chart_view_end: 1.0,
                        chart_n_sec: 0.5, chart_adjust_n: false,
                    });
                } else {
                    let target = song_select::scroll_target_for(self.prev_folder_idx, self.prev_diff_idx);
                    let state = SongSelectState { folder_idx: self.prev_folder_idx, diff_idx: self.prev_diff_idx, scroll_y: target, target_scroll_y: target, ..SongSelectState::new(self.folders_cache.clone(), config.clone()) };
                    next = Some(AppState::SongSelect { state, config: config.clone() });
                }
            }
            let to_results = matches!(next, Some(AppState::Results { .. }));
            if let Some(s) = next { self.state = s; }
            if to_results { self.window.request_redraw(); }
            return;
        }
        if !pressed { return; }
        match &mut self.state {
            AppState::Splash { .. } => match key {
                KeyCode::Escape => next = Some(AppState::ExitConfirm),
                _ => {
                    let empty = vec![];
                    self.render = None;
                    next = Some(AppState::MainMenu { tab: MainMenuTab::Play, cover_regions: empty, cover_idx: 0, cycle_start: std::time::Instant::now() - std::time::Duration::from_secs(10), config: self.config.clone() });
                }
            },
            AppState::MainMenu { ref mut tab, ref config, .. } => match key {
                KeyCode::Escape => { self.render = None; next = Some(AppState::Splash { cover_regions: vec![], cover_idx: 0, cycle_start: std::time::Instant::now() - std::time::Duration::from_secs(10) }); }
                KeyCode::ArrowLeft => *tab = match *tab { MainMenuTab::Settings => MainMenuTab::Exit, MainMenuTab::Play => MainMenuTab::Settings, MainMenuTab::Edit => MainMenuTab::Play, MainMenuTab::Browse => MainMenuTab::Edit, MainMenuTab::Exit => MainMenuTab::Browse },
                KeyCode::ArrowRight => *tab = match *tab { MainMenuTab::Settings => MainMenuTab::Play, MainMenuTab::Play => MainMenuTab::Edit, MainMenuTab::Edit => MainMenuTab::Browse, MainMenuTab::Browse => MainMenuTab::Exit, MainMenuTab::Exit => MainMenuTab::Settings },
                KeyCode::Enter => match *tab {
                    MainMenuTab::Play => {
                        let empty = vec![];
                        self.render = None;
                        next = Some(AppState::PlayMode { tab: PlayModeTab::Solo, cover_regions: empty, cover_idx: 0, cycle_start: std::time::Instant::now() - std::time::Duration::from_secs(10), config: config.clone() });
                    }
                    MainMenuTab::Settings => { self.render = None; next = Some(AppState::Settings { primary: 0, secondary: 0, binding_idx: None, adjuster: None, cover_regions: vec![], cover_idx: 0, cycle_start: std::time::Instant::now() - std::time::Duration::from_secs(10), config: config.clone(), from: SettingsFrom::MainMenu }); }
                    MainMenuTab::Exit => next = Some(AppState::ExitConfirm),
                    _ => {}
                },
                _ => {}
            },
            AppState::PlayMode { ref mut tab, ref config, .. } => match key {
                KeyCode::Escape => {
                    let empty = vec![];
                    self.render = None;
                    next = Some(AppState::MainMenu { tab: MainMenuTab::Play, cover_regions: empty, cover_idx: 0, cycle_start: std::time::Instant::now() - std::time::Duration::from_secs(10), config: config.clone() });
                }
                KeyCode::ArrowLeft => *tab = match *tab { PlayModeTab::Back => PlayModeTab::Empty4, PlayModeTab::Solo => PlayModeTab::Back, PlayModeTab::Empty2 => PlayModeTab::Solo, PlayModeTab::Empty3 => PlayModeTab::Empty2, PlayModeTab::Empty4 => PlayModeTab::Empty3 },
                KeyCode::ArrowRight => *tab = match *tab { PlayModeTab::Back => PlayModeTab::Solo, PlayModeTab::Solo => PlayModeTab::Empty2, PlayModeTab::Empty2 => PlayModeTab::Empty3, PlayModeTab::Empty3 => PlayModeTab::Empty4, PlayModeTab::Empty4 => PlayModeTab::Back },
                KeyCode::Enter => match *tab {
                    PlayModeTab::Back => {
                        let empty = vec![];
                        self.render = None;
                        next = Some(AppState::MainMenu { tab: MainMenuTab::Play, cover_regions: empty, cover_idx: 0, cycle_start: std::time::Instant::now() - std::time::Duration::from_secs(10), config: config.clone() });
                    }
                    PlayModeTab::Solo => {
                        let state = SongSelectState::new(self.folders_cache.clone(), config.clone());
                        self.render = None;
                        next = Some(AppState::SongSelect { state, config: config.clone() });
                    }
                    _ => {}
                },
                _ => {}
            },
            AppState::SongSelect { ref mut state, ref mut config } => match key {
                KeyCode::ArrowUp => {
                    let prev_cover = state.current_diff().cover_path.clone();
                    if state.diff_idx > 0 { state.diff_idx -= 1; }
                    else if state.folder_idx > 0 {
                        state.folder_idx -= 1;
                        state.diff_idx = state.current_folder().diffs.len().saturating_sub(1);
                        let cp = state.current_diff().cover_path.clone();
                        self.pending_cover = cp;
                        state.target_scroll_y = compute_target_scroll(state);
                        state.scroll_y = state.target_scroll_y;
                        return;
                    }
                    let new_cover = state.current_diff().cover_path.clone();
                    if prev_cover != new_cover { self.pending_cover = new_cover; }
                    state.target_scroll_y = compute_target_scroll(state);
                    state.scroll_y = state.target_scroll_y;
                }
                KeyCode::ArrowDown => {
                    let prev_cover = state.current_diff().cover_path.clone();
                    let max_d = state.current_folder().diffs.len().saturating_sub(1);
                    if state.diff_idx < max_d { state.diff_idx += 1; }
                    else if state.folder_idx + 1 < state.folders.len() {
                        state.folder_idx += 1;
                        state.diff_idx = 0;
                        let cp = state.current_diff().cover_path.clone();
                        self.pending_cover = cp;
                        state.target_scroll_y = compute_target_scroll(state);
                        state.scroll_y = state.target_scroll_y;
                        return;
                    }
                    let new_cover = state.current_diff().cover_path.clone();
                    if prev_cover != new_cover { self.pending_cover = new_cover; }
                    state.target_scroll_y = compute_target_scroll(state);
                    state.scroll_y = state.target_scroll_y;
                }
                KeyCode::ArrowLeft => if state.folder_idx > 0 {
                    state.folder_idx -= 1; state.diff_idx = 0;
                    state.target_scroll_y = compute_target_scroll(state);
                    state.scroll_y = state.target_scroll_y;
                    let cp = state.current_diff().cover_path.clone();
                    self.pending_cover = cp;
                    return;
                }
                KeyCode::ArrowRight => if state.folder_idx + 1 < state.folders.len() {
                    state.folder_idx += 1; state.diff_idx = 0;
                    state.target_scroll_y = compute_target_scroll(state);
                    state.scroll_y = state.target_scroll_y;
                    let cp = state.current_diff().cover_path.clone();
                    self.pending_cover = cp;
                    return;
                }
                KeyCode::Enter => {
                    self.prev_folder_idx = state.folder_idx;
                    self.prev_diff_idx = state.diff_idx;
                    let folder = state.current_folder();
                    let diff_idx = state.diff_idx.min(folder.jsons.len().saturating_sub(1));
                    let json = folder.jsons[diff_idx].clone();
                    if let Ok((meta, bnotes)) = load_beatmap_rox(&json) {
                        let sname = meta.song.clone();
                        let rnotes: Vec<NoteRT> = bnotes.into_iter().map(|n| NoteRT { time: n.time, end_time: n.end_time, lane: n.lane, note_type: n.note_type, hit: false, missed: false, holding: false, ghost: false, stuck_y: None, release_time: None }).collect();
                        let mut audio = BassAudio::init().unwrap();
                        let ap = if (config.song_rate - 1.0).abs() > 0.001 {
                            let t = format!(".temp_{}x_{}", config.song_rate, Path::new(&sname).file_name().unwrap().to_string_lossy());
                            let tp = Path::new(&json).parent().unwrap().join(Path::new(&t).with_extension("wav"));
                            let ts = tp.to_string_lossy().to_string();
                            if !tp.exists() { sonic::generate_stretched_audio(&sname, &ts, config.song_rate as f32, Some(&audio)); drop(audio); audio = BassAudio::init().unwrap(); }
                            ts
                        } else { sname.clone() };
                        audio.load(&ap).expect("audio");
                        let cover = meta.bg.as_ref().map(|bg| Path::new(&json).parent().unwrap().join(bg));
                        let cpu = CpuSkin::load(&config.active_skin, cover.as_deref());
                        let sc = cpu.config.clone();
                        let render = pollster::block_on(RenderCtx::new(self.window.clone(), cpu, &self.extra_chars));
                        let tdur = rnotes.iter().map(|n| n.end_time.max(n.time)).fold(1.0, f64::max);
                        let engine = GameEngine::new(audio, config.clone(), sname, json, rnotes, tdur, meta.offset, config.mirror_mode, cover.as_ref().map(|p| p.to_string_lossy().to_string()), Some(render.skin_regions()), Some(sc), self.window.clone(), render);
                        next = Some(AppState::Gameplay { engine, replay_data: None, config: config.clone() });
                    }
                }
                KeyCode::KeyS => { self.render = None; next = Some(AppState::Settings { primary: 0, secondary: 0, binding_idx: None, adjuster: None, cover_regions: vec![], cover_idx: 0, cycle_start: std::time::Instant::now() - std::time::Duration::from_secs(10), config: config.clone(), from: SettingsFrom::SongSelect }); }
                KeyCode::Escape => { let empty = vec![]; self.render = None; next = Some(AppState::MainMenu { tab: MainMenuTab::Play, cover_regions: empty, cover_idx: 0, cycle_start: std::time::Instant::now() - std::time::Duration::from_secs(10), config: config.clone() }); }
                KeyCode::Comma => { config.scroll_speed = (config.scroll_speed - 0.5).max(5.0); }
                KeyCode::Period => { config.scroll_speed += 0.5; }
                KeyCode::KeyA => config.global_offset -= 5.0,
                KeyCode::KeyD => config.global_offset += 5.0,
                KeyCode::KeyW => config.song_rate = (config.song_rate - 0.1).max(0.5),
                KeyCode::KeyE => config.song_rate = (config.song_rate + 0.1).min(2.0),
                KeyCode::KeyF => config.fullscreen = !config.fullscreen,
                KeyCode::KeyR => {
                    let map_path = state.current_folder().jsons[state.diff_idx].clone();
                    let entries: Vec<_> = crate::replay::list_replays(&map_path).into_iter()
                        .filter_map(|p| crate::replay::ReplayData::load(&p).ok().map(|d| (p, d)))
                        .collect();
                    next = Some(AppState::ReplayList { state: menu::replay_list::ReplayListState { map_path, entries, selected: 0, config: config.clone() } });
                }
                _ => {}
            }
            AppState::ReplayList { ref mut state } => match key {
                KeyCode::Escape | KeyCode::KeyR => {
                    let cfg = state.config.clone();
                    let new_state = SongSelectState::new(self.folders_cache.clone(), cfg.clone());
                    self.render = None;
                    next = Some(AppState::SongSelect { state: new_state, config: cfg });
                }
                KeyCode::ArrowUp | KeyCode::KeyW => if state.selected > 0 { state.selected -= 1; }
                KeyCode::ArrowDown | KeyCode::KeyS => if state.selected + 1 < state.entries.len() { state.selected += 1; }
                KeyCode::Enter => {
                    if state.entries.is_empty() { return; }
                    let (ref _rpath, ref rdata) = state.entries[state.selected];
                    let config = state.config.clone();
                    if let Ok((meta, _bnotes)) = load_beatmap_rox(&rdata.map_path) {
                        let sname = meta.song.clone();
                        let mut audio = BassAudio::init().unwrap();
                        let ap = if (rdata.song_rate - 1.0).abs() > 0.001 {
                            let t = format!(".temp_{}x_{}", rdata.song_rate, Path::new(&sname).file_name().unwrap().to_string_lossy());
                            let tp = Path::new(&rdata.map_path).parent().unwrap().join(Path::new(&t).with_extension("wav"));
                            let ts = tp.to_string_lossy().to_string();
                            if !tp.exists() { sonic::generate_stretched_audio(&sname, &ts, rdata.song_rate as f32, Some(&audio)); drop(audio); audio = BassAudio::init().unwrap(); }
                            ts
                        } else { sname.clone() };
                        let _ = audio.load(&ap);
                        let cover = meta.bg.as_ref().map(|bg| Path::new(&rdata.map_path).parent().unwrap().join(bg));
                        let cpu = CpuSkin::load(&config.active_skin, cover.as_deref());
                        let sc = cpu.config.clone();
                        let render = pollster::block_on(RenderCtx::new(self.window.clone(), cpu, &self.extra_chars));
                        drop(audio); // 释放 BASS 让 ReplayEngine 自己初始化
                        match crate::replay_viewer::ReplayEngine::new(
                            rdata.clone(), &ap, &rdata.map_path,
                            Some(render.skin_regions().clone()), Some(sc),
                            self.window.clone(), render,
                        ) {
                            Ok(engine) => {
                                self.render = None;
                                next = Some(AppState::ReplayPlayback { engine, replay: rdata.clone(), config });
                            }
                            Err(e) => log::error!("ReplayEngine: {e}"),
                        }
                    }
                }
                _ => {}
            }
            AppState::ReplayPlayback { engine, replay, config } => {
                if engine.finished && key == KeyCode::Enter {
                    let r = replay;
                    let acc = r.acc;
                    let stars_val = crate::pp::calculate_stars(&r.map_path, r.song_rate);
                    let rank = crate::ui::theme::rank_from_acc(acc, r.counts.good, r.counts.ok, r.counts.meh, r.counts.miss).to_string();
                    let tobj = engine.total_objects();
                    let cp = if tobj > 0 { r.max_combo as f64 / tobj as f64 } else { 0.0 };
                    let standard = standardized_score(acc / 100.0, cp);
                    let pp_val = crate::pp::calculate_pp(&r.map_path, r.song_rate, acc, r.counts.miss, r.max_combo);
                    let cover_path = std::fs::read_to_string(&r.map_path).ok()
                        .and_then(|s| serde_json::from_str::<serde_json::Value>(&s).ok())
                        .and_then(|v| v["meta"]["bg"].as_str().map(|bg| {
                            std::path::Path::new(&r.map_path).parent().unwrap().join(bg).to_string_lossy().to_string()
                        }));
                    let result = GameResult {
                        score: r.score, standardized_score: standard, acc, max_combo: r.max_combo,
                        perfect_count: r.counts.perfect, great_count: r.counts.great,
                        good_count: r.counts.good, ok_count: r.counts.ok,
                        meh_count: r.counts.meh, miss_count: r.counts.miss,
                        total_notes: r.total_notes, total_objects: tobj,
                        song_name: r.map_path.clone(), map_path: r.map_path.clone(),
                        song_rate: r.song_rate, od: r.od, mirror_mode: r.mirror_mode,
                        rank, stars: stars_val, pp: pp_val, cover_path,
                    };
                    let offsets = crate::game::results::compute_hit_offsets(&r, &result.map_path);
                    self.render = None;
                    next = Some(AppState::Results {
                        result, replay: Some(r.clone()), config: config.clone(), cover_region: None, offsets,
                        page: 0, chart_view_start: 0.0, chart_view_end: 1.0,
                        chart_n_sec: 0.5, chart_adjust_n: false,
                    });
                } else if !engine.finished {
                    match key {
                        KeyCode::Escape => {
                            let state = SongSelectState::new(self.folders_cache.clone(), config.clone());
                            self.render = None;
                            next = Some(AppState::SongSelect { state, config: config.clone() });
                        }
                        KeyCode::Space => engine.toggle_pause(),
                        KeyCode::ArrowLeft => engine.seek(engine.current_time() - 5000.0),
                        KeyCode::ArrowRight => engine.seek(engine.current_time() + 5000.0),
                        KeyCode::KeyF => { if pressed { engine.show_fps = !engine.show_fps; } }
                        _ => {}
                    }
                }
            }
            AppState::Settings { primary, secondary, binding_idx, adjuster, config, from, .. } => {
                // 调节器模式
                if let Some(ref mut adj) = adjuster {
                    match key {
                        KeyCode::Escape => { *adjuster = None; config.save("config.json"); }
                        KeyCode::Enter => { (adj.setter)(config, adj.value); *adjuster = None; config.save("config.json"); }
                        KeyCode::ArrowLeft | KeyCode::ArrowDown => adj.value = (adj.value - adj.step).max(adj.min),
                        KeyCode::ArrowRight | KeyCode::ArrowUp => adj.value = (adj.value + adj.step).min(adj.max),
                        _ => {}
                    }
                    return;
                }
                // 键位绑定模式
                if let Some(idx) = *binding_idx {
                    if let Some(n) = key_name(key) { config.key_bindings[idx] = n.to_string(); config.save("config.json"); }
                    *binding_idx = None;
                    return;
                }
                // 正常导航
                match key {
                    KeyCode::Escape => { config.save("config.json"); let empty = vec![]; self.render = None; next = Some(match *from { SettingsFrom::SongSelect => { let state = SongSelectState::new(self.folders_cache.clone(), config.clone()); AppState::SongSelect { state, config: config.clone() } } SettingsFrom::MainMenu => AppState::MainMenu { tab: MainMenuTab::Play, cover_regions: empty, cover_idx: 0, cycle_start: std::time::Instant::now() - std::time::Duration::from_secs(10), config: config.clone() } }); }
                    KeyCode::ArrowLeft => *primary = (*primary + 4) % 5,
                    KeyCode::ArrowRight => *primary = (*primary + 1) % 5,
                    KeyCode::ArrowUp => if *secondary > 0 { *secondary -= 1; },
                    KeyCode::ArrowDown => { let max = settings_secondary_count(*primary); if *secondary + 1 < max { *secondary += 1; } }
                    KeyCode::Enter => settings_activate(primary, secondary, binding_idx, adjuster, config),
                    // 快捷键始终有效
                    KeyCode::Digit1 => *binding_idx = Some(0),
                    KeyCode::Digit2 => *binding_idx = Some(1),
                    KeyCode::Digit3 => *binding_idx = Some(2),
                    KeyCode::Digit4 => *binding_idx = Some(3),
                    _ => { settings_shortcut(config, key); config.save("config.json"); }
                }
            }
            AppState::Preview { song, diff, stars, config, .. } => match key {
                KeyCode::Enter => {
                    let path = song.jsons[*diff].clone();
                    if let Ok((meta, bnotes)) = load_beatmap_rox(&path) {
                        let sname = meta.song.clone();
                        let rnotes: Vec<NoteRT> = bnotes.into_iter().map(|n| NoteRT { time: n.time, end_time: n.end_time, lane: n.lane, note_type: n.note_type, hit: false, missed: false, holding: false, ghost: false, stuck_y: None, release_time: None }).collect();
                        let mut audio = BassAudio::init().unwrap();
                        let ap = if (config.song_rate - 1.0).abs() > 0.001 { let t = format!(".temp_{}x_{}", config.song_rate, Path::new(&sname).file_name().unwrap().to_string_lossy()); let tp = Path::new(&path).parent().unwrap().join(Path::new(&t).with_extension("wav")); let ts = tp.to_string_lossy().to_string(); if !tp.exists() { sonic::generate_stretched_audio(&sname, &ts, config.song_rate as f32, Some(&audio)); drop(audio); audio = BassAudio::init().unwrap(); } ts } else { sname.clone() };
                        audio.load(&ap).expect("audio");
                        let cover = meta.bg.as_ref().map(|bg| Path::new(&path).parent().unwrap().join(bg));
                        let cpu = CpuSkin::load(&config.active_skin, cover.as_deref());
                        let sc = cpu.config.clone();
                        let render = pollster::block_on(RenderCtx::new(self.window.clone(), cpu, &self.extra_chars));
                        let tdur = rnotes.iter().map(|n| n.end_time.max(n.time)).fold(1.0, f64::max);
                        let engine = GameEngine::new(audio, config.clone(), sname, path, rnotes, tdur, meta.offset, config.mirror_mode, cover.as_ref().map(|p| p.to_string_lossy().to_string()), Some(render.skin_regions()), Some(sc), self.window.clone(), render);
                        next = Some(AppState::Gameplay { engine, replay_data: None, config: config.clone() });
                    }
                }
                KeyCode::ArrowLeft => if *diff > 0 {
                    *diff -= 1;
                    if let Some(j) = song.jsons.get(*diff) {
                        if let Ok((_, _ns)) = load_beatmap_rox(j) {
                            *stars = pp::calculate_stars(j, config.song_rate);
                        }
                    }
                }
                KeyCode::ArrowRight => if *diff + 1 < song.jsons.len() {
                    *diff += 1;
                    if let Some(j) = song.jsons.get(*diff) {
                        if let Ok((_, _ns)) = load_beatmap_rox(j) {
                            *stars = pp::calculate_stars(j, config.song_rate);
                        }
                    }
                }
                KeyCode::Escape => { let state = SongSelectState::new(self.folders_cache.clone(), config.clone()); self.render = None; next = Some(AppState::SongSelect { state, config: config.clone() }); }
                _ => {}
            },
            AppState::Results { result, replay, config, page, chart_view_start, chart_view_end, chart_n_sec, chart_adjust_n, .. } => {
                if *chart_adjust_n {
                    // N 调节模式 (弹窗)
                    let step = if self.shift_held { 1.0 } else { 0.1 };
                    match key {
                        KeyCode::KeyA => *chart_n_sec = (*chart_n_sec - step).max(0.1),
                        KeyCode::KeyD => *chart_n_sec = (*chart_n_sec + step).min(30.0),
                        KeyCode::KeyX | KeyCode::Escape | KeyCode::Enter => *chart_adjust_n = false,
                        _ => {}
                    }
                } else {
                    // 图表平移/缩放 (全页面通用)
                    let fast = self.shift_held;
                    let pan_step = if fast { 0.1 } else { 0.02 };
                    let zoom_step = if fast { 0.1 } else { 0.02 };
                    match key {
                        KeyCode::Enter => {
                            let target = song_select::scroll_target_for(self.prev_folder_idx, self.prev_diff_idx);
                            let state = SongSelectState { folder_idx: self.prev_folder_idx, diff_idx: self.prev_diff_idx, scroll_y: target, target_scroll_y: target, ..SongSelectState::new(self.folders_cache.clone(), config.clone()) };
                            self.render = None;
                            next = Some(AppState::SongSelect { state, config: config.clone() });
                        }
                        KeyCode::KeyS => {
                            if let Some(ref r) = replay {
                                let _ = std::fs::create_dir_all("replays");
                                let stem = std::path::Path::new(&r.map_path).file_stem().map(|s| s.to_string_lossy()).unwrap_or_default();
                                let date_part = r.date.replace(':', "-").replace(' ', "_");
                                let fname = format!("replays/{}_{}.json.gz", stem, date_part);
                                if let Err(e) = r.save(&fname) {
                                    log::error!("[Replay] save failed: {}", e);
                                } else {
                                    log::info!("[Replay] saved {}", fname);
                                }
                            } else {
                                log::warn!("[Replay] S pressed but replay is None");
                            }
                        }
                        KeyCode::KeyR => {
                            if let Ok((meta, bnotes)) = load_beatmap_rox(&result.map_path) {
                                let sname = meta.song.clone();
                                let rnotes: Vec<NoteRT> = bnotes.into_iter().map(|n| NoteRT { time: n.time, end_time: n.end_time, lane: n.lane, note_type: n.note_type, hit: false, missed: false, holding: false, ghost: false, stuck_y: None, release_time: None }).collect();
                                let mut audio = BassAudio::init().unwrap();
                                let ap = if (config.song_rate - 1.0).abs() > 0.001 { let t = format!(".temp_{}x_{}", config.song_rate, Path::new(&sname).file_name().unwrap().to_string_lossy()); let tp = Path::new(&result.map_path).parent().unwrap().join(Path::new(&t).with_extension("wav")); let ts = tp.to_string_lossy().to_string(); if !tp.exists() { sonic::generate_stretched_audio(&sname, &ts, config.song_rate as f32, Some(&audio)); drop(audio); audio = BassAudio::init().unwrap(); } ts } else { sname.clone() };
                                audio.load(&ap).expect("audio");
                                let cover = meta.bg.as_ref().map(|bg| Path::new(&result.map_path).parent().unwrap().join(bg));
                                let cpu = CpuSkin::load(&config.active_skin, cover.as_deref());
                                let sc = cpu.config.clone();
                                let render = pollster::block_on(RenderCtx::new(self.window.clone(), cpu, &self.extra_chars));
                                let tdur = rnotes.iter().map(|n| n.end_time.max(n.time)).fold(1.0, f64::max);
                                let engine = GameEngine::new(audio, config.clone(), sname, result.map_path.clone(), rnotes, tdur, meta.offset, config.mirror_mode, cover.as_ref().map(|p| p.to_string_lossy().to_string()), Some(render.skin_regions()), Some(sc), self.window.clone(), render);
                                self.render = None;
                                next = Some(AppState::Gameplay { engine, replay_data: None, config: config.clone() });
                            }
                        }
                        // 翻页
                        KeyCode::ArrowLeft | KeyCode::ArrowUp => *page = 1 - *page,
                        KeyCode::ArrowRight | KeyCode::ArrowDown => *page = 1 - *page,
                        // A: 缓慢左移  D: 缓慢右移
                        KeyCode::KeyA => {
                            *chart_view_start = (*chart_view_start - pan_step).max(0.0);
                            *chart_view_end = (*chart_view_end - pan_step).max(*chart_view_end - *chart_view_start);
                        }
                        KeyCode::KeyD => {
                            *chart_view_start = (*chart_view_start + pan_step).min(1.0 - (*chart_view_end - *chart_view_start));
                            *chart_view_end = (*chart_view_end + pan_step).min(1.0);
                        }
                        // Z: 扩大展示区间 (zoom out)  C: 缩小展示区间 (zoom in)
                        KeyCode::KeyZ => {
                            let mid = (*chart_view_start + *chart_view_end) / 2.0;
                            let half = (*chart_view_end - *chart_view_start) / 2.0 * (1.0 + zoom_step);
                            *chart_view_start = (mid - half).max(0.0);
                            *chart_view_end = (mid + half).min(1.0);
                        }
                        KeyCode::KeyC => {
                            let mid = (*chart_view_start + *chart_view_end) / 2.0;
                            let half = (*chart_view_end - *chart_view_start) / 2.0 * (1.0 - zoom_step);
                            *chart_view_start = (mid - half).max(0.0);
                            *chart_view_end = (mid + half).min(1.0);
                        }
                        // X: N 步长调节弹窗
                        KeyCode::KeyX => *chart_adjust_n = true,
                        _ => {}
                    }
                }
            },
            AppState::ExitConfirm => match key { KeyCode::Enter => el.exit(), KeyCode::Escape => { self.render = None; next = Some(AppState::Splash { cover_regions: vec![], cover_idx: 0, cycle_start: std::time::Instant::now() - std::time::Duration::from_secs(10) }); } _ => {} },
            _ => {}
        }
        let to_results = matches!(next, Some(AppState::Results { .. }));
        if let Some(s) = next { self.state = s; }
        if to_results { self.window.request_redraw(); }
    }
}

fn settings_secondary_count(primary: usize) -> usize {
    menu::settings::secondary_count(primary)
}

fn settings_activate(primary: &mut usize, secondary: &mut usize, binding_idx: &mut Option<usize>, adjuster: &mut Option<Adjuster>, config: &mut GameConfig) {
    match *primary {
        0 => match *secondary {
            2 => { let v = config.hit_position; *adjuster = Some(Adjuster { label: "判定线位置", value: v, min: 300.0, max: 580.0, step: 5.0, setter: |c, val| c.hit_position = val }); }
            5 => config.mirror_mode = !config.mirror_mode,
            _ => {}
        },
        2 => { *binding_idx = Some(*secondary); }
        3 => match *secondary {
            0 => config.fullscreen = !config.fullscreen,
            1 => config.show_fps = !config.show_fps,
            _ => {}
        },
        _ => {}
    }
    config.save("config.json");
}

fn settings_shortcut(config: &mut GameConfig, key: KeyCode) {
    match key {
        KeyCode::KeyL => config.scroll_speed = (config.scroll_speed - 1.0).max(5.0),
        KeyCode::KeyJ => config.scroll_speed += 1.0,
        KeyCode::KeyO => config.od = (config.od - 0.5).max(0.0),
        KeyCode::KeyP => config.od = (config.od + 0.5).min(11.0),
        KeyCode::KeyU => config.hit_position = (config.hit_position - 10.0).max(300.0),
        KeyCode::KeyI => config.hit_position = (config.hit_position + 10.0).min(580.0),
        KeyCode::KeyK => config.stage_spacing = (config.stage_spacing - 5.0).max(50.0),
        KeyCode::KeyM => config.stage_spacing += 5.0,
        KeyCode::KeyN => config.stage_scale = (config.stage_scale - 0.1).max(0.5),
        KeyCode::Comma => config.stage_scale = (config.stage_scale + 0.1).min(2.0),
        KeyCode::KeyB => config.mirror_mode = !config.mirror_mode,
        KeyCode::KeyA => config.global_offset -= 5.0,
        KeyCode::KeyD => config.global_offset += 5.0,
        KeyCode::KeyW => config.song_rate = (config.song_rate - 0.1).max(0.5),
        KeyCode::KeyE => config.song_rate = (config.song_rate + 0.1).min(2.0),
        KeyCode::KeyF => config.fullscreen = !config.fullscreen,
        KeyCode::KeyT | KeyCode::KeyY => {
            let skins = crate::skin::list_skins();
            let cur = &config.active_skin;
            let cur_idx = skins.iter().position(|s| s == cur).map(|i| i + 1).unwrap_or(0);
            let total = skins.len() + 1;
            let new_idx = if key == KeyCode::KeyT {
                (cur_idx + 1) % total
            } else {
                (cur_idx + total - 1) % total
            };
            config.active_skin = if new_idx == 0 { String::new() } else { skins[new_idx - 1].clone() };
        }
        _ => {}
    }
}

fn key_name(k: KeyCode) -> Option<&'static str> { match k {
    KeyCode::KeyA=>"a",KeyCode::KeyB=>"b",KeyCode::KeyC=>"c",KeyCode::KeyD=>"d",KeyCode::KeyE=>"e",KeyCode::KeyF=>"f",
    KeyCode::KeyG=>"g",KeyCode::KeyH=>"h",KeyCode::KeyI=>"i",KeyCode::KeyJ=>"j",KeyCode::KeyK=>"k",KeyCode::KeyL=>"l",
    KeyCode::KeyM=>"m",KeyCode::KeyN=>"n",KeyCode::KeyO=>"o",KeyCode::KeyP=>"p",KeyCode::KeyQ=>"q",KeyCode::KeyR=>"r",
    KeyCode::KeyS=>"s",KeyCode::KeyT=>"t",KeyCode::KeyU=>"u",KeyCode::KeyV=>"v",KeyCode::KeyW=>"w",KeyCode::KeyX=>"x",
    KeyCode::KeyY=>"y",KeyCode::KeyZ=>"z",KeyCode::Comma=>",",KeyCode::Period=>".",KeyCode::Semicolon=>";",KeyCode::Quote=>"'",
    KeyCode::BracketLeft=>"[",KeyCode::BracketRight=>"]",KeyCode::Slash=>"/",
    KeyCode::Digit1=>"1",KeyCode::Digit2=>"2",KeyCode::Digit3=>"3",KeyCode::Digit4=>"4",
    KeyCode::Numpad0=>"0",KeyCode::Numpad1=>"1",KeyCode::Numpad2=>"2",KeyCode::Numpad3=>"3",
    KeyCode::Numpad4=>"4",KeyCode::Numpad5=>"5",KeyCode::Numpad6=>"6",KeyCode::Numpad7=>"7",
    KeyCode::Numpad8=>"8",KeyCode::Numpad9=>"9",
    _=>return None,
}.into() }

fn compute_target_scroll(state: &SongSelectState) -> f32 {
    let (fh, dh, gp): (f32, f32, f32) = (50.0, 48.0, 2.0);
    let mut ty: f32 = 0.0;
    for fi in 0..=state.folder_idx {
        if fi > 0 { ty += 8.0; }
        ty += fh;
        // 只有选中的文件夹才展开难度
        if fi == state.folder_idx {
            for di in 0..state.folders[fi].diffs.len() {
                if di == state.diff_idx { ty += dh / 2.0; break; }
                ty += dh + gp;
            }
        }
    }
    (ty - SCREEN_H as f32 / 2.0).max(0.0)
}
