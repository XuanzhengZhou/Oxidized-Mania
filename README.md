# Oxidized Mania

A 4K vertical-scrolling rhythm game built with Rust + wgpu. Completely rewritten from [osu-mania-4k-by-pygame](https://github.com/XuanzhengZhou/osu-maina_4k_by_pygame) — the name "Oxidized" reflects this origin: the Python prototype reborn through Rust's oxidation.

GPU-accelerated rendering, 240Hz frame-locked, osu!-compatible OD judgment, skin switching, replay recording/playback.

> [中文文档 (Chinese README)](README_CN.md)

---

## Quick Start

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

> Requires Rust 1.95+. Install: `curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh`

---

## Performance

| Metric | Value |
|--------|-------|
| Avg frame time | 1.0–2.5 ms |
| Frame rate | Auto-adapts to display refresh rate (60/120/144/240Hz) via Mailbox VSync |
| Memory usage | ~300 MB |
| Binary size | ~10 MB |
| GPU quads per frame | 13–50 |
| Glyph texture | 1 MB (R8Unorm) |
| Lines of code | ~7,400 Rust |

Frame rate automatically matches your display's refresh rate (60Hz → 60fps, 144Hz → 144fps, 240Hz → 240fps). Frame timing is uniformly paced with sub-millisecond jitter. Runs smoothly on MacBook-class hardware. 10-year-old machines (Intel HD 4000) automatically fall back to traditional VSync (`Fifo`) if Mailbox is unsupported.

---

## Features

- **GPU Rendering**: wgpu backend (Metal/DX12/Vulkan), instanced draw with ~50 quads/frame
- **osu!-compatible Judgment**: OD 0-11 DifficultyRange with 6-tier judgments (Perfect/Great/Good/Ok/Meh/Miss), rate-scaled
- **Skin System**: osu! skin.ini compatible — notes, holds, key pads, hit bursts, stage backgrounds all customizable
- **Replay System**: Key-level recording (press + release), gzip JSON format, with seek/FF/RW/pause support
- **rosu-pp Integration**: Star rating + PP calculation, displayed in song select
- **Mirror Mode**: One-key lane flip
- **Audio Offset**: osu!-style global_offset to compensate for audio/input latency
- **Variable Speed**: 0.5x–2.0x playback with sonic pitch-preserving time-stretch
- **Mailbox VSync**: Tear-free with minimum latency; auto-fallback to traditional VSync when unsupported
- **240Hz Support**: Uniform frame pacing, zero dropped frames

---

## Included Content

### Default Skin `skins/A`
Green orb style, ~300 KB. Round key pads with clear hit burst effects. Suitable for beginners and most players.

### Default Song Pack `songs/1360153 Various Artists - Malody Essential Pack`
27 beginner-friendly 4K charts from the Malody community. Moderate difficulty, ideal for practice and testing.

---

## Usage Guide

### Importing Songs

Place osu! `.osz` files in the `songs/` directory, or extract them into folders containing `.osu` files and audio.

Supported formats: osu! mania `.osu` charts, project-native JSON charts.

```
songs/
├── Your Song Folder/
│   ├── song.osu
│   ├── audio.mp3
│   └── bg.jpg
```

Restart the game to see new songs in song select.

### Importing Skins

Place osu! skin folders (containing `skin.ini`) in the `skins/` directory:

```
skins/
├── Your Skin/
│   ├── skin.ini
│   ├── mania-key1.png
│   ├── mania-key1D.png
│   ├── mania-note1.png
│   └── ...
```

osu! skin.ini `[Mania]` 4K configuration is supported.

### Switching Skins

1. Enter Settings (select Settings in main menu, or press S in song select)
2. Navigate to "皮肤设置" (Skin Settings)
3. Press **T** (next) / **Y** (previous) to cycle through skins
4. Changes auto-save and take effect upon returning to song select

### Adjusting Offset

In settings or song select:
- Press **A**: offset -5ms (game clock advances → notes appear earlier)
- Press **D**: offset +5ms (compensates audio latency)

Positive = notes appear before audio (compensates audio lag). Negative = notes appear after audio (compensates input lag). Current offset is shown in settings.

### Watching Replays

1. Press **R** in song select
2. Replay list appears (date / rank badge / score / ACC%)
3. ↑↓ to navigate, Enter to play
4. ESC to return

### Replay Controls

| Key | Action |
|-----|--------|
| **Space** | Pause / Resume |
| **←** | Rewind 5s |
| **→** | Fast-forward 5s |
| **ESC** | Exit replay |

Replay HUD shows: Combo / Score / ACC% / KPS / Rate / Mirror status / Progress bar.

### Other Shortcuts

| Key | Context | Action |
|-----|---------|--------|
| D/F/J/K | Gameplay | 4-lane keys (customizable) |
| S | End of play | Save replay |
| R | Song select | Browse replays |
| M | Song select | Toggle mirror mode |
| W/E | Song select | Adjust playback rate |
| L/J | Settings/Song select | Adjust scroll speed |
| O/P | Settings/Song select | Adjust OD |
| F | Settings | Toggle fullscreen |
| B | Settings | Toggle mirror mode |

---

## Architecture

```
src/
├── main.rs                 # Entry: AppState FSM + InputState + cover loading
├── config.rs               # GameConfig load/save (JSON)
├── history.rs              # Play history records
├── pp.rs                   # rosu-pp wrapper: star rating + PP
├── replay.rs               # ReplayData struct + gzip JSON save/load
├── replay_viewer.rs        # ReplayEngine: playback, seek, pause
├── skin.rs                 # CpuSkin load + GPU texture atlas
├── beatmap.rs              # BeatmapMeta + load_beatmap()
├── sonic.rs                # Sonic C FFI pitch-preserving time-stretch
├── mania_difficulty.rs     # Star rating calc (retained, superseded by rosu-pp)
│
├── render/
│   ├── context.rs          # RenderCtx: wgpu device/Surface/QuadRenderer/TextRenderer
│   ├── quad.rs             # QuadRenderer: instanced batch rendering
│   └── text.rs             # TextRenderer: fontdue GPU glyph atlas
│
├── game/
│   ├── engine.rs           # GameEngine: main game loop
│   ├── notes.rs            # Note processing + rendering (process_notes)
│   ├── judgment.rs         # osu! DifficultyRange 6-tier judgment
│   ├── scoring.rs          # Score accumulation + accuracy()
│   ├── results.rs          # Results screen: dual ring + stats charts
│   ├── hud.rs              # In-game HUD (Combo/Score/ACC/FPS)
│   └── pause.rs            # Pause overlay
│
├── menu/
│   ├── mod.rs              # SongEntry + load_songs() + cover cycling
│   ├── splash.rs           # Splash screen: logo circle + blurred covers
│   ├── main_menu.rs        # Main menu: 5 tabs + osu!-style circle
│   ├── play_mode.rs        # Play mode selection
│   ├── song_select.rs      # osu!-style song select
│   ├── settings.rs         # Settings: dual-column layout + adjuster
│   ├── replay_list.rs      # Replay list cards
│   ├── preview.rs          # Chart preview
│   └── exit.rs             # Exit confirmation
│
├── ui/
│   ├── mod.rs              # Shared UI: menu background, circle, tabs
│   ├── theme.rs            # osu! palette + rank_color/star_color
│   └── primitives.rs       # Shape primitives: trapezoid, capsule, etc.
│
└── audio/
    └── bass.rs             # BASS audio engine FFI
```

### Key Functions

| Function | Location | Purpose |
|----------|----------|---------|
| `AppState` FSM | `main.rs` | Splash→MainMenu→SongSelect→Gameplay→Results |
| `GameEngine::render_frame()` | `engine.rs` | Main frame: bg→stage→notes→keypads→bursts→HUD |
| `process_notes()` | `notes.rs` | O(n) single-pass: note culling + miss detection + rendering |
| `judge_tap()` / `judge_hold_release()` | `judgment.rs` | osu! DifficultyRange, OD 0-11 + rate scaling |
| `QuadRenderer::push_rect()` | `quad.rs` | Instanced quad batch, 40B/quad, Unorm8x4 color |
| `TextRenderer::queue_text()` | `text.rs` | Glyph atlas rendering, OnceLock cache, R8Unorm 1MB GPU |
| `CpuSkin::load()` | `skin.rs` | CPU skin load + GPU atlas(4096), Mutex cache |
| `ReplayEngine::new()` | `replay_viewer.rs` | Replay init: event sorting + pre-computed judgments |
| `ReplayData::load()` | `replay.rs` | gzip JSON deserialization, auto-compat with Python `.osr` |
| `calculate_stars()` / `calculate_pp()` | `pp.rs` | rosu-pp integration, stars cache |

---

## Building on All Platforms

### Prerequisites

- Rust 1.95+ (via `rustup`)
- Git

### macOS

```bash
xcode-select --install  # if not already installed
cargo build --release
DYLD_LIBRARY_PATH="./libs:$DYLD_LIBRARY_PATH" cargo run --release
```

> Prebuilt `libs/libbass.dylib` is included.

### Windows

```powershell
cargo build --release
.\target\release\oxidized_mania.exe
```

> Download `bass.dll` from [un4seen.com](https://www.un4seen.com/) and place it in `libs/`.

### Linux

```bash
sudo apt install libasound2-dev pkg-config  # Ubuntu/Debian
cargo build --release
LD_LIBRARY_PATH="./libs:$LD_LIBRARY_PATH" cargo run --release
```

> Download `libbass.so` from [un4seen.com](https://www.un4seen.com/) and place it in `libs/`.

---

## Optimization History

Three rounds of optimization. dhat cumulative heap allocation reduced from ~18,000MB to ~2,500MB (↓86%). All stuttering eliminated.

| Round | Focus | Key Optimizations |
|-------|-------|-------------------|
| 1 | CPU Cache | OnceLock glyph cache (fontdue 12GB→104MB), skin Mutex cache (image 460MB→29MB), const arrays for skin keys |
| 2 | Algorithm+GPU | compute_hit_offsets caching, HashMap→match, glyph atlas RGBA8→R8 (4MB→1MB), KPS monotonic cursor |
| 3 | Frame Rate+Bugfixes | Mailbox 240Hz lock, global_offset fix, replay stage_spacing/scale fix, replay ACC precision fix |

See [CLAUDE.md](CLAUDE.md) for detailed optimization notes.

---

## Acknowledgments

- [osu!](https://github.com/ppy/osu) — Original game design and invaluable reference
- [rosu-pp](https://github.com/MaxOhn/rosu-pp) — Rust osu! difficulty & PP calculation library
- [XuanzhengZhou/osu-mania-4k-by-pygame](https://github.com/XuanzhengZhou/osu-maina_4k_by_pygame) — This project's Python prototype

---

## License

MIT License
