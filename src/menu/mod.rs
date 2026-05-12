pub mod exit;
pub mod main_menu;
pub mod play_mode;
pub mod preview;
pub mod settings;
pub mod replay_list;
pub mod song_select;
pub mod splash;

#[derive(Clone)]
pub struct SongEntry {
    pub dir_name: String,
    pub path: String,
    pub jsons: Vec<String>,
    pub selected_diff: usize,
}

pub fn load_songs() -> Vec<SongEntry> {
    let mut songs = Vec::new();
    if let Ok(entries) = std::fs::read_dir("songs") {
        for entry in entries.flatten() {
            if !entry.file_type().map_or(false, |t| t.is_dir()) { continue; }
            let mut jsons: Vec<String> = Vec::new();
            if let Ok(files) = std::fs::read_dir(entry.path()) {
                for f in files.flatten() {
                    let p = f.path();
                    if p.extension().and_then(|e| e.to_str()).map_or(false, |e| matches!(e, "json" | "osu" | "mc" | "sm" | "qua")) {
                        jsons.push(p.to_string_lossy().to_string());
                    }
                }
            }
            if !jsons.is_empty() { jsons.sort(); songs.push(SongEntry { dir_name: entry.file_name().to_string_lossy().to_string(), path: entry.path().to_string_lossy().to_string(), jsons, selected_diff: 0 }); }
        }
    }
    songs.sort_by(|a, b| a.dir_name.cmp(&b.dir_name));
    songs
}

/// 扫描所有歌曲名/难度名，收集需要渲染的额外字符
pub fn collect_ui_chars(songs: &[SongEntry]) -> Vec<char> {
    let mut chars: Vec<char> = "单人模式开始游玩退出设置编辑浏览皮肤流速调节OD判定精度轨道布局镜像模式音频倍速播放全局偏移键位显示FPS全屏分辨率导入切换删除默认关于返回继续游玩历史结算谱面星级歌曲难度时长按键总数单点长条时间谱师".chars().collect();
    for s in songs {
        chars.extend(s.dir_name.chars());
        for j in &s.jsons {
            if let Some(stem) = std::path::Path::new(j).file_stem() {
                chars.extend(stem.to_string_lossy().chars());
            }
        }
    }
    chars.sort();
    chars.dedup();
    chars
}
