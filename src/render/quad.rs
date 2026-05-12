use bytemuck::{Pod, Zeroable};
use crate::skin::TextureAtlas;
use wgpu::util::DeviceExt as _;

// ─── GPU 数据结构 ───

#[repr(C)]
#[derive(Copy, Clone, Debug, Pod, Zeroable)]
struct QuadVertex {
    corner: [f32; 2],
}

#[repr(C)]
#[derive(Copy, Clone, Debug, Pod, Zeroable)]
pub struct QuadInstance {
    pub offset: [f32; 2],
    pub size: [f32; 2],
    pub uv_offset: [f32; 2],
    pub uv_scale: [f32; 2],
    pub color: [u8; 4],
    pub tex_index: u32, // 0=colored, 1=skin atlas
}

// ─── 着色器 ───

const SHADER: &str = r#"
struct VertexInput {
    @location(0) corner: vec2<f32>,
    @location(1) offset: vec2<f32>,
    @location(2) size: vec2<f32>,
    @location(3) uv_offset: vec2<f32>,
    @location(4) uv_scale: vec2<f32>,
    @location(5) color: vec4<f32>,
    @location(6) tex_index: u32,
};

struct VertexOutput {
    @builtin(position) position: vec4<f32>,
    @location(0) color: vec4<f32>,
    @location(1) @interpolate(flat) tex_index: u32,
    @location(2) uv: vec2<f32>,
};

@group(0) @binding(0) var t_atlas: texture_2d<f32>;
@group(0) @binding(1) var s_atlas: sampler;
@group(0) @binding(2) var<uniform> screen: vec2<f32>;

@vertex
fn vs_main(in: VertexInput) -> VertexOutput {
    let screen_w = screen.x;
    let screen_h = screen.y;
    let x = (in.offset.x + in.corner.x * in.size.x) / screen_w * 2.0 - 1.0;
    let y = 1.0 - (in.offset.y + in.corner.y * in.size.y) / screen_h * 2.0;
    let uv = in.uv_offset + in.corner * in.uv_scale;
    return VertexOutput(vec4<f32>(x, y, 0.0, 1.0), in.color, in.tex_index, uv);
}

@fragment
fn fs_main(in: VertexOutput) -> @location(0) vec4<f32> {
    if in.tex_index == 1u {
        let tex_color = textureSample(t_atlas, s_atlas, in.uv);
        return vec4<f32>(in.color.rgb, in.color.a * tex_color.a) * tex_color;
    }
    return in.color;
}
"#;

// ─── QuadRenderer ───

pub struct QuadRenderer {
    vertex_buffer: wgpu::Buffer,
    pub instance_buffers: Vec<wgpu::Buffer>,
    pub current_buffer: usize,
    screen_buffer: wgpu::Buffer,
    bind_group: wgpu::BindGroup,
    pipeline: wgpu::RenderPipeline,
    pub instances: Vec<QuadInstance>,
    max_instances: usize,
}

impl QuadRenderer {
    pub fn update_screen(&self, queue: &wgpu::Queue, w: f32, h: f32) {
        queue.write_buffer(&self.screen_buffer, 0, bytemuck::cast_slice(&[w, h]));
    }
}

impl QuadRenderer {
    pub fn new(
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        format: wgpu::TextureFormat,
        atlas: &TextureAtlas,
    ) -> Self {
        let vertices: [QuadVertex; 4] = [
            QuadVertex { corner: [0.0, 0.0] },
            QuadVertex { corner: [1.0, 0.0] },
            QuadVertex { corner: [0.0, 1.0] },
            QuadVertex { corner: [1.0, 1.0] },
        ];

        let vertex_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("quad_vtx"),
            contents: bytemuck::cast_slice(&vertices),
            usage: wgpu::BufferUsages::VERTEX,
        });

        let max_instances: usize = 8192;
        let buf_size = (max_instances * std::mem::size_of::<QuadInstance>()) as u64;
        let mut instance_buffers = Vec::with_capacity(3);
        for i in 0..3 {
            instance_buffers.push(device.create_buffer(&wgpu::BufferDescriptor {
                label: Some(&format!("quad_inst_{}", i)),
                size: buf_size,
                usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
                mapped_at_creation: false,
            }));
        }

        let atlas_view = &atlas.view;
        let atlas_sampler = &atlas.sampler;

        // 屏幕尺寸 uniform buffer
        let screen_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("quad_screen"), size: 8,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        queue.write_buffer(&screen_buffer, 0, bytemuck::cast_slice(&[800.0f32, 600.0f32]));

        let bind_group_layout =
            device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                label: Some("quad_bgl"),
                entries: &[
                    wgpu::BindGroupLayoutEntry {
                        binding: 0, visibility: wgpu::ShaderStages::FRAGMENT,
                        ty: wgpu::BindingType::Texture {
                            sample_type: wgpu::TextureSampleType::Float { filterable: true },
                            view_dimension: wgpu::TextureViewDimension::D2, multisampled: false,
                        }, count: None,
                    },
                    wgpu::BindGroupLayoutEntry {
                        binding: 1, visibility: wgpu::ShaderStages::FRAGMENT,
                        ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                        count: None,
                    },
                    wgpu::BindGroupLayoutEntry {
                        binding: 2, visibility: wgpu::ShaderStages::VERTEX,
                        ty: wgpu::BindingType::Buffer { ty: wgpu::BufferBindingType::Uniform, has_dynamic_offset: false, min_binding_size: Some(std::num::NonZeroU64::new(8).unwrap()) },
                        count: None,
                    },
                ],
            });

        let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("quad_bg"), layout: &bind_group_layout,
            entries: &[
                wgpu::BindGroupEntry { binding: 0, resource: wgpu::BindingResource::TextureView(atlas_view) },
                wgpu::BindGroupEntry { binding: 1, resource: wgpu::BindingResource::Sampler(atlas_sampler) },
                wgpu::BindGroupEntry { binding: 2, resource: screen_buffer.as_entire_binding() },
            ],
        });

        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("quad_shader"),
            source: wgpu::ShaderSource::Wgsl(SHADER.into()),
        });

        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("quad_layout"),
            bind_group_layouts: &[&bind_group_layout],
            push_constant_ranges: &[],
        });

        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("quad_pipeline"),
            layout: Some(&pipeline_layout),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: Some("vs_main"),
                buffers: &[
                    wgpu::VertexBufferLayout {
                        array_stride: std::mem::size_of::<QuadVertex>() as u64,
                        step_mode: wgpu::VertexStepMode::Vertex,
                        attributes: &[wgpu::VertexAttribute {
                            format: wgpu::VertexFormat::Float32x2,
                            offset: 0,
                            shader_location: 0,
                        }],
                    },
                    wgpu::VertexBufferLayout {
                        array_stride: std::mem::size_of::<QuadInstance>() as u64,
                        step_mode: wgpu::VertexStepMode::Instance,
                        attributes: &[
                            wgpu::VertexAttribute { format: wgpu::VertexFormat::Float32x2, offset: 0, shader_location: 1 },
                            wgpu::VertexAttribute { format: wgpu::VertexFormat::Float32x2, offset: 8, shader_location: 2 },
                            wgpu::VertexAttribute { format: wgpu::VertexFormat::Float32x2, offset: 16, shader_location: 3 },
                            wgpu::VertexAttribute { format: wgpu::VertexFormat::Float32x2, offset: 24, shader_location: 4 },
                            wgpu::VertexAttribute { format: wgpu::VertexFormat::Unorm8x4, offset: 32, shader_location: 5 },
                            wgpu::VertexAttribute { format: wgpu::VertexFormat::Uint32, offset: 36, shader_location: 6 },
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
            vertex_buffer,
            instance_buffers, current_buffer: 0,
            screen_buffer,
            bind_group, pipeline,
            instances: Vec::with_capacity(256), max_instances,
        }
    }

    pub fn push_rect(&mut self, x: f32, y: f32, w: f32, h: f32, color: [u8; 4]) {
        self.instances.push(QuadInstance {
            offset: [x, y],
            size: [w, h],
            uv_offset: [0.0, 0.0],
            uv_scale: [0.0, 0.0],
            color,
            tex_index: 0,
        });
    }

    pub fn push_textured_rect(
        &mut self,
        x: f32, y: f32, w: f32, h: f32,
        uv_x: f32, uv_y: f32, uv_w: f32, uv_h: f32,
        color: [u8; 4],
    ) {
        self.instances.push(QuadInstance {
            offset: [x, y],
            size: [w, h],
            uv_offset: [uv_x, uv_y],
            uv_scale: [uv_w, uv_h],
            color,
            tex_index: 1,
        });
    }

    pub fn last_buffer(&self) -> usize {
        if self.current_buffer == 0 { self.instance_buffers.len() - 1 } else { self.current_buffer - 1 }
    }

    pub fn upload(&mut self, queue: &wgpu::Queue) -> usize {
        let count = self.instances.len();
        if count == 0 { return 0; }
        let data = bytemuck::cast_slice(&self.instances);
        let size = (count * std::mem::size_of::<QuadInstance>()) as u64;
        let buf = &self.instance_buffers[self.current_buffer];
        queue.write_buffer(buf, 0, &data[..size as usize]);
        self.current_buffer = (self.current_buffer + 1) % self.instance_buffers.len();
        count
    }

    pub fn clear(&mut self) { self.instances.clear(); }

    pub fn draw<'a>(&'a self, render_pass: &mut wgpu::RenderPass<'a>, buf_idx: usize, count: usize) {
        if count == 0 { return; }
        render_pass.set_pipeline(&self.pipeline);
        render_pass.set_bind_group(0, &self.bind_group, &[]);
        render_pass.set_vertex_buffer(0, self.vertex_buffer.slice(..));
        render_pass.set_vertex_buffer(1, self.instance_buffers[buf_idx].slice(..));
        render_pass.draw(0..4, 0..count as u32);
    }
}
