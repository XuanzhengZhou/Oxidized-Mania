// Azusa 技能曲线分析 — 从 azusaEstimator.js 移植
//
// 逐 note 计算 Speed/Stamina/Chord/Tech 四个维度的技能值，
// 使用 4 个指数衰减窗口模拟人类感知，分位数统计得出难度数值。

use crate::beatmap::Note;

// ─── 配置常量（对应 JS AZUSA_CONFIG） ───

const ROW_TOLERANCE_MS: f64 = 2.0;
const LOCAL_POWER: f64 = 2.15;
const DECAY_WINDOWS_MS: [f64; 4] = [140.0, 280.0, 560.0, 980.0];
const DECAY_WEIGHTS: [f64; 4] = [0.34, 0.30, 0.22, 0.14];
const SKILL_WEIGHTS: SkillWeights = SkillWeights {
    speed: 0.38,
    stamina: 0.26,
    chord: 0.18,
    tech: 0.18,
};

struct SkillWeights {
    speed: f64,
    stamina: f64,
    chord: f64,
    tech: f64,
}

// ─── 内部数据结构 ───

struct TapNote {
    t: f64,
    c: usize,
    hand: usize,
    row_size: usize,
}

pub(crate) struct AzusaResult {
    pub numeric: f64,
    pub speed_q97: f64,
    pub stamina_q97: f64,
    pub chord_q97: f64,
    pub tech_q97: f64,
    pub anchor_imbalance: f64,
    pub chord_rate: f64,
    pub jack_q95: f64,
}

// ─── 工具函数 ───

fn clamp(v: f64, lo: f64, hi: f64) -> f64 {
    v.max(lo).min(hi)
}

fn safe_div(a: f64, b: f64, fallback: f64) -> f64 {
    if !a.is_finite() || !b.is_finite() || b.abs() < 1e-9 {
        fallback
    } else {
        a / b
    }
}

fn quantile_from_sorted(sorted: &[f64], q: f64) -> f64 {
    if sorted.is_empty() {
        return 0.0;
    }
    let qc = clamp(q, 0.0, 1.0);
    let t = qc * (sorted.len() - 1) as f64;
    let left = t.floor() as usize;
    let right = (left + 1).min(sorted.len() - 1);
    let w = t - left as f64;
    sorted[left] * (1.0 - w) + sorted[right] * w
}

fn power_mean(values: &[f64], p: f64) -> f64 {
    if values.is_empty() {
        return 0.0;
    }
    let mut acc = 0.0;
    for &v in values {
        acc += v.max(0.0).powf(p);
    }
    (acc / values.len() as f64).powf(1.0 / p)
}

// ─── Tap 列表构建 ───

fn build_tap_notes(notes: &[Note]) -> Vec<TapNote> {
    let mut taps: Vec<TapNote> = notes
        .iter()
        .map(|n| TapNote {
            t: n.time,
            c: n.lane,
            hand: if n.lane < 2 { 0 } else { 1 },
            row_size: 1,
        })
        .collect();

    taps.sort_by(|a, b| {
        if (a.t - b.t).abs() > 1e-9 {
            a.t.partial_cmp(&b.t).unwrap()
        } else {
            a.c.cmp(&b.c)
        }
    });
    taps
}

fn annotate_rows(taps: &mut [TapNote], tolerance_ms: f64) {
    if taps.is_empty() {
        return;
    }
    let mut row_start = 0usize;
    for i in 1..=taps.len() {
        let should_flush = i == taps.len() || (taps[i].t - taps[row_start].t).abs() > tolerance_ms;
        if !should_flush {
            continue;
        }
        let row_size = i - row_start;
        for j in row_start..i {
            taps[j].row_size = row_size;
        }
        row_start = i;
    }
}

fn exp_decay_factor(dt_ms: f64, tau_ms: f64) -> f64 {
    if !dt_ms.is_finite() || dt_ms <= 0.0 {
        1.0
    } else {
        (-dt_ms / tau_ms).exp()
    }
}

fn skill_from_states(states: &[f64; 4]) -> f64 {
    let mut sum = 0.0;
    for i in 0..4 {
        sum += states[i] * DECAY_WEIGHTS[i];
    }
    sum
}

// ─── 难度曲线 ───

struct DifficultyCurve {
    local: Vec<f64>,
    speed_series: Vec<f64>,
    stamina_series: Vec<f64>,
    chord_series: Vec<f64>,
    tech_series: Vec<f64>,
    times: Vec<f64>,
    density250: Vec<f64>,
    density500: Vec<f64>,
    jack_raw_series: Vec<f64>,
    column_counts: [usize; 4],
    chord_note_count: usize,
}

struct States {
    speed: [f64; 4],
    stamina: [f64; 4],
    chord: [f64; 4],
    tech: [f64; 4],
}

fn build_difficulty_curve(taps: &[TapNote]) -> DifficultyCurve {
    if taps.is_empty() {
        return DifficultyCurve {
            local: vec![],
            speed_series: vec![],
            stamina_series: vec![],
            chord_series: vec![],
            tech_series: vec![],
            times: vec![],
            density250: vec![],
            density500: vec![],
            jack_raw_series: vec![],
            column_counts: [0; 4],
            chord_note_count: 0,
        };
    }

    let mut states = States {
        speed: [0.0; 4],
        stamina: [0.0; 4],
        chord: [0.0; 4],
        tech: [0.0; 4],
    };

    let mut last_by_column = [-1e9_f64; 4];
    let mut last_by_hand = [-1e9_f64; 2];

    let mut density250 = Vec::with_capacity(taps.len());
    let mut density500 = Vec::with_capacity(taps.len());
    let mut jack_raw_series = Vec::with_capacity(taps.len());
    let mut column_counts = [0usize; 4];
    let mut chord_note_count = 0usize;

    let mut cursor250 = 0usize;
    let mut cursor500 = 0usize;

    let mut local = Vec::with_capacity(taps.len());
    let mut speed_series = Vec::with_capacity(taps.len());
    let mut stamina_series = Vec::with_capacity(taps.len());
    let mut chord_series = Vec::with_capacity(taps.len());
    let mut tech_series = Vec::with_capacity(taps.len());
    let mut times = Vec::with_capacity(taps.len());

    let mut prev_time = taps[0].t;
    let mut prev_any1 = -1e9_f64;
    let mut prev_any2 = -1e9_f64;
    let mut prev_col = 0usize;

    for i in 0..taps.len() {
        let note = &taps[i];
        let t = note.t;
        let c = note.c;

        column_counts[c] += 1;
        if note.row_size >= 2 {
            chord_note_count += 1;
        }

        let dt_global = if i == 0 { 0.0 } else { (t - prev_time).max(0.0) };
        let dt_same = (t - last_by_column[c]).max(0.0);
        let dt_hand = (t - last_by_hand[note.hand]).max(0.0);
        let dt_any = (t - prev_any1).max(0.0);

        while cursor250 < i && t - taps[cursor250].t > 250.0 {
            cursor250 += 1;
        }
        while cursor500 < i && t - taps[cursor500].t > 500.0 {
            cursor500 += 1;
        }

        let d250 = (i - cursor250 + 1) as f64 / 0.25;
        let d500 = (i - cursor500 + 1) as f64 / 0.5;
        density250.push(d250);
        density500.push(d500);

        let jack = (190.0 / (dt_same + 35.0)).powf(1.16);
        jack_raw_series.push(jack);
        let stream = (170.0 / (dt_any + 30.0)).powf(1.07);
        let hand_stream = (185.0 / (dt_hand + 42.0)).powf(1.08);

        let movement = (c as f64 - prev_col as f64).abs() / 3.0;
        let rhythm_ratio = safe_div(dt_any.max(1.0), (t - prev_any2).max(1.0), 1.0);
        let rhythm_chaos = (clamp(rhythm_ratio, 0.2, 5.0).log2()).abs();

        let row_chord = (note.row_size.saturating_sub(1)) as f64;
        let chord = (row_chord + 1.0).powf(1.22) - 1.0;

        let speed_input = 0.54 * stream + 0.28 * hand_stream + 0.18 * jack;
        let stamina_input = 0.48 * (d500 / 11.0) + 0.27 * (d250 / 15.0) + 0.25 * stream;
        let chord_input = chord * (1.0 + 0.22 * stream.min(1.5));
        let tech_input = 0.45 * rhythm_chaos
            + 0.30 * movement
            + 0.25 * if row_chord > 0.0 { 1.0 + 0.3 * row_chord } else { 0.0 };

        for j in 0..4 {
            let tau = DECAY_WINDOWS_MS[j];
            let decay = exp_decay_factor(dt_global, tau);
            states.speed[j] = states.speed[j] * decay + speed_input;
            states.stamina[j] = states.stamina[j] * decay + stamina_input;
            states.chord[j] = states.chord[j] * decay + chord_input;
            states.tech[j] = states.tech[j] * decay + tech_input;
        }

        let speed_skill = skill_from_states(&states.speed);
        let stamina_skill = skill_from_states(&states.stamina);
        let chord_skill = skill_from_states(&states.chord);
        let tech_skill = skill_from_states(&states.tech);

        let p = LOCAL_POWER;
        let num = SKILL_WEIGHTS.speed * speed_skill.max(0.0).powf(p)
            + SKILL_WEIGHTS.stamina * stamina_skill.max(0.0).powf(p)
            + SKILL_WEIGHTS.chord * chord_skill.max(0.0).powf(p)
            + SKILL_WEIGHTS.tech * tech_skill.max(0.0).powf(p);
        let den = SKILL_WEIGHTS.speed
            + SKILL_WEIGHTS.stamina
            + SKILL_WEIGHTS.chord
            + SKILL_WEIGHTS.tech;
        let combined = (num / den).powf(1.0 / p);

        local.push(combined);
        speed_series.push(speed_skill);
        stamina_series.push(stamina_skill);
        chord_series.push(chord_skill);
        tech_series.push(tech_skill);
        times.push(t);

        prev_any2 = prev_any1;
        prev_any1 = t;
        prev_time = t;
        prev_col = c;
        last_by_column[c] = t;
        last_by_hand[note.hand] = t;
    }

    DifficultyCurve {
        local,
        speed_series,
        stamina_series,
        chord_series,
        tech_series,
        times,
        density250,
        density500,
        jack_raw_series,
        column_counts,
        chord_note_count,
    }
}

// ─── 分位数统计 ───

struct SkillStats {
    q97: f64,
    q94: f64,
    q90: f64,
    q75: f64,
    q50: f64,
    tail_mean: f64,
}

fn summarize_skill(values: &[f64]) -> SkillStats {
    let mut sorted = values.to_vec();
    sorted.sort_by(|a, b| a.partial_cmp(b).unwrap());

    let q97 = quantile_from_sorted(&sorted, 0.97);
    let q94 = quantile_from_sorted(&sorted, 0.94);
    let q90 = quantile_from_sorted(&sorted, 0.90);
    let q75 = quantile_from_sorted(&sorted, 0.75);
    let q50 = quantile_from_sorted(&sorted, 0.50);

    let tail_count = (8usize).max((sorted.len() as f64 * 0.04).floor() as usize);
    let tail = &sorted[sorted.len().saturating_sub(tail_count)..];
    let tail_mean = if tail.is_empty() {
        0.0
    } else {
        tail.iter().sum::<f64>() / tail.len() as f64
    };

    SkillStats {
        q97,
        q94,
        q90,
        q75,
        q50,
        tail_mean,
    }
}

// ─── 数值计算 ───

fn compute_azusa_numeric_from_curve(curve: &DifficultyCurve) -> f64 {
    let local = &curve.local;
    if local.is_empty() {
        return 0.0;
    }

    let speed = summarize_skill(&curve.speed_series);
    let stamina = summarize_skill(&curve.stamina_series);
    let chord = summarize_skill(&curve.chord_series);
    let tech = summarize_skill(&curve.tech_series);

    let density250 = power_mean(&curve.density250, 1.18);
    let density500 = power_mean(&curve.density500, 1.12);
    let note_count = curve.times.len() as f64;
    let length_boost = (1.0 + note_count / 140.0).ln();

    let peak_blend = 0.26 * speed.q97
        + 0.24 * stamina.q97
        + 0.18 * chord.q97
        + 0.12 * tech.q97
        + 0.07 * speed.q90
        + 0.05 * stamina.q90
        + 0.03 * chord.q90
        + 0.02 * tech.q90;

    let sustain_blend = 0.20 * speed.q75
        + 0.18 * stamina.q75
        + 0.11 * chord.q75
        + 0.08 * tech.q75
        + 0.12 * speed.tail_mean
        + 0.10 * stamina.tail_mean
        + 0.06 * chord.tail_mean
        + 0.05 * tech.tail_mean;

    let density_blend = 0.14 * (1.0 + density250).ln() + 0.22 * (1.0 + density500).ln();
    let mid_blend =
        0.18 * speed.q50 + 0.15 * stamina.q50 + 0.10 * chord.q50 + 0.08 * tech.q50;

    let raw = 0.58 * peak_blend
        + 0.24 * sustain_blend
        + 0.10 * density_blend
        + 0.08 * mid_blend
        + 0.06 * length_boost;
    let scaled = 0.82 + 0.41 * raw;

    let max_column = *curve.column_counts.iter().max().unwrap_or(&0) as f64;
    let anchor_imbalance =
        safe_div((max_column / note_count.max(1.0)) - 0.25, 0.75, 0.0);
    let chord_rate = safe_div(curve.chord_note_count as f64, note_count.max(1.0), 0.0);
    let mut jack_sorted = curve.jack_raw_series.clone();
    jack_sorted.sort_by(|a, b| a.partial_cmp(b).unwrap());
    let jack_q95 = quantile_from_sorted(&jack_sorted, 0.95);

    let jack_anchor_boost = clamp(
        1.65 * anchor_imbalance.max(0.0)
            * (1.0 - 1.85 * chord_rate).max(0.0)
            * (jack_q95 - 2.2).max(0.0),
        0.0,
        2.2,
    );

    let low_jack_boost = clamp(
        1.1 * clamp((12.2 - scaled) / 4.5, 0.0, 1.0)
            * (anchor_imbalance - 0.08).max(0.0)
            * (jack_q95 - 1.7).max(0.0)
            * (0.9 + 0.6 * (0.22 - chord_rate).max(0.0)),
        0.0,
        1.35,
    );

    let corrected = scaled + jack_anchor_boost + low_jack_boost;
    clamp(corrected, -2.0, 20.0)
}

// ─── 公共 API ───

pub(crate) fn calculate_azusa(notes: &[Note], song_rate: f64) -> Result<AzusaResult, String> {
    if notes.is_empty() {
        return Err("No notes for Azusa analysis".into());
    }

    let taps = build_tap_notes(notes);
    let time_scale = if song_rate != 0.0 { 1.0 / song_rate } else { 1.0 };

    let mut scaled_taps: Vec<TapNote> = if (time_scale - 1.0).abs() < 1e-9 {
        taps
    } else {
        taps.into_iter()
            .map(|n| TapNote {
                t: n.t * time_scale,
                c: n.c,
                hand: n.hand,
                row_size: n.row_size,
            })
            .collect()
    };

    annotate_rows(&mut scaled_taps, ROW_TOLERANCE_MS * time_scale);
    let curve = build_difficulty_curve(&scaled_taps);
    let numeric = compute_azusa_numeric_from_curve(&curve);

    let speed = summarize_skill(&curve.speed_series);
    let stamina = summarize_skill(&curve.stamina_series);
    let chord = summarize_skill(&curve.chord_series);
    let tech = summarize_skill(&curve.tech_series);

    let note_count = curve.times.len() as f64;
    let max_column = *curve.column_counts.iter().max().unwrap_or(&0) as f64;
    let anchor_imbalance =
        safe_div((max_column / note_count.max(1.0)) - 0.25, 0.75, 0.0);
    let chord_rate = safe_div(curve.chord_note_count as f64, note_count.max(1.0), 0.0);
    let mut jack_sorted = curve.jack_raw_series.clone();
    jack_sorted.sort_by(|a, b| a.partial_cmp(b).unwrap());
    let jack_q95 = quantile_from_sorted(&jack_sorted, 0.95);

    Ok(AzusaResult {
        numeric,
        speed_q97: speed.q97,
        stamina_q97: stamina.q97,
        chord_q97: chord.q97,
        tech_q97: tech.q97,
        anchor_imbalance,
        chord_rate,
        jack_q95,
    })
}
