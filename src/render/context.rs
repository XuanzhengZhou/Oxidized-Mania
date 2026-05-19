use std::collections::HashMap;
use std::sync::Arc;
use wgpu::{Device, Queue, Surface, SurfaceConfiguration, SurfaceTexture};
use winit::window::Window;

use super::quad::QuadRenderer;
use super::text::TextRenderer;
use crate::skin::{AtlasRegion, CpuSkin, TextureAtlas};

// ─── 共享 GPU 资源（整个应用生命周期只创建一次）───

pub(crate) struct GpuInner {
    pub device: Device,
    pub queue: Queue,
}

/// 应用全局 GPU 上下文：Instance + Adapter + Device + Queue
/// 只创建一次，所有 RenderCtx 通过 Arc 共享 Device/Queue
pub struct GpuContext {
    pub instance: wgpu::Instance,
    pub adapter: wgpu::Adapter,
    pub format: wgpu::TextureFormat,
    inner: Arc<GpuInner>,
}

impl GpuContext {
    pub async fn new(window: &Window) -> Self {
        let instance = wgpu::Instance::new(&wgpu::InstanceDescriptor::default());
        let tmp_surface = instance.create_surface(Arc::new(window.clone())).unwrap();
        let adapter = instance.request_adapter(&wgpu::RequestAdapterOptions {
            power_preference: wgpu::PowerPreference::HighPerformance,
            compatible_surface: Some(&tmp_surface),
            force_fallback_adapter: false,
        }).await.unwrap();

        let (device, queue) = adapter.request_device(
            &wgpu::DeviceDescriptor {
                label: Some("rhythm"), required_features: wgpu::Features::empty(),
                required_limits: wgpu::Limits::default(), ..Default::default()
            }, None,
        ).await.unwrap();

        let surface_caps = tmp_surface.get_capabilities(&adapter);
        let format = surface_caps.formats.iter().find(|f| f.is_srgb()).copied().unwrap_or(surface_caps.formats[0]);

        Self { instance, adapter, format, inner: Arc::new(GpuInner { device, queue }) }
    }

    pub(crate) fn inner(&self) -> Arc<GpuInner> { self.inner.clone() }
}

// ─── 渲染上下文 ───

pub struct RenderCtx {
    pub surface: Surface<'static>,
    /// 共享 GPU 设备+队列（pub 支持字段级借检查 split-borrow）
    pub gpu: Arc<GpuInner>,
    pub config: SurfaceConfiguration,
    pub quad: QuadRenderer,
    pub text: TextRenderer,
    pub atlas: TextureAtlas,
    /// 逻辑坐标系: 宽度按物理宽高比动态计算 (对标 Python logical_w)
    pub logical_w: u32,
    pub logical_h: u32,
}

impl RenderCtx {
    pub async fn new(window: Arc<Window>, cpu_skin: CpuSkin, extra_chars: &[char], gpu: &GpuContext) -> Self {
        let surface = gpu.instance.create_surface(window.clone()).unwrap();
        let device = &gpu.inner.device;
        let queue = &gpu.inner.queue;

        let size = window.inner_size();
        let surface_caps = surface.get_capabilities(&gpu.adapter);

        let present_mode = if surface_caps.present_modes.contains(&wgpu::PresentMode::Mailbox) {
            wgpu::PresentMode::Mailbox
        } else {
            wgpu::PresentMode::Fifo
        };
        log::info!("[Render] present_mode: {:?}", present_mode);

        let config = SurfaceConfiguration {
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT, format: gpu.format,
            width: size.width, height: size.height,
            present_mode, desired_maximum_frame_latency: 2,
            alpha_mode: surface_caps.alpha_modes[0], view_formats: vec![],
        };
        surface.configure(device, &config);

        let aspect = size.width as f32 / size.height.max(1) as f32;
        let logical_h: u32 = 600;
        let logical_w: u32 = (logical_h as f32 * aspect).max(800.0) as u32;

        let atlas = cpu_skin.build_atlas(device, queue);
        let quad = QuadRenderer::new(device, queue, gpu.format, &atlas);
        let text = TextRenderer::new(device, queue, gpu.format, extra_chars);

        quad.update_screen(queue, logical_w as f32, logical_h as f32);
        text.update_screen(queue, logical_w as f32, logical_h as f32);
        crate::game::notes::set_screen_w(logical_w as f32);

        Self {
            surface, gpu: gpu.inner(),
            config, quad, text, atlas, logical_w, logical_h,
        }
    }

    pub fn skin_regions(&self) -> HashMap<String, AtlasRegion> {
        self.atlas.regions.clone()
    }

    pub fn resize(&mut self, width: u32, height: u32) {
        if width > 0 && height > 0 {
            self.config.width = width; self.config.height = height;
            self.surface.configure(&self.gpu.device, &self.config);
            let aspect = width as f32 / height.max(1) as f32;
            self.logical_w = (600.0 * aspect).max(800.0) as u32;
            self.quad.update_screen(&self.gpu.queue, self.logical_w as f32, self.logical_h as f32);
            self.text.update_screen(&self.gpu.queue, self.logical_w as f32, self.logical_h as f32);
            crate::game::notes::set_screen_w(self.logical_w as f32);
        }
    }

    pub fn begin_frame(&mut self) -> Result<SurfaceTexture, wgpu::SurfaceError> {
        self.surface.get_current_texture()
    }

    pub fn end_frame(&self, output: SurfaceTexture) {
        output.present();
    }

    pub fn set_spectrum(&mut self, w: u32, h: u32, colormap: &str) {
        self.quad.set_spectrum(&self.gpu.device, &self.gpu.queue, w, h, colormap, &self.atlas);
    }
    pub fn update_spectrum_rect(&self, x: u32, y: u32, w: u32, h: u32, data: &[u8]) {
        self.quad.update_spectrum_rect(&self.gpu.queue, x, y, w, h, data);
    }
    pub fn update_colormap(&mut self, name: &str) {
        self.quad.update_colormap(&self.gpu.device, &self.gpu.queue, name, &self.atlas);
    }

    pub fn update_cover(&self, cover_path: &std::path::Path) -> bool {
        let region = match self.atlas.regions.get("bg_cover") {
            Some(r) => r,
            None => { log::warn!("[Cover] bg_cover not in atlas"); return false; }
        };
        let atlas_size = self.atlas.size;
        let px = (region.uv_x * atlas_size as f32) as u32;
        let py = (region.uv_y * atlas_size as f32) as u32;

        let (data, w, h) = match crate::skin::load_png(cover_path) {
            Some((d, iw, ih)) => crate::skin::resize_image(&d, iw, ih, region.width, region.height),
            None => { log::warn!("[Cover] failed to load: {:?}", cover_path); return false; }
        };

        self.gpu.queue.write_texture(
            wgpu::TexelCopyTextureInfo {
                texture: &self.atlas.texture,
                mip_level: 0,
                origin: wgpu::Origin3d { x: px, y: py, z: 0 },
                aspect: wgpu::TextureAspect::All,
            },
            &data,
            wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(w * 4),
                rows_per_image: Some(h),
            },
            wgpu::Extent3d { width: w, height: h, depth_or_array_layers: 1 },
        );
        log::info!("[Cover] updated in-place: {:?}", cover_path.file_name());
        true
    }
}
