use crate::config::GameConfig;
use crate::game::notes::{SCREEN_H, screen_w};
use crate::render::quad::QuadRenderer;
use crate::render::text::TextRenderer;
use crate::skin::AtlasRegion;
use crate::ui::{draw_menu_background};
use crate::ui::theme::{WHITE, GRAY_160};

const PRIMARY_UNSEL: [u8; 4] = [36, 34, 41, 120];
const PRIMARY_SEL:  [u8; 4] = [45, 42, 59, 160];
const SECONDARY_BG: [u8; 4] = [48, 46, 55, 140];
const LEFT_BG:      [u8; 4] = [24, 22, 30, 140];

const PRIMARY_LABELS: &[&str] = &["游玩设置", "音频设置", "键位设置", "显示设置", "皮肤设置", "制谱器"];

use crate::editor::config::SpectrogramConfig;

pub fn render_settings(
    quad: &mut QuadRenderer,
    text: &mut TextRenderer,
    cover_region: Option<&AtlasRegion>,
    primary: usize,
    secondary: usize,
    binding_idx: Option<usize>,
    adjuster: Option<&crate::Adjuster>,
    config: &GameConfig,
    spect_config: Option<&SpectrogramConfig>,
) {
    let w = screen_w();
    draw_menu_background(quad, cover_region);

    let left_w = w * 0.25;
    let mid_x = left_w;
    let mid_w = w * 0.5;

    // 左栏背景 + 一级菜单
    quad.push_rect(0.0, 0.0, left_w, SCREEN_H, LEFT_BG);
    for (i, &label) in PRIMARY_LABELS.iter().enumerate() {
        let y = 30.0 + i as f32 * 56.0;
        quad.push_rect(4.0, y, left_w - 8.0, 50.0, if i == primary { PRIMARY_SEL } else { PRIMARY_UNSEL });
        text.queue_text(label, 14.0, y + 16.0, 16.0, if i == primary { WHITE } else { GRAY_160 });
    }

    // 中栏背景 + 二级菜单
    quad.push_rect(mid_x, 0.0, mid_w, SCREEN_H, SECONDARY_BG);
    let items = secondary_items(primary, config, spect_config);
    for (i, (label, val)) in items.iter().enumerate() {
        let y = 30.0 + i as f32 * 40.0;
        let color = if i == secondary { WHITE } else { GRAY_160 };
        text.queue_text(label, mid_x + 14.0, y + 4.0, 15.0, color);
        text.queue_text(val, mid_x + mid_w - 120.0, y + 4.0, 15.0, color);
        if i == secondary {
            quad.push_rect(mid_x + 4.0, y + 8.0, 4.0, 20.0, WHITE);
        }
    }

    // 键位绑定提示
    if let Some(idx) = binding_idx {
        quad.push_rect(mid_x, SCREEN_H - 60.0, mid_w, 40.0, [0,0,0,180]);
        text.queue_text(&format!(">>> 按下轨道 {} 的新按键 ...", idx + 1), mid_x + 14.0, SCREEN_H - 32.0, 14.0, WHITE);
    }

    // 全屏调节器
    if let Some(adj) = adjuster {
        quad.push_rect(0.0, 0.0, w, SCREEN_H, [0, 0, 0, 200]);
        let cx = w / 2.0; let cy = SCREEN_H / 2.0;
        text.queue_text(adj.label, cx - 80.0, cy - 80.0, 24.0, WHITE);
        text.queue_text(&format!("{:.1}", adj.value), cx - 40.0, cy - 30.0, 48.0, WHITE);
        let bar_w: f32 = 300.0;
        let pct: f32 = ((adj.value - adj.min) / (adj.max - adj.min)).clamp(0.0, 1.0) as f32;
        quad.push_rect(cx - bar_w/2.0, cy + 40.0, bar_w, 16.0, [60,60,60,255]);
        quad.push_rect(cx - bar_w/2.0, cy + 40.0, bar_w * pct, 16.0, WHITE);
        text.queue_text("[← →] 微调  [↑ ↓] 粗调  [Enter] 确认  [ESC] 取消", cx - 160.0, cy + 80.0, 12.0, GRAY_160);
    }

    // 底部提示
    if binding_idx.is_none() && adjuster.is_none() {
        text.queue_text("[← →] 一级  [↑ ↓] 二级  [Enter] 选择  [ESC] 返回", w/2.0 - 150.0, SCREEN_H - 16.0, 11.0, GRAY_160);
    }

    // 右区曲绘
    let rx = mid_x + mid_w;
    if let Some(region) = cover_region {
        let rw = w - rx;
        let ch = rw * region.height as f32 / region.width as f32;
        quad.push_textured_rect(rx, SCREEN_H/2.0 - ch/2.0, rw, ch,
            region.uv_x, region.uv_y, region.uv_w, region.uv_h, [120, 120, 130, 180]);
    }
}

pub fn secondary_items(primary: usize, config: &GameConfig, spect_config: Option<&SpectrogramConfig>) -> Vec<(&'static str, String)> {
    match primary {
        0 => vec![
            ("流速", format!("{:.2}", config.scroll_speed)),
            ("OD", format!("{:.1}", config.od)),
            ("判定线位置", format!("{:.0}px", config.hit_position)),
            ("轨道间距", format!("{:.0}px", config.stage_spacing)),
            ("舞台缩放", format!("{:.2}x", config.stage_scale)),
            ("镜像模式", if config.mirror_mode { "开".into() } else { "关".into() }),
        ],
        1 => vec![
            ("播放倍速", format!("{:.2}x", config.song_rate)),
            ("全局偏移", format!("{}ms", config.global_offset as i32)),
        ],
        2 => vec![
            ("轨道 1", config.key_bindings.get(0).cloned().unwrap_or_default().to_uppercase()),
            ("轨道 2", config.key_bindings.get(1).cloned().unwrap_or_default().to_uppercase()),
            ("轨道 3", config.key_bindings.get(2).cloned().unwrap_or_default().to_uppercase()),
            ("轨道 4", config.key_bindings.get(3).cloned().unwrap_or_default().to_uppercase()),
        ],
        3 => vec![
            ("全屏模式", if config.fullscreen { "开".into() } else { "关".into() }),
            ("FPS 显示", if config.show_fps { "开".into() } else { "关".into() }),
        ],
        4 => vec![
            ("当前皮肤", if config.active_skin.is_empty() { "默认".into() } else { config.active_skin.clone() }),
            ("切换皮肤", "按 T / Y".into()),
        ],
        5 => spect_config.map(|sc| vec![
            ("显示频谱", if sc.show_spectrogram { "开".into() } else { "关".into() }),
            ("频谱配色", sc.colormap.clone()),
            ("Mel 频带数", format!("{}", sc.n_mels)),
            ("FFT 大小", format!("{}", sc.n_fft)),
            ("Hop 步长", format!("{}", sc.hop_length)),
            ("频率上限", format!("{:.0}Hz", sc.freq_max)),
            ("频率下限", format!("{:.0}Hz", sc.freq_min)),
            ("降噪门限", format!("{:.2}", sc.noise_gate)),
        ]).unwrap_or_default(),
        _ => vec![],
    }
}

pub fn secondary_count(primary: usize) -> usize {
    match primary { 0 => 6, 1 => 2, 2 => 4, 3 => 2, 4 => 2, 5 => 8, _ => 0 }
}
