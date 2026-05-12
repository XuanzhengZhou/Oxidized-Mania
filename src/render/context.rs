use std::collections::HashMap;
use std::sync::Arc;
use wgpu::{Device, Queue, Surface, SurfaceConfiguration, SurfaceTexture};
use winit::window::Window;

use super::quad::QuadRenderer;
use super::text::TextRenderer;
use crate::skin::{AtlasRegion, CpuSkin, TextureAtlas};

pub struct RenderCtx {
    pub surface: Surface<'static>,
    pub device: Device,
    pub queue: Queue,
    pub config: SurfaceConfiguration,
    pub quad: QuadRenderer,
    pub text: TextRenderer,
    pub atlas: TextureAtlas,
    /// 逻辑坐标系: 宽度按物理宽高比动态计算 (对标 Python logical_w)
    pub logical_w: u32,
    pub logical_h: u32,
}

impl RenderCtx {
    pub async fn new(window: Arc<Window>, cpu_skin: CpuSkin, extra_chars: &[char]) -> Self {
        let instance = wgpu::Instance::new(&wgpu::InstanceDescriptor::default());
        let surface = instance.create_surface(window.clone()).unwrap();
        let adapter = instance.request_adapter(&wgpu::RequestAdapterOptions {
            power_preference: wgpu::PowerPreference::HighPerformance,
            compatible_surface: Some(&surface),
            force_fallback_adapter: false,
        }).await.unwrap();

        let (device, queue) = adapter.request_device(
            &wgpu::DeviceDescriptor {
                label: Some("rhythm"), required_features: wgpu::Features::empty(),
                required_limits: wgpu::Limits::default(), ..Default::default()
            }, None,
        ).await.unwrap();

        let size = window.inner_size();
        let surface_caps = surface.get_capabilities(&adapter);
        let format = surface_caps.formats.iter().find(|f| f.is_srgb()).copied().unwrap_or(surface_caps.formats[0]);

        // Mailbox: GPU 全速渲染，显示器取最新帧，零撕裂 + 帧节奏均匀
        // 不支持时回退到 Fifo（传统垂直同步，全平台通用）
        let present_mode = if surface_caps.present_modes.contains(&wgpu::PresentMode::Mailbox) {
            wgpu::PresentMode::Mailbox
        } else {
            wgpu::PresentMode::Fifo
        };
        log::info!("[Render] present_mode: {:?}", present_mode);

        let config = SurfaceConfiguration {
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT, format,
            width: size.width, height: size.height,
            present_mode, desired_maximum_frame_latency: 2,
            alpha_mode: surface_caps.alpha_modes[0], view_formats: vec![],
        };
        surface.configure(&device, &config);

        // 逻辑坐标系: 高 600, 宽按屏幕比例 (对标 Python)
        let aspect = size.width as f32 / size.height.max(1) as f32;
        let logical_h: u32 = 600;
        let logical_w: u32 = (logical_h as f32 * aspect).max(800.0) as u32;

        let atlas = cpu_skin.build_atlas(&device, &queue);
        let quad = QuadRenderer::new(&device, &queue, format, &atlas);
        let text = TextRenderer::new(&device, &queue, format, extra_chars);

        // 初始化屏幕尺寸
        quad.update_screen(&queue, logical_w as f32, logical_h as f32);
        text.update_screen(&queue, logical_w as f32, logical_h as f32);
        crate::game::notes::set_screen_w(logical_w as f32);

        Self { surface, device, queue, config, quad, text, atlas, logical_w, logical_h }
    }

    pub fn skin_regions(&self) -> HashMap<String, AtlasRegion> {
        self.atlas.regions.clone()
    }

    pub fn resize(&mut self, width: u32, height: u32) {
        if width > 0 && height > 0 {
            self.config.width = width; self.config.height = height;
            self.surface.configure(&self.device, &self.config);
            let aspect = width as f32 / height.max(1) as f32;
            self.logical_w = (600.0 * aspect).max(800.0) as u32;
            self.quad.update_screen(&self.queue, self.logical_w as f32, self.logical_h as f32);
            self.text.update_screen(&self.queue, self.logical_w as f32, self.logical_h as f32);
            crate::game::notes::set_screen_w(self.logical_w as f32);
        }
    }

    pub fn begin_frame(&mut self) -> Result<SurfaceTexture, wgpu::SurfaceError> {
        self.surface.get_current_texture()
    }

    pub fn end_frame(&self, output: SurfaceTexture) {
        output.present();
    }
}
