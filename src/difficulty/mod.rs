pub mod azusa;
pub mod calibration;
pub mod daniel;
pub mod sunny;

use crate::beatmap::Note;

/// 难度分析结果
#[derive(Debug, Clone)]
pub struct DifficultyResult {
    /// 最终校准后的数值难度 (约 -2 ~ 20)
    pub numeric_difficulty: f64,
    /// 人类可读的 RC 段位标签，如 "Reform 7 mid"
    pub rc_label: String,
    /// Daniel 算法的原始星数
    pub star_rating: f64,
    /// 四个技能维度的分项评分
    pub dimensions: DifficultyDimensions,
}

impl DifficultyResult {
    /// 模糊难度标签: "about Reform 7 (mid)"
    pub fn fuzzy_label(&self) -> String {
        let n = self.numeric_difficulty;
        if !n.is_finite() { return "about Unknown".into(); }
        let tier_name = calibration::numeric_base_name(n.round() as i32);
        let center = n.round();
        let offset = n - center;
        let sub = if offset < -0.17 { "low" } else if offset > 0.17 { "high" } else { "mid" };
        format!("about {} ({})", tier_name, sub)
    }
    /// 用于缓存的简化摘要
    pub fn cache_key(&self) -> String { self.rc_label.clone() }
}

/// 四个技能维度的最高分位值
#[derive(Debug, Clone, Default)]
pub struct DifficultyDimensions {
    pub speed: f64,
    pub stamina: f64,
    pub chord: f64,
    pub tech: f64,
}

/// 主入口：分析谱面的人类可读难度
///
/// # Arguments
/// * `notes` - 谱面音符列表
/// * `song_rate` - 播放倍速 (1.0 = 原速)
///
/// # Returns
/// `DifficultyResult` 包含数值难度、RC 标签、星数和四个维度分项
pub fn analyze_difficulty(notes: &[Note], song_rate: f64, od: f64) -> Result<DifficultyResult, String> {
    if notes.is_empty() {
        return Err("No notes to analyze".into());
    }

    if song_rate <= 0.0 {
        return Err("Song rate must be positive".into());
    }

    // Step 1: Azusa 技能曲线分析
    let azusa_result = azusa::calculate_azusa(notes, song_rate)?;

    // Step 2: Daniel Rework 管道
    let (daniel_star, daniel_numeric) = daniel::calculate_daniel(notes, song_rate, od)?;

    // Step 3: Sunny Rework (独立星数)
    let sunny_star = sunny::calculate_sunny(notes, song_rate, od)?;
    let sunny_numeric = 2.85 + 1.33 * sunny_star;

    // Step 4: 校准混合 → RC 标签
    let (final_numeric, rc_label) = calibration::blend_and_calibrate(
        azusa_result.numeric,
        daniel_numeric,
        sunny_numeric,
        song_rate,
        &azusa_result,
    );

    Ok(DifficultyResult {
        numeric_difficulty: final_numeric,
        rc_label,
        star_rating: daniel_star,
        dimensions: DifficultyDimensions {
            speed: azusa_result.speed_q97,
            stamina: azusa_result.stamina_q97,
            chord: azusa_result.chord_q97,
            tech: azusa_result.tech_q97,
        },
    })
}

/// 从谱面路径快速获取难度标签（带 .rox 缓存）
pub fn analyze_path_label(map_path: &str, song_rate: f64, od: f64) -> String {
    match crate::beatmap::load_beatmap_rox(map_path) {
        Ok((_, notes)) => match analyze_difficulty(&notes, song_rate, od) {
            Ok(d) => d.fuzzy_label(),
            Err(_) => String::new(),
        },
        Err(_) => String::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    fn test_dir() -> &'static str {
        "/Users/apple/Documents/代码/osu!lazer重制版/Oxided Mania/referance/songs"
    }

    fn analyze_osu_file(path: &str) -> Result<DifficultyResult, String> {
        let (_, notes) = crate::beatmap::load_beatmap_rox(path)?;
        analyze_difficulty(&notes, 1.0, 8.0)
    }

    #[test]
    fn test_dan_v3_all() {
        let dirs = &[
            ("Jack", "1701660 Various Artists - Malody 4K Regular Dan v3-Jack"),
            ("Technical", "1701662 Various Artists - Malody 4K Regular Dan v3-Technical"),
            ("Speed", "1701664 Various Artists - Malody 4K Regular Dan v3-Speed"),
            ("Stream", "1701667 Various Artists - Malody 4K Regular Dan v3-Stream"),
        ];

        println!("\n========== Malody 4K Regular Dan v3 — 难度分析 ==========\n");

        for &(dan_type, dir_name) in dirs {
            let dir_path = Path::new(test_dir()).join(dir_name);
            println!("─── {} ───", dan_type);

            let mut results = Vec::new();
            if let Ok(entries) = std::fs::read_dir(&dir_path) {
                for entry in entries.flatten() {
                    let p = entry.path();
                    if p.extension().map_or(false, |e| e == "osu") {
                        let fname = p.to_string_lossy().to_string();
                        let title = p.file_stem()
                            .map(|s| s.to_string_lossy().to_string())
                            .unwrap_or_default();
                        if title.contains("delete upon download") {
                            continue;
                        }

                        match analyze_osu_file(&fname) {
                            Ok(r) => results.push((title, r)),
                            Err(e) => println!("  FAIL {} : {}", title, e),
                        }
                    }
                }
            }

            results.sort_by(|a, b| {
                a.1.numeric_difficulty.partial_cmp(&b.1.numeric_difficulty).unwrap()
            });

            for (_title, r) in &results {
                println!(
                    "  {:>5.2}  {:>18}  S={:>5.1}  M={:>5.1}  C={:>5.1}  T={:>5.1}",
                    r.numeric_difficulty,
                    r.rc_label,
                    r.dimensions.speed,
                    r.dimensions.stamina,
                    r.dimensions.chord,
                    r.dimensions.tech,
                );
            }
            println!();
        }
    }

    #[test]
    fn test_debug_one() {
        // 单个谱面调试：显示完整中间值
        let path = "/Users/apple/Documents/代码/osu!lazer重制版/Oxided Mania/referance/songs/1701667 Various Artists - Malody 4K Regular Dan v3-Stream/Various Artists - Malody 4K Regular Dan v3-Stream (Muses) [Reg-3 Hoshi ga Furanai Machi  Hylotl].osu";
        let (_, notes) = crate::beatmap::load_beatmap_rox(path).expect("load");
        println!("Notes count: {}", notes.len());

        let azusa_r = azusa::calculate_azusa(&notes, 1.0).expect("azusa");
        println!("Azusa primary_numeric: {:.4}", azusa_r.numeric);
        println!("  speed_q97: {:.2}, stamina_q97: {:.2}, chord_q97: {:.2}, tech_q97: {:.2}",
            azusa_r.speed_q97, azusa_r.stamina_q97, azusa_r.chord_q97, azusa_r.tech_q97);
        println!("  anchor_imbalance: {:.4}, chord_rate: {:.4}, jack_q95: {:.2}",
            azusa_r.anchor_imbalance, azusa_r.chord_rate, azusa_r.jack_q95);

        let (daniel_star, daniel_numeric) = daniel::calculate_daniel(&notes, 1.0, 8.0).expect("daniel");
        println!("Daniel star: {:.4}, daniel_numeric: {:.4}", daniel_star, daniel_numeric);

        let sunny_star = sunny::calculate_sunny(&notes, 1.0, 8.0).expect("sunny");
        let sunny_numeric = 2.85 + 1.33 * sunny_star;
        println!("Sunny star: {:.4}, sunny_numeric: {:.4}", sunny_star, sunny_numeric);

        let (final_numeric, rc_label) = calibration::blend_and_calibrate(
            azusa_r.numeric, daniel_numeric, sunny_numeric, 1.0, &azusa_r,
        );
        println!("Final: {:.4} -> {}", final_numeric, rc_label);
    }
}
