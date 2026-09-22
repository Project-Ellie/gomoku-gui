//! The window: a surface, the input, and the frame loop.
//!
//! Frames are drawn on demand. An idle window costs nothing.

use std::sync::Arc;

use anyhow::{Context as _, Result};
use gomoku_core::Game;
use winit::application::ApplicationHandler;
use winit::event::{ElementState, MouseButton, MouseScrollDelta, WindowEvent};
use winit::event_loop::{ActiveEventLoop, ControlFlow, EventLoop};
use winit::keyboard::{Key, NamedKey};
use winit::window::{Window, WindowId};

use crate::audio::Audio;
use crate::camera::Camera;
use crate::render::{Globals, Renderer, SHELL, SLATE, StoneInstance};

/// How far the pointer may move and still count as a click rather than a drag.
const CLICK_SLOP: f32 = 4.0;
/// The number of samples per pixel.
const SAMPLES: u32 = 4;

/// The application.
pub struct App {
    game: Game,
    camera: Camera,
    audio: Option<Audio>,
    state: Option<State>,
    /// Where the pointer is, in physical pixels.
    pointer: [f32; 2],
    /// Where the left button went down, if it is down.
    press: Option<[f32; 2]>,
    /// The number of frames to draw before quitting. Used by the smoke test.
    frame_limit: Option<u32>,
    frames: u32,
    dirty: bool,
    /// True once the loop has been asked to stop, so that a queued redraw does
    /// not draw one more frame.
    closing: bool,
}

struct State {
    window: Arc<Window>,
    surface: wgpu::Surface<'static>,
    config: wgpu::SurfaceConfiguration,
    device: wgpu::Device,
    queue: wgpu::Queue,
    renderer: Renderer,
    multisampled: wgpu::Texture,
}

impl App {
    /// A new application. `frame_limit` quits after that many frames.
    pub fn new(game: Game, audio: Option<Audio>, frame_limit: Option<u32>) -> App {
        App {
            game,
            camera: Camera::fit([1200, 950]),
            audio,
            state: None,
            pointer: [0.0, 0.0],
            press: None,
            frame_limit,
            frames: 0,
            dirty: true,
            closing: false,
        }
    }

    /// Open the window and run until it is closed.
    pub fn run(mut self) -> Result<()> {
        let event_loop = EventLoop::new().context("cannot open an event loop")?;
        event_loop.set_control_flow(ControlFlow::Wait);
        event_loop
            .run_app(&mut self)
            .context("the window loop failed")
    }

    fn size(&self) -> [u32; 2] {
        match &self.state {
            Some(state) => [state.config.width, state.config.height],
            None => [1200, 950],
        }
    }

    fn request_redraw(&mut self) {
        self.dirty = true;
        if let Some(state) = &self.state {
            state.window.request_redraw();
        }
    }

    fn undo(&mut self) {
        if self.game.undo() {
            self.request_redraw();
        }
    }

    fn fit(&mut self) {
        self.camera.reset(self.size());
        self.request_redraw();
    }

    fn flip(&mut self) {
        self.camera.flipped = !self.camera.flipped;
        self.request_redraw();
    }

    /// Place a stone, and knock when it lands.
    fn place(&mut self, intersection: [u8; 2]) {
        let Some(point) = gomoku_core::point(intersection[1], intersection[0]) else {
            return;
        };
        if self.game.stone_at(point).is_some() {
            return;
        }
        if self.game.play(point).is_err() {
            return;
        }
        if let Some(audio) = &mut self.audio {
            audio.knock(neighbours(&self.game, intersection));
        }
        self.request_redraw();
    }

    fn title(&self) -> String {
        let status = match self.game.outcome() {
            gomoku_core::Outcome::Ongoing => match self.game.to_move() {
                gomoku_core::Color::Black => "Black to move".to_string(),
                gomoku_core::Color::White => "White to move".to_string(),
            },
            gomoku_core::Outcome::Won { winner, .. } => match winner {
                gomoku_core::Color::Black => "Black wins".to_string(),
                gomoku_core::Color::White => "White wins".to_string(),
            },
            gomoku_core::Outcome::Draw => "A draw".to_string(),
        };
        format!("Gomoku — {status} — {} stones", self.game.len())
    }

    /// Draw one frame. Returns true when the frame limit is reached.
    fn draw(&mut self) -> bool {
        if self.closing {
            return true;
        }
        let Some(state) = &mut self.state else {
            return true;
        };
        let frame = match state.surface.get_current_texture() {
            wgpu::CurrentSurfaceTexture::Success(frame) => frame,
            wgpu::CurrentSurfaceTexture::Suboptimal(frame) => {
                // The surface no longer matches the window. Reconfigure, and use
                // the frame that was returned.
                state.surface.configure(&state.device, &state.config);
                frame
            }
            wgpu::CurrentSurfaceTexture::Outdated | wgpu::CurrentSurfaceTexture::Lost => {
                state.surface.configure(&state.device, &state.config);
                return false;
            }
            wgpu::CurrentSurfaceTexture::Validation => {
                log::warn!("the surface reported a validation error; skipping the frame");
                return false;
            }
            // A timeout or an occluded window: try again when there is something
            // to draw.
            wgpu::CurrentSurfaceTexture::Timeout | wgpu::CurrentSurfaceTexture::Occluded => {
                return false;
            }
        };

        let view = frame
            .texture
            .create_view(&wgpu::TextureViewDescriptor::default());
        let multisampled = state
            .multisampled
            .create_view(&wgpu::TextureViewDescriptor::default());

        let size = [state.config.width, state.config.height];
        let mut globals = Renderer::default_globals(size[0], size[1]);
        globals.view = [
            self.camera.centre[0],
            self.camera.centre[1],
            self.camera.pixels_per_cell,
            if self.camera.flipped { 1.0 } else { 0.0 },
        ];
        state.renderer.set_globals(&state.queue, &globals);
        state
            .renderer
            .set_stones(&state.queue, &stone_instances(&self.game));

        let mut encoder = state
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("frame"),
            });
        state.renderer.render(
            &mut encoder,
            &multisampled,
            &view,
            wgpu::Color {
                r: 0.02,
                g: 0.02,
                b: 0.03,
                a: 1.0,
            },
        );
        state.queue.submit([encoder.finish()]);
        // Presentation belongs to the queue, and a dropped surface texture is
        // discarded rather than shown.
        state.queue.present(frame);

        self.dirty = false;
        self.frames += 1;
        match self.frame_limit {
            Some(limit) if self.frames >= limit => {
                log::info!("drew {limit} frames; exiting");
                self.closing = true;
                true
            }
            _ => false,
        }
    }

    fn configure(&mut self, event_loop: &ActiveEventLoop, window: Arc<Window>) -> Result<State> {
        let size = window.inner_size();
        let instance = wgpu::Instance::new(wgpu::InstanceDescriptor {
            backends: wgpu::Backends::METAL | wgpu::Backends::VULKAN | wgpu::Backends::GL,
            flags: wgpu::InstanceFlags::default(),
            memory_budget_thresholds: wgpu::MemoryBudgetThresholds::default(),
            backend_options: wgpu::BackendOptions::default(),
            display: None,
        });
        let surface = instance
            .create_surface(Arc::clone(&window))
            .context("cannot draw on this window")?;
        let adapter = pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions {
            power_preference: wgpu::PowerPreference::HighPerformance,
            compatible_surface: Some(&surface),
            force_fallback_adapter: false,
            // Limit bucketing hides the real adapter limits. It matters only for
            // browsers that expose the GPU to untrusted content.
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

        let capabilities = surface.get_capabilities(&adapter);
        let format = capabilities
            .formats
            .iter()
            .copied()
            .find(wgpu::TextureFormat::is_srgb)
            .unwrap_or(capabilities.formats[0]);
        let alpha_mode = capabilities
            .alpha_modes
            .iter()
            .copied()
            .find(|mode| *mode == wgpu::CompositeAlphaMode::Opaque)
            .unwrap_or(capabilities.alpha_modes[0]);
        let config = wgpu::SurfaceConfiguration {
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
            format,
            color_space: wgpu::SurfaceColorSpace::Auto,
            width: size.width.max(1),
            height: size.height.max(1),
            present_mode: wgpu::PresentMode::Fifo,
            desired_maximum_frame_latency: 2,
            alpha_mode,
            view_formats: Vec::new(),
        };
        surface.configure(&device, &config);

        let renderer = Renderer::new(&device, &queue, format, SAMPLES);
        let multisampled = create_multisampled(&device, &config);
        let _ = event_loop;
        Ok(State {
            window,
            surface,
            config,
            device,
            queue,
            renderer,
            multisampled,
        })
    }
}

fn create_multisampled(
    device: &wgpu::Device,
    config: &wgpu::SurfaceConfiguration,
) -> wgpu::Texture {
    device.create_texture(&wgpu::TextureDescriptor {
        label: Some("multisampled"),
        size: wgpu::Extent3d {
            width: config.width,
            height: config.height,
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: SAMPLES,
        dimension: wgpu::TextureDimension::D2,
        format: config.format,
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
        view_formats: &[],
    })
}

/// How many of the four orthogonal neighbours of an intersection hold a stone.
fn neighbours(game: &Game, intersection: [u8; 2]) -> usize {
    let mut count = 0;
    for (dx, dy) in [(-1_i8, 0_i8), (1, 0), (0, -1), (0, 1)] {
        let column = intersection[0] as i8 + dx;
        let row = intersection[1] as i8 + dy;
        if !(0..15).contains(&column) || !(0..15).contains(&row) {
            continue;
        }
        if let Some(point) = gomoku_core::point(row as u8, column as u8) {
            if game.stone_at(point).is_some() {
                count += 1;
            }
        }
    }
    count
}

/// The stones to draw, in play order.
pub fn stone_instances(game: &Game) -> Vec<StoneInstance> {
    game.stones()
        .enumerate()
        .map(|(index, (point, colour))| {
            let material = match colour {
                gomoku_core::Color::Black => SLATE,
                gomoku_core::Color::White => SHELL,
            };
            // A seed per stone, so that the texture of no two stones matches.
            let seed = ((index as f32 + 1.0) * 0.618_034).fract() * 37.0;
            StoneInstance {
                centre: [point.col() as f32, point.row() as f32],
                colour: [
                    material.albedo[0],
                    material.albedo[1],
                    material.albedo[2],
                    material.roughness,
                ],
                params: [seed, material.kind, 0.0, 0.0],
            }
        })
        .collect()
}

impl ApplicationHandler for App {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        if self.state.is_some() {
            return;
        }
        let attributes = Window::default_attributes()
            .with_title(self.title())
            .with_inner_size(winit::dpi::LogicalSize::new(1180.0_f64, 950.0_f64));
        let window = match event_loop.create_window(attributes) {
            Ok(window) => Arc::new(window),
            Err(error) => {
                log::error!("cannot open a window: {error}");
                event_loop.exit();
                return;
            }
        };
        match self.configure(event_loop, Arc::clone(&window)) {
            Ok(state) => {
                state.window.set_title(&self.title());
                self.camera = Camera::fit([state.config.width, state.config.height]);
                self.state = Some(state);
                self.request_redraw();
            }
            Err(error) => {
                log::error!("{error:#}");
                event_loop.exit();
            }
        }
    }

    fn window_event(
        &mut self,
        event_loop: &ActiveEventLoop,
        _window_id: WindowId,
        event: WindowEvent,
    ) {
        match event {
            WindowEvent::CloseRequested => event_loop.exit(),
            WindowEvent::Resized(size) => {
                if let Some(state) = &mut self.state {
                    if size.width > 0 && size.height > 0 {
                        state.config.width = size.width;
                        state.config.height = size.height;
                        state.surface.configure(&state.device, &state.config);
                        state.multisampled = create_multisampled(&state.device, &state.config);
                    }
                }
                self.camera.clamp(self.size());
                self.request_redraw();
            }
            WindowEvent::ScaleFactorChanged { .. } => {
                self.request_redraw();
            }
            WindowEvent::CursorMoved { position, .. } => {
                self.pointer = [position.x as f32, position.y as f32];
                if let Some(press) = self.press {
                    let delta = [self.pointer[0] - press[0], self.pointer[1] - press[1]];
                    if delta[0].abs() > CLICK_SLOP || delta[1].abs() > CLICK_SLOP {
                        self.press = None;
                        self.camera.pan(self.size(), delta);
                        self.request_redraw();
                    }
                }
            }
            WindowEvent::MouseInput { state, button, .. } => {
                if button != MouseButton::Left {
                    return;
                }
                match state {
                    ElementState::Pressed => self.press = Some(self.pointer),
                    ElementState::Released => {
                        if let Some(press) = self.press.take() {
                            let moved = (self.pointer[0] - press[0]).abs() > CLICK_SLOP
                                || (self.pointer[1] - press[1]).abs() > CLICK_SLOP;
                            if !moved {
                                if let Some(intersection) =
                                    self.camera.intersection(self.size(), self.pointer)
                                {
                                    self.place(intersection);
                                }
                            }
                        }
                    }
                }
            }
            WindowEvent::MouseWheel { delta, .. } => {
                let amount = match delta {
                    MouseScrollDelta::LineDelta(_, lines) => lines * 0.15,
                    MouseScrollDelta::PixelDelta(position) => position.y as f32 * 0.0015,
                };
                self.camera
                    .zoom_about(self.size(), self.pointer, amount.exp());
                self.request_redraw();
            }
            WindowEvent::KeyboardInput { event, .. } => {
                if event.state != ElementState::Pressed {
                    return;
                }
                match &event.logical_key {
                    Key::Named(NamedKey::Escape) => event_loop.exit(),
                    Key::Named(NamedKey::Space) | Key::Named(NamedKey::Backspace) => self.undo(),
                    Key::Character(text) => match text.as_str() {
                        "f" | "0" => self.fit(),
                        "u" => self.undo(),
                        "v" => self.flip(),
                        _ => {}
                    },
                    _ => {}
                }
            }
            WindowEvent::RedrawRequested => {
                if self.state.is_none() {
                    event_loop.exit();
                    return;
                }
                if self.draw() {
                    event_loop.exit();
                    return;
                }
                if let Some(state) = &self.state {
                    state.window.set_title(&self.title());
                }
            }
            _ => {}
        }
    }

    fn about_to_wait(&mut self, event_loop: &ActiveEventLoop) {
        // A frame is drawn when something changed, and while a frame limit is
        // being counted down.
        if !self.closing && (self.dirty || self.frame_limit.is_some()) {
            if let Some(state) = &self.state {
                state.window.request_redraw();
            }
        }
        let _ = event_loop;
    }

    fn exiting(&mut self, _event_loop: &ActiveEventLoop) {
        log::info!("closing after {} frames", self.frames);
    }
}

/// The uniform block for a camera and a window, with the default materials.
pub fn globals_for(camera: &Camera, size: [u32; 2]) -> Globals {
    let mut globals = Renderer::default_globals(size[0], size[1]);
    globals.view = [
        camera.centre[0],
        camera.centre[1],
        camera.pixels_per_cell,
        if camera.flipped { 1.0 } else { 0.0 },
    ];
    globals
}
