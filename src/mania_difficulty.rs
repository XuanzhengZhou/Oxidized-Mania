// osu! 同款 Mania 难度星数计算
// 对标 Python mania_difficulty.py / C++ mania_difficulty_precise.cpp

use crate::beatmap::Note;

// ─── HitObject ───

#[derive(Debug, Clone)]
pub struct HitObject {
    pub start_time: f64,
    pub end_time: f64,
    pub column: usize,
}

impl HitObject {
    pub fn from_notes(notes: &[Note]) -> Vec<Self> {
        notes
            .iter()
            .map(|n| Self {
                start_time: n.time,
                end_time: n.end_time,
                column: n.lane,
            })
            .collect()
    }
}

// ─── DifficultyHitObject ───

struct DifficultyHitObject {
    start_time: f64,
    end_time: f64,
    column: usize,
    index: usize,
    delta_time: f64,          // (start_time - previous.start_time) / speed
    column_strain_time: f64,  // 同列相邻音符时间差 / speed
    previous_hit_objects: Vec<Option<usize>>,
}

// ─── 数学工具 ───

fn definitely_bigger(a: f64, b: f64, epsilon: f64) -> bool {
    a - b > epsilon
}

fn logistic(x: f64, midpoint_offset: f64, multiplier: f64, max_value: f64) -> f64 {
    max_value / (1.0 + (multiplier * (midpoint_offset - x)).exp())
}

fn apply_decay(value: f64, delta_time: f64, decay_base: f64) -> f64 {
    value * decay_base.powf(delta_time / 1000.0)
}

// ─── 个体应变评估 (单轨难度) ───

fn evaluate_individual_strain(
    obj: &DifficultyHitObject,
    all_objects: &[DifficultyHitObject],
) -> f64 {
    let mut hold_factor = 1.0;

    for prev_idx in obj.previous_hit_objects.iter().flatten() {
        let prev = &all_objects[*prev_idx];
        if definitely_bigger(prev.end_time, obj.end_time, 1.0)
            && definitely_bigger(obj.start_time, prev.start_time, 1.0)
        {
            hold_factor = 1.25;
            break;
        }
    }

    2.0 * hold_factor
}

// ─── 整体应变评估 (多轨交互难度) ───

const OVERALL_RELEASE_THRESHOLD: f64 = 30.0;

fn evaluate_overall_strain(
    obj: &DifficultyHitObject,
    all_objects: &[DifficultyHitObject],
    speed: f64,
) -> f64 {
    let release_threshold = OVERALL_RELEASE_THRESHOLD / speed;
    let mut is_overlapping = false;
    let mut closest_end_time = (obj.end_time - obj.start_time).abs();
    let mut hold_factor = 1.0;

    for prev_idx in obj.previous_hit_objects.iter().flatten() {
        let prev = &all_objects[*prev_idx];

        is_overlapping |= definitely_bigger(prev.end_time, obj.start_time, 1.0)
            && definitely_bigger(obj.end_time, prev.end_time, 1.0)
            && definitely_bigger(obj.start_time, prev.start_time, 1.0);

        if definitely_bigger(prev.end_time, obj.end_time, 1.0)
            && definitely_bigger(obj.start_time, prev.start_time, 1.0)
        {
            hold_factor = 1.25;
        }

        closest_end_time =
            closest_end_time.min((obj.end_time - prev.end_time).abs());
    }

    let hold_addition = if is_overlapping {
        logistic(closest_end_time, release_threshold, 0.27, 1.0)
    } else {
        0.0
    };

    (1.0 + hold_addition) * hold_factor
}

// ─── Strain 技能 ───

struct StrainSkill {
    section_length: f64,
    current_section_peak: f64,
    current_section_end: f64,
    strain_peaks: Vec<f64>,

    highest_individual_strain: f64,
    overall_strain: f64,
    current_strain: f64,
    individual_strains: Vec<f64>,
    speed: f64,
}

impl StrainSkill {
    fn new(total_columns: usize, speed: f64) -> Self {
        Self {
            section_length: 400.0 / speed,
            current_section_peak: 0.0,
            current_section_end: 0.0,
            strain_peaks: Vec::new(),
            highest_individual_strain: 1.0,
            overall_strain: 1.0,
            current_strain: 0.0,
            individual_strains: vec![0.0; total_columns],
            speed,
        }
    }

    fn process(&mut self, obj: &DifficultyHitObject, all_objects: &[DifficultyHitObject]) {
        if obj.index == 0 {
            self.current_section_end =
                (obj.start_time / self.section_length).ceil() * self.section_length;
            self.current_strain =
                self.calc_initial_strain(self.current_section_end, obj, all_objects);
        }

        while obj.start_time > self.current_section_end {
            self.save_peak();
            self.current_section_peak =
                self.calc_initial_strain(self.current_section_end, obj, all_objects);
            self.current_section_end += self.section_length;
        }

        self.current_strain = self.strain_value_at(obj, all_objects);
        self.current_section_peak = self.current_section_peak.max(self.current_strain);
    }

    fn strain_value_at(&mut self, obj: &DifficultyHitObject, all_objects: &[DifficultyHitObject]) -> f64 {
        self.current_strain += self.strain_value_of(obj, all_objects);
        self.current_strain
    }

    fn strain_value_of(&mut self, obj: &DifficultyHitObject, all_objects: &[DifficultyHitObject]) -> f64 {
        // 个体应变
        self.individual_strains[obj.column] = apply_decay(
            self.individual_strains[obj.column],
            obj.column_strain_time,
            0.125,
        );
        self.individual_strains[obj.column] +=
            evaluate_individual_strain(obj, all_objects);

        // chord: 同时刻音符取最大个体应变
        self.highest_individual_strain = if obj.delta_time <= 1.0 {
            self.highest_individual_strain
                .max(self.individual_strains[obj.column])
        } else {
            self.individual_strains[obj.column]
        };

        // 整体应变
        self.overall_strain = apply_decay(self.overall_strain, obj.delta_time, 0.30);
        self.overall_strain += evaluate_overall_strain(obj, all_objects, self.speed);

        // 增量 = highest + overall - current_strain
        // 使得 current_strain 始终等于 highest + overall
        self.highest_individual_strain + self.overall_strain - self.current_strain
    }

    fn calc_initial_strain(
        &self,
        offset: f64,
        obj: &DifficultyHitObject,
        all_objects: &[DifficultyHitObject],
    ) -> f64 {
        // index 从 1 开始（跳过了第一个 HitObject），index==1 没有前一个 DifficultyHitObject
        if obj.index <= 1 {
            return 0.0;
        }
        // all_objects[index-2] 是前一个 DifficultyHitObject
        let prev = &all_objects[obj.index - 2];
        apply_decay(
            self.highest_individual_strain,
            offset - prev.start_time,
            0.125,
        ) + apply_decay(self.overall_strain, offset - prev.start_time, 0.30)
    }

    fn save_peak(&mut self) {
        self.strain_peaks.push(self.current_section_peak);
    }

    fn difficulty_value(&self) -> f64 {
        let mut difficulty = 0.0;
        let mut weight: f64 = 1.0;

        let mut peaks: Vec<f64> = self
            .strain_peaks
            .iter()
            .copied()
            .filter(|&p| p > 0.0)
            .collect();
        if self.current_section_peak > 0.0 {
            peaks.push(self.current_section_peak);
        }

        peaks.sort_by(|a, b| b.partial_cmp(a).unwrap());

        for peak in peaks {
            difficulty += peak * weight;
            weight *= 0.9;
        }

        difficulty
    }
}

// ─── 难度计算器 ───

pub struct DifficultyCalculator {
    hit_objects: Vec<HitObject>,
    total_columns: usize,
    speed: f64,
}

impl DifficultyCalculator {
    pub fn new(hit_objects: Vec<HitObject>, total_columns: usize, speed: f64) -> Self {
        let mut objects = hit_objects;
        objects.sort_by(|a, b| a.start_time.partial_cmp(&b.start_time).unwrap());
        Self { hit_objects: objects, total_columns, speed }
    }

    pub fn calculate(&self) -> f64 {
        if self.hit_objects.is_empty() {
            return 0.0;
        }

        let diff_objects = self.build_diff_objects();
        if diff_objects.is_empty() {
            return 0.0;
        }

        let mut skill = StrainSkill::new(self.total_columns, self.speed);
        for i in 0..diff_objects.len() {
            skill.process(&diff_objects[i], &diff_objects);
        }

        skill.difficulty_value() * 0.018
    }

    fn build_diff_objects(&self) -> Vec<DifficultyHitObject> {
        let ho = &self.hit_objects;
        if ho.len() <= 1 {
            return Vec::new();
        }

        let n = ho.len() - 1;
        let mut objects: Vec<DifficultyHitObject> = Vec::with_capacity(n);

        // 第1步: 创建对象 (跳过第一个 HitObject, 对标 Python/C++)
        for i in 0..n {
            let current = &ho[i + 1];
            // 对标 Python: previous is None → delta_time = 0
            let delta_time = if i > 0 {
                (current.start_time - objects[i - 1].start_time) / self.speed
            } else {
                0.0
            };

            objects.push(DifficultyHitObject {
                start_time: current.start_time,
                end_time: current.end_time,
                column: current.column,
                index: i + 1,
                delta_time,
                column_strain_time: 0.0,
                previous_hit_objects: vec![None; self.total_columns],
            });
        }

        // 第2步: 传播 previous_hit_objects
        for i in 1..objects.len() {
            for k in 0..self.total_columns {
                objects[i].previous_hit_objects[k] =
                    objects[i - 1].previous_hit_objects[k];
            }
            let prev_col = objects[i - 1].column;
            objects[i].previous_hit_objects[prev_col] = Some(i - 1);
        }

        // 第3步: 计算 column_strain_time
        for i in 0..objects.len() {
            let col = objects[i].column;
            let mut prev_time: Option<f64> = None;
            for j in (0..i).rev() {
                if objects[j].column == col {
                    prev_time = Some(objects[j].start_time);
                    break;
                }
            }
            objects[i].column_strain_time = match prev_time {
                Some(pt) => (objects[i].start_time - pt) / self.speed,
                None => objects[i].start_time / self.speed,
            };
        }

        objects
    }
}

// ─── 便捷函数：从 beatmap Note 直接计算星数 ───

pub fn calculate_stars(notes: &[Note], total_columns: usize, speed: f64) -> f64 {
    let hit_objects = HitObject::from_notes(notes);
    DifficultyCalculator::new(hit_objects, total_columns, speed).calculate()
}

// ─── 从 JSON 文件计算星数 (对标 Python calculate_stars_for_json) ───

use std::path::Path;

pub fn calculate_stars_for_json(json_path: &str, total_columns: usize, speed: f64) -> f64 {
    let path = Path::new(json_path);
    match crate::beatmap::load_beatmap(path.to_str().unwrap_or(json_path)) {
        Ok((_, notes)) => calculate_stars(&notes, total_columns, speed),
        Err(e) => {
            log::warn!("Difficulty calc failed for {}: {}", json_path, e);
            0.0
        }
    }
}

// ─── 测试 ───

#[cfg(test)]
mod tests {
    use super::*;

    /// 构造测试用的 HitObject
    fn h(start: f64, end: f64, col: usize) -> HitObject {
        HitObject { start_time: start, end_time: end, column: col }
    }

    /// 基础单轨连打
    #[test]
    fn test_single_column_stream() {
        let objects = vec![
            h(1000.0, 1000.0, 0),
            h(1200.0, 1200.0, 0),
            h(1400.0, 1400.0, 0),
            h(1600.0, 1600.0, 0),
        ];
        let stars = DifficultyCalculator::new(objects, 4, 1.0).calculate();
        assert!(stars > 0.0, "单轨连打应产生非零星数");
        assert!(stars < 20.0, "星数应在合理范围");
    }

    /// Chord (同时多轨)
    #[test]
    fn test_chords() {
        let mut objects = Vec::new();
        for t in (1000..3000).step_by(200) {
            for col in 0..4 {
                objects.push(h(t as f64, t as f64, col));
            }
        }
        let stars = DifficultyCalculator::new(objects, 4, 1.0).calculate();
        assert!(stars > 1.0, "四轨 chord 应产生较高星数");
    }

    /// 空谱面
    #[test]
    fn test_empty() {
        let stars = DifficultyCalculator::new(vec![], 4, 1.0).calculate();
        assert_eq!(stars, 0.0);
    }

    /// 单音符
    #[test]
    fn test_single_note() {
        let stars = DifficultyCalculator::new(vec![h(1000.0, 1000.0, 0)], 4, 1.0).calculate();
        assert_eq!(stars, 0.0);
    }

    /// 倍速提升应变
    #[test]
    fn test_speed_increases_difficulty() {
        let objects: Vec<HitObject> = (0..16)
            .map(|i| h(1000.0 + i as f64 * 150.0, 1000.0 + i as f64 * 150.0, 0))
            .collect();
        let stars_1x = DifficultyCalculator::new(objects.clone(), 4, 1.0).calculate();
        let stars_2x = DifficultyCalculator::new(objects, 4, 2.0).calculate();
        assert!(stars_2x > stars_1x, "倍速应增加星数");
    }

    /// 与 Python 参考值交叉验证 (加载实际谱面文件)
    #[test]
    fn test_cross_validate_with_python() {
        let test_files: &[(&str, f64)] = &[
            ("songs/120 Jack Practice/Various Artists - 120BPM Jack Practice (HoshiMiya_) [JerryC - Canon Rock 1.2x].json", 4.812205),
            ("songs/1360153 Various Artists - Malody Essential Pack/Various Artists - Malody Essential Pack (Magikarp1234) [Latitude (Curta1n's 4K Hyper Lv.15)].json", 2.515174),
            ("songs/1701660 Various Artists - Malody 4K Regular Dan v3-Jack/Various Artists - Malody 4K Regular Dan v3-Jack (Muses) [Reg-10 Hiensou  Hinaka_Yuki].json", 5.398972),
            ("songs/1701662 Various Artists - Malody 4K Regular Dan v3-Technical/Various Artists - Malody 4K Regular Dan v3-Technical (Muses) [Reg-10 Rocky Buinne  tera].json", 4.891548),
            ("songs/1701667 Various Artists - Malody 4K Regular Dan v3-Stream/Various Artists - Malody 4K Regular Dan v3-Stream (Muses) [Reg-10 Spin Eternally  Oekakizuki].json", 5.409876),
            ("songs/1701668 Various Artists - Malody 4K Regular Dan v3-Starter/Various Artists - Malody 4K Regular Dan v3-Starter (Muses) [Reg-0 Map-1 Ikitoshi ikerumono  Promiii].json", 2.318420),
        ];

        for (rel_path, expected) in test_files {
            let path = Path::new(rel_path);
            if !path.exists() {
                eprintln!("Skip: {} not found", rel_path);
                continue;
            }
            let stars = calculate_stars_for_json(rel_path, 4, 1.0);
            let diff = (stars - expected).abs();
            eprintln!("  {:.6} vs Python {:.6}  diff={:.6}", stars, expected, diff);
            assert!(
                diff < 0.0001,
                "{}: Rust={:.6} Python={:.6} diff={:.6}",
                rel_path, stars, expected, diff
            );
        }
    }
}
