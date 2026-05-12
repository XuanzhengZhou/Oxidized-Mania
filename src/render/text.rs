use std::collections::HashMap;
use std::sync::OnceLock;
use bytemuck::{Pod, Zeroable};
use fontdue::Font;
use wgpu::util::DeviceExt as _;

const ATLAS_SIZE: u32 = 1024;
const FONT_SIZE: f32 = 36.0;

// 字形光栅化缓存 — 避免每次重建 TextRenderer 都重新解析字体
struct GlyphCache {
    atlas_pixels: Vec<u8>,
    glyphs: HashMap<char, GlyphInfo>,
    baseline_offset: f32,
}
static GLYPH_CACHE: OnceLock<GlyphCache> = OnceLock::new();

// ─── GPU 数据结构 ───

#[repr(C)]
#[derive(Copy, Clone, Debug, Pod, Zeroable)]
struct GlyphInstance {
    offset: [f32; 2],
    size: [f32; 2],
    uv_offset: [f32; 2],
    uv_size: [f32; 2],
    color: [u8; 4],
}

// ─── 着色器 ───

const GLYPH_SHADER: &str = r#"
struct VertexInput {
    @location(0) corner: vec2<f32>,
    @location(1) offset: vec2<f32>,
    @location(2) size: vec2<f32>,
    @location(3) uv_offset: vec2<f32>,
    @location(4) uv_size: vec2<f32>,
    @location(5) color: vec4<f32>,
};

struct VertexOutput {
    @builtin(position) position: vec4<f32>,
    @location(0) color: vec4<f32>,
    @location(1) uv: vec2<f32>,
};

@group(0) @binding(2) var<uniform> screen: vec2<f32>;

@vertex
fn vs_main(in: VertexInput) -> VertexOutput {
    let screen_w = screen.x;
    let screen_h = screen.y;
    let x = (in.offset.x + in.corner.x * in.size.x) / screen_w * 2.0 - 1.0;
    let y = 1.0 - (in.offset.y + in.corner.y * in.size.y) / screen_h * 2.0;
    return VertexOutput(
        vec4<f32>(x, y, 0.0, 1.0),
        in.color,
        in.uv_offset + in.corner * in.uv_size,
    );
}

@group(0) @binding(0) var t_glyph: texture_2d<f32>;
@group(0) @binding(1) var s_glyph: sampler;

@fragment
fn fs_main(in: VertexOutput) -> @location(0) vec4<f32> {
    let alpha = textureSample(t_glyph, s_glyph, in.uv).r;
    return vec4<f32>(in.color.rgb, in.color.a * alpha);
}
"#;

// ─── GlyphInfo ───

#[derive(Clone)]
struct GlyphInfo {
    x: f32, y: f32, w: f32, h: f32,
    advance: f32,
    xmin: f32, ymin: f32, // 相对 baseline 的偏移
}

// ─── TextQuad (CPU side) ───

pub struct TextQuad {
    pub x: f32,
    pub y: f32,
    pub w: f32,
    pub h: f32,
    pub uv_x: f32,
    pub uv_y: f32,
    pub uv_w: f32,
    pub uv_h: f32,
    pub color: [u8; 4],
}

// ─── TextRenderer ───

pub struct TextRenderer {
    glyphs: HashMap<char, GlyphInfo>,
    font_size_px: f32,
    baseline_offset: f32, // 主字体基线偏移 (ymin绝对值，统一所有字形基线)
    pub queued_quads: Vec<TextQuad>,
    glyph_buf: Vec<GlyphInstance>,
    vertex_buf: wgpu::Buffer,
    instance_bufs: Vec<wgpu::Buffer>,
    current_buf: usize,
    screen_buffer: wgpu::Buffer,
    atlas_texture: wgpu::Texture,
    atlas_view: wgpu::TextureView,
    sampler: wgpu::Sampler,
    bind_group: wgpu::BindGroup,
    pipeline: wgpu::RenderPipeline,
}

fn rasterize_all_glyphs(extra_chars: &[char]) -> GlyphCache {
    let font_primary = {
        let paths = ["assets/Aller/Aller_Bd.ttf", "assets/Aller/Aller_Rg.ttf"];
        let mut data = None;
        for p in &paths { if let Ok(d) = std::fs::read(p) { data = Some(d); break; } }
        data.and_then(|d| Font::from_bytes(d, fontdue::FontSettings::default()).ok())
    };
    let font_fallback = {
        let paths = ["assets/Hiragino Sans GB.ttc", "assets/SourceHanSansCN-Bold.otf", "../SourceHanSansCN-Bold.otf"];
        let mut data = None;
        for p in &paths { if let Ok(d) = std::fs::read(p) { data = Some(d); break; } }
        data.and_then(|d| Font::from_bytes(d, fontdue::FontSettings::default()).ok())
    };

    let base_ascent = font_primary.as_ref()
        .and_then(|f| f.horizontal_line_metrics(FONT_SIZE))
        .map(|m| m.ascent)
        .unwrap_or(FONT_SIZE * 0.7);

    let mut chars: Vec<char> = (32u8..=126u8)
        .map(|c| c as char)
        .chain(['★','▼','▶','◀','【','】','：','；','（','）','×','谱','面','倍','速','结','算'])
        .collect();
    chars.extend(extra_chars);
    chars.sort();
    chars.dedup();

    let mut atlas_pixels = vec![0u8; (ATLAS_SIZE * ATLAS_SIZE) as usize];
    let mut glyphs = HashMap::new();
    let mut cursor_x: u32 = 2;
    let mut cursor_y: u32 = 2;
    let mut row_height: u32 = 0;

    for &ch in &chars {
        let mut metrics = None;
        let mut bitmap = Vec::new();
        if let Some(ref f) = font_primary {
            let (m, b) = f.rasterize(ch, FONT_SIZE);
            if m.width > 0 && !b.is_empty() { metrics = Some(m); bitmap = b; }
        }
        if metrics.is_none() {
            if let Some(ref f) = font_fallback {
                let (m, b) = f.rasterize(ch, FONT_SIZE);
                metrics = Some(m); bitmap = b;
            }
        }
        let Some(metrics) = metrics else {
            glyphs.insert(ch, GlyphInfo { x: 0.0, y: 0.0, w: 0.0, h: 0.0, advance: 0.0, xmin: 0.0, ymin: 0.0 });
            continue;
        };
        if metrics.width == 0 || metrics.height == 0 {
            glyphs.insert(ch, GlyphInfo { x: 0.0, y: 0.0, w: 0.0, h: 0.0, advance: metrics.advance_width, xmin: 0.0, ymin: 0.0 });
            continue;
        }
        let gw = metrics.width as u32;
        let gh = metrics.height as u32;
        if cursor_x + gw + 2 > ATLAS_SIZE { cursor_x = 2; cursor_y += row_height + 2; row_height = 0; }
        for row in 0..gh {
            for col in 0..gw {
                let alpha = bitmap[(row * gw + col) as usize];
                if alpha == 0 { continue; }
                let idx = ((cursor_y + row) * ATLAS_SIZE + (cursor_x + col)) as usize;
                atlas_pixels[idx] = alpha;
            }
        }
        glyphs.insert(ch, GlyphInfo {
            x: cursor_x as f32 / ATLAS_SIZE as f32, y: cursor_y as f32 / ATLAS_SIZE as f32,
            w: gw as f32 / ATLAS_SIZE as f32, h: gh as f32 / ATLAS_SIZE as f32,
            advance: metrics.advance_width, xmin: metrics.xmin as f32, ymin: metrics.ymin as f32,
        });
        cursor_x += gw + 2;
        row_height = row_height.max(gh);
    }
    GlyphCache { atlas_pixels, glyphs, baseline_offset: base_ascent }
}

impl TextRenderer {
    pub fn new(device: &wgpu::Device, queue: &wgpu::Queue, format: wgpu::TextureFormat, extra_chars: &[char]) -> Self {
        let cached = GLYPH_CACHE.get_or_init(|| rasterize_all_glyphs(extra_chars));
        let glyphs = cached.glyphs.clone();
        let baseline_offset = cached.baseline_offset;
        let base_ascent = baseline_offset;

        // GPU 资源 — 使用缓存的字形像素数据
        let atlas_texture = device.create_texture_with_data(
            queue,
            &wgpu::TextureDescriptor {
                label: Some("glyph_atlas"),
                size: wgpu::Extent3d {
                    width: ATLAS_SIZE,
                    height: ATLAS_SIZE,
                    depth_or_array_layers: 1,
                },
                mip_level_count: 1,
                sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                format: wgpu::TextureFormat::R8Unorm,
                usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
                view_formats: &[],
            },
            wgpu::util::TextureDataOrder::LayerMajor,
            &cached.atlas_pixels,
        );

        let atlas_view = atlas_texture.create_view(&wgpu::TextureViewDescriptor::default());
        let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            address_mode_u: wgpu::AddressMode::ClampToEdge,
            address_mode_v: wgpu::AddressMode::ClampToEdge,
            address_mode_w: wgpu::AddressMode::ClampToEdge,
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            ..Default::default()
        });

        let screen_buf = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("glyph_screen"), size: 8,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        queue.write_buffer(&screen_buf, 0, bytemuck::cast_slice(&[800.0f32, 600.0f32]));

        let bind_group_layout =
            device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                label: Some("glyph_bgl"),
                entries: &[
                    wgpu::BindGroupLayoutEntry { binding: 0, visibility: wgpu::ShaderStages::FRAGMENT,
                        ty: wgpu::BindingType::Texture { sample_type: wgpu::TextureSampleType::Float { filterable: true },
                            view_dimension: wgpu::TextureViewDimension::D2, multisampled: false, }, count: None,
                    },
                    wgpu::BindGroupLayoutEntry { binding: 1, visibility: wgpu::ShaderStages::FRAGMENT,
                        ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering), count: None,
                    },
                    wgpu::BindGroupLayoutEntry { binding: 2, visibility: wgpu::ShaderStages::VERTEX,
                        ty: wgpu::BindingType::Buffer { ty: wgpu::BufferBindingType::Uniform, has_dynamic_offset: false,
                            min_binding_size: Some(std::num::NonZeroU64::new(8).unwrap()) }, count: None,
                    },
                ],
            });

        let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("glyph_bg"), layout: &bind_group_layout,
            entries: &[
                wgpu::BindGroupEntry { binding: 0, resource: wgpu::BindingResource::TextureView(&atlas_view) },
                wgpu::BindGroupEntry { binding: 1, resource: wgpu::BindingResource::Sampler(&sampler) },
                wgpu::BindGroupEntry { binding: 2, resource: screen_buf.as_entire_binding() },
            ],
        });

        // 单位正方形顶点
        let vertices: [[f32; 2]; 4] = [[0.0, 0.0], [1.0, 0.0], [0.0, 1.0], [1.0, 1.0]];
        let vertex_buf = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("glyph_vtx"),
            contents: bytemuck::cast_slice(&vertices),
            usage: wgpu::BufferUsages::VERTEX,
        });

        let max_instances = 2048;
        let buf_size = (max_instances * std::mem::size_of::<GlyphInstance>()) as u64;
        let mut instance_bufs = Vec::with_capacity(3);
        for i in 0..3 {
            instance_bufs.push(device.create_buffer(&wgpu::BufferDescriptor {
                label: Some(&format!("glyph_inst_{}", i)),
                size: buf_size,
                usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
                mapped_at_creation: false,
            }));
        }

        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("glyph_shader"),
            source: wgpu::ShaderSource::Wgsl(GLYPH_SHADER.into()),
        });

        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("glyph_layout"),
            bind_group_layouts: &[&bind_group_layout],
            push_constant_ranges: &[],
        });

        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("glyph_pipeline"),
            layout: Some(&pipeline_layout),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: Some("vs_main"),
                buffers: &[
                    wgpu::VertexBufferLayout {
                        array_stride: 8,
                        step_mode: wgpu::VertexStepMode::Vertex,
                        attributes: &[wgpu::VertexAttribute {
                            format: wgpu::VertexFormat::Float32x2,
                            offset: 0,
                            shader_location: 0,
                        }],
                    },
                    wgpu::VertexBufferLayout {
                        array_stride: std::mem::size_of::<GlyphInstance>() as u64,
                        step_mode: wgpu::VertexStepMode::Instance,
                        attributes: &[
                            wgpu::VertexAttribute {
                                format: wgpu::VertexFormat::Float32x2,
                                offset: 0,
                                shader_location: 1,
                            },
                            wgpu::VertexAttribute {
                                format: wgpu::VertexFormat::Float32x2,
                                offset: 8,
                                shader_location: 2,
                            },
                            wgpu::VertexAttribute {
                                format: wgpu::VertexFormat::Float32x2,
                                offset: 16,
                                shader_location: 3,
                            },
                            wgpu::VertexAttribute {
                                format: wgpu::VertexFormat::Float32x2,
                                offset: 24,
                                shader_location: 4,
                            },
                            wgpu::VertexAttribute {
                                format: wgpu::VertexFormat::Unorm8x4,
                                offset: 32,
                                shader_location: 5,
                            },
                        ],
                    },
                ],
                compilation_options: Default::default(),
            },
            fragment: Some(wgpu::FragmentState {
                module: &shader,
                entry_point: Some("fs_main"),
                targets: &[Some(wgpu::ColorTargetState {
                    format,
                    blend: Some(wgpu::BlendState::ALPHA_BLENDING),
                    write_mask: wgpu::ColorWrites::ALL,
                })],
                compilation_options: Default::default(),
            }),
            primitive: wgpu::PrimitiveState {
                topology: wgpu::PrimitiveTopology::TriangleStrip,
                ..Default::default()
            },
            depth_stencil: None,
            multisample: wgpu::MultisampleState::default(),
            multiview: None,
            cache: None,
        });

        Self {
            glyphs,
            font_size_px: FONT_SIZE,
            baseline_offset: base_ascent,
            queued_quads: Vec::new(),
            glyph_buf: Vec::with_capacity(256),
            vertex_buf, instance_bufs, current_buf: 0,
            screen_buffer: screen_buf,
            atlas_texture, atlas_view, sampler,
            bind_group, pipeline,
        }
    }

    pub fn queue_text(
        &mut self,
        text: &str,
        x: f32,
        y: f32,
        px_size: f32,
        color: [u8; 4],
    ) {
        let scale = px_size / self.font_size_px;
        let mut pen_x = x;

        for ch in text.chars() {
            if let Some(info) = self.glyphs.get(&ch) {
                if info.w > 0.0 && info.h > 0.0 {
                    let qw = (info.w * ATLAS_SIZE as f32) * scale;
                    let qh = (info.h * ATLAS_SIZE as f32) * scale;

                    // 底部对齐：所有字形底部对齐在 y + px_size
                    let y_top = y + px_size - qh;
                    self.queued_quads.push(TextQuad {
                        x: pen_x + info.xmin * scale,
                        y: y_top,
                        w: qw,
                        h: qh,
                        uv_x: info.x,
                        uv_y: info.y,
                        uv_w: info.w,
                        uv_h: info.h,
                        color,
                    });
                }
                pen_x += info.advance * scale;
            }
        }
    }

    pub fn update_screen(&self, queue: &wgpu::Queue, w: f32, h: f32) {
        queue.write_buffer(&self.screen_buffer, 0, bytemuck::cast_slice(&[w, h]));
    }

    pub fn clear(&mut self) {
        self.queued_quads.clear();
    }

    pub fn current_buffer(&self) -> usize {
        if self.current_buf == 0 {
            self.instance_bufs.len() - 1
        } else {
            self.current_buf - 1
        }
    }

    pub fn upload(&mut self, queue: &wgpu::Queue) -> usize {
        let count = self.queued_quads.len();
        if count == 0 {
            return 0;
        }

        self.glyph_buf.clear();
        self.glyph_buf.extend(self.queued_quads.iter().map(|q| GlyphInstance {
            offset: [q.x, q.y],
            size: [q.w, q.h],
            uv_offset: [q.uv_x, q.uv_y],
            uv_size: [q.uv_w, q.uv_h],
            color: q.color,
        }));

        let data = bytemuck::cast_slice(&self.glyph_buf);
        let buf = &self.instance_bufs[self.current_buf];
        queue.write_buffer(buf, 0, &data[..data.len()]);
        self.current_buf = (self.current_buf + 1) % self.instance_bufs.len();
        count
    }

    pub fn draw<'a>(&'a self, render_pass: &mut wgpu::RenderPass<'a>, instance_count: usize) {
        if instance_count == 0 {
            return;
        }
        let idx = if self.current_buf == 0 {
            self.instance_bufs.len() - 1
        } else {
            self.current_buf - 1
        };
        render_pass.set_pipeline(&self.pipeline);
        render_pass.set_bind_group(0, &self.bind_group, &[]);
        render_pass.set_vertex_buffer(0, self.vertex_buf.slice(..));
        render_pass.set_vertex_buffer(1, self.instance_bufs[idx].slice(..));
        render_pass.draw(0..4, 0..instance_count as u32);
    }
}
