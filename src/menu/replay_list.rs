use crate::config::GameConfig;
use crate::game::notes::{screen_w, SCREEN_H};
use crate::render::quad::QuadRenderer;
use crate::render::text::TextRenderer;
use crate::replay::ReplayData;
use crate::ui::theme;

pub struct ReplayListState {
    pub map_path: String,
    pub entries: Vec<(String, ReplayData)>,
    pub selected: usize,
    pub config: GameConfig,
    pub folder_idx: usize,
    pub diff_idx: usize,
}

pub fn render(state: &ReplayListState, quad: &mut QuadRenderer, text: &mut TextRenderer) {
    let w = screen_w();
    quad.push_rect(0.0, 0.0, w, SCREEN_H, [0, 0, 0, 180]);

    let card_w = 600.0;
    let card_h = 420.0;
    let cx = w / 2.0 - card_w / 2.0;
    let cy = SCREEN_H / 2.0 - card_h / 2.0 - 20.0;

    quad.push_rect(cx, cy, card_w, card_h, [30, 30, 45, 255]);

    let map_name = std::path::Path::new(&state.map_path)
        .file_stem()
        .map(|s| s.to_string_lossy())
        .unwrap_or_default();
    let title = format!("Replays — {}", truncate(&map_name, 40));
    text.queue_text(&title, cx + 15.0, cy + 22.0, 18.0, theme::WHITE);

    quad.push_rect(cx + 15.0, cy + 35.0, card_w - 30.0, 2.0, [60, 60, 80, 255]);

    let list_y = cy + 50.0;
    let row_h = 44.0;
    let visible_rows = ((card_h - 120.0) / row_h) as usize;
    let max_scroll = state.entries.len().saturating_sub(visible_rows);
    let scroll = (state.selected).min(max_scroll);
    let start = scroll;

    for i in start..state.entries.len().min(start + visible_rows) {
        let (_, ref data) = state.entries[i];
        let ry = list_y + (i - start) as f32 * row_h;

        if i == state.selected {
            quad.push_rect(cx + 10.0, ry - 2.0, card_w - 20.0, row_h - 2.0, [50, 50, 75, 200]);
        }

        let rank = theme::rank_from_acc(
            data.acc,
            data.counts.good,
            data.counts.ok,
            data.counts.meh,
            data.counts.miss,
        );
        let rc = theme::rank_color(rank);
        quad.push_rect(cx + 20.0, ry + 8.0, 24.0, 24.0, rc);
        text.queue_text(rank, cx + 28.0, ry + 18.0, 14.0, theme::WHITE);

        text.queue_text(&data.date, cx + 60.0, ry + 16.0, 13.0, theme::GRAY_160);

        let score_str = format_score(data.score);
        text.queue_text(&score_str, cx + 280.0, ry + 16.0, 14.0, theme::WHITE);

        let acc_str = format!("{:.2}%", data.acc);
        text.queue_text(&acc_str, cx + 420.0, ry + 16.0, 14.0, [0, 255, 200, 255]);

        if (data.song_rate - 1.0).abs() > 0.001 {
            let rate_str = format!("{:.1}x", data.song_rate);
            text.queue_text(&rate_str, cx + 520.0, ry + 16.0, 12.0, [200, 255, 100, 255]);
        }
    }

    let hint_y = cy + card_h - 36.0;
    quad.push_rect(cx, hint_y - 8.0, card_w, 2.0, [60, 60, 80, 255]);
    text.queue_text(
        "[↑↓] Select  [ENTER] Watch  [ESC/R] Back",
        cx + 15.0,
        hint_y + 16.0,
        13.0,
        theme::GRAY_160,
    );

    if state.entries.is_empty() {
        text.queue_text("(no replays found)", cx + card_w / 2.0 - 60.0, cy + card_h / 2.0, 16.0, theme::GRAY_160);
    }
}

fn truncate(s: &str, max: usize) -> String {
    if s.chars().count() <= max {
        s.to_string()
    } else {
        s.chars().take(max - 3).collect::<String>() + "..."
    }
}

fn format_score(score: u32) -> String {
    if score >= 1_000_000 {
        format!("{:.1}M", score as f64 / 1_000_000.0)
    } else if score >= 1_000 {
        format!("{:.0}K", score as f64 / 1_000.0)
    } else {
        format!("{}", score)
    }
}
