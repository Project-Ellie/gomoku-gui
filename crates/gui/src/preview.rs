//! Rendering one frame without a window.
//!
//! This exists so that the look can be checked without opening a window, and so
//! that the pipeline can be exercised on a machine with no display attached.

use std::path::Path;

use anyhow::{Context as _, Result};

/// The colour format used when there is no surface to take a format from.
pub const PREVIEW_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Rgba8Unorm;

/// A device and queue, with no surface. Used by the preview and by any future
/// offscreen work.
pub struct HeadlessGpu {
    /// The GPU.
    pub device: wgpu::Device,
    /// The submission queue.
    pub queue: wgpu::Queue,
}

impl HeadlessGpu {
    /// Open the default adapter.
    pub fn new() -> Result<HeadlessGpu> {
        let instance = wgpu::Instance::new(wgpu::InstanceDescriptor {
            backends: wgpu::Backends::METAL | wgpu::Backends::VULKAN | wgpu::Backends::GL,
            flags: wgpu::InstanceFlags::default(),
            memory_budget_thresholds: wgpu::MemoryBudgetThresholds::default(),
            backend_options: wgpu::BackendOptions::default(),
            display: None,
        });
        let adapter = pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions {
            power_preference: wgpu::PowerPreference::HighPerformance,
            compatible_surface: None,
            force_fallback_adapter: false,
            apply_limit_buckets: false,
        }))
        .context("no graphics adapter is available")?;
        let (device, queue) = pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor {
            label: Some("gomoku"),
            required_features: wgpu::Features::empty(),
            required_limits: wgpu::Limits::default(),
            experimental_features: wgpu::ExperimentalFeatures::default(),
            memory_hints: wgpu::MemoryHints::default(),
            trace: wgpu::Trace::Off,
        }))
        .context("the graphics adapter refused a device")?;
        Ok(HeadlessGpu { device, queue })
    }

    /// Render one frame and write it as a 24-bit BMP.
    pub fn write_frame(
        &self,
        path: &Path,
        width: u32,
        height: u32,
        renderer: &crate::render::Renderer,
    ) -> Result<()> {
        self.write_frame_with(path, width, height, renderer, |_, _| {})
    }

    /// Render one frame and return the pixels: tightly packed RGBA, top row
    /// first.
    pub fn read_frame(
        &self,
        width: u32,
        height: u32,
        renderer: &crate::render::Renderer,
    ) -> Result<Vec<u8>> {
        self.frame_rgba(width, height, renderer, |_, _| {})
    }

    /// Render one frame, let the caller add more passes, and write a BMP.
    pub fn write_frame_with(
        &self,
        path: &Path,
        width: u32,
        height: u32,
        renderer: &crate::render::Renderer,
        extra: impl FnOnce(&mut wgpu::CommandEncoder, &wgpu::TextureView),
    ) -> Result<()> {
        let pixels = self.frame_rgba(width, height, renderer, extra)?;
        write_bmp(path, width, height, &pixels)
    }

    /// Render one frame, let the caller add more passes, and return the pixels:
    /// tightly packed RGBA, top row first.
    fn frame_rgba(
        &self,
        width: u32,
        height: u32,
        renderer: &crate::render::Renderer,
        extra: impl FnOnce(&mut wgpu::CommandEncoder, &wgpu::TextureView),
    ) -> Result<Vec<u8>> {
        let samples = renderer.sample_count();
        let multisampled = self.device.create_texture(&wgpu::TextureDescriptor {
            label: Some("preview multisampled"),
            size: wgpu::Extent3d {
                width,
                height,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: samples,
            dimension: wgpu::TextureDimension::D2,
            format: PREVIEW_FORMAT,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
            view_formats: &[],
        });
        let resolved = self.device.create_texture(&wgpu::TextureDescriptor {
            label: Some("preview resolved"),
            size: wgpu::Extent3d {
                width,
                height,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: PREVIEW_FORMAT,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
            view_formats: &[],
        });

        // A copy needs each row to start on a 256-byte boundary.
        let unpadded = width * 4;
        let padded = unpadded.div_ceil(256) * 256;
        let readback = self.device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("preview readback"),
            size: (padded * height) as u64,
            usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
            mapped_at_creation: false,
        });

        let view =
            |texture: &wgpu::Texture| texture.create_view(&wgpu::TextureViewDescriptor::default());
        let mut encoder = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("preview"),
            });
        renderer.render(
            &mut encoder,
            &view(&multisampled),
            &view(&resolved),
            wgpu::Color {
                r: 0.02,
                g: 0.02,
                b: 0.03,
                a: 1.0,
            },
        );
        extra(&mut encoder, &view(&resolved));
        encoder.copy_texture_to_buffer(
            wgpu::TexelCopyTextureInfo {
                texture: &resolved,
                mip_level: 0,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            wgpu::TexelCopyBufferInfo {
                buffer: &readback,
                layout: wgpu::TexelCopyBufferLayout {
                    offset: 0,
                    bytes_per_row: Some(padded),
                    rows_per_image: Some(height),
                },
            },
            wgpu::Extent3d {
                width,
                height,
                depth_or_array_layers: 1,
            },
        );
        self.queue.submit([encoder.finish()]);

        readback.slice(..).map_async(wgpu::MapMode::Read, |result| {
            if let Err(error) = result {
                log::error!("the preview could not be read back: {error}");
            }
        });
        self.device
            .poll(wgpu::PollType::Wait {
                submission_index: None,
                timeout: None,
            })
            .context("the GPU did not finish the preview")?;

        let data = readback
            .slice(..)
            .get_mapped_range()
            .context("the preview buffer could not be mapped")?;
        let mut pixels = Vec::with_capacity((unpadded * height) as usize);
        for row in 0..height as usize {
            let start = row * padded as usize;
            pixels.extend_from_slice(&data[start..start + unpadded as usize]);
        }
        drop(data);
        readback.unmap();
        Ok(pixels)
    }
}

impl HeadlessGpu {
    /// Render the board, then the interface, into one file. Used to check the
    /// interface without opening a window.
    pub fn write_ui_frame(
        &self,
        path: &Path,
        size: u32,
        scale: f32,
        renderer: &mut crate::render::Renderer,
        session: &mut crate::session::Session,
    ) -> Result<()> {
        // `size` is in interface points and the image is in physical pixels, which
        // is how a window on a display with a scale factor is laid out.
        let pixels = (size as f32 * scale).round() as u32;
        let context = egui::Context::default();
        let mut egui_renderer = egui_wgpu::Renderer::new(
            &self.device,
            PREVIEW_FORMAT,
            egui_wgpu::RendererOptions {
                msaa_samples: 1,
                ..Default::default()
            },
        );
        let input = egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(
                egui::Pos2::ZERO,
                egui::vec2(size as f32, size as f32),
            )),
            viewports: {
                let mut map = egui::ViewportIdMap::default();
                map.insert(
                    egui::ViewportId::ROOT,
                    egui::ViewportInfo {
                        native_pixels_per_point: Some(scale),
                        ..Default::default()
                    },
                );
                map
            },
            ..Default::default()
        };
        let mut requests = crate::ui::Requests::default();
        let output = context.run_ui(input, |ui| {
            requests = crate::ui::draw(ui, session);
        });
        let _ = requests;
        let egui::FullOutput {
            textures_delta,
            shapes,
            pixels_per_point,
            ..
        } = output;
        crate::render::apply_textures(
            &mut egui_renderer,
            &self.device,
            &self.queue,
            &textures_delta,
        );
        let jobs = context.tessellate(shapes, pixels_per_point);
        let screen = egui_wgpu::ScreenDescriptor {
            size_in_pixels: [pixels, pixels],
            pixels_per_point,
        };
        let mut textures_delta = textures_delta;
        textures_delta.clear();
        // The same path as the window: the interface has reported the area, so the
        // camera and the renderer can be pointed at it.
        crate::app::prepare_frame(session, renderer, &self.queue, [pixels, pixels]);

        self.write_frame_with(path, pixels, pixels, renderer, |encoder, target| {
            let uploads =
                egui_renderer.update_buffers(&self.device, &self.queue, encoder, &jobs, &screen);
            self.queue.submit(uploads);
            let pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("interface"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: target,
                    depth_slice: None,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Load,
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
                multiview_mask: None,
            });
            let mut pass = pass.forget_lifetime();
            egui_renderer.render(&mut pass, &jobs, &screen);
        })
    }
}

/// Write 24-bit BMP data, with the rows in the order a BMP expects: bottom up.
/// The input is tightly packed RGBA, top row first.
fn write_bmp(path: &Path, width: u32, height: u32, rgba: &[u8]) -> Result<()> {
    let row_bytes = width * 3;
    let row_padding = (4 - (row_bytes % 4)) % 4;
    let image_bytes = (row_bytes + row_padding) * height;
    let file_bytes = 14 + 40 + image_bytes;

    let mut out = Vec::with_capacity(file_bytes as usize);
    out.extend_from_slice(b"BM");
    out.extend_from_slice(&file_bytes.to_le_bytes());
    out.extend_from_slice(&0_u16.to_le_bytes());
    out.extend_from_slice(&0_u16.to_le_bytes());
    out.extend_from_slice(&(14_u32 + 40).to_le_bytes());
    out.extend_from_slice(&40_u32.to_le_bytes());
    out.extend_from_slice(&(width as i32).to_le_bytes());
    out.extend_from_slice(&(height as i32).to_le_bytes());
    out.extend_from_slice(&1_u16.to_le_bytes());
    out.extend_from_slice(&24_u16.to_le_bytes());
    out.extend_from_slice(&0_u32.to_le_bytes());
    out.extend_from_slice(&image_bytes.to_le_bytes());
    out.extend_from_slice(&2835_u32.to_le_bytes());
    out.extend_from_slice(&2835_u32.to_le_bytes());
    out.extend_from_slice(&0_u32.to_le_bytes());
    out.extend_from_slice(&0_u32.to_le_bytes());

    for row in 0..height {
        let source = (height - 1 - row) as usize * width as usize * 4;
        for column in 0..width as usize {
            let pixel = source + column * 4;
            out.push(rgba[pixel + 2]);
            out.push(rgba[pixel + 1]);
            out.push(rgba[pixel]);
        }
        out.extend(std::iter::repeat_n(0_u8, row_padding as usize));
    }

    std::fs::write(path, out).with_context(|| format!("cannot write {}", path.display()))
}
