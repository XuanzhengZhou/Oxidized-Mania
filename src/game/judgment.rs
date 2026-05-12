/// osu! DifficultyRange 公式: OD 0/5/10 三锚点线性插值
fn dr(d0: f64, d5: f64, d10: f64, od: f64) -> f64 {
    if od > 5.0 { d5 + (d10 - d5) * (od - 5.0) / 5.0 }
    else if od < 5.0 { d5 - (d5 - d0) * (5.0 - od) / 5.0 }
    else { d5 }
}

#[derive(Debug, Clone)]
pub struct JudgmentWindows {
    pub perfect: f64, // 305 (炫彩)
    pub great: f64,   // 300
    pub good: f64,    // 200
    pub ok: f64,      // 100
    pub meh: f64,     // 50
    pub miss: f64,    // MISS
}

impl JudgmentWindows {
    pub fn new(od: f64, song_rate: f64) -> Self {
        // osu!lazer 同款: floor(val * rate) + 0.5
        let f = |d0: f64, d5: f64, d10: f64| -> f64 {
            (dr(d0, d5, d10, od) * song_rate).floor() + 0.5
        };
        Self {
            perfect: f(22.4, 19.4, 13.9),
            great:   f(64.0, 49.0, 34.0),
            good:    f(97.0, 82.0, 67.0),
            ok:      f(127.0, 112.0, 97.0),
            meh:     f(151.0, 136.0, 121.0),
            miss:    f(188.0, 173.0, 158.0),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum JudgmentResult {
    Perfect, // 305 — 炫彩 (hit_300g)
    Great,   // 300
    Good,    // 200
    Ok,      // 100
    Meh,     // 50
    Miss,
}

impl JudgmentResult {
    pub fn score_value(self) -> u32 { match self {
        Self::Perfect => 305, Self::Great => 300,
        Self::Good => 200, Self::Ok => 100, Self::Meh => 50,
        Self::Miss => 0,
    }}
    pub fn name(self) -> &'static str { match self {
        Self::Perfect => "PERFECT", Self::Great => "GREAT",
        Self::Good => "GOOD", Self::Ok => "OK", Self::Meh => "MEH",
        Self::Miss => "MISS",
    }}
}

pub fn judge_tap(note_time: f64, current_time: f64, windows: &JudgmentWindows) -> JudgmentResult {
    let diff = (note_time - current_time).abs();
    if diff <= windows.perfect { JudgmentResult::Perfect }
    else if diff <= windows.great { JudgmentResult::Great }
    else if diff <= windows.good { JudgmentResult::Good }
    else if diff <= windows.ok { JudgmentResult::Ok }
    else if diff <= windows.meh { JudgmentResult::Meh }
    else { JudgmentResult::Miss }
}

pub fn judge_hold_release(end_time: f64, current_time: f64, windows: &JudgmentWindows) -> JudgmentResult {
    judge_tap(end_time, current_time, windows)
}
