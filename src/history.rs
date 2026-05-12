use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::time::{SystemTime, UNIX_EPOCH};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HistoryRecord {
    pub score: u32,
    pub acc: f64,
    pub time: String,
    pub rate: f64,
    #[serde(default)]
    pub rank: String,
    #[serde(default)]
    pub od: f64,
    #[serde(default)]
    pub mirror: bool,
}

pub type HistoryData = HashMap<String, Vec<HistoryRecord>>;

pub fn load_history(path: &str) -> HistoryData {
    std::fs::read_to_string(path)
        .ok()
        .and_then(|s| serde_json::from_str(&s).ok())
        .unwrap_or_default()
}

pub fn save_history(path: &str, data: &HistoryData) {
    if let Ok(json) = serde_json::to_string_pretty(data) {
        let _ = std::fs::write(path, json);
    }
}

pub fn add_record(
    data: &mut HistoryData,
    map_rel_path: &str,
    score: u32,
    acc: f64,
    rate: f64,
    rank: &str,
    od: f64,
    mirror: bool,
) {
    let entry = data.entry(map_rel_path.to_string()).or_default();

    // YYYY-MM-DD HH:MM 格式 (UTC+8)
    let ts = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    let secs = ts + 8 * 3600; // UTC+8
    let days_since_epoch = secs / 86400;
    let rem = secs % 86400;
    let h = rem / 3600;
    let m = (rem % 3600) / 60;
    // 从 Unix epoch (1970-01-01) 计算实际日期
    let (y, mo, d) = unix_to_date(days_since_epoch as i64);
    let time_str = format!("{:04}-{:02}-{:02} {:02}:{:02}", y, mo, d, h, m);
    entry.push(HistoryRecord {
        score, acc, time: time_str, rate,
        rank: rank.to_string(), od, mirror,
    });
    entry.sort_by(|a, b| b.score.cmp(&a.score));
    entry.truncate(10);
}

// Unix epoch days → (year, month, day)
fn unix_to_date(days: i64) -> (i64, u32, u32) {
    let mut y = 1970i64;
    let mut remaining = days;
    loop {
        let days_in_year = if is_leap(y) { 366 } else { 365 };
        if remaining < days_in_year { break; }
        remaining -= days_in_year;
        y += 1;
    }
    let months_days = if is_leap(y) {
        [31,29,31,30,31,30,31,31,30,31,30,31]
    } else {
        [31,28,31,30,31,30,31,31,30,31,30,31]
    };
    let mut mo = 1u32;
    for &md in &months_days {
        if remaining < md as i64 { break; }
        remaining -= md as i64;
        mo += 1;
    }
    (y, mo, (remaining + 1) as u32)
}
fn is_leap(y: i64) -> bool { (y % 4 == 0 && y % 100 != 0) || y % 400 == 0 }
