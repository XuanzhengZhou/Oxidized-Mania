use crate::game::notes::SCREEN_H;
use crate::render::quad::QuadRenderer;
use crate::render::text::TextRenderer;
use crate::skin::AtlasRegion;
use crate::ui::draw_menu_background;

const AUDIO_EXTS: &[&str] = &["mp3", "ogg", "wav", "flac", "m4a", "aac", "aiff", "wma"];

enum TreeNode {
    Folder { name: String, path: String, expanded: bool, children: Vec<TreeNode> },
    AudioFile { name: String, full_path: String },
}

pub struct EditorFileBrowser {
    roots: Vec<TreeNode>,
    pub selected: usize,
    pub scroll: f32,
    pub audio_count: u32,
    flat: Vec<FlatItemOwned>,
}

struct FlatItemOwned {
    kind: u8, // 0=folder, 1=audio
    name: String,
    full_path: String,
    depth: u32,
    expanded: bool,
}

pub fn scan() -> EditorFileBrowser {
    fn scan_dir(path: &std::path::Path) -> Vec<TreeNode> {
        let mut children = Vec::new();
        let mut subdirs: Vec<(String, std::path::PathBuf)> = Vec::new();
        let mut audio_files: Vec<(String, String)> = Vec::new();

        if let Ok(entries) = std::fs::read_dir(path) {
            for entry in entries.flatten() {
                let ft = entry.file_type().ok();
                let name = entry.file_name().to_string_lossy().to_string();
                if ft.map_or(false, |t| t.is_dir()) {
                    subdirs.push((name, entry.path()));
                } else {
                    let p = entry.path();
                    let ext = p.extension().and_then(|e| e.to_str()).unwrap_or("");
                    if AUDIO_EXTS.contains(&ext.to_lowercase().as_str()) {
                        audio_files.push((name, p.to_string_lossy().to_string()));
                    }
                }
            }
        }
        subdirs.sort_by(|a, b| a.0.to_lowercase().cmp(&b.0.to_lowercase()));
        audio_files.sort_by(|a, b| a.0.to_lowercase().cmp(&b.0.to_lowercase()));

        for (name, full_path) in audio_files {
            children.push(TreeNode::AudioFile { name, full_path });
        }
        for (name, sub_path) in subdirs {
            let sub = scan_dir(&sub_path);
            if !sub.is_empty() {
                children.push(TreeNode::Folder { name, path: sub_path.to_string_lossy().to_string(), expanded: false, children: sub });
            }
        }
        children
    }

    let roots = scan_dir(std::path::Path::new("songs"));
    let mut fb = EditorFileBrowser { roots, selected: 0, scroll: 0.0, audio_count: 0, flat: Vec::new() };
    fb.rebuild_flat();
    fb
}

impl EditorFileBrowser {
    fn rebuild_flat(&mut self) {
        self.flat.clear();
        self.audio_count = 0;
        fn flatten(nodes: &[TreeNode], flat: &mut Vec<FlatItemOwned>, audio_count: &mut u32) {
            for node in nodes {
                match node {
                    TreeNode::Folder { name, path, expanded, children } => {
                        flat.push(FlatItemOwned { kind: 0, name: name.clone(), full_path: path.clone(), depth: 0, expanded: *expanded });
                        if *expanded {
                            flatten_inner(children, flat, 1, audio_count);
                        }
                    }
                    TreeNode::AudioFile { name, full_path } => {
                        flat.push(FlatItemOwned { kind: 1, name: name.clone(), full_path: full_path.clone(), depth: 0, expanded: false });
                        *audio_count += 1;
                    }
                }
            }
        }
        fn flatten_inner(nodes: &[TreeNode], flat: &mut Vec<FlatItemOwned>, depth: u32, audio_count: &mut u32) {
            for node in nodes {
                match node {
                    TreeNode::Folder { name, path, expanded, children } => {
                        flat.push(FlatItemOwned { kind: 0, name: name.clone(), full_path: path.clone(), depth, expanded: *expanded });
                        if *expanded {
                            flatten_inner(children, flat, depth + 1, audio_count);
                        }
                    }
                    TreeNode::AudioFile { name, full_path } => {
                        flat.push(FlatItemOwned { kind: 1, name: name.clone(), full_path: full_path.clone(), depth, expanded: false });
                        *audio_count += 1;
                    }
                }
            }
        }
        flatten(&self.roots, &mut self.flat, &mut self.audio_count);
    }

    fn toggle_folder(&mut self, path: &str) {
        fn toggle(nodes: &mut Vec<TreeNode>, path: &str) -> bool {
            for node in nodes.iter_mut() {
                match node {
                    TreeNode::Folder { path: p, expanded, children, .. } if p == path => {
                        *expanded = !*expanded;
                        return true;
                    }
                    TreeNode::Folder { children, .. } => {
                        if toggle(children, path) { return true; }
                    }
                    _ => {}
                }
            }
            false
        }
        toggle(&mut self.roots, path);
        self.rebuild_flat();
    }
}

pub fn render(
    state: &EditorFileBrowser,
    quad: &mut QuadRenderer,
    text: &mut TextRenderer,
    cover_region: Option<&AtlasRegion>,
) {
    let w = crate::game::notes::screen_w();
    draw_menu_background(quad, cover_region);

    quad.push_rect(0.0, 0.0, w, 40.0, [10, 10, 25, 220]);
    text.queue_text("选择音频 — 歌曲目录树", 16.0, 8.0, 18.0, [255, 255, 255, 255]);
    text.queue_text("[↑↓] 浏览  [Enter] 展开/确认  [ESC] 返回", w - 280.0, 8.0, 11.0, [160, 160, 180, 255]);

    if state.flat.is_empty() {
        text.queue_text("songs/ 目录未找到音频文件", w / 2.0 - 100.0, SCREEN_H / 2.0, 16.0, [180, 180, 200, 255]);
        return;
    }

    let list_top = 48.0;
    let row_h = 26.0;
    let visible = ((SCREEN_H - list_top - 28.0) / row_h) as usize;
    let max_scroll = (state.flat.len().saturating_sub(visible)) as f32 * row_h;
    let scroll = state.scroll.clamp(0.0, max_scroll.max(0.0));

    for (i, item) in state.flat.iter().enumerate() {
        let y = list_top + i as f32 * row_h - scroll;
        if y < list_top - row_h || y > SCREEN_H { continue; }

        let sel = i == state.selected;
        if sel {
            quad.push_rect(4.0, y, w - 8.0, row_h, [45, 42, 59, 200]);
            quad.push_rect(4.0, y + 2.0, 3.0, row_h - 4.0, [100, 200, 255, 255]);
        }

        let indent = item.depth as f32 * 20.0 + 18.0;
        match item.kind {
            0 => {
                let arrow = if item.expanded { "▼" } else { "▶" };
                let label = format!("{} [文件夹] {}", arrow, item.name);
                let color = if sel { [255, 255, 255, 255] } else { [180, 180, 200, 255] };
                text.queue_text(&label, indent, y + 2.0, 13.0, color);
            }
            1 => {
                let label = format!("  * {}", item.name);
                let color = if sel { [200, 255, 200, 255] } else { [140, 180, 140, 255] };
                text.queue_text(&label, indent, y + 2.0, 13.0, color);
            }
            _ => {}
        }
    }

    quad.push_rect(0.0, SCREEN_H - 24.0, w, 24.0, [10, 10, 25, 200]);
    let info = format!("{} 个文件夹 / {} 首音频", state.roots.len(), state.audio_count);
    text.queue_text(&info, 16.0, SCREEN_H - 16.0, 11.0, [140, 140, 160, 255]);
}

pub fn handle_key(state: &mut EditorFileBrowser, key: winit::keyboard::KeyCode) -> Option<String> {
    match key {
        winit::keyboard::KeyCode::ArrowUp => {
            if state.selected > 0 { state.selected -= 1; }
        }
        winit::keyboard::KeyCode::ArrowDown => {
            if state.selected + 1 < state.flat.len() { state.selected += 1; }
        }
        winit::keyboard::KeyCode::Enter => {
            if state.selected < state.flat.len() {
                let (is_folder, path) = {
                    let item = &state.flat[state.selected];
                    (item.kind == 0, item.full_path.clone())
                };
                if is_folder {
                    state.toggle_folder(&path);
                    if state.selected >= state.flat.len() {
                        state.selected = state.flat.len().saturating_sub(1);
                    }
                    return None;
                } else {
                    return Some(path);
                }
            }
        }
        winit::keyboard::KeyCode::Escape => return Some(String::new()),
        _ => {}
    }
    // 自动滚动
    let row_h = 26.0;
    let list_top = 48.0;
    let visible_h = SCREEN_H - list_top - 28.0;
    let sel_top = state.selected as f32 * row_h;
    let sel_bot = sel_top + row_h;
    if sel_top < state.scroll { state.scroll = sel_top; }
    if sel_bot > state.scroll + visible_h { state.scroll = sel_bot - visible_h; }
    None
}
