//! The window: a surface, the interface, the file handling, and the frame loop.
//!
//! The board is drawn by our own passes. The interface is drawn by egui on top,
//! in the same surface, after the samples are resolved. Events go to egui first;
//! the board only sees a pointer or a key that egui did not want.

use std::sync::Arc;

use anyhow::{Context as _, Result};
use gomoku_core::Settings;
use winit::application::ApplicationHandler;
use winit::event::{ElementState, MouseButton, MouseScrollDelta, WindowEvent};
use winit::event_loop::{ActiveEventLoop, ControlFlow, EventLoop};
use winit::keyboard::{Key, NamedKey};
use winit::window::{Window, WindowId};

use crate::audio::Audio;
use crate::camera::Viewport;
use crate::render::{Globals, GlyphTexture, Renderer, SHELL, SLATE, StoneInstance, WoodTexture};
use crate::session::{After, Dialog, Session, WindowGeometry};
use crate::ui::{self, Requests};

/// How far the pointer may move and still count as a click rather than a drag.
const CLICK_SLOP: f32 = 4.0;
/// The number of samples per pixel.
const SAMPLES: u32 = 4;

/// The application.
pub struct App {
    session: Session,
    audio: Option<Audio>,
    state: Option<State>,
    /// Where the left button went down, if it is down.
    press: Option<[f32; 2]>,
    /// The number of frames to draw before quitting. Used by the smoke test.
    frame_limit: Option<u32>,
    frames: u32,
    dirty: bool,
    closing: bool,
    geometry: WindowGeometry,
}

struct State {
    window: Arc<Window>,
    surface: wgpu::Surface<'static>,
    config: wgpu::SurfaceConfiguration,
    device: wgpu::Device,
    queue: wgpu::Queue,
    renderer: Renderer,
    multisampled: wgpu::Texture,
    egui: egui::Context,
    egui_state: egui_winit::State,
    egui_renderer: egui_wgpu::Renderer,
}

impl App {
    /// A new application. `frame_limit` quits after that many frames.
    pub fn new(
        settings: Settings,
        game: gomoku_core::Game,
        audio: Option<Audio>,
        frame_limit: Option<u32>,
    ) -> App {
        let geometry = WindowGeometry {
            width: settings.window.width,
            height: settings.window.height,
            x: settings.window.x,
            y: settings.window.y,
            maximized: settings.window.maximized,
        };
        let mut session = Session::new(&settings);
        // A game can be handed in, which is how `--demo` puts stones on the board.
        session.game = game;
        App {
            session,
            audio,
            state: None,
            press: None,
            frame_limit,
            frames: 0,
            dirty: true,
            closing: false,
            geometry,
        }
    }

    /// Open the window and run until it is closed.
    pub fn run(mut self) -> Result<()> {
        match self.session.autosave_to_offer() {
            Some(offer) => {
                log::trace!("offering autosave from {:?}", offer);
                self.session.dialog = Some(Dialog::Resume { source: offer });
            }
            None => {
                log::trace!("no autosave to offer");
            }
        }
        let event_loop = EventLoop::new().context("cannot open an event loop")?;
        event_loop.set_control_flow(ControlFlow::Wait);
        let result = event_loop.run_app(&mut self);
        self.write_settings();
        result.context("the window loop failed")
    }

    /// Write the settings, and the game in progress, on the way out.
    fn write_settings(&mut self) {
        match crate::session::settings_path() {
            Ok(path) => {
                let settings = self.session.settings(&self.geometry);
                if let Err(error) = gomoku_core::save_settings(&path, &settings) {
                    log::warn!("the settings could not be saved: {error}");
                }
            }
            Err(error) => log::warn!("the settings have no home: {error}"),
        }
        if self.session.changed() {
            self.session.autosave();
        } else if self.session.game.is_empty() {
            self.session.discard_autosave();
        }
    }

    fn request_redraw(&mut self) {
        self.dirty = true;
        if let Some(state) = &self.state {
            state.window.request_redraw();
        }
    }

    /// Perform an action that may lose unsaved changes.
    fn attempt(&mut self, then: After, event_loop: &ActiveEventLoop) {
        if self.session.changed() {
            self.session.dialog = Some(Dialog::Unsaved { then });
            self.request_redraw();
            return;
        }
        self.perform(then, event_loop);
    }

    /// Perform an action, whatever the state of the game.
    fn perform(&mut self, then: After, event_loop: &ActiveEventLoop) {
        match then {
            After::NewGame => {
                self.session.new_game();
                self.request_redraw();
            }
            After::Open => self.ask_to_open(),
            After::OpenPuzzle => self.ask_to_open_puzzle(),
            After::Quit => {
                self.closing = true;
                event_loop.exit();
            }
        }
    }

    /// Ask for a file and open it.
    fn ask_to_open(&mut self) {
        let mut dialog = rfd::FileDialog::new()
            .add_filter("Gomoku game", &["json"])
            .set_title("Open a game");
        if let Some(directory) = &self.session.last_directory {
            dialog = dialog.set_directory(directory);
        }
        if let Some(path) = dialog.pick_file() {
            self.session.open(&path);
            self.request_redraw();
        }
    }

    /// Ask for a puzzle file and load it.
    fn ask_to_open_puzzle(&mut self) {
        let mut dialog = rfd::FileDialog::new()
            .add_filter("Gomoku puzzle", &["json"])
            .set_title("Load a puzzle");
        if let Some(directory) = &self.session.last_directory {
            dialog = dialog.set_directory(directory);
        }
        if let Some(path) = dialog.pick_file() {
            self.session.open_puzzle(&path);
            self.request_redraw();
        }
    }

    /// Ask for a file and save to it.
    fn ask_to_save(&mut self) {
        let mut dialog = rfd::FileDialog::new()
            .add_filter("Gomoku game", &["json"])
            .set_title("Save the game")
            .set_file_name("gomoku.json");
        if let Some(directory) = &self.session.last_directory {
            dialog = dialog.set_directory(directory);
        }
        if let Some(path) = dialog.save_file() {
            self.session.save_to(&path);
            self.request_redraw();
        }
    }

    /// Save, asking for a file when the game has no name yet.
    fn save(&mut self, event_loop: &ActiveEventLoop) {
        if self.session.save() {
            self.ask_to_save();
            return;
        }
        if let Some(after) = self.session.after_save.take() {
            self.perform(after, event_loop);
        }
        self.request_redraw();
    }

    /// Act on what the interface asked for.
    fn run_requests(&mut self, requests: Requests, event_loop: &ActiveEventLoop) {
        if let Some(after) = self.session.perform.take() {
            self.perform(after, event_loop);
            return;
        }
        if requests.quit {
            self.attempt(After::Quit, event_loop);
        } else if requests.new_game {
            self.attempt(After::NewGame, event_loop);
        } else if requests.open {
            self.attempt(After::Open, event_loop);
        } else if requests.open_puzzle {
            self.attempt(After::OpenPuzzle, event_loop);
        } else if requests.save {
            self.save(event_loop);
        } else if requests.save_as {
            self.ask_to_save();
        }
    }

    fn draw(&mut self, event_loop: &ActiveEventLoop) -> bool {
        if self.closing {
            return true;
        }
        let Some(state) = &mut self.state else {
            return true;
        };

        let frame = match state.surface.get_current_texture() {
            wgpu::CurrentSurfaceTexture::Success(frame) => frame,
            wgpu::CurrentSurfaceTexture::Suboptimal(frame) => {
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

        // The interface runs first, because it is what works out where the board
        // may be drawn: the panels take their space from the window.
        let raw_input = state.egui_state.take_egui_input(&state.window);
        let mut requests = Requests::default();
        let full_output = state.egui.run_ui(raw_input, |ui| {
            requests = ui::draw(ui, &mut self.session);
        });
        let egui::FullOutput {
            platform_output,
            textures_delta,
            shapes,
            pixels_per_point,
            ..
        } = full_output;
        state
            .egui_state
            .handle_platform_output(&state.window, platform_output);
        crate::render::apply_textures(
            &mut state.egui_renderer,
            &state.device,
            &state.queue,
            &textures_delta,
        );
        let jobs = state.egui.tessellate(shapes, pixels_per_point);
        let screen = egui_wgpu::ScreenDescriptor {
            size_in_pixels: [state.config.width, state.config.height],
            pixels_per_point: state.egui.pixels_per_point(),
        };
        // The board, in the area that the interface left for it.
        prepare_frame(
            &mut self.session,
            &mut state.renderer,
            &state.queue,
            [state.config.width, state.config.height],
        );

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
        let uploads = state.egui_renderer.update_buffers(
            &state.device,
            &state.queue,
            &mut encoder,
            &jobs,
            &screen,
        );
        {
            let pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("interface"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &view,
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
            state.egui_renderer.render(&mut pass, &jobs, &screen);
        }
        let mut textures_delta = textures_delta;
        textures_delta.clear();

        state
            .queue
            .submit(uploads.into_iter().chain([encoder.finish()]));
        state.queue.present(frame);

        if let Some(touching) = self.session.knock.take() {
            if let Some(audio) = &mut self.audio {
                if self.session.sound {
                    audio.set_volume(self.session.volume);
                    audio.knock(touching);
                }
            }
        }
        if self.session.title_stale {
            state.window.set_title(&self.session.title());
            self.session.title_stale = false;
        }

        self.run_requests(requests, event_loop);

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

    fn configure(&mut self, window: Arc<Window>) -> Result<State> {
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
        // A format without the sRGB encoding: the board encodes its own output
        // and egui writes its own, so the hardware must not encode again.
        let format = capabilities
            .formats
            .iter()
            .copied()
            .find(|format| !format.is_srgb())
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

        let wood = WoodTexture::load_wood()?;
        let glyphs = GlyphTexture::load_glyphs()?;
        let renderer = Renderer::new(&device, &queue, format, SAMPLES, &wood, &glyphs);
        let multisampled = create_multisampled(&device, &config);

        let egui = egui::Context::default();
        let egui_state = egui_winit::State::new(
            egui.clone(),
            egui::ViewportId::ROOT,
            &window,
            Some(window.scale_factor() as f32),
            None,
            Some(8192),
        );
        let egui_renderer = egui_wgpu::Renderer::new(
            &device,
            format,
            egui_wgpu::RendererOptions {
                msaa_samples: 1,
                ..Default::default()
            },
        );

        // Until the interface reports the area that it leaves for the board, the
        // viewport is the whole window, which is the best that is known.
        self.session.viewport = Viewport::window(config.width, config.height);
        self.session.pixels_per_point = egui.pixels_per_point();
        self.session.fitted = false;

        Ok(State {
            window,
            surface,
            config,
            device,
            queue,
            renderer,
            multisampled,
            egui,
            egui_state,
            egui_renderer,
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

/// Point the camera and the renderer at the area that the interface left, and
/// hand the board to the renderer. The window and the preview both use this, so
/// that a preview shows what the window shows.
pub fn prepare_frame(
    session: &mut Session,
    renderer: &mut Renderer,
    queue: &wgpu::Queue,
    surface: [u32; 2],
) {
    // The interface knows the area for the board, and the surface knows how big
    // the whole window is. The projection needs both.
    session.viewport.frame = [surface[0] as f32, surface[1] as f32];
    // A resize can leave the view outside the area, and the interface has already
    // placed the view for its first frame.
    session.camera.clamp(session.viewport);
    renderer.set_globals(queue, &globals_for(session));
    renderer.set_stones(queue, &stone_instances(&session.game));
}

/// The uniform block for a session: the view, the viewport, and the materials.
fn globals_for(session: &Session) -> Globals {
    let look = crate::render::WoodLook {
        gain: session.gain,
        ..crate::render::WOOD
    };
    let mut globals = Renderer::globals_with_wood(session.viewport, look);
    globals.view = [
        session.camera.centre[0],
        session.camera.centre[1],
        session.camera.pixels_per_cell,
        if session.camera.flipped { 1.0 } else { 0.0 },
    ];
    globals.last_move = if session.toggles.last_move {
        session
            .game
            .last_move()
            .map(|point| [point.col() as f32, point.row() as f32, 1.0, 0.0])
            .unwrap_or([0.0; 4])
    } else {
        [0.0; 4]
    };
    globals.toggles = [
        if session.toggles.coordinates {
            1.0
        } else {
            0.0
        },
        0.0,
        0.0,
        0.0,
    ];
    globals
}

/// The stones to draw, in play order.
pub fn stone_instances(game: &gomoku_core::Game) -> Vec<StoneInstance> {
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
            .with_title(self.session.title())
            .with_inner_size(winit::dpi::LogicalSize::new(
                self.geometry.width as f64,
                self.geometry.height as f64,
            ));
        let window = match event_loop.create_window(attributes) {
            Ok(window) => Arc::new(window),
            Err(error) => {
                log::error!("cannot open a window: {error}");
                event_loop.exit();
                return;
            }
        };
        match self.configure(Arc::clone(&window)) {
            Ok(state) => {
                state.window.set_title(&self.session.title());
                self.session.title_stale = false;
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
        // The interface sees every event first.
        if let Some(state) = &mut self.state {
            let response = state.egui_state.on_window_event(&state.window, &event);
            if response.repaint {
                self.dirty = true;
            }
        }
        // The interface takes the pointer while it is dragging something, such as
        // the divider of the side panel. It also reports that the pointer is over
        // it everywhere else in the window, which must not stop play: the board is
        // drawn outside egui, so the application decides who a click belongs to.
        let wants_pointer = self
            .state
            .as_ref()
            .map(|state| state.egui.egui_is_using_pointer())
            .unwrap_or(false)
            || !self.session.on_board();
        let wants_keyboard = self
            .state
            .as_ref()
            .map(|state| state.egui.egui_wants_keyboard_input())
            .unwrap_or(false);
        let modal = self.session.dialog.is_some();

        match event {
            WindowEvent::CloseRequested => {
                if self.session.changed() {
                    self.session.dialog = Some(Dialog::Unsaved { then: After::Quit });
                    self.request_redraw();
                } else {
                    event_loop.exit();
                }
            }
            WindowEvent::Resized(size) => {
                if let Some(state) = &mut self.state {
                    if size.width > 0 && size.height > 0 {
                        state.config.width = size.width;
                        state.config.height = size.height;
                        state.surface.configure(&state.device, &state.config);
                        state.multisampled = create_multisampled(&state.device, &state.config);
                        // The area for the board is not known here: the interface
                        // reports it in the next frame. `prepare_frame` keeps the
                        // view inside whatever area that is.
                    }
                }
                self.request_redraw();
            }
            WindowEvent::Moved(position) => {
                self.geometry.x = Some(position.x as f32);
                self.geometry.y = Some(position.y as f32);
            }
            WindowEvent::ScaleFactorChanged { .. } => {
                if let Some(state) = &self.state {
                    self.session.pixels_per_point = state.egui.pixels_per_point();
                }
                self.request_redraw();
            }
            WindowEvent::CursorMoved { position, .. } => {
                self.session.pointer = [position.x as f32, position.y as f32];
                self.session.update_hover();
                self.dirty = true;
                if let Some(press) = self.press {
                    let delta = [
                        self.session.pointer[0] - press[0],
                        self.session.pointer[1] - press[1],
                    ];
                    if delta[0].abs() > CLICK_SLOP || delta[1].abs() > CLICK_SLOP {
                        self.press = None;
                        self.session.camera.pan(self.session.viewport, delta);
                        self.session.update_hover();
                    }
                }
            }
            WindowEvent::MouseInput { state, button, .. } => {
                if button != MouseButton::Left || wants_pointer || modal {
                    return;
                }
                match state {
                    ElementState::Pressed => self.press = Some(self.session.pointer),
                    ElementState::Released => {
                        if let Some(press) = self.press.take() {
                            let moved = (self.session.pointer[0] - press[0]).abs() > CLICK_SLOP
                                || (self.session.pointer[1] - press[1]).abs() > CLICK_SLOP;
                            if !moved {
                                if let Some(intersection) = self
                                    .session
                                    .camera
                                    .intersection(self.session.viewport, self.session.pointer)
                                {
                                    if let Some(point) =
                                        gomoku_core::point(intersection[1], intersection[0])
                                    {
                                        self.session.place(point);
                                        self.session.update_hover();
                                    }
                                }
                            }
                        }
                    }
                }
                self.request_redraw();
            }
            WindowEvent::MouseWheel { delta, .. } => {
                if wants_pointer {
                    return;
                }
                let amount = match delta {
                    MouseScrollDelta::LineDelta(_, lines) => lines * 0.15,
                    MouseScrollDelta::PixelDelta(position) => position.y as f32 * 0.0015,
                };
                self.session.camera.zoom_about(
                    self.session.viewport,
                    self.session.pointer,
                    amount.exp(),
                );
                self.session.update_hover();
                self.request_redraw();
            }
            WindowEvent::KeyboardInput { event, .. } => {
                if event.state != ElementState::Pressed || wants_keyboard {
                    return;
                }
                match &event.logical_key {
                    Key::Named(NamedKey::Escape) => {
                        if self.session.dialog.is_some() {
                            self.session.dialog = None;
                            self.request_redraw();
                        } else if self.session.changed() {
                            self.session.dialog = Some(Dialog::Unsaved { then: After::Quit });
                            self.request_redraw();
                        } else {
                            event_loop.exit();
                        }
                    }
                    Key::Named(NamedKey::ArrowLeft) => {
                        self.session.rewind();
                        self.request_redraw();
                    }
                    Key::Named(NamedKey::ArrowRight) => {
                        self.session.forward();
                        self.request_redraw();
                    }
                    Key::Named(NamedKey::ArrowUp) | Key::Named(NamedKey::Home) => {
                        self.session.seek(0);
                        self.request_redraw();
                    }
                    Key::Named(NamedKey::ArrowDown) | Key::Named(NamedKey::End) => {
                        self.session.seek(usize::MAX);
                        self.request_redraw();
                    }
                    Key::Named(NamedKey::Space) | Key::Named(NamedKey::Backspace) => {
                        self.session.undo();
                        self.request_redraw();
                    }
                    Key::Character(text) => {
                        for command in text.chars() {
                            match command {
                                'u' => self.session.undo(),
                                'f' | '0' => self.session.fit(),
                                'v' => self.session.flip(),
                                'c' => {
                                    self.session.toggles.coordinates =
                                        !self.session.toggles.coordinates
                                }
                                'm' => {
                                    self.session.toggles.move_numbers =
                                        !self.session.toggles.move_numbers
                                }
                                'l' => {
                                    self.session.toggles.last_move = !self.session.toggles.last_move
                                }
                                'w' => {
                                    self.session.toggles.win_line = !self.session.toggles.win_line
                                }
                                '+' | '=' => self.session.camera.zoom_about(
                                    self.session.viewport,
                                    [
                                        self.session.viewport.width * 0.5,
                                        self.session.viewport.height * 0.5,
                                    ],
                                    1.15,
                                ),
                                '-' => self.session.camera.zoom_about(
                                    self.session.viewport,
                                    [
                                        self.session.viewport.width * 0.5,
                                        self.session.viewport.height * 0.5,
                                    ],
                                    1.0 / 1.15,
                                ),
                                _ => {}
                            }
                        }
                        self.session.update_hover();
                        self.request_redraw();
                    }
                    _ => {}
                }
            }
            WindowEvent::RedrawRequested => {
                if self.state.is_none() {
                    event_loop.exit();
                    return;
                }
                // The interface runs inside the draw, so its requests are known
                // only afterwards.
                if self.draw(event_loop) {
                    event_loop.exit();
                }
            }
            _ => {}
        }
    }

    fn about_to_wait(&mut self, _event_loop: &ActiveEventLoop) {
        if !self.closing && (self.dirty || self.frame_limit.is_some()) {
            if let Some(state) = &self.state {
                state.window.request_redraw();
            }
        }
    }

    fn exiting(&mut self, _event_loop: &ActiveEventLoop) {
        log::info!("closing after {} frames", self.frames);
    }
}
