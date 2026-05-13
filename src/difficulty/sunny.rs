// Sunny Rework 难度算法 — 从 sunnyAlgorithm.js 精确移植
//
// 与 Daniel 的核心区别：
// 1. 完整 LN (Hold) 处理：keyUsage/400 覆盖 hold body
// 2. 不同 streamBooster 公式
// 3. 新增 Rbar (LN 释放密度)
// 4. DAll rightPart 包含 Rbar 项
// 5. LN 长度参与 totalNotes 修正

use crate::beatmap::{Note, NoteType};

// ═══ 二分/工具（同 Daniel） ═══

fn bisect_left(arr: &[f64], t: f64) -> usize {
    let (mut lo, mut hi) = (0usize, arr.len());
    while lo < hi { let m = (lo + hi) >> 1; if arr[m] < t { lo = m + 1; } else { hi = m; } }
    lo
}
fn bisect_right(arr: &[f64], t: f64) -> usize {
    let (mut lo, mut hi) = (0usize, arr.len());
    while lo < hi { let m = (lo + hi) >> 1; if arr[m] <= t { lo = m + 1; } else { hi = m; } }
    lo
}
fn cumulative_sum(x: &[f64], f: &[f64]) -> Vec<f64> {
    let mut c = vec![0.0; x.len()];
    for i in 1..x.len() { c[i] = c[i - 1] + f[i - 1] * (x[i] - x[i - 1]); }
    c
}
fn query_cumsum(q: f64, x: &[f64], cf: &[f64], f: &[f64]) -> f64 {
    if q <= x[0] { return 0.0; }
    if q >= x[x.len() - 1] { return cf[cf.len() - 1]; }
    let i = bisect_right(x, q).saturating_sub(1);
    cf[i] + f[i] * (q - x[i])
}
fn smooth_on_corners(x: &[f64], f: &[f64], w: f64, scale: f64, mode: &str) -> Vec<f64> {
    let cf = cumulative_sum(x, f);
    let (n, last) = (f.len(), x.len() - 1);
    let mut g = vec![0.0; n];
    for i in 0..n {
        let s = x[i]; let a = (s - w).max(x[0]); let b = (s + w).min(x[last]);
        let v = query_cumsum(b, x, &cf, f) - query_cumsum(a, x, &cf, f);
        g[i] = if mode == "avg" { if b > a { v / (b - a) } else { 0.0 } } else { scale * v };
    }
    g
}
fn interp_values(nx: &[f64], ox: &[f64], ov: &[f64]) -> Vec<f64> {
    let mut out = vec![0.0; nx.len()]; let mut idx = 0usize; let ol = ox.len() - 1;
    for i in 0..nx.len() {
        let x = nx[i];
        if x <= ox[0] { out[i] = ov[0]; continue; }
        if x >= ox[ol] { out[i] = ov[ol]; continue; }
        while idx + 1 < ox.len() && ox[idx + 1] < x { idx += 1; }
        let (x0, x1) = (ox[idx], ox[idx + 1]);
        if x1 == x0 { out[i] = ov[idx]; continue; }
        let t = (x - x0) / (x1 - x0);
        out[i] = ov[idx] + t * (ov[idx + 1] - ov[idx]);
    }
    out
}
fn step_interp(nx: &[f64], ox: &[f64], ov: &[f64]) -> Vec<f64> {
    let mut out = vec![0.0; nx.len()]; let mut idx = 0usize; let vl = ov.len().saturating_sub(1);
    for i in 0..nx.len() { let x = nx[i]; while idx + 1 < ox.len() && ox[idx + 1] <= x { idx += 1; } out[i] = ov[idx.min(vl)]; }
    out
}
fn rescale_high(sr: f64) -> f64 { if sr <= 9.0 { sr } else { 9.0 + (sr - 9.0) / 1.2 } }

fn merge_by_head(a: &[(usize, f64, f64)], b: &[(usize, f64, f64)]) -> Vec<(usize, f64, f64)> {
    let mut r = Vec::with_capacity(a.len() + b.len()); let (mut i, mut j) = (0, 0);
    while i < a.len() && j < b.len() {
        if a[i].1 <= b[j].1 { r.push(a[i]); i += 1; } else { r.push(b[j]); j += 1; }
    }
    r.extend_from_slice(&a[i..]); r.extend_from_slice(&b[j..]); r
}

// ═══ Sunny 特有：Note = (lane, head, tail)  tail=-1 为 Tap ═══

type SunnyNote = (usize, f64, f64);

// ═══ 预处理 ═══

struct Prep { x: f64, k: usize, t: f64, note_seq: Vec<SunnyNote>, note_seq_by_column: Vec<Vec<SunnyNote>>, ln_seq: Vec<SunnyNote>, tail_seq: Vec<SunnyNote> }

fn preprocess(notes: &[Note], song_rate: f64, od: f64) -> Result<Prep, String> {
    let ts = if song_rate != 0.0 { 1.0 / song_rate } else { 1.0 };

    let mut note_seq: Vec<SunnyNote> = notes.iter().map(|n| {
        let h = (n.time * ts).floor();
        let t = if n.note_type == NoteType::Hold { (n.end_time * ts).floor() } else { -1.0 };
        (n.lane, h, t)
    }).collect();

    note_seq.sort_by(|a, b| if (a.1 - b.1).abs() > 1e-9 { a.1.partial_cmp(&b.1).unwrap() } else { a.0.cmp(&b.0) });

    let k = 4usize;
    let mut nsbc: Vec<Vec<SunnyNote>> = vec![vec![]; k];
    for &n in &note_seq { if n.0 < k { nsbc[n.0].push(n); } }

    let ln_seq: Vec<SunnyNote> = note_seq.iter().filter(|n| n.2 >= 0.0).copied().collect();
    let mut tail_seq = ln_seq.clone();
    tail_seq.sort_by(|a, b| a.2.partial_cmp(&b.2).unwrap());

    let mut x = 0.3_f64 * ((64.5 - (od * 3.0).ceil()) / 500.0).sqrt();
    x = x.min(0.6 * (x - 0.09) + 0.09);

    let mh = note_seq.iter().map(|n| n.1).fold(0.0_f64, f64::max);
    let mt = note_seq.iter().map(|n| n.2).fold(0.0_f64, f64::max);
    let t = mh.max(mt) + 1.0;

    Ok(Prep { x, k, t, note_seq, note_seq_by_column: nsbc, ln_seq, tail_seq })
}

// ═══ Corners ═══

struct Corners { all: Vec<f64>, base: Vec<f64>, a: Vec<f64> }

fn get_corners(t: f64, ns: &[SunnyNote]) -> Corners {
    use std::collections::BTreeSet;
    let mut bs = BTreeSet::new();
    for &(_, h, tl) in ns { bs.insert(h as i64); if tl >= 0.0 { bs.insert(tl as i64); } }
    let cp: Vec<i64> = bs.iter().copied().collect();
    for s in &cp { bs.insert(s + 501); bs.insert(s - 499); bs.insert(s + 1); }
    bs.insert(0); bs.insert(t as i64);
    let base: Vec<f64> = bs.iter().map(|&v| v as f64).filter(|&v| v >= 0.0 && v <= t).collect();

    let mut as_ = BTreeSet::new();
    for &(_, h, tl) in ns { as_.insert(h as i64); if tl >= 0.0 { as_.insert(tl as i64); } }
    let cpa: Vec<i64> = as_.iter().copied().collect();
    for s in &cpa { as_.insert(s + 1000); as_.insert(s - 1000); }
    as_.insert(0); as_.insert(t as i64);
    let a: Vec<f64> = as_.iter().map(|&v| v as f64).filter(|&v| v >= 0.0 && v <= t).collect();

    let mut all_s = BTreeSet::new();
    for &v in &base { all_s.insert(v as i64); }
    for &v in &a { all_s.insert(v as i64); }
    let all: Vec<f64> = all_s.iter().map(|&v| v as f64).collect();
    Corners { all, base, a }
}

// ═══ Key Usage (Sunny: LN 感知) ═══

fn get_key_usage(k: usize, ns: &[SunnyNote], bc: &[f64]) -> Vec<Vec<bool>> {
    let tl = bc[bc.len() - 1];
    let mut ku: Vec<Vec<bool>> = (0..k).map(|_| vec![false; bc.len()]).collect();
    for &(c, h, t) in ns {
        if c >= k { continue; }
        let s = (h - 150.0).max(0.0);
        let e = if t < 0.0 { h + 150.0 } else { (t + 150.0).min(tl - 1.0) };
        for idx in bisect_left(bc, s)..bisect_left(bc, e) { ku[c][idx] = true; }
    }
    ku
}

fn get_key_usage_400(k: usize, ns: &[SunnyNote], bc: &[f64]) -> Vec<Vec<f64>> {
    let tm = bc[bc.len() - 1];
    let mut ku: Vec<Vec<f64>> = (0..k).map(|_| vec![0.0; bc.len()]).collect();
    for &(c, h, t) in ns {
        if c >= k { continue; }
        let st = h.max(0.0);
        let et = if t < 0.0 { h } else { t.min(tm - 1.0) };
        let l400 = bisect_left(bc, (st - 400.0).max(0.0));
        let l = bisect_left(bc, st);
        let r = bisect_left(bc, et);
        let r400 = bisect_left(bc, et + 400.0);  // JS uncapped — bisect_left 自然返回 len
        let body_val = 3.75 + (et - st).min(1500.0) / 150.0;
        for idx in l..r { ku[c][idx] += body_val; }
        for idx in l400..l { let d = bc[idx] - st; ku[c][idx] += 3.75 - (3.75 / 160000.0) * d * d; }
        for idx in r..r400 { let d = (bc[idx] - et).abs(); ku[c][idx] += 3.75 - (3.75 / 160000.0) * d * d; }
    }
    ku
}

// ═══ Anchor ═══

fn compute_anchor(k: usize, ku400: &[Vec<f64>], bc: &[f64]) -> Vec<f64> {
    let mut a = vec![0.0; bc.len()];
    for idx in 0..bc.len() {
        let mut cnt: Vec<f64> = (0..k).map(|c| ku400[c][idx]).collect();
        cnt.sort_by(|a, b| b.partial_cmp(a).unwrap());
        let nz: Vec<f64> = cnt.into_iter().filter(|&v| v != 0.0).collect();
        if nz.len() > 1 {
            let (mut walk, mut mw) = (0.0, 0.0);
            for i in 0..(nz.len() - 1) {
                let w = 1.0 - 4.0 * (0.5 - nz[i + 1] / nz[i]).powi(2);
                walk += nz[i] * w; mw += nz[i];
            }
            a[idx] = if mw > 0.0 { walk / mw } else { 0.0 };
        }
    }
    for v in &mut a { *v = 1.0 + (*v - 0.18).min(5.0 * (*v - 0.22).powi(3)); }
    a
}

// ═══ LN Bodies 稀疏表示 ═══

struct LnRep { points: Vec<f64>, cumsum: Vec<f64>, values: Vec<f64> }

fn ln_bodies_sparse(ln_seq: &[SunnyNote], tt: f64) -> LnRep {
    let mut diff = std::collections::BTreeMap::<i64, f64>::new();
    for &(_, h, tl) in ln_seq {
        let t0 = (h + 60.0).min(tl); let t1 = (h + 120.0).min(tl);
        *diff.entry(t0 as i64).or_insert(0.0) += 1.3;
        *diff.entry(t1 as i64).or_insert(0.0) += -0.3;
        *diff.entry(tl as i64).or_insert(0.0) -= 1.0;
    }
    let mut ps = std::collections::BTreeSet::new();
    ps.insert(0i64); ps.insert(tt as i64);
    for k in diff.keys() { ps.insert(*k); }
    let points: Vec<f64> = ps.iter().map(|&v| v as f64).collect();
    let (mut values, mut cumsum, mut curr) = (Vec::new(), vec![0.0], 0.0);
    for i in 0..(points.len() - 1) {
        if let Some(&d) = diff.get(&(points[i] as i64)) { curr += d; }
        let v = curr.min(2.5 + 0.5 * curr);
        values.push(v);
        cumsum.push(cumsum[cumsum.len() - 1] + (points[i + 1] - points[i]) * v);
    }
    LnRep { points, cumsum, values }
}

fn ln_sum(a: f64, b: f64, r: &LnRep) -> f64 {
    let i = bisect_right(&r.points, a).saturating_sub(1);
    let j = bisect_right(&r.points, b).saturating_sub(1);
    if i == j { return (b - a) * r.values[i]; }
    (r.points[i + 1] - a) * r.values[i] + r.cumsum[j] - r.cumsum[i + 1] + (b - r.points[j]) * r.values[j]
}

// ═══ Jbar ═══

fn compute_jbar(k: usize, x: f64, nsbc: &[Vec<SunnyNote>], bc: &[f64]) -> (Vec<Vec<f64>>, Vec<f64>) {
    let jnf = |d: f64| -> f64 { 1.0 - 7e-5 * (0.15 + (d - 0.08).abs()).powi(-4) };
    let mut jks: Vec<Vec<f64>> = (0..k).map(|_| vec![0.0; bc.len()]).collect();
    let mut dks: Vec<Vec<f64>> = (0..k).map(|_| vec![1e9; bc.len()]).collect();
    for col in 0..k {
        let notes = &nsbc[col];
        for i in 0..(notes.len().saturating_sub(1)) {
            let (s, e) = (notes[i].1, notes[i + 1].1);
            let (li, ri) = (bisect_left(bc, s), bisect_left(bc, e));
            if li >= ri { continue; }
            let delta = 0.001 * (e - s);
            let val = delta.recip() * (delta + 0.11 * x.powf(0.25)).recip() * jnf(delta);
            for idx in li..ri { jks[col][idx] = val; dks[col][idx] = delta; }
        }
    }
    let jbks: Vec<Vec<f64>> = (0..k).map(|c| smooth_on_corners(bc, &jks[c], 500.0, 0.001, "sum")).collect();
    let mut jb = vec![0.0; bc.len()];
    for i in 0..bc.len() {
        let (mut num, mut den) = (0.0, 0.0);
        for col in 0..k { let v = jbks[col][i]; let w = 1.0 / dks[col][i]; num += v.max(0.0).powi(5) * w; den += w; }
        jb[i] = (num / den.max(1e-9)).powf(0.2);
    }
    (dks, jb)
}

// ═══ Xbar (Sunny 修正 crossMatrix) ═══

fn xbar_cross(k: usize) -> Vec<f64> {
    match k {
        1 => vec![-1.0], 2 => vec![0.075, 0.075], 3 => vec![0.125, 0.05, 0.125],
        4 => vec![0.175, 0.25, 0.05, 0.25, 0.175],
        _ => (0..=k).map(|_| 1.0 / (k + 1) as f64).collect(),
    }
}

fn compute_xbar(k: usize, x: f64, nsbc: &[Vec<SunnyNote>], ac: &[Vec<usize>], bc: &[f64]) -> Vec<f64> {
    let cc = xbar_cross(k); let nk = k + 1;
    let mut xks: Vec<Vec<f64>> = (0..nk).map(|_| vec![0.0; bc.len()]).collect();
    let mut fc: Vec<Vec<f64>> = (0..nk).map(|_| vec![0.0; bc.len()]).collect();
    let acl = ac.len().saturating_sub(1);

    for kp in 0..nk {
        let n_pair: Vec<SunnyNote> = if kp == 0 { nsbc[0].clone() }
            else if kp == k { nsbc[k - 1].clone() }
            else { merge_by_head(&nsbc[kp - 1], &nsbc[kp]) };

        for i in 1..n_pair.len() {
            let (s, e) = (n_pair[i - 1].1, n_pair[i].1);
            let (li, ri) = (bisect_left(bc, s), bisect_left(bc, e));
            if li >= ri { continue; }
            let delta = 0.001 * (e - s);
            let mut val = 0.16 * x.max(delta).powi(-2);
            let lc = &ac[li.min(acl)]; let rc_ = &ac[ri.min(acl)];
            // JS: k-1 = -1 时 includes(-1) 永远 false; k=K 时 includes(K) 永远 false
            let left_miss = if kp == 0 { true } else { !lc.contains(&(kp - 1)) && !rc_.contains(&(kp - 1)) };
            let right_miss = if kp >= k { true } else { !lc.contains(&kp) && !rc_.contains(&kp) };
            if left_miss || right_miss { val *= 1.0 - cc[kp]; }
            let fv = (0.4 * delta.max(0.06).max(0.75 * x).powi(-2) - 80.0).max(0.0);
            for idx in li..ri { xks[kp][idx] = val; fc[kp][idx] = fv; }
        }
    }

    let mut xb = vec![0.0; bc.len()];
    for i in 0..bc.len() {
        let s1: f64 = (0..nk).map(|kp| xks[kp][i] * cc[kp]).sum();
        let s2: f64 = (0..k).map(|kp| { let p = fc[kp][i] * cc[kp] * fc[kp + 1][i] * cc[kp + 1]; if p > 0.0 { p.sqrt() } else { 0.0 } }).sum();
        xb[i] = s1 + s2;
    }
    smooth_on_corners(bc, &xb, 500.0, 0.001, "sum")
}

// ═══ Pbar (Sunny: 不同 streamBooster + LN) ═══

fn compute_pbar(x: f64, ns: &[SunnyNote], ln_rep: &LnRep, anchor: &[f64], bc: &[f64]) -> Vec<f64> {
    let sboost = |delta: f64| -> f64 {
        let e = 7.5 / delta;
        if 160.0 < e && e < 360.0 { 1.0 + 1.7e-7 * (e - 160.0) * (e - 360.0).powi(2) } else { 1.0 }
    };
    let mut ps = vec![0.0; bc.len()];

    for i in 0..(ns.len().saturating_sub(1)) {
        let (hl, hr) = (ns[i].1, ns[i + 1].1);
        let dt = hr - hl;
        if dt < 1e-9 {
            let spike = 1000.0 * (0.02 * (4.0 / x - 24.0)).powf(0.25);
            for idx in bisect_left(bc, hl)..bisect_right(bc, hl) { ps[idx] += spike; }
            continue;
        }
        let (li, ri) = (bisect_left(bc, hl), bisect_left(bc, hr));
        if li >= ri { continue; }
        let delta = 0.001 * dt;
        let v = 1.0 + 6.0 * 0.001 * ln_sum(hl, hr, ln_rep);
        let bv = sboost(delta);
        let inc = if delta < (2.0 * x) / 3.0 {
            delta.recip() * (0.08 * x.recip() * (1.0 - 24.0 * x.recip() * (delta - x / 2.0).powi(2))).powf(0.25) * bv.max(v)
        } else {
            delta.recip() * (0.08 * x.recip() * (1.0 - 24.0 * x.recip() * (x / 6.0).powi(2))).powf(0.25) * bv.max(v)
        };
        for idx in li..ri { ps[idx] += (inc * anchor[idx]).min(inc.max(inc * 2.0 - 10.0)); }
    }
    smooth_on_corners(bc, &ps, 500.0, 0.001, "sum")
}

// ═══ Abar ═══

fn compute_abar(k: usize, ac: &[Vec<usize>], dks: &[Vec<f64>], acorn: &[f64], bc: &[f64]) -> Vec<f64> {
    let nk = k.saturating_sub(1);
    if nk == 0 { return vec![1.0; acorn.len()]; }
    let mut dk: Vec<Vec<f64>> = (0..nk).map(|_| vec![0.0; bc.len()]).collect();
    let acl = ac.len().saturating_sub(1);
    for i in 0..bc.len() {
        let cols = &ac[i.min(acl)];
        for j in 0..(cols.len().saturating_sub(1)) {
            let (k0, k1) = (cols[j], cols[j + 1]);
            if k0 < nk { dk[k0][i] = (dks[k0][i] - dks[k1][i]).abs() + 0.4 * (dks[k0][i].max(dks[k1][i]) - 0.11).max(0.0); }
        }
    }
    let mut as_ = vec![1.0; acorn.len()];
    for i in 0..acorn.len() {
        let mut idx = bisect_left(bc, acorn[i]).min(bc.len().saturating_sub(1));
        let cols = &ac[idx.min(acl)];
        for j in 0..(cols.len().saturating_sub(1)) {
            let (k0, k1) = (cols[j], cols[j + 1]);
            if k0 >= nk { continue; }
            let dv = dk[k0][idx]; let d0 = dks[k0][idx]; let d1 = dks[k1][idx];
            if dv < 0.02 { as_[i] *= (0.75 + 0.5 * d0.max(d1)).min(1.0); }
            else if dv < 0.07 { as_[i] *= (0.65 + 5.0 * dv + 0.5 * d0.max(d1)).min(1.0); }
        }
    }
    smooth_on_corners(acorn, &as_, 250.0, 1.0, "avg")
}

// ═══ Rbar (Sunny 特有: LN 释放密度) ═══

fn next_in_col(note: &SunnyNote, times: &[f64], col_notes: &[SunnyNote]) -> SunnyNote {
    let idx = bisect_left(times, note.1);
    if idx + 1 < col_notes.len() { col_notes[idx + 1] } else { (0, 1e9, 1e9) }
}

fn compute_rbar(k: usize, x: f64, nsbc: &[Vec<SunnyNote>], tail_seq: &[SunnyNote], bc: &[f64]) -> Vec<f64> {
    let mut rs = vec![0.0; bc.len()];
    let tbc: Vec<Vec<f64>> = (0..k).map(|c| nsbc[c].iter().map(|n| n.1).collect()).collect();
    let mut il = Vec::with_capacity(tail_seq.len());
    for &(col, hi, ti) in tail_seq {
        let (_, hj, _) = next_in_col(&(col, hi, ti), &tbc[col], &nsbc[col]);
        let ih = 0.001 * (ti - hi - 80.0).abs() / x;
        let it = 0.001 * (hj - ti - 80.0).abs() / x;
        il.push(2.0 / (2.0 + (-5.0 * (ih - 0.75)).exp() + (-5.0 * (it - 0.75)).exp()));
    }
    for i in 0..(tail_seq.len().saturating_sub(1)) {
        let (ts, te) = (tail_seq[i].2, tail_seq[i + 1].2);
        let (li, ri) = (bisect_left(bc, ts), bisect_left(bc, te));
        if li >= ri { continue; }
        let rv = 0.08 * (0.001 * (te - ts)).powf(-0.5) * x.recip() * (1.0 + 0.8 * (il[i] + il[i + 1]));
        for idx in li..ri { rs[idx] = rv; }
    }
    smooth_on_corners(bc, &rs, 500.0, 0.001, "sum")
}

// ═══ CStep / KsStep ═══

fn compute_c_and_ks(k: usize, ns: &[SunnyNote], ku: &[Vec<bool>], bc: &[f64]) -> (Vec<f64>, Vec<f64>) {
    let mut nht: Vec<f64> = ns.iter().map(|n| n.1).collect();
    nht.sort_by(|a, b| a.partial_cmp(b).unwrap());
    let (mut cs, mut lo, mut hi) = (vec![0.0; bc.len()], 0usize, 0usize);
    for i in 0..bc.len() {
        let (_s, low, high) = (bc[i], bc[i] - 500.0, bc[i] + 500.0);
        while lo < nht.len() && nht[lo] < low { lo += 1; }
        while hi < nht.len() && nht[hi] < high { hi += 1; }
        cs[i] = (hi - lo) as f64;
    }
    let mut ks = vec![0.0; bc.len()];
    for i in 0..bc.len() { let c: usize = (0..k).filter(|&c| ku[c][i]).count(); ks[i] = (c as f64).max(1.0); }
    (cs, ks)
}

// ═══ 主计算 ═══

/// Sunny Rework: 计算星数
///
/// 返回 star_rating，用于 Azusa 校准管线
pub(crate) fn calculate_sunny(notes: &[Note], song_rate: f64, od: f64) -> Result<f64, String> {
    let p = preprocess(notes, song_rate, od)?;
    if p.note_seq.is_empty() || p.k == 0 { return Err("No notes".into()); }

    let c = get_corners(p.t, &p.note_seq);
    let ku = get_key_usage(p.k, &p.note_seq, &c.base);
    let ac: Vec<Vec<usize>> = (0..c.base.len()).map(|i| (0..p.k).filter(|&cl| ku[cl][i]).collect()).collect();
    let ku400 = get_key_usage_400(p.k, &p.note_seq, &c.base);
    let anchor = compute_anchor(p.k, &ku400, &c.base);

    let (dks, jb_base) = compute_jbar(p.k, p.x, &p.note_seq_by_column, &c.base);
    let jb = interp_values(&c.all, &c.base, &jb_base);
    let xb_base = compute_xbar(p.k, p.x, &p.note_seq_by_column, &ac, &c.base);
    let xb = interp_values(&c.all, &c.base, &xb_base);
    let ln_rep = ln_bodies_sparse(&p.ln_seq, p.t);
    let pb_base = compute_pbar(p.x, &p.note_seq, &ln_rep, &anchor, &c.base);
    let pb = interp_values(&c.all, &c.base, &pb_base);
    let ab_base = compute_abar(p.k, &ac, &dks, &c.a, &c.base);
    let ab = interp_values(&c.all, &c.a, &ab_base);
    let rb_base = compute_rbar(p.k, p.x, &p.note_seq_by_column, &p.tail_seq, &c.base);
    let rb = interp_values(&c.all, &c.base, &rb_base);

    let (c_step, ks_step) = compute_c_and_ks(p.k, &p.note_seq, &ku, &c.base);
    let carr = step_interp(&c.all, &c.base, &c_step);
    let ksarr = step_interp(&c.all, &c.base, &ks_step);

    let n = c.all.len();
    let mut da = vec![0.0; n];
    for i in 0..n {
        let ks = ksarr[i].max(1.0);
        let left = 0.4 * (ab[i].powf(3.0 / ks) * jb[i].min(8.0 + 0.85 * jb[i])).powf(1.5);
        let right = 0.6 * (ab[i].powf(2.0 / 3.0) * (0.8 * pb[i] + rb[i] * 35.0 / (carr[i] + 8.0))).powf(1.5);
        let sa = (left + right).powf(2.0 / 3.0);
        let ta = ab[i].powf(3.0 / ks) * xb[i] / (xb[i] + sa + 1.0);
        da[i] = 2.7 * sa.powf(0.5) * ta.powf(1.5) + sa * 0.27;
    }

    let mut gaps = vec![0.0; n];
    gaps[0] = (c.all[1] - c.all[0]) / 2.0;
    gaps[n - 1] = (c.all[n - 1] - c.all[n - 2]) / 2.0;
    for i in 1..(n - 1) { gaps[i] = (c.all[i + 1] - c.all[i - 1]) / 2.0; }
    let ew: Vec<f64> = carr.iter().enumerate().map(|(i, &cc)| cc * gaps[i]).collect();

    let mut si: Vec<usize> = (0..n).collect();
    si.sort_by(|&a, &b| da[a].partial_cmp(&da[b]).unwrap());
    let ds: Vec<f64> = si.iter().map(|&i| da[i]).collect();
    let ws: Vec<f64> = si.iter().map(|&i| ew[i]).collect();

    let mut cw = vec![0.0; ws.len()]; let mut rn = 0.0;
    for i in 0..ws.len() { rn += ws[i]; cw[i] = rn; }
    let tw = cw[cw.len() - 1];
    if !tw.is_finite() || tw <= 0.0 { return Ok(0.0); }

    let nc: Vec<f64> = cw.iter().map(|&w| w / tw).collect();
    let tp = [0.945, 0.935, 0.925, 0.915, 0.845, 0.835, 0.825, 0.815];
    let li = ds.len().saturating_sub(1);
    let pi: Vec<usize> = tp.iter().map(|&p| bisect_left(&nc, p).min(li)).collect();
    let g1: f64 = pi[0..4].iter().map(|&i| ds[i]).sum::<f64>() / 4.0;
    let g2: f64 = pi[4..8].iter().map(|&i| ds[i]).sum::<f64>() / 4.0;

    let (mut num, mut den) = (0.0, 0.0);
    for i in 0..ds.len() { num += ds[i].powi(5) * ws[i]; den += ws[i]; }
    let wm = (num / den.max(1e-9)).powf(0.2);
    let mut sr = 0.88 * g1 * 0.25 + 0.94 * g2 * 0.2 + wm * 0.55;

    let ln_len: f64 = p.ln_seq.iter().map(|&(_, h, t)| ((t - h).min(1000.0) / 200.0)).sum();
    let tn = p.note_seq.len() as f64 + 0.5 * ln_len;
    sr *= tn / (tn + 60.0);
    sr = rescale_high(sr) * 0.975;
    Ok(sr)
}
