#[derive(Debug, Clone, Default)]
pub struct Score {
    pub perfect_count: u32,  // 305
    pub great_count: u32,    // 300
    pub good_count: u32,     // 200
    pub ok_count: u32,       // 100
    pub meh_count: u32,      // 50
    pub miss_count: u32,
    pub combo: u32,
    pub max_combo: u32,
    pub total_score: u32,
    pub total_notes: u32, // 预计算: hold=2, tap=1
}

impl Score {
    pub fn add_judgment(&mut self, j: super::judgment::JudgmentResult) {
        match j {
            super::judgment::JudgmentResult::Perfect => { self.total_score += 305; self.perfect_count += 1; self.combo += 1; }
            super::judgment::JudgmentResult::Great =>   { self.total_score += 300; self.great_count += 1; self.combo += 1; }
            super::judgment::JudgmentResult::Good =>    { self.total_score += 200; self.good_count += 1; self.combo += 1; }
            super::judgment::JudgmentResult::Ok =>      { self.total_score += 100; self.ok_count += 1; self.combo += 1; }
            super::judgment::JudgmentResult::Meh =>     { self.total_score += 50; self.meh_count += 1; self.combo += 1; }
            super::judgment::JudgmentResult::Miss =>    { self.miss_count += 1; self.combo = 0; }
        }
        self.max_combo = self.max_combo.max(self.combo);
    }

    /// 对标 Python: acc = score / (processed * 305) * 100
    pub fn accuracy(&self) -> f64 {
        let processed = self.judged_count();
        if processed == 0 { return 100.0; }
        self.total_score as f64 / (processed as f64 * 305.0) * 100.0
    }

    pub fn judged_count(&self) -> u32 {
        self.perfect_count + self.great_count + self.good_count + self.ok_count + self.meh_count + self.miss_count
    }
}
