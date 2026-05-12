# Oxidized Mania

基于 Rust + wgpu 的 4K 下落式节奏游戏。从 [osu-mania-4k-by-pygame](https://github.com/XuanzhengZhou/osu-maina_4k_by_pygame) 用 Rust 完全重写而来——"氧化"(Oxidized)即源于此：Python 原型经 Rust 锈蚀重生。

GPU 加速渲染、240Hz 帧率锁定、osu! 同款 OD 判定系统、皮肤切换、回放录制/播放。

---

## 快速开始

### macOS

```bash
cd Oxidized-Mania
DYLD_LIBRARY_PATH="./libs:$DYLD_LIBRARY_PATH" cargo run --release
```

### Windows

```powershell
cd Oxidized-Mania
cargo run --release
```

### Linux

```bash
cd Oxidized-Mania
LD_LIBRARY_PATH="./libs:$LD_LIBRARY_PATH" cargo run --release
```

> 需要 Rust 1.95+。安装：`curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh`

---

## 性能指标

| 指标 | 数值 |
|------|------|
| 平均帧时间 | 1.0–2.5 ms |
| 帧率 | 自动适配显示器刷新率（60/120/144/240Hz），Mailbox VSync |
| 内存占用 | ~300 MB |
| 二进制大小 | ~10 MB |
| GPU 每帧 quad 数 | 13–50 |
| 字形纹理 | 1 MB (R8Unorm) |
| 代码行数 | ~7,400 行 Rust |

帧率自动匹配显示器刷新率（60Hz→60fps、144Hz→144fps、240Hz→240fps），帧节奏均匀、亚毫秒级抖动。MacBook 级硬件即可流畅运行。10 年老机器（Intel HD 4000）若不支持 Mailbox 则自动降级为传统 VSync（`Fifo`）。

---

## 软件特点

- **GPU 渲染**：wgpu 后端，Metal/DX12/Vulkan 跨平台，instanced draw 每帧 ~50 quad
- **osu! 同款判定**：OD 0-11 DifficultyRange 六段判定（Perfect/Great/Good/Ok/Meh/Miss），支持倍速缩放
- **皮肤系统**：osu! skin.ini 兼容，音符/长条/按键底板/判定特效/舞台背景均可自定义
- **回放系统**：按键级录制（press + release），gzip JSON 格式，支持快进/快退/暂停/seek
- **rosu-pp 集成**：难度星数 + PP 计算，选歌界面实时显示
- **镜像模式**：一键翻转轨道
- **音频偏移**：osu! 同款 global_offset，补偿音频/输入延迟
- **倍速播放**：0.5x–2.0x sonic 变速不变调
- **Mailbox 垂直同步**：零撕裂 + 最低延迟，不支持时自动回退传统 VSync
- **240Hz 完美支持**：帧节奏均匀，无跳帧

---

## 附带的默认内容

项目预装开箱即用的皮肤和曲包：

### 默认皮肤 `skins/A`
绿色球皮风格，几百 KB，圆形按键底板 + 清晰判定特效。适合新手和大多数玩家。

### 默认曲包 `songs/1360153 Various Artists - Malody Essential Pack`
来自 Malody 社区的 27 首入门级 4K 谱面，难度适中，适合新手练习和测试。

---

## 使用指南

### 导入谱面

将 osu! `.osz` 文件放入 `songs/` 目录，或直接放入解压后的文件夹（含 `.osu` 和音频文件）。

支持格式：osu! mania `.osu` 谱面、项目自定义 JSON 谱面。

```
songs/
├── Your Song Folder/
│   ├── song.osu
│   ├── audio.mp3
│   └── bg.jpg
```

启动游戏后即可在选歌界面看到。

### 导入皮肤

将 osu! 皮肤文件夹（含 `skin.ini`）放入 `skins/` 目录：

```
skins/
├── Your Skin/
│   ├── skin.ini
│   ├── mania-key1.png
│   ├── mania-key1D.png
│   ├── mania-note1.png
│   └── ...
```

支持 osu! skin.ini 的 `[Mania]` 4K 配置。

### 切换皮肤

1. 进入设置界面（主菜单选 Settings，或选歌界面按 S）
2. 右侧导航选"皮肤设置"
3. 按 **T**（下一个）/ **Y**（上一个）循环切换
4. 切换后自动保存，返回选歌界面生效

### 调节延迟

在设置界面或选歌界面：
- 按 **A**：偏移 -5ms（游戏时钟提前 → 音符更早出现）
- 按 **D**：偏移 +5ms（补偿音频延迟）

正值 = 音符比音频早（补偿音频延迟），负值 = 音符比音频晚（补偿输入延迟）。设置界面实时显示当前偏移值。

### 观看回放

1. 在选歌界面按 **R**
2. 弹出回放列表（日期 / Rank 色块 / Score / ACC%）
3. ↑↓ 导航，Enter 播放
4. ESC 返回

### 回放控制

| 键 | 功能 |
|----|------|
| **Space** | 暂停 / 继续 |
| **←** | 快退 5 秒 |
| **→** | 快进 5 秒 |
| **ESC** | 退出回放 |

回放界面 HUD 显示：Combo / Score / ACC% / KPS / 倍速 / Mirror 状态 / 红色进度条。

### 其他快捷键

| 键 | 位置 | 功能 |
|----|------|------|
| D/F/J/K | 游玩 | 4 轨按键（可自定义） |
| S | 游玩结束 | 保存回放 |
| R | 选歌界面 | 浏览回放 |
| M | 选歌界面 | 切换镜像模式 |
| W/E | 选歌界面 | 调节倍速 |
| L/J | 设置/选歌 | 调节流速 |
| O/P | 设置/选歌 | 调节 OD |
| F | 设置 | 切换全屏 |
| B | 设置 | 切换镜像模式 |

---

## 项目架构

```
src/
├── main.rs                 # 入口：AppState 状态机 + InputState + 封面加载
├── config.rs               # GameConfig 加载/保存（JSON）
├── history.rs              # 游玩历史记录
├── pp.rs                   # rosu-pp 封装：星数 + PP 计算
├── replay.rs               # ReplayData 数据结构 + gzip JSON 保存/加载
├── replay_viewer.rs        # ReplayEngine：回放播放、seek、暂停
├── skin.rs                 # CpuSkin 加载 + GPU 纹理图集构建
├── beatmap.rs              # BeatmapMeta + load_beatmap()
├── sonic.rs                # sonic C FFI 变速不变调
├── mania_difficulty.rs     # 难度星数计算（保留，已由 rosu-pp 替代）
│
├── render/
│   ├── context.rs          # RenderCtx：wgpu 设备/Surface/QuadRenderer/TextRenderer
│   ├── quad.rs             # QuadRenderer：instanced batch 渲染
│   └── text.rs             # TextRenderer：fontdue GPU 字形图集
│
├── game/
│   ├── engine.rs           # GameEngine：游戏主循环（对标 gameplay.py）
│   ├── notes.rs            # 远跳 + 底漏 + 渲染 process_notes()
│   ├── judgment.rs         # osu! DifficultyRange 六段判定
│   ├── scoring.rs          # Score 累计 + accuracy()
│   ├── results.rs          # 结算界面：双圆环 + 统计图表
│   ├── hud.rs              # 游玩 HUD（Combo/Score/ACC/FPS）
│   └── pause.rs            # 暂停界面
│
├── menu/
│   ├── mod.rs              # SongEntry + load_songs() + 封面轮播
│   ├── splash.rs           # 启动界面：Logo 圆 + 模糊曲绘
│   ├── main_menu.rs        # 欢迎菜单：5 选项卡 + osu! 圆圈
│   ├── play_mode.rs        # 游玩模式选择
│   ├── song_select.rs      # osu! 风格选歌界面
│   ├── settings.rs         # 设置界面：双栏布局 + 全屏调节器
│   ├── replay_list.rs      # 回放列表卡片
│   ├── preview.rs          # 谱面预览
│   └── exit.rs             # 退出确认
│
├── ui/
│   ├── mod.rs              # draw_menu_background/draw_osu_circle/draw_menu_tabs
│   ├── theme.rs            # osu! 色板 + rank_color/star_color
│   └── primitives.rs       # draw_trapezoid/draw_capsule 等形状
│
└── audio/
    └── bass.rs             # BASS 音频引擎 FFI
```

### 关键函数

| 函数 | 位置 | 作用 |
|------|------|------|
| `AppState` 状态机 | `main.rs` | Splash→MainMenu→SongSelect→Gameplay→Results |
| `GameEngine::render_frame()` | `engine.rs` | 游戏主帧：背景→舞台→音符→按键底板→判定特效→HUD |
| `process_notes()` | `notes.rs` | 远跳+底漏+渲染三合一，O(n) 单次扫描全部活跃音符 |
| `judge_tap()` / `judge_hold_release()` | `judgment.rs` | osu! DifficultyRange 判定，OD 0-11 + 倍速缩放 |
| `QuadRenderer::push_rect()` | `quad.rs` | instanced quad batch，40B/quad，Unorm8x4 颜色 |
| `TextRenderer::queue_text()` | `text.rs` | 字形图集渲染，OnceLock 缓存，R8Unorm 1MB GPU |
| `CpuSkin::load()` | `skin.rs` | CPU 皮肤加载 + GPU 纹理图集(4096)构建，Mutex 缓存 |
| `ReplayEngine::new()` | `replay_viewer.rs` | 回放初始化：事件排序 + 预计算判定列表 |
| `ReplayData::load()` | `replay.rs` | gzip JSON 反序列化，自动兼容 Python `.osr` 格式 |
| `calculate_stars()` / `calculate_pp()` | `pp.rs` | rosu-pp 集成，星数缓存 |

---

## 全平台编译

### 前置条件

- Rust 1.95+（`rustup` 安装）
- Git（克隆仓库）

### macOS

```bash
# 确保 Xcode Command Line Tools 已安装
xcode-select --install

# 构建
cargo build --release

# 运行
DYLD_LIBRARY_PATH="./libs:$DYLD_LIBRARY_PATH" cargo run --release
```

> 预编译的 `libs/libbass.dylib` 已包含在项目中。

### Windows

```powershell
# 构建
cargo build --release

# 运行
.\target\release\oxidized_mania.exe
```

> BASS 库 `bass.dll` 需放在 `libs/` 目录下（从 [un4seen.com](https://www.un4seen.com/) 下载）。

### Linux

```bash
# 安装依赖
sudo apt install libasound2-dev pkg-config  # Ubuntu/Debian

# 构建
cargo build --release

# 运行
LD_LIBRARY_PATH="./libs:$LD_LIBRARY_PATH" cargo run --release
```

> 需从 [un4seen.com](https://www.un4seen.com/) 下载 `libbass.so` 放入 `libs/` 目录。

---

## 性能优化历程

三轮优化，dhat 累计堆分配从 ~18,000MB 降至 ~2,500MB（↓86%），卡顿完全消除。

| 轮次 | 重点 | 关键优化 |
|------|------|---------|
| 第一轮 | CPU 缓存 | OnceLock 字形缓存(fontdue 12GB→104MB)、皮肤 Mutex 缓存(image 460MB→29MB)、静态数组替代 format! |
| 第二轮 | 算法+GPU | compute_hit_offsets 缓存、HashMap→match、字形图集 RGBA8→R8(4MB→1MB)、KPS 单调游标 |
| 第三轮 | 帧率+Bug | Mailbox 240Hz 锁定、global_offset 修复、回放 stage_spacing/scale 修复、回放 ACC 精度修复 |

详见 [CLAUDE.md](CLAUDE.md) 性能优化章节。

---

## 致谢

- [osu!](https://github.com/ppy/osu) — 提供了原始设计和诸多借鉴
- [rosu-pp](https://github.com/MaxOhn/rosu-pp) — Rust osu! 难度与 PP 计算库
- [XuanzhengZhou/osu-mania-4k-by-pygame](https://github.com/XuanzhengZhou/osu-maina_4k_by_pygame) — 本项目 Python 原型

---

## 许可证

MIT License
