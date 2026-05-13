// 校准 + 混合 + RC 标签映射 — 从 azusaEstimator.js 移植

use crate::difficulty::azusa::AzusaResult;

// ─── 校准数据表 ───

const AZUSA_CALIBRATION_LOW: &[(f64, f64, f64)] = &[
    (1.9220, 1.9220, 1.0000), (2.3660, 2.7684, 1.6667), (2.8394, 2.8394, 2.0000),
    (2.8584, 3.7162, 2.3333), (3.7798, 3.7798, 3.0000), (3.8667, 3.8667, 3.0000),
    (4.2067, 5.2039, 4.3333), (5.2506, 5.7713, 5.0667), (5.8603, 6.1512, 5.3333),
    (6.3292, 6.8785, 6.0000), (7.1715, 7.3617, 6.2000), (7.4079, 7.8734, 7.2000),
    (8.0160, 8.4003, 8.2500), (8.4133, 8.4133, 9.0000), (8.9031, 9.4775, 9.5667),
    (9.6488, 9.6488, 10.0000), (9.8301, 9.8301, 10.3000),
];

const AZUSA_CALIBRATION_HIGH: &[(f64, f64, f64)] = &[
    (11.4336, 11.4336, 10.4000), (11.4436, 11.4436, 10.5000), (11.6012, 11.6665, 10.6500),
    (11.6696, 12.2317, 11.5000), (12.3295, 12.3919, 11.7500), (12.5238, 12.5238, 12.0000),
    (12.5318, 12.8329, 12.1400), (12.8605, 12.9781, 12.2800), (12.9868, 13.1170, 12.7800),
    (13.2003, 13.4418, 12.7857), (13.4660, 13.5829, 12.9250), (13.6044, 13.9924, 13.3667),
    (14.0583, 14.0583, 13.4000), (14.0795, 14.2266, 13.4600), (14.2346, 14.2346, 13.6000),
    (14.2414, 14.2414, 13.7000), (14.2903, 14.2903, 14.0000), (14.3258, 14.4760, 14.1200),
    (14.5365, 14.6006, 14.1333), (14.7269, 14.8716, 14.1333), (15.0048, 15.0048, 14.4000),
    (15.0521, 15.0521, 14.4000), (15.0521, 15.0521, 14.4000), (15.0950, 15.0950, 14.4000),
    (15.2335, 15.2335, 14.4000), (15.2388, 15.5821, 14.7385), (15.6977, 15.7002, 14.8500),
    (15.7535, 16.1593, 15.0667), (16.2009, 16.2958, 15.1000), (16.3172, 16.4748, 15.7600),
    (16.5620, 16.9083, 15.9833), (16.9485, 16.9485, 16.0000), (17.0216, 17.3799, 16.1000),
    (17.4616, 17.4616, 16.4000), (17.5167, 17.5167, 16.4000), (17.5306, 17.9077, 16.6400),
    (18.1973, 18.1973, 17.2000), (18.2026, 18.2026, 17.2000), (18.4562, 19.3477, 17.9500),
];

const AZUSA_ISOTONIC: &[(f64, f64)] = &[
    (1.29, 1.0), (1.39, 1.0), (1.47, 1.0), (1.90, 2.0), (2.06, 2.0), (2.22, 2.0),
    (2.32, 2.0), (2.51, 3.0), (2.90, 3.3333), (2.98, 3.3333), (4.01, 4.0), (4.51, 4.0),
    (4.83, 4.2), (4.94, 5.0), (5.04, 5.0), (5.20, 5.0), (5.28, 5.0), (5.33, 5.6667),
    (5.59, 5.6667), (5.77, 6.0), (5.87, 6.0), (6.07, 6.6), (6.33, 6.7333), (6.92, 6.7333),
    (7.11, 7.0), (7.46, 8.3), (8.05, 8.3), (8.25, 8.3333), (8.48, 8.3333), (9.32, 9.1833),
    (9.62, 9.1833), (9.64, 9.5), (9.71, 9.5), (9.98, 10.325), (10.15, 10.325),
    (10.30, 10.3714), (10.99, 10.3714), (11.00, 10.9), (11.04, 10.9), (11.07, 11.2286),
    (11.36, 11.2286), (11.45, 11.8667), (11.74, 11.8667), (11.93, 12.0875),
    (12.20, 12.0875), (12.29, 12.4667), (12.52, 12.4667), (12.56, 12.5), (12.64, 12.5),
    (12.74, 12.56), (12.92, 12.56), (12.98, 12.6), (12.99, 12.7), (13.00, 13.0),
    (13.04, 13.2667), (13.28, 13.2667), (13.29, 13.5333), (13.33, 13.5333),
    (13.34, 13.55), (13.36, 13.55), (13.40, 13.62), (13.56, 13.62), (13.72, 13.8),
    (13.95, 14.0), (14.02, 14.0), (14.05, 14.05), (14.20, 14.05), (14.21, 14.2),
    (14.34, 14.2), (14.37, 14.2667), (14.44, 14.4), (14.47, 14.5), (14.52, 14.675),
    (14.67, 14.675), (14.80, 14.825), (14.90, 14.825), (14.93, 15.0), (15.15, 15.0),
    (15.31, 15.2), (15.35, 15.2), (15.37, 15.6667), (15.53, 15.6667), (15.54, 15.675),
    (15.72, 15.675), (15.72, 15.8), (15.75, 15.9), (15.78, 16.0), (16.07, 16.0),
    (16.09, 16.2667), (16.15, 16.2667), (16.35, 16.4), (16.41, 16.4), (16.51, 16.4),
    (16.53, 16.5333), (16.65, 16.5333), (17.55, 17.2), (17.68, 17.2), (17.91, 17.95),
    (18.02, 17.95),
];

const GREEK_NAMES: &[&str] = &[
    "Alpha", "Beta", "Gamma", "Delta", "Epsilon",
    "Emik Zeta", "Thaumiel Eta", "CloverWisp Theta", "Iota", "Kappa",
];

const RC_TIERS: &[(&str, f64)] = &[
    ("low", -0.4), ("mid/low", -0.2), ("mid", 0.0),
    ("mid/high", 0.2), ("high", 0.4),
];

// ─── 工具 ───

fn clamp(v: f64, lo: f64, hi: f64) -> f64 { v.max(lo).min(hi) }

fn safe_div(a: f64, b: f64, f: f64) -> f64 {
    if !a.is_finite() || !b.is_finite() || b.abs() < 1e-9 { f } else { a / b }
}

// ─── 估值 ───

fn estimate_sunny_numeric(star: f64) -> f64 {
    if !star.is_finite() { 0.0 } else { clamp(2.85 + 1.33 * star, -2.0, 20.0) }
}

// ─── 混合 ───

struct Blend {
    low_gate: f64,
    high_gate: f64,
    low_base: f64,
    high_base: f64,
    lg_src: f64,  // lowGateSource for lowLift calculation
}

fn resolve_blend(primary: f64, daniel: f64, sunny: f64, azusa: &AzusaResult) -> Blend {
    let lg_src = if daniel.is_finite() { daniel } else if sunny.is_finite() { sunny } else { primary };
    let low_gate = clamp((9.61 - lg_src) / 4.94, 0.0, 1.0);
    let high_gate = 1.0 - low_gate;

    let low_base = {
        let mut v = -8.317 + 1.536 * sunny + 0.011 * primary;
        if daniel.is_finite() { v += 0.049 * daniel; }
        if low_gate > 0.0 {
            v += low_gate * (0.442 * (sunny - 9.84).max(0.0)
                + 0.016 * (primary - 10.4).max(0.0)
                + 0.235 * (7.935 - sunny).max(0.0).powi(2));
        }
        v
    };

    let high_base = {
        let d = if daniel.is_finite() { daniel } else if sunny.is_finite() { sunny } else { primary };
        let pu = if primary.is_finite() { primary } else { d };
        let su = if sunny.is_finite() { sunny } else { d };
        let mut v = 0.809 * d + 0.057 * pu + 0.165 * su + 0.183;
        let hm = clamp((lg_src - 14.83) / 2.667, 0.0, 1.0);
        if hm > 0.0 {
            v += hm * (-0.154 * (pu - d).max(0.0) + 0.081 * (su - d).max(0.0));
        }
        v += clamp(0.96 * (azusa.jack_q95 - 2.08).max(0.0)
            * (0.24 - azusa.chord_rate).max(0.0)
            * (azusa.anchor_imbalance - 0.10).max(0.0), 0.0, 0.88);
        v
    };

    Blend { low_gate, high_gate, low_base, high_base, lg_src }
}

// ─── 校准插值 ───

fn interp_blocks(v: f64, blocks: &[(f64, f64, f64)]) -> f64 {
    if blocks.is_empty() { return v; }
    if v <= blocks[0].0 { return blocks[0].2; }
    let last = blocks.len() - 1;
    for i in 0..blocks.len() {
        let (x0, x1, y) = blocks[i];
        if v >= x0 && v <= x1 { return y; }
        if i < last && v > x1 && v < blocks[i + 1].0 {
            let t = safe_div(v - x1, blocks[i + 1].0 - x1, 0.0);
            return y * (1.0 - t) + blocks[i + 1].2 * t;
        }
    }
    blocks[last].2
}

fn interp_points(v: f64, pts: &[(f64, f64)]) -> f64 {
    if pts.len() < 2 { return v; }
    if v <= pts[0].0 { return pts[0].1; }
    let last = pts.len() - 1;
    if v >= pts[last].0 { return pts[last].1; }
    for i in 0..last {
        let (x0, y0) = pts[i]; let (x1, y1) = pts[i + 1];
        if v >= x0 && v <= x1 { return y0 + safe_div((v - x0) * (y1 - y0), x1 - x0, 0.0); }
    }
    v
}

// ─── 残差修正 ───

fn gap_residual(x: f64, b: &Blend, a: &AzusaResult, p: f64, s: f64, d: f64) -> f64 {
    let hg = b.high_gate; let ds = d - s; let sp = s - p;
    clamp(
        4.335282 + (-0.170459 * x) + (-1.622303 * (11.0 - x).max(0.0))
        + (1.328125 * (12.5 - x).max(0.0)) + (-0.042829 * (14.0 - x).max(0.0))
        + (-0.834997 * hg) + (3.060352 * hg * (11.0 - x).max(0.0))
        + (-1.744638 * hg * (12.5 - x).max(0.0)) + (0.409922 * ds) + (0.041072 * sp)
        + (-0.388231 * hg * ds) + (-0.170185 * hg * sp) + (3.466868 * a.anchor_imbalance)
        + (-1.743778 * a.chord_rate) + (-0.094758 * a.jack_q95)
        + (2.626366 * a.anchor_imbalance * a.jack_q95)
        + (1.836357 * a.chord_rate * a.jack_q95)
        + (-2.612648 * hg * a.anchor_imbalance) + (-2.493596 * hg * a.chord_rate),
        -1.2, 1.2)
}

fn post_gap_residual(x: f64, b: &Blend, a: &AzusaResult, p: f64, s: f64, d: f64) -> f64 {
    let hg = b.high_gate; let ds = d - s; let sp = s - p;
    clamp(0.4 * (
        0.979895 + (0.053556 * x) + (-1.050405 * (11.0 - x).max(0.0))
        + (0.942552 * (12.5 - x).max(0.0)) + (0.048841 * (14.0 - x).max(0.0))
        + (-1.636218 * hg) + (0.956025 * hg * (11.0 - x).max(0.0))
        + (-0.975188 * hg * (12.5 - x).max(0.0)) + (0.195107 * ds) + (-0.064291 * sp)
        + (-0.231542 * hg * ds) + (0.082201 * hg * sp)
        + (-0.634013 * a.anchor_imbalance) + (-0.490303 * a.chord_rate)
        + (-0.135176 * a.jack_q95) + (-0.992539 * a.anchor_imbalance * a.jack_q95)
        + (-0.164219 * a.chord_rate * a.jack_q95) + (-1.027392 * hg * a.anchor_imbalance)
        + (0.961530 * hg * a.chord_rate)),
        -1.0, 1.0)
}

// ─── 标签 ───

/// 数值 → 基础段位名 "Reform 7"
pub fn numeric_base_name(numeric: i32) -> String {
    match numeric {
        ..=-1 => format!("Intro {}", clamp((numeric + 3) as f64, 1.0, 3.0) as i32),
        0..=10 => format!("Reform {}", numeric),
        _ => {
            let i = clamp((numeric - 11) as f64, 0.0, (GREEK_NAMES.len() - 1) as f64) as usize;
            GREEK_NAMES[i].to_string()
        }
    }
}

fn rc_base_label(base: i32) -> String {
    match base {
        ..=0 => format!("Intro {}", clamp((base + 3) as f64, 1.0, 3.0) as i32),
        1..=10 => format!("Reform {}", base),
        _ => {
            let i = clamp((base - 11) as f64, 0.0, (GREEK_NAMES.len() - 1) as f64) as usize;
            GREEK_NAMES[i].to_string()
        }
    }
}

/// 数值 → RC 段位标签
pub fn numeric_to_rc_label(numeric: f64) -> String {
    if !numeric.is_finite() { return "Unknown".to_string(); }
    let c = clamp(numeric, -2.4, 20.4);
    let mut best = ("", -2i32, f64::INFINITY);
    for base in -2..=20 {
        for &(suf, off) in RC_TIERS {
            let d = (c - (base as f64 + off)).abs();
            if d < best.2 { best = (suf, base, d); }
        }
    }
    format!("{} {}", rc_base_label(best.1), best.0)
}

// ─── 主 API ───

pub(crate) fn blend_and_calibrate(
    azusa_numeric: f64, daniel_numeric: f64, sunny_numeric: f64,
    song_rate: f64, azusa: &AzusaResult,
) -> (f64, String) {
    // JS: Daniel 无 native numeric 时按速率缩放 Daniel 贡献
    let daniel_for_blend = {
        let hs = azusa_numeric.max(sunny_numeric).max(daniel_numeric);
        if hs < 14.0 && daniel_numeric.is_finite() {
            let speed_delta = song_rate - 1.0;
            let scale = if speed_delta < 0.0 {
                clamp(-speed_delta * 0.43, 0.0, 1.0)
            } else {
                clamp(speed_delta * 0.35, 0.0, 1.0)
            };
            daniel_numeric * scale
        } else {
            daniel_numeric
        }
    };
    let sunny_est = if sunny_numeric.is_finite() { sunny_numeric }
        else { estimate_sunny_numeric(daniel_numeric) };
    let b = resolve_blend(azusa_numeric, daniel_for_blend, sunny_est, azusa);

    // JS: lowLift = max(0, 9.889 - lowGateSource) * 0.257
    let low_lift = (9.889 - b.lg_src).max(0.0) * 0.257;
    let mixed = b.low_base * b.low_gate + (b.high_base + low_lift) * b.high_gate;

    let cal = {
        let lo = interp_blocks(mixed, AZUSA_CALIBRATION_LOW);
        let hi = interp_blocks(mixed, AZUSA_CALIBRATION_HIGH);
        let s = b.low_gate + b.high_gate;
        if s <= 1e-6 { if mixed < 11.0 { lo } else { hi } }
        else { (b.low_gate * lo + b.high_gate * hi) / s }
    };

    let r1 = gap_residual(cal, &b, azusa, azusa_numeric, sunny_est, daniel_for_blend);
    let pre = clamp(cal + r1, -2.0, 20.0);
    let out = interp_points(pre, AZUSA_ISOTONIC);
    let r2 = post_gap_residual(out, &b, azusa, azusa_numeric, sunny_est, daniel_for_blend);
    let fin = clamp(out + r2, -2.0, 20.0);

    (fin, numeric_to_rc_label(fin))
}
