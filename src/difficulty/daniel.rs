// Daniel Rework 难度算法 — 从 danielAlgorithm.js 移植
//
// 20 步数学管道：提取 Jbar(纵连)/Xbar(交错)/Pbar(连打)/Abar(协调)
// 四个密度函数，加权分位数融合为原始星数，再映射为 Daniel 数值难度。

use crate::beatmap::Note;

// ─── 常量和配置 ───

/// Note: OD is unused — Daniel has its own OD-independent path.
/// The OD-dependent branch exists in JS but is effectively dead code for this port.

// ─── 查找/二分工具 ───

fn bisect_left(arr: &[f64], target: f64) -> usize {
    let mut lo = 0usize;
    let mut hi = arr.len();
    while lo < hi {
        let mid = (lo + hi) >> 1;
        if arr[mid] < target {
            lo = mid + 1;
        } else {
            hi = mid;
        }
    }
    lo
}

fn bisect_right(arr: &[f64], target: f64) -> usize {
    let mut lo = 0usize;
    let mut hi = arr.len();
    while lo < hi {
        let mid = (lo + hi) >> 1;
        if arr[mid] <= target {
            lo = mid + 1;
        } else {
            hi = mid;
        }
    }
    lo
}

// ─── 累积和 / 滑动窗口平滑 ───

fn cumulative_sum(x: &[f64], f: &[f64]) -> Vec<f64> {
    let mut cf = vec![0.0; x.len()];
    for i in 1..x.len() {
        cf[i] = cf[i - 1] + f[i - 1] * (x[i] - x[i - 1]);
    }
    cf
}

fn query_cumsum(q: f64, x: &[f64], cf: &[f64], f: &[f64]) -> f64 {
    if q <= x[0] {
        return 0.0;
    }
    if q >= x[x.len() - 1] {
        return cf[cf.len() - 1];
    }
    let i = bisect_right(x, q) - 1;
    cf[i] + f[i] * (q - x[i])
}

fn smooth_on_corners(x: &[f64], f: &[f64], window: f64, scale: f64, mode: &str) -> Vec<f64> {
    let cf = cumulative_sum(x, f);
    let n = f.len();
    let mut g = vec![0.0; n];
    let last = x.len() - 1;
    for i in 0..n {
        let s = x[i];
        let a = (s - window).max(x[0]);
        let b = (s + window).min(x[last]);
        let val = query_cumsum(b, x, &cf, f) - query_cumsum(a, x, &cf, f);
        if mode == "avg" {
            g[i] = if b > a { val / (b - a) } else { 0.0 };
        } else {
            g[i] = scale * val;
        }
    }
    g
}

// ─── 插值工具 ───

fn interp_values(new_x: &[f64], old_x: &[f64], old_vals: &[f64]) -> Vec<f64> {
    let mut out = vec![0.0; new_x.len()];
    let mut idx = 0usize;
    let old_last = old_x.len() - 1;

    for i in 0..new_x.len() {
        let x = new_x[i];
        if x <= old_x[0] {
            out[i] = old_vals[0];
            continue;
        }
        if x >= old_x[old_last] {
            out[i] = old_vals[old_last];
            continue;
        }
        while idx + 1 < old_x.len() && old_x[idx + 1] < x {
            idx += 1;
        }
        let x0 = old_x[idx];
        let x1 = old_x[idx + 1];
        if x1 == x0 {
            out[i] = old_vals[idx];
            continue;
        }
        let t = (x - x0) / (x1 - x0);
        out[i] = old_vals[idx] + t * (old_vals[idx + 1] - old_vals[idx]);
    }
    out
}

fn step_interp(new_x: &[f64], old_x: &[f64], old_vals: &[f64]) -> Vec<f64> {
    let mut out = vec![0.0; new_x.len()];
    let mut idx = 0usize;
    let v_last = old_vals.len().saturating_sub(1);
    for i in 0..new_x.len() {
        let x = new_x[i];
        while idx + 1 < old_x.len() && old_x[idx + 1] <= x {
            idx += 1;
        }
        out[i] = old_vals[idx.min(v_last)];
    }
    out
}

// ─── 预处理：将 Note 转为 Daniel 需要的格式 ───

struct DanielPreprocess {
    x: f64,    // OD 相关常数
    k: usize,  // 轨道数 (固定 4)
    t: f64,    // 谱面总时长 ms
    note_seq: Vec<(usize, f64)>,       // (column, time_ms)
    note_seq_by_column: Vec<Vec<(usize, f64)>>,
}

fn preprocess(notes: &[Note], song_rate: f64, od: f64) -> Result<DanielPreprocess, String> {
    let time_scale = if song_rate != 0.0 { 1.0 / song_rate } else { 1.0 };

    let mut note_seq: Vec<(usize, f64)> = notes
        .iter()
        .map(|n| (n.lane, (n.time * time_scale).floor()))
        .collect();

    note_seq.sort_by(|a, b| {
        if (a.1 - b.1).abs() > 1e-9 {
            a.1.partial_cmp(&b.1).unwrap()
        } else {
            a.0.cmp(&b.0)
        }
    });

    let k = 4usize;

    let mut note_seq_by_column: Vec<Vec<(usize, f64)>> = vec![vec![]; k];
    for &n in &note_seq {
        let col = n.0;
        if col < k {
            note_seq_by_column[col].push(n);
        }
    }

    // x = 0.3 * sqrt((64.5 - ceil(OD*3)) / 500)
    let od_ceil = (od * 3.0).ceil();
    let mut x = 0.3 * ((64.5 - od_ceil) / 500.0_f64).sqrt();
    x = x.min(0.6 * (x - 0.09) + 0.09);

    let t = if note_seq.is_empty() {
        0.0
    } else {
        note_seq[note_seq.len() - 1].1 + 1.0
    };

    Ok(DanielPreprocess {
        x,
        k,
        t,
        note_seq,
        note_seq_by_column,
    })
}

// ─── Corners: 时间采样点 ───

struct Corners {
    all_corners: Vec<f64>,
    base_corners: Vec<f64>,
    a_corners: Vec<f64>,
}

fn get_corners(t: f64, note_seq: &[(usize, f64)]) -> Corners {
    use std::collections::BTreeSet;

    let mut base_set = BTreeSet::new();
    for &(_, h) in note_seq {
        let h_i64 = h as i64;
        base_set.insert(h_i64);
        base_set.insert(h_i64 + 501);
        if h_i64 >= 499 {
            base_set.insert(h_i64 - 499);
        }
        base_set.insert(h_i64 + 1);
    }
    base_set.insert(0i64);
    base_set.insert(t as i64);

    let base_corners: Vec<f64> = base_set.into_iter().map(|v| v as f64).collect();

    let mut a_set = BTreeSet::new();
    for &(_, h) in note_seq {
        let h_i64 = h as i64;
        a_set.insert(h_i64);
        a_set.insert(h_i64 + 1000);
        if h_i64 >= 1000 {
            a_set.insert(h_i64 - 1000);
        }
    }
    a_set.insert(0i64);
    a_set.insert(t as i64);

    let a_corners: Vec<f64> = a_set.into_iter().map(|v| v as f64).collect();

    let mut all_set = BTreeSet::new();
    for &v in &base_corners {
        all_set.insert(v as i64);
    }
    for &v in &a_corners {
        all_set.insert(v as i64);
    }
    let all_corners: Vec<f64> = all_set.into_iter().map(|v| v as f64).collect();

    Corners {
        all_corners,
        base_corners,
        a_corners,
    }
}

// ─── Key Usage: 轨道活跃标记 ───

fn get_key_usage(k: usize, note_seq: &[(usize, f64)], base_corners: &[f64]) -> Vec<Vec<u8>> {
    let mut key_usage: Vec<Vec<u8>> = (0..k).map(|_| vec![0u8; base_corners.len()]).collect();

    for &(col, h) in note_seq {
        if col >= k {
            continue;
        }
        let t_last = base_corners[base_corners.len() - 1];
        let start = (h - 150.0).max(0.0);
        let end = (h + 150.0).min(t_last - 1.0);
        let left = bisect_left(base_corners, start);
        let right = bisect_left(base_corners, end);
        for idx in left..right {
            key_usage[col][idx] = 1;
        }
    }
    key_usage
}

fn get_key_usage_400(k: usize, note_seq: &[(usize, f64)], base_corners: &[f64]) -> Vec<Vec<f64>> {
    let mut ku: Vec<Vec<f64>> = (0..k)
        .map(|_| vec![0.0; base_corners.len()])
        .collect();

    let t_max = base_corners[base_corners.len() - 1];

    for &(col, h) in note_seq {
        if col >= k {
            continue;
        }
        let left_idx = bisect_left(base_corners, (h - 400.0).max(0.0));
        let center_idx = bisect_left(base_corners, h);
        let right_idx = bisect_left(base_corners, (h + 400.0).min(t_max));

        if center_idx < base_corners.len() {
            ku[col][center_idx] += 3.75;
        }
        for idx in left_idx..center_idx {
            let d = base_corners[idx] - h;
            ku[col][idx] += 3.75 - (3.75 / 160000.0) * d * d;
        }
        for idx in (center_idx + 1)..right_idx {
            let d = base_corners[idx] - h;
            ku[col][idx] += 3.75 - (3.75 / 160000.0) * d * d;
        }
    }
    ku
}

// ─── Anchor: 锚键检测 ───

fn compute_anchor(k: usize, key_usage_400: &[Vec<f64>], base_corners: &[f64]) -> Vec<f64> {
    let mut anchor = vec![0.0; base_corners.len()];

    for idx in 0..base_corners.len() {
        let mut counts: Vec<f64> = (0..k).map(|c| key_usage_400[c][idx]).collect();
        counts.sort_by(|a, b| b.partial_cmp(a).unwrap());

        let non_zero: Vec<f64> = counts.into_iter().filter(|&v| v > 0.0).collect();
        let mut raw = 0.0_f64;
        if non_zero.len() > 1 {
            let mut walk = 0.0;
            let mut max_walk = 0.0;
            for i in 0..(non_zero.len() - 1) {
                let ratio = non_zero[i + 1] / non_zero[i];
                let weight = 1.0 - 4.0 * (0.5 - ratio) * (0.5 - ratio);
                walk += non_zero[i] * weight;
                max_walk += non_zero[i];
            }
            raw = if max_walk > 0.0 { walk / max_walk } else { 0.0 };
        }
        anchor[idx] = 1.0 + (raw - 0.18).min(5.0 * (raw - 0.22).powi(3));
    }
    anchor
}

// ─── Jbar: 纵连密度 ───

fn compute_jbar(
    k: usize,
    x: f64,
    note_seq_by_column: &[Vec<(usize, f64)>],
    base_corners: &[f64],
) -> (Vec<Vec<f64>>, Vec<f64>) {
    let jack_nerfer = |delta: f64| -> f64 { 1.0 - 7e-5 * (0.15 + (delta - 0.08).abs()).powi(-4) };

    let mut jks: Vec<Vec<f64>> = (0..k).map(|_| vec![0.0; base_corners.len()]).collect();
    let mut delta_ks: Vec<Vec<f64>> = (0..k)
        .map(|_| vec![1e9_f64; base_corners.len()])
        .collect();

    for col in 0..k {
        let notes = &note_seq_by_column[col];
        for i in 0..(notes.len().saturating_sub(1)) {
            let start = notes[i].1;
            let end = notes[i + 1].1;
            if end <= start {
                continue;
            }

            let left_idx = bisect_left(base_corners, start);
            let right_idx = bisect_left(base_corners, end);
            if left_idx >= right_idx {
                continue;
            }

            let delta = 0.001 * (end - start);
            let val = delta.recip() * (delta + 0.11 * x.powf(0.25)).recip() * jack_nerfer(delta);

            for idx in left_idx..right_idx {
                jks[col][idx] = val;
                delta_ks[col][idx] = delta;
            }
        }
    }

    let jbar_ks: Vec<Vec<f64>> = (0..k)
        .map(|c| smooth_on_corners(base_corners, &jks[c], 500.0, 0.001, "sum"))
        .collect();

    let mut jbar = vec![0.0; base_corners.len()];
    for i in 0..base_corners.len() {
        let mut num = 0.0;
        let mut den = 0.0;
        for col in 0..k {
            let v = jbar_ks[col][i];
            let w = 1.0 / delta_ks[col][i].max(1e-9);
            num += v.max(0.0).powi(5) * w;
            den += w;
        }
        jbar[i] = (num / den.max(1e-9)).powf(0.2);
    }

    (delta_ks, jbar)
}

// ─── Xbar: 交错密度 ───

fn get_cross_matrix(k: usize) -> Vec<f64> {
    // JS crossMatrix 按 K 直接索引，crossMatrix[K] 有 K+1 个元素
    match k {
        0 => vec![-1.0],
        1 => vec![-1.0],
        2 => vec![0.075, 0.075, 0.075],
        3 => vec![0.125, 0.05, 0.125, 0.125],
        4 => vec![0.175, 0.25, 0.05, 0.25, 0.175],
        _ => (0..=k).map(|_| 1.0 / (k + 1) as f64).collect(),
    }
}

fn compute_xbar(
    k: usize,
    x: f64,
    note_seq_by_column: &[Vec<(usize, f64)>],
    active_columns: &[Vec<usize>],
    base_corners: &[f64],
) -> Vec<f64> {
    let cross_coeff = get_cross_matrix(k);
    let nk = k + 1;
    let mut xks: Vec<Vec<f64>> = (0..nk).map(|_| vec![0.0; base_corners.len()]).collect();
    let mut fast_cross: Vec<Vec<f64>> = (0..nk).map(|_| vec![0.0; base_corners.len()]).collect();
    let ac_last = active_columns.len().saturating_sub(1);

    for kp in 0..nk {
        let notes_in_pair: Vec<(usize, f64)> = if kp == 0 {
            note_seq_by_column[0].clone()
        } else if kp == k {
            note_seq_by_column[k - 1].clone()
        } else {
            let mut merged =
                [note_seq_by_column[kp - 1].clone(), note_seq_by_column[kp].clone()].concat();
            merged.sort_by(|a, b| a.1.partial_cmp(&b.1).unwrap());
            merged
        };

        for i in 1..notes_in_pair.len() {
            let start = notes_in_pair[i - 1].1;
            let end = notes_in_pair[i].1;
            if end <= start {
                continue;
            }

            let left_idx = bisect_left(base_corners, start);
            let right_idx = bisect_left(base_corners, end);
            if right_idx <= left_idx {
                continue;
            }

            let delta = 0.001 * (end - start);
            let mut val = 0.16 * x.max(delta).powi(-2);

            let left_cols = &active_columns[left_idx.min(ac_last)];
            let right_cols = &active_columns[right_idx.min(ac_last)];

            let left_inactive = !left_cols.contains(&(kp.saturating_sub(1)))
                && !right_cols.contains(&(kp.saturating_sub(1)));
            let right_inactive =
                !left_cols.contains(&kp.min(k - 1)) && !right_cols.contains(&kp.min(k - 1));

            if left_inactive || right_inactive {
                val *= 1.0 - cross_coeff[kp];
            }

            let fast_val = (0.4 * delta.max(0.06).max(0.75 * x).powi(-2) - 80.0).max(0.0);

            for idx in left_idx..right_idx {
                xks[kp][idx] = val;
                fast_cross[kp][idx] = fast_val;
            }
        }
    }

    let mut x_base = vec![0.0; base_corners.len()];
    for i in 0..base_corners.len() {
        let mut sum1 = 0.0;
        for kp in 0..nk {
            sum1 += xks[kp][i] * cross_coeff[kp];
        }
        let mut sum2 = 0.0;
        for kp in 0..k {
            let pair = fast_cross[kp][i] * cross_coeff[kp] * fast_cross[kp + 1][i] * cross_coeff[kp + 1];
            if pair > 0.0 {
                sum2 += pair.sqrt();
            }
        }
        x_base[i] = sum1 + sum2;
    }

    smooth_on_corners(base_corners, &x_base, 500.0, 0.001, "sum")
}

// ─── Pbar: 连打密度 ───

fn compute_pbar(
    x: f64,
    note_seq: &[(usize, f64)],
    anchor: &[f64],
    base_corners: &[f64],
) -> Vec<f64> {
    let stream_booster = |delta: f64| -> f64 {
        let bpm_max = (7.5 / delta.max(1e-9)).min(420.0);
        let primary = 0.10 / (1.0 + (-0.06 * (bpm_max - 175.0)).exp());
        let secondary = if bpm_max >= 200.0 && bpm_max <= 350.0 {
            0.30 * (1.0 - (-0.02 * (bpm_max - 200.0)).exp())
        } else {
            0.0
        };
        1.0 + primary + secondary
    };

    let mut p_step = vec![0.0; base_corners.len()];

    for i in 0..(note_seq.len().saturating_sub(1)) {
        let hl = note_seq[i].1;
        let hr = note_seq[i + 1].1;
        let delta_time = hr - hl;

        if delta_time < 1e-9 {
            let spike = 1000.0 * (0.02 * (4.0 / x - 24.0)).powf(0.25);
            let left = bisect_left(base_corners, hl);
            let right = bisect_right(base_corners, hl);
            for idx in left..right {
                p_step[idx] += spike;
            }
            continue;
        }

        let left_idx = bisect_left(base_corners, hl);
        let right_idx = bisect_left(base_corners, hr);
        if right_idx <= left_idx {
            continue;
        }

        let delta = 0.001 * delta_time;
        let b_val = stream_booster(delta);
        let base_inc = (0.08 * x.recip() * (1.0 - 24.0 * x.recip() * (x / 6.0).powi(2))).powf(0.25);

        let inc = if delta < (2.0 * x) / 3.0 {
            delta.recip()
                * (0.08 * x.recip() * (1.0 - 24.0 * x.recip() * (delta - x / 2.0).powi(2)))
                    .powf(0.25)
                * b_val.max(1.0)
        } else {
            delta.recip() * base_inc * b_val.max(1.0)
        };

        for idx in left_idx..right_idx {
            let boosted = inc * anchor[idx];
            p_step[idx] += boosted.min(inc.max(inc * 2.0 - 10.0));
        }
    }

    smooth_on_corners(base_corners, &p_step, 500.0, 0.001, "sum")
}

// ─── Abar: 协调密度 ───

fn compute_abar(
    k: usize,
    active_columns: &[Vec<usize>],
    delta_ks: &[Vec<f64>],
    a_corners: &[f64],
    base_corners: &[f64],
) -> Vec<f64> {
    let nk = k.saturating_sub(1);
    if nk == 0 {
        return vec![1.0; a_corners.len()];
    }

    let mut dks: Vec<Vec<f64>> = (0..nk).map(|_| vec![0.0; base_corners.len()]).collect();

    for i in 0..base_corners.len() {
        let cols = &active_columns[i.min(active_columns.len().saturating_sub(1))];
        for j in 0..(cols.len().saturating_sub(1)) {
            let k0 = cols[j];
            let k1 = cols[j + 1];
            if k0 < nk {
                dks[k0][i] = (delta_ks[k0][i] - delta_ks[k1][i]).abs()
                    + 0.4 * (delta_ks[k0][i].max(delta_ks[k1][i]) - 0.11).max(0.0);
            }
        }
    }

    let mut a_step = vec![1.0; a_corners.len()];
    let ac_last = active_columns.len().saturating_sub(1);

    for i in 0..a_corners.len() {
        let mut idx = bisect_left(base_corners, a_corners[i]);
        idx = idx.min(base_corners.len().saturating_sub(1));

        let cols = &active_columns[idx.min(ac_last)];
        for j in 0..(cols.len().saturating_sub(1)) {
            let k0 = cols[j];
            let k1 = cols[j + 1];
            if k0 >= nk {
                continue;
            }
            let d_val = dks[k0][idx];
            let dk0 = delta_ks[k0][idx];
            let dk1 = delta_ks[k1][idx];

            if d_val < 0.02 {
                a_step[i] *= (0.75 + 0.5 * dk0.max(dk1)).min(1.0);
            } else if d_val < 0.07 {
                a_step[i] *= (0.65 + 5.0 * d_val + 0.5 * dk0.max(dk1)).min(1.0);
            }
        }
    }

    smooth_on_corners(a_corners, &a_step, 250.0, 1.0, "avg")
}

// ─── CStep / KsStep ───

fn compute_c_and_ks(
    k: usize,
    note_seq: &[(usize, f64)],
    key_usage: &[Vec<u8>],
    base_corners: &[f64],
) -> (Vec<f64>, Vec<f64>) {
    let mut note_hit_times: Vec<f64> = note_seq.iter().map(|n| n.1).collect();
    note_hit_times.sort_by(|a, b| a.partial_cmp(b).unwrap());

    let mut c_step = vec![0.0; base_corners.len()];
    let mut lo = 0usize;
    let mut hi = 0usize;

    for i in 0..base_corners.len() {
        let s = base_corners[i];
        let low = s - 500.0;
        let high = s + 500.0;

        while lo < note_hit_times.len() && note_hit_times[lo] < low {
            lo += 1;
        }
        while hi < note_hit_times.len() && note_hit_times[hi] < high {
            hi += 1;
        }
        c_step[i] = (hi - lo) as f64;
    }

    let mut ks_step = vec![0.0; base_corners.len()];
    for i in 0..base_corners.len() {
        let mut count = 0;
        for col in 0..k {
            if key_usage[col][i] > 0 {
                count += 1;
            }
        }
        ks_step[i] = (count as f64).max(1.0);
    }

    (c_step, ks_step)
}

// ─── 主计算 ───

/// Daniel Rework 算法：计算原始星数和 Daniel 数值难度
///
/// 返回 `(star_rating, daniel_numeric)`，其中 daniel_numeric 在 -2..20 范围。
pub(crate) fn calculate_daniel(notes: &[Note], song_rate: f64, od: f64) -> Result<(f64, f64), String> {
    let prep = preprocess(notes, song_rate, od)?;

    if prep.note_seq.is_empty() || prep.k == 0 || prep.t <= 0.0 {
        return Err("No valid notes for Daniel analysis".into());
    }

    let corners = get_corners(prep.t, &prep.note_seq);

    let key_usage = get_key_usage(prep.k, &prep.note_seq, &corners.base_corners);

    let active_columns: Vec<Vec<usize>> = (0..corners.base_corners.len())
        .map(|i| {
            let mut active = Vec::new();
            for col in 0..prep.k {
                if key_usage[col][i] > 0 {
                    active.push(col);
                }
            }
            active
        })
        .collect();

    let key_usage_400 = get_key_usage_400(prep.k, &prep.note_seq, &corners.base_corners);
    let anchor = compute_anchor(prep.k, &key_usage_400, &corners.base_corners);

    let (delta_ks, jbar_base) =
        compute_jbar(prep.k, prep.x, &prep.note_seq_by_column, &corners.base_corners);
    let jbar = interp_values(&corners.all_corners, &corners.base_corners, &jbar_base);

    let xbar_base = compute_xbar(
        prep.k,
        prep.x,
        &prep.note_seq_by_column,
        &active_columns,
        &corners.base_corners,
    );
    let xbar = interp_values(&corners.all_corners, &corners.base_corners, &xbar_base);

    let pbar_base = compute_pbar(prep.x, &prep.note_seq, &anchor, &corners.base_corners);
    let pbar = interp_values(&corners.all_corners, &corners.base_corners, &pbar_base);

    let abar_base = compute_abar(
        prep.k,
        &active_columns,
        &delta_ks,
        &corners.a_corners,
        &corners.base_corners,
    );
    let abar = interp_values(&corners.all_corners, &corners.a_corners, &abar_base);

    let (c_step, ks_step) = compute_c_and_ks(prep.k, &prep.note_seq, &key_usage, &corners.base_corners);
    let c_arr = step_interp(&corners.all_corners, &corners.base_corners, &c_step);
    let ks_arr = step_interp(&corners.all_corners, &corners.base_corners, &ks_step);

    // DAll: 融合四个密度函数
    let n_all = corners.all_corners.len();
    let mut d_all = vec![0.0; n_all];
    for i in 0..n_all {
        let ks_i = ks_arr[i].max(1.0);
        let left =
            0.4 * (abar[i].powf(3.0 / ks_i) * jbar[i].min(8.0 + 0.85 * jbar[i])).powf(1.5);
        let right = 0.6 * (abar[i].powf(2.0 / 3.0) * (0.8 * pbar[i])).powf(1.5);
        let s_all = (left + right).powf(2.0 / 3.0);
        let t_all = abar[i].powf(3.0 / ks_i) * xbar[i] / (xbar[i] + s_all + 1.0);
        d_all[i] = 2.7 * s_all.powf(0.5) * t_all.powf(1.5) + s_all * 0.27;
    }

    // 权重: CArr * 区间宽度
    let mut gaps = vec![0.0; n_all];
    gaps[0] = (corners.all_corners[1] - corners.all_corners[0]) / 2.0;
    gaps[n_all - 1] =
        (corners.all_corners[n_all - 1] - corners.all_corners[n_all - 2]) / 2.0;
    for i in 1..(n_all - 1) {
        gaps[i] = (corners.all_corners[i + 1] - corners.all_corners[i - 1]) / 2.0;
    }

    let effective_weights: Vec<f64> =
        c_arr.iter().enumerate().map(|(i, &c)| c * gaps[i]).collect();

    // 按 DAll 排序
    let mut sorted_indices: Vec<usize> = (0..n_all).collect();
    sorted_indices.sort_by(|&a, &b| d_all[a].partial_cmp(&d_all[b]).unwrap());

    let d_sorted: Vec<f64> = sorted_indices.iter().map(|&i| d_all[i]).collect();
    let w_sorted: Vec<f64> = sorted_indices.iter().map(|&i| effective_weights[i]).collect();

    let mut cum_weights = vec![0.0; w_sorted.len()];
    let mut running = 0.0;
    for i in 0..w_sorted.len() {
        running += w_sorted[i];
        cum_weights[i] = running;
    }

    let total_weight = cum_weights[cum_weights.len() - 1];
    if !total_weight.is_finite() || total_weight <= 0.0 {
        return Ok((0.0, 0.0));
    }

    let norm_cum: Vec<f64> = cum_weights.iter().map(|&w| w / total_weight).collect();

    let target_percentiles = [0.945, 0.935, 0.925, 0.915, 0.845, 0.835, 0.825, 0.815];
    let last_idx = d_sorted.len().saturating_sub(1);
    let pct_indices: Vec<usize> = target_percentiles
        .iter()
        .map(|&p| bisect_left(&norm_cum, p).min(last_idx))
        .collect();

    let first_group: f64 = pct_indices[0..4].iter().map(|&i| d_sorted[i]).sum::<f64>() / 4.0;
    let second_group: f64 = pct_indices[4..8].iter().map(|&i| d_sorted[i]).sum::<f64>() / 4.0;

    let mut num = 0.0;
    let mut den = 0.0;
    for i in 0..d_sorted.len() {
        num += d_sorted[i].powi(5) * w_sorted[i];
        den += w_sorted[i];
    }
    let weighted_mean = (num / den.max(1e-9)).powf(0.2);

    let mut sr = 0.88 * first_group * 0.25 + 0.94 * second_group * 0.2 + weighted_mean * 0.55;
    sr *= prep.note_seq.len() as f64 / (prep.note_seq.len() as f64 + 60.0);
    sr = rescale_high(sr) * 0.975;

    let daniel_numeric = star_to_daniel_numeric(sr);

    Ok((sr, daniel_numeric))
}

fn rescale_high(sr: f64) -> f64 {
    if sr <= 9.0 {
        sr
    } else {
        9.0 + (sr - 9.0) * (1.0 / 1.2)
    }
}

fn star_to_daniel_numeric(star: f64) -> f64 {
    if star >= 6.56 {
        let normalized = ((star - 6.56) / 0.58).clamp(0.0, 9.99);
        (11.0 + normalized).clamp(-2.0, 20.0)
    } else {
        let low = -2.0 + 13.0 * (star / 6.56).clamp(0.0, 1.0).powf(1.72);
        low.clamp(-2.0, 20.0)
    }
}
