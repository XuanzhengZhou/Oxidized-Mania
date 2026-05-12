// osu! 风格色板 + 星数颜色映射

// ─── 主题色 ───

pub const BG_DARK: [u8; 4] = [18, 18, 24, 255];
pub const PINK_LOGO: [u8; 4] = [0xDC, 0x68, 0x9D, 255];
pub const WHITE: [u8; 4] = [255, 255, 255, 255];
pub const GRAY_160: [u8; 4] = [160, 160, 170, 255];

pub const TAB_SETTINGS: [u8; 4] = [85, 85, 85, 255];
pub const TAB_PLAY: [u8; 4] = [97, 70, 197, 255];
pub const TAB_EDIT: [u8; 4] = [228, 173, 59, 255];
pub const TAB_BROWSE: [u8; 4] = [173, 203, 63, 255];
pub const TAB_EXIT: [u8; 4] = [219, 69, 151, 255];
pub const BTN_BACK: [u8; 4] = [52, 58, 91, 255];

pub const PANEL_TRAPEZOID: [u8; 4] = [51, 54, 54, 255];
pub const MAPPER_TEXT: [u8; 4] = [173, 199, 215, 255];

pub const SETTING_UNSEL: [u8; 4] = [36, 34, 41, 255];
pub const SETTING_SEL: [u8; 4] = [45, 42, 59, 255];
pub const SETTING_SEC_BG: [u8; 4] = [48, 46, 55, 255];

// Rank 颜色
pub const RANK_SS: [u8; 4] = [0xDE, 0x31, 0xAE, 255];
pub const RANK_S:  [u8; 4] = [0x02, 0xB5, 0xC3, 255];
pub const RANK_A:  [u8; 4] = [0x88, 0xDA, 0x20, 255];
pub const RANK_B:  [u8; 4] = [0xE3, 0xB1, 0x30, 255];
pub const RANK_C:  [u8; 4] = [0xFF, 0x8E, 0x5D, 255];
pub const RANK_D:  [u8; 4] = [0xFF, 0x5A, 0x5A, 255];

pub fn rank_color(rank: &str) -> [u8; 4] {
    match rank {
        "SS" => RANK_SS, "S" => RANK_S, "A" => RANK_A,
        "B" => RANK_B, "C" => RANK_C, _ => RANK_D,
    }
}

pub fn rank_from_acc(acc: f64, good: u32, ok: u32, meh: u32, miss: u32) -> &'static str {
    if acc >= 100.0 && good == 0 && ok == 0 && meh == 0 && miss == 0 { "SS" }
    else if acc >= 95.0 { "S" }
    else if acc >= 90.0 { "A" }
    else if acc >= 80.0 { "B" }
    else if acc >= 70.0 { "C" }
    else { "D" }
}

/// osu! 9点星数色标插值
pub fn star_color(stars: f64) -> [u8; 4] {
    let spectrum: [(f64, [u8; 3]); 9] = [
        (0.0, [170, 170, 170]), (1.0, [66, 144, 251]), (1.5, [79, 255, 213]),
        (2.5, [124, 255, 79]),  (3.5, [246, 240, 92]),  (4.5, [255, 128, 104]),
        (5.5, [255, 78, 111]),  (6.5, [198, 69, 184]),  (8.0, [101, 99, 222]),
    ];
    if stars <= spectrum[0].0 { let c = spectrum[0].1; return [c[0], c[1], c[2], 255]; }
    if stars >= spectrum[8].0 { let c = spectrum[8].1; return [c[0], c[1], c[2], 255]; }
    for i in 0..8 {
        let (s0, c0) = (spectrum[i].0, spectrum[i].1);
        let (s1, c1) = (spectrum[i+1].0, spectrum[i+1].1);
        if s0 <= stars && stars <= s1 {
            let t = (stars - s0) / (s1 - s0);
            let r = (c0[0] as f64 + (c1[0] as f64 - c0[0] as f64) * t) as u8;
            let g = (c0[1] as f64 + (c1[1] as f64 - c0[1] as f64) * t) as u8;
            let b = (c0[2] as f64 + (c1[2] as f64 - c0[2] as f64) * t) as u8;
            return [r, g, b, 255];
        }
    }
    [200, 200, 200, 255]
}
