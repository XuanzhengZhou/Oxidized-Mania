// osu! 风格选歌界面: 左侧梯形面板 + 右侧卡牌轮播
use crate::config::GameConfig;
use crate::render::quad::QuadRenderer;
use crate::render::text::TextRenderer;
use crate::game::notes::SCREEN_H;
use crate::ui::theme::{self, WHITE, GRAY_160};
use crate::ui::primitives;

// ─── 数据结构 ───

#[derive(Debug, Clone)]
pub struct DiffMeta {
    pub name: String,
    pub stars: f64,
    pub notes_count: usize,
    pub duration: f64,
    pub bpm: f64,
    pub mapper: String,
    pub tap_count: usize,
    pub hold_count: usize,
    pub cover_path: Option<String>,
    pub rc_label: String,
    pub dim_speed: f64, pub dim_stamina: f64,
    pub dim_chord: f64, pub dim_tech: f64,
}

#[derive(Debug, Clone)]
pub struct FolderMeta {
    pub dir_name: String,
    pub author: String,
    pub jsons: Vec<String>,
    pub diffs: Vec<DiffMeta>,
}

pub struct SongSelectState {
    pub folders: Vec<FolderMeta>,
    pub folder_idx: usize,
    pub diff_idx: usize,
    pub scroll_y: f32,
    pub target_scroll_y: f32,
    pub config: GameConfig,
    pub cover_region: Option<crate::skin::AtlasRegion>,
    pub hist: crate::history::HistoryData,
}

impl SongSelectState {
    pub fn new(folders: Vec<FolderMeta>, config: GameConfig) -> Self {
        let hist = crate::history::load_history("history.json");
        SongSelectState {
            folders, folder_idx: 0, diff_idx: 0,
            scroll_y: 0.0, target_scroll_y: 0.0,
            config, cover_region: None, hist,
        }
    }
    pub fn current_folder(&self) -> &FolderMeta { &self.folders[self.folder_idx] }
    pub fn current_diff(&self) -> &DiffMeta {
        let f = self.current_folder();
        let idx = self.diff_idx.min(f.diffs.len().saturating_sub(1));
        &f.diffs[idx]
    }
    pub fn scroll_to_current_diff(&self) -> f32 {
        scroll_target_for(self.folder_idx, self.diff_idx)
    }
}

/// 计算将指定难度滚动到屏幕中央所需的 target_scroll_y
pub fn scroll_target_for(folder_idx: usize, diff_idx: usize) -> f32 {
    let fh: f32 = 50.0; let dh: f32 = 48.0; let gap: f32 = 2.0;
    let diff_row = dh + gap;
    let y_pos = folder_idx as f32 * fh + fh + diff_idx as f32 * diff_row;
    let target = (y_pos + 10.0 - crate::game::notes::SCREEN_H / 2.0 + dh / 2.0).max(0.0);
    target
}

// ─── 从 SongEntry 构建 FolderMeta ───

pub fn build_folder_meta(songs: &[crate::menu::SongEntry], config: &GameConfig) -> Vec<FolderMeta> {
    songs.iter().map(|s| {
        let author = s.dir_name.split(" - ").next().unwrap_or(&s.dir_name).to_string();
        let mut diffs = Vec::new();
        for j in &s.jsons {
            if let Ok((meta, notes)) = crate::beatmap::load_beatmap_rox(j) {
                let cover_path = meta.bg.clone(); // load_beatmap_rox 已解析为全路径
                let raw_name = std::path::Path::new(j)
                    .file_stem().map(|st| st.to_string_lossy().to_string())
                    .unwrap_or_default();
                let name = parse_diff_name(&raw_name);
                let mapper = parse_mapper(&name);
                let bpm = meta.bpm;
                let total = notes.len();
                let taps = notes.iter().filter(|n| n.end_time <= n.time).count();
                let holds = total - taps;
                let dur = notes.iter().map(|n| n.end_time.max(n.time)).fold(0.0, f64::max);
                let stars = crate::pp::calculate_stars(j, config.song_rate);
                let (rc_label, dim_speed, dim_stamina, dim_chord, dim_tech) =
                    match crate::difficulty::analyze_difficulty(&notes, config.song_rate, config.od) {
                        Ok(d) => (d.fuzzy_label(), d.dimensions.speed, d.dimensions.stamina, d.dimensions.chord, d.dimensions.tech),
                        Err(_) => (String::new(), 0.0, 0.0, 0.0, 0.0),
                    };
                diffs.push(DiffMeta { name, stars, notes_count: total, duration: dur, bpm, mapper, tap_count: taps, hold_count: holds, cover_path, rc_label, dim_speed, dim_stamina, dim_chord, dim_tech });
            }
        }
        // 按星数升序排列难度
        let mut indices: Vec<usize> = (0..diffs.len()).collect();
        indices.sort_by(|&a, &b| diffs[a].stars.partial_cmp(&diffs[b].stars).unwrap());
        let sorted_diffs: Vec<_> = indices.iter().map(|&i| diffs[i].clone()).collect();
        let sorted_jsons: Vec<_> = indices.iter().map(|&i| s.jsons[i].clone()).collect();
        FolderMeta { dir_name: s.dir_name.clone(), author, jsons: sorted_jsons, diffs: sorted_diffs }
    }).collect()
}

fn parse_mapper(s: &str) -> String {
    if let Some(p) = s.find('(') { if let Some(e) = s[p..].find(')') { return s[p+1..p+e].to_string(); } }
    String::new()
}

/// 从完整文件名中提取难度名: "Pack (Mapper) [Song].json" → "Song"
fn parse_diff_name(raw: &str) -> String {
    if let Some(open) = raw.rfind('[') {
        if let Some(close) = raw[open..].find(']') {
            return raw[open+1..open+close].to_string();
        }
    }
    // 无括号回退: 取最后一个 - 之后的部分
    if let Some(pos) = raw.rfind(" - ") {
        return raw[pos+3..].to_string();
    }
    raw.to_string()
}

// ─── 渲染 ───

const CARD_BG: [u8; 4] = [28, 28, 34, 170];
const CARD_SEL_BG: [u8; 4] = [38, 38, 48, 200];
const PANEL_ALPHA: [u8; 4] = [51, 54, 54, 185];

pub fn render(quad: &mut QuadRenderer, text: &mut TextRenderer, state: &SongSelectState, _config: &GameConfig) {
    let w = crate::game::notes::screen_w();

    // 背景
    quad.push_rect(0.0, 0.0, w, SCREEN_H, theme::BG_DARK);
    // 曲绘 (全屏，被卡片和面板覆盖)
    if let Some(ref cover) = state.cover_region {
        let cover_w = w;
        let cover_h = cover_w * cover.height as f32 / cover.width as f32;
        let cover_y = SCREEN_H / 2.0 - cover_h / 2.0;
        quad.push_textured_rect(0.0, cover_y, cover_w, cover_h,
            cover.uv_x, cover.uv_y, cover.uv_w, cover.uv_h,
            [200, 200, 200, 120]); // 低亮度低透明度
    }
    // 左侧暗色遮罩 (模拟模糊)
    quad.push_rect(0.0, 0.0, w * 0.55, SCREEN_H, [0, 0, 0, 100]);

    let top_w = w * 0.50;
    let bot_w = w * 0.33;
    let panel_h = SCREEN_H * 0.50;

    // === 梯形面板 ===
    primitives::draw_trapezoid(quad, 0.0, 0.0, top_w, bot_w, panel_h, PANEL_ALPHA, 60);

    // 面板内容
    let diff = state.current_diff();
    let folder = state.current_folder();
    let sc = theme::star_color(diff.stars);
    let dur_s = diff.duration / 1000.0 / _config.song_rate as f64;
    let dur = format!("{:02}:{:02}", dur_s as u64 / 60, dur_s as u64 % 60);

    text.queue_text(&folder.dir_name, 20.0, 20.0, 18.0, WHITE);
    text.queue_text(&folder.author, 20.0, 48.0, 14.0, WHITE);
    text.queue_text(&format!("{}  BPM {:.0}", dur, diff.bpm), 20.0, 72.0, 12.0, WHITE);

    // 星数胶囊
    primitives::draw_capsule(quad, 20.0, 100.0, 75.0, 20.0, sc);
    text.queue_text(&format!("{:.2}", diff.stars), 26.0, 102.0, 11.0, [0,0,0,255]);
    // 难度名 + by 谱师
    let dn = trunc(&diff.name, 70);
    text.queue_text(&dn, 105.0, 102.0, 14.0, sc);
    if !diff.mapper.is_empty() {
        text.queue_text("by ", 105.0, 122.0, 11.0, sc);
        text.queue_text(&diff.mapper, 125.0, 122.0, 11.0, theme::MAPPER_TEXT);
    }

    // RC 难度标签（星数胶囊下方 3 行）
    if !diff.rc_label.is_empty() {
        text.queue_text(&diff.rc_label, 20.0, 170.0, 10.0, GRAY_160);
    }
    // 四维技能条（"单点"上一行，全称，宽度×1.8）
    let sy = panel_h - 55.0;
    let skill_y = (sy - 24.0) as f32;
    let skill_x0 = 20.0f32;
    let skill_w = 86.0f32;
    let skill_gap = 4.0f32;
    let dims: [(&str, f64); 4] = [("Speed", diff.dim_speed), ("Stamina", diff.dim_stamina), ("Chord", diff.dim_chord), ("Tech", diff.dim_tech)];
    for (i, (label, val)) in dims.iter().enumerate() {
        let sx = skill_x0 + i as f32 * (skill_w + skill_gap);
        let pct = (*val as f32 / 45.0).clamp(0.0, 1.0);
        let bar_h = pct * 12.0;
        let c = theme::star_color(*val * 0.25);
        quad.push_rect(sx, skill_y + 12.0 - bar_h, skill_w, bar_h, c);
        quad.push_rect(sx, skill_y + 12.0, skill_w, 1.0, [80, 80, 90, 120]);
        text.queue_text(label, sx + 2.0, skill_y - 2.0, 8.0, GRAY_160);
    }

    let sy = panel_h - 55.0;
    text.queue_text(&format!("单点: {}", diff.tap_count), 20.0, sy, 12.0, GRAY_160);
    text.queue_text(&format!("长条: {}", diff.hold_count), 20.0, sy + 18.0, 12.0, GRAY_160);
    text.queue_text(&format!("4K  |  OD: {:.1}", _config.od), top_w * 0.55, sy, 12.0, GRAY_160);

    // === 历史卡片 ===
    let hy = panel_h + 5.0;
    let hist = &state.hist;
    let cur_json = &folder.jsons[state.diff_idx.min(folder.jsons.len().saturating_sub(1))];
    if let Some(recs) = hist.get(cur_json) {
        for (i, rec) in recs.iter().take(5).enumerate() {
            let cy = hy + i as f32 * 62.0;
            if cy > SCREEN_H - 10.0 { break; }
            quad.push_rect(4.0, cy, bot_w - 8.0, 56.0, CARD_BG);
            text.queue_text(&rec.time, 12.0, cy + 4.0, 10.0, GRAY_160);
            text.queue_text(&format!("{}", rec.score), 12.0, cy + 18.0, 13.0, WHITE);
            // 旧记录可能没有 od/rank，从 acc 推算 rank
            let od_str = if rec.od > 0.0 {
                format!("OD: {:.1}", rec.od)
            } else {
                String::new()
            };
            let rank_str = if rec.rank.is_empty() {
                theme::rank_from_acc(rec.acc, 0, 0, 0, 0).to_string()
            } else {
                rec.rank.clone()
            };
            text.queue_text(&format!("ACC: {:.1}%  {}", rec.acc, od_str), 12.0, cy + 36.0, 10.0, GRAY_160);
            if !rank_str.is_empty() {
                let rc = theme::rank_color(&rank_str);
                quad.push_rect(bot_w - 44.0, cy + 10.0, 36.0, 34.0, rc);
                let tc = if matches!(rank_str.as_str(), "SS"|"S") { [0,0,0,255] } else { WHITE };
                text.queue_text(&rank_str, bot_w - 38.0, cy + 14.0, 20.0, tc);
            }
        }
    }

    // === 右侧轮播 ===
    let cx = top_w + 5.0;
    let sy2 = state.scroll_y + (state.target_scroll_y - state.scroll_y) * 0.15;
    let dy = 10.0 - sy2;
    let fh: f32 = 50.0; let dh: f32 = 48.0; let gap: f32 = 2.0;

    let mut cur_y = dy;
    for (fi, folder) in state.folders.iter().enumerate() {
        if cur_y > SCREEN_H + 100.0 { break; }
        let is_active = fi == state.folder_idx;
        let block_h = if is_active { fh + folder.diffs.len() as f32 * (dh + gap) } else { fh } + 8.0;
        if cur_y + block_h < -100.0 { cur_y += block_h; continue; }

        // 文件夹卡片头
        let is_active = fi == state.folder_idx;
        quad.push_rect(cx, cur_y, w - cx - 10.0, fh, if is_active { CARD_SEL_BG } else { CARD_BG });
        text.queue_text(&folder.dir_name, cx + 8.0, cur_y + 6.0, 14.0, WHITE);
        text.queue_text(&folder.author, cx + 8.0, cur_y + 26.0, 11.0, WHITE);
        cur_y += fh;

        // 只展开选中文件夹的难度卡片
        if is_active {
            for (di, diff) in folder.diffs.iter().enumerate() {
                if cur_y > SCREEN_H + 100.0 { break; }
                if cur_y + dh >= -50.0 {
                    let is_sel = di == state.diff_idx;
                    let indent = if is_sel { cx + 12.0 } else { cx + 30.0 };
                    let cw = w - indent - 10.0;
                    quad.push_rect(indent, cur_y, cw, dh, if is_sel { CARD_SEL_BG } else { CARD_BG });
                    quad.push_rect(indent, cur_y, 4.0, dh, theme::star_color(diff.stars));
                    text.queue_text(&trunc(&diff.name, 70), indent + 10.0, cur_y + 4.0, 13.0, WHITE);
                    if !diff.mapper.is_empty() {
                        text.queue_text(&format!("谱师: {}", diff.mapper), indent + 10.0, cur_y + 22.0, 10.0, GRAY_160);
                    }
                    primitives::draw_capsule(quad, indent + cw - 68.0, cur_y + 14.0, 60.0, 18.0, theme::star_color(diff.stars));
                    text.queue_text(&format!("{:.2}", diff.stars), indent + cw - 64.0, cur_y + 16.0, 10.0, [0,0,0,255]);
                }
                cur_y += dh + gap;
            }
        }
        cur_y += 8.0;
    }

    // 底部提示条
    quad.push_rect(0.0, SCREEN_H - 22.0, w, 22.0, [12, 12, 16, 220]);
    text.queue_text("[←→] 文件夹  [↑↓] 难度  [ENTER] 游玩  [S] 设置  [ESC] 返回",
        w / 2.0 - 200.0, SCREEN_H - 17.0, 11.0, GRAY_160);
}

fn trunc(s: &str, max: usize) -> String {
    if s.chars().count() <= max { s.to_string() }
    else { s.chars().take(max - 3).chain("...".chars()).collect() }
}
