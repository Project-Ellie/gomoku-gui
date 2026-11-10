//! The window: a surface, the interface, the file handling, and the frame loop.
//!
//! The board is drawn by our own passes. The interface is drawn by egui on top,
//! in the same surface, after the samples are resolved. Events go to egui first;
//! the board only sees a pointer or a key that egui did not want.

#[cfg(test)]
#[path = "app_tests.rs"]
mod tests;

use std::path::{Path, PathBuf};
use std::sync::Arc;

use anyhow::{Context as _, Result};
use gomoku_core::Settings;
use winit::application::ApplicationHandler;
use winit::event::{ElementState, MouseButton, MouseScrollDelta, WindowEvent};
use winit::event_loop::{ActiveEventLoop, ControlFlow, EventLoop};
use winit::keyboard::{Key, ModifiersState, NamedKey};
use winit::window::{Window, WindowId};

use crate::ai::AiReply;
use crate::ai_worker::Worker;
use crate::audio::Audio;
use crate::camera::Viewport;
use crate::render::{BoardImages, Globals, Renderer, SHELL, SLATE, StoneInstance};
use crate::session::{After, PanelSection, Session, WindowGeometry};
use crate::ui::{self, Requests};

/// How far the pointer may move and still count as a click rather than a drag.
const CLICK_SLOP: f32 = 4.0;
/// The number of samples per pixel.
const SAMPLES: u32 = 4;
/// How long settings must stay unchanged before they are written.
const SETTINGS_SAVE_DELAY: std::time::Duration = std::time::Duration::from_secs(1);

/// Whether saving may reuse the current destination.
enum SaveMode {
    CurrentOrPick,
    Pick,
}

enum AppEvent {
    AiCompleted(AiReply),
}

/// The last observed and last written settings, and the latest change.
struct SettingsSave {
    saved: Settings,
    observed: Settings,
    changed_at: Option<std::time::Instant>,
}

impl SettingsSave {
    fn new(settings: Settings) -> Self {
        Self {
            saved: settings.clone(),
            observed: settings,
            changed_at: None,
        }
    }

    fn observe(&mut self, settings: &Settings, now: std::time::Instant) {
        if settings == &self.observed {
            return;
        }
        self.observed.clone_from(settings);
        self.changed_at = (settings != &self.saved).then_some(now);
    }

    fn is_due(&self, now: std::time::Instant) -> bool {
        self.changed_at
            .is_some_and(|changed| now.saturating_duration_since(changed) >= SETTINGS_SAVE_DELAY)
    }

    fn deadline(&self) -> Option<std::time::Instant> {
        self.changed_at.map(|changed| changed + SETTINGS_SAVE_DELAY)
    }

    fn record_saved(&mut self, settings: Settings) {
        self.observed.clone_from(&settings);
        self.saved = settings;
        self.changed_at = None;
    }

    fn postpone(&mut self, now: std::time::Instant) {
        self.changed_at = (self.observed != self.saved).then_some(now);
    }
}

/// The application.
/// How long after focus is regained a press still counts as the
/// activating click and is swallowed instead of playing a stone.
const FOCUS_CLICK_GUARD: std::time::Duration = std::time::Duration::from_millis(300);

/// Swallows the click that reactivates the window so it cannot also place
/// a stone or push a button. macOS delivers `Focused(true)` just before
/// the activating press; that press and its release are consumed. The
/// guard is time-boxed so a clickless focus change (Cmd-Tab) can only eat
/// a press that lands within the same gesture.
#[derive(Default)]
struct FocusClickGuard {
    gained: Option<std::time::Instant>,
    swallow_release: bool,
}

impl FocusClickGuard {
    fn focused(&mut self, gained: bool, now: std::time::Instant) {
        self.gained = gained.then_some(now);
    }

    /// Whether this button event is part of the activating click and must
    /// not be processed.
    fn swallows(&mut self, state: ElementState, now: std::time::Instant) -> bool {
        match state {
            ElementState::Pressed => match self.gained.take() {
                Some(gained) if now.duration_since(gained) < FOCUS_CLICK_GUARD => {
                    self.swallow_release = true;
                    true
                }
                _ => false,
            },
            ElementState::Released => std::mem::take(&mut self.swallow_release),
        }
    }
}

pub struct App {
    // None for previews and headless construction. Started only by run().
    worker: Option<Result<Worker, String>>,
    session: Session,
    /// The Lab Explorer format registry. The application owns it because it
    /// is what opens corpus folders and records; the only format so far is
    /// psq.
    registry: gomoku_core::Registry,
    audio: Option<Audio>,
    state: Option<State>,
    /// Where the left button went down, if it is down.
    press: Option<[f32; 2]>,
    /// The click that reactivates the window is for the window, not for
    /// the board or a button (macOS click-through).
    focus_click: FocusClickGuard,
    /// The currently held keyboard modifiers.
    modifiers: ModifiersState,
    /// The number of frames to draw before quitting. Used by the smoke test.
    frame_limit: Option<u32>,
    /// An optional command-line override for every interface text style.
    font: Option<PathBuf>,
    frames: u32,
    dirty: bool,
    /// When the interface asked for its next frame, if it did. egui
    /// reports this per frame as `FullOutput::repaint_after`; honouring
    /// it is what makes clicks settle without a mouse wiggle.
    wake_at: Option<std::time::Instant>,
    /// A redraw was requested but no RedrawRequested has arrived yet.
    /// Re-requesting while one is pending starves the display pass on
    /// macOS: the pending event is coalesced away and never dispatched,
    /// which froze modal dialogs until the next mouse move.
    redraw_pending: bool,
    closing: bool,
    geometry: WindowGeometry,
    settings_save: SettingsSave,
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
        session.install_game(game);
        let settings_save = SettingsSave::new(session.settings(&geometry));
        App {
            worker: None,
            session,
            registry: gomoku_core::Registry::builder()
                .add(gomoku_core::PsqFormat)
                .build(),
            audio,
            state: None,
            press: None,
            focus_click: FocusClickGuard::default(),
            modifiers: ModifiersState::empty(),
            frame_limit,
            font: None,
            frames: 0,
            dirty: true,
            wake_at: None,
            redraw_pending: false,
            closing: false,
            geometry,
            settings_save,
        }
    }

    fn window_attributes(&self) -> winit::window::WindowAttributes {
        let attributes = Window::default_attributes()
            .with_title(self.session.title())
            .with_inner_size(winit::dpi::LogicalSize::new(
                self.geometry.width as f64,
                self.geometry.height as f64,
            ));
        if let (Some(x), Some(y)) = (self.geometry.x, self.geometry.y) {
            attributes.with_position(winit::dpi::LogicalPosition::new(x as f64, y as f64))
        } else {
            attributes
        }
    }

    fn record_window_position(
        &mut self,
        position: winit::dpi::PhysicalPosition<i32>,
        scale_factor: f64,
    ) {
        let logical = position.to_logical::<f32>(scale_factor);
        self.geometry.x = Some(logical.x);
        self.geometry.y = Some(logical.y);
    }

    /// Open a puzzle file before the window appears, as `--puzzle` asks.
    pub fn open_puzzle(&mut self, path: &std::path::Path) {
        self.session.open_puzzle(path);
    }

    /// Select a TTF/OTF/TTC font file for every interface text style.
    pub fn set_font(&mut self, path: PathBuf) {
        self.font = Some(path);
    }

    /// Open the window and run until it is closed.
    pub fn run(mut self) -> Result<()> {
        self.discard_startup_autosave(
            crate::session::autosave_path(),
            crate::session::autosave_state_path(),
        );
        let event_loop = EventLoop::<AppEvent>::with_user_event()
            .build()
            .context("cannot open an event loop")?;
        let proxy = event_loop.create_proxy();
        self.worker = Some(
            Worker::baseline(move |reply| proxy.send_event(AppEvent::AiCompleted(reply)).is_ok())
                .map_err(|error| error.to_string()),
        );
        event_loop.set_control_flow(ControlFlow::Wait);
        let result = event_loop.run_app(&mut self);
        self.close_worker();
        self.write_settings(
            crate::session::settings_path(),
            crate::session::autosave_path(),
            crate::session::autosave_state_path(),
        );
        result.context("the window loop failed")
    }

    /// Delete recovery files from the previous sitting without offering them.
    fn discard_startup_autosave(
        &mut self,
        autosave_path: Result<PathBuf>,
        autosave_state_path: Result<PathBuf>,
    ) {
        self.session
            .discard_autosave_from([autosave_path, autosave_state_path].into_iter().flatten());
        log::trace!("discarded any startup autosave");
    }

    /// Save changed settings after a quiet period, without polling the filesystem.
    fn save_changed_settings(
        &mut self,
        now: std::time::Instant,
        settings_path: impl FnOnce() -> Result<PathBuf>,
    ) {
        let settings = self.session.settings(&self.geometry);
        self.settings_save.observe(&settings, now);
        if !self.settings_save.is_due(now) {
            return;
        }
        let result = settings_path().and_then(|path| {
            gomoku_core::save_settings(&path, &settings).map_err(anyhow::Error::new)
        });
        match result {
            Ok(()) => self.settings_save.record_saved(settings),
            Err(error) => {
                log::warn!("the settings could not be saved: {error}");
                self.settings_save.postpone(now);
            }
        }
    }

    /// Write the settings, and the game in progress, on the way out.
    fn write_settings(
        &mut self,
        settings_path: Result<PathBuf>,
        autosave_path: Result<PathBuf>,
        autosave_state_path: Result<PathBuf>,
    ) {
        // Frame-limited exits also need to save the real game, not a preview.
        self.session.dismiss_dialog();
        match settings_path {
            Ok(path) => {
                let settings = self.session.settings(&self.geometry);
                match gomoku_core::save_settings(&path, &settings) {
                    Ok(()) => self.settings_save.record_saved(settings),
                    Err(error) => log::warn!("the settings could not be saved: {error}"),
                }
            }
            Err(error) => log::warn!("the settings have no home: {error}"),
        }
        if self.session.changed() {
            if let (Ok(path), Ok(state_path)) = (autosave_path, autosave_state_path) {
                self.session.autosave_to(&path, &state_path);
            }
        } else if self.session.is_fresh_game() {
            self.session
                .discard_autosave_from([autosave_path, autosave_state_path].into_iter().flatten());
        }
    }

    fn close_worker(&mut self) {
        self.closing = true;
        if let Some(Ok(worker)) = &mut self.worker {
            worker.close();
        }
    }

    // Called after input/UI transactions have settled, even with a future wake_at.
    fn reconcile_ai(&mut self) {
        if self.closing {
            return;
        }
        let Some(worker) = self.worker.as_ref() else {
            return;
        };
        if let Some(request) = self.session.next_ai_request() {
            let result = match worker {
                Ok(worker) => worker.submit(request),
                Err(error) => Err(Box::new(request.reply(Err(error.clone())))),
            };
            if let Err(reply) = result {
                self.session.apply_ai_reply(*reply);
                self.request_redraw();
            }
        }
    }

    fn prepare_wait(&mut self) -> ControlFlow {
        self.reconcile_ai();
        let now = std::time::Instant::now();
        if !self.closing {
            if self.state.is_some() {
                self.save_changed_settings(now, crate::session::settings_path);
            } else {
                // Headless apps still exercise change detection and deadlines,
                // but writes always need an explicit test destination.
                let settings = self.session.settings(&self.geometry);
                self.settings_save.observe(&settings, now);
            }
        }
        // Input dirtying egui must not wait for an older animation or cursor
        // deadline: request_redraw() schedules the event that wakes this Wait.
        if !self.closing && (self.dirty || self.frame_limit.is_some()) {
            self.request_redraw();
            return ControlFlow::Wait;
        }
        let mut wait_until = if self.closing {
            None
        } else {
            self.settings_save.deadline()
        };
        if let Some(wake) = self.wake_at {
            if now >= wake {
                self.wake_at = None;
                self.request_redraw();
            } else {
                wait_until = Some(wait_until.map_or(wake, |save| save.min(wake)));
            }
        }
        wait_until.map_or(ControlFlow::Wait, ControlFlow::WaitUntil)
    }

    fn handle_app_event(&mut self, event: AppEvent) {
        if self.closing {
            return;
        }
        match event {
            AppEvent::AiCompleted(reply) => {
                self.session.apply_ai_reply(reply);
                self.session.update_hover();
                self.request_redraw();
            }
        }
    }

    fn request_redraw(&mut self) {
        self.dirty = true;
        if self.redraw_pending {
            return; // one request in flight is enough; see redraw_pending
        }
        if let Some(state) = &self.state {
            self.redraw_pending = true;
            state.window.request_redraw();
        }
    }

    /// Perform an action immediately; unsaved games never block it.
    fn attempt(&mut self, then: After, exit: &mut impl FnMut()) {
        self.perform(then, exit);
        self.request_redraw();
    }

    /// Perform an action, whatever the state of the game.
    fn perform(&mut self, then: After, exit: &mut impl FnMut()) {
        match then {
            After::NewGame => {
                self.session.new_game();
                self.request_redraw();
            }
            After::Open => self.ask_to_open(),
            After::OpenPuzzle => self.ask_to_open_puzzle(),
            After::Quit => {
                self.close_worker();
                exit();
            }
        }
    }

    /// Ask for a file and open it.
    fn ask_to_open(&mut self) {
        self.session.begin_native_dialog();
        let mut dialog = rfd::FileDialog::new()
            .add_filter("Gomoku game", &["json"])
            .set_title("Open a game");
        if let Some(directory) = &self.session.last_directory {
            dialog = dialog.set_directory(directory);
        }
        let path = dialog.pick_file();
        self.session.end_native_dialog();
        if let Some(path) = path {
            self.session.open(&path);
        }
        self.request_redraw();
    }

    /// Ask for a puzzle file and load it.
    fn ask_to_open_puzzle(&mut self) {
        self.session.begin_native_dialog();
        let mut dialog = rfd::FileDialog::new()
            .add_filter("Gomoku puzzle", &["json"])
            .set_title("Load a puzzle");
        if let Some(directory) = &self.session.last_directory {
            dialog = dialog.set_directory(directory);
        }
        let path = dialog.pick_file();
        self.session.end_native_dialog();
        if let Some(path) = path {
            self.session.open_puzzle(&path);
        }
        self.request_redraw();
    }

    /// Ask for a corpus folder and open it in the Explorer.
    ///
    /// The folder picker is injectable, like the save picker, so that
    /// headless tests never open a native dialog.
    fn ask_to_open_explorer(
        &mut self,
        pick_folder: &mut impl FnMut(Option<&Path>) -> Option<PathBuf>,
    ) {
        self.session.begin_native_dialog();
        let directory = self
            .session
            .last_corpus_directory
            .as_ref()
            .or(self.session.last_directory.as_ref())
            .cloned();
        let path = pick_folder(directory.as_deref());
        self.session.end_native_dialog();
        if let Some(path) = path {
            self.open_corpus_folder(&path);
        }
        self.request_redraw();
    }

    /// Open a corpus folder in the Lab Explorer, reporting any mismatch.
    fn open_corpus_folder(&mut self, path: &Path) {
        if !self.registry.looks_like_corpus(path) {
            self.session.fail(
                "This folder doesn't look like a Gomoku corpus",
                &"no recognized record files were found in this folder",
            );
            self.request_redraw();
            return;
        }
        match self.registry.open(path) {
            Ok(gomoku_core::OpenedThing::Corpus { records, .. }) => {
                // Opening a corpus is a load: the next picker starts here,
                // not wherever navigation happened to begin last time.
                self.session.remember_corpus_directory(path);
                let entries = records
                    .into_iter()
                    .map(|meta| gomoku_core::IndexEntry {
                        index: meta.index,
                        kind: meta.kind,
                        name: meta.name,
                        path: meta.source,
                    })
                    .collect();
                self.session.open_explorer_corpus(entries);
            }
            Ok(gomoku_core::OpenedThing::SingleRecord(_)) => {
                self.session.fail(
                    "The Explorer needs a folder",
                    &"pick a folder of records, not a single file",
                );
            }
            Err(error) => self.session.fail("The folder could not be opened", &error),
        }
    }

    /// Save using the same transaction for existing paths and Save As.
    fn save(&mut self, mode: SaveMode, pick: &mut impl FnMut(Option<&Path>) -> Option<PathBuf>) {
        let directory = self.session.last_directory.clone();
        let pick = || pick(directory.as_deref());
        match mode {
            SaveMode::Pick => self.session.save_as(pick),
            SaveMode::CurrentOrPick => self.session.save(pick),
        };
        self.request_redraw();
    }

    /// Act on what the interface asked for.
    fn run_requests(
        &mut self,
        requests: Requests,
        pick: &mut impl FnMut(Option<&Path>) -> Option<PathBuf>,
        pick_folder: &mut impl FnMut(Option<&Path>) -> Option<PathBuf>,
        exit: &mut impl FnMut(),
    ) {
        if requests.open_explorer {
            self.ask_to_open_explorer(pick_folder);
        }
        if requests.reopen_explorer {
            if let Some(path) = self.session.last_corpus_directory.clone() {
                self.open_corpus_folder(&path);
                self.request_redraw();
            }
        }
        if let Some(index) = requests.explorer_select {
            self.session.load_explorer_record(&self.registry, index);
            self.request_redraw();
        }
        if requests.quit {
            self.attempt(After::Quit, exit);
        } else if requests.new_game {
            self.attempt(After::NewGame, exit);
        } else if requests.open {
            self.attempt(After::Open, exit);
        } else if requests.open_puzzle {
            self.attempt(After::OpenPuzzle, exit);
        } else if requests.save {
            self.save(SaveMode::CurrentOrPick, pick);
        } else if requests.save_as {
            self.save(SaveMode::Pick, pick);
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
        // dirty accumulates during the frame: egui's repaint requests,
        // request_redraw() calls, and the settling frame after input all
        // set it. It is reset here, at the start, never overwritten later.
        self.dirty = false;

        let view = frame
            .texture
            .create_view(&wgpu::TextureViewDescriptor::default());
        let multisampled = state
            .multisampled
            .create_view(&wgpu::TextureViewDescriptor::default());

        // The interface runs first, because it is what works out where the board
        // may be drawn: the panels take their space from the window.
        let raw_input = state.egui_state.take_egui_input(&state.window);
        // A frame that consumed input is never allowed to be the last frame:
        // on macOS a burst-final present can sit unflushed until the next
        // event, which reads as "the click only lands when the mouse moves".
        let had_input = !raw_input.events.is_empty();
        let mut requests = Requests::default();
        let full_output = state.egui.run_ui(raw_input, |ui| {
            requests = ui::draw(ui, &mut self.session);
        });
        // The interface's own repaint requests ride the Wait loop with us:
        // an immediate request stays dirty, a delayed one becomes a wake-up.
        // egui 0.36 reports the delay per viewport; there is only the root.
        let repaint_after = full_output
            .viewport_output
            .values()
            .map(|viewport| viewport.repaint_delay)
            .min()
            .unwrap_or(std::time::Duration::MAX);
        if repaint_after.is_zero() {
            self.dirty = true;
        } else if repaint_after != std::time::Duration::MAX {
            self.wake_at = Some(std::time::Instant::now() + repaint_after);
        }
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

        self.run_requests(
            requests,
            &mut pick_save_file,
            &mut pick_corpus_folder,
            &mut || event_loop.exit(),
        );

        // A frame that consumed input is never allowed to be the last
        // frame; everything else (repaint_after, request_redraw) has
        // already accumulated into dirty during the frame.
        self.dirty |= had_input;
        self.frames += 1;
        match self.frame_limit {
            Some(limit) if self.frames >= limit => {
                log::info!("drew {limit} frames; exiting");
                self.close_worker();
                true
            }
            _ => false,
        }
    }

    /// Handle a global key after egui and modal ownership have been checked.
    fn handle_key(&mut self, logical_key: &Key, exit: &mut impl FnMut()) {
        // Explorer's egui handler owns these keys while its section is open.
        // In particular, arrows must not also seek the game underneath it.
        if self.session.open_panel_section() == PanelSection::Explorer
            && matches!(
                logical_key,
                Key::Named(NamedKey::ArrowUp | NamedKey::ArrowDown | NamedKey::Enter)
            )
        {
            return;
        }
        match logical_key {
            Key::Named(NamedKey::Escape) => {
                self.attempt(After::Quit, exit);
            }
            Key::Named(NamedKey::ArrowLeft) if self.modifiers.contains(ModifiersState::SUPER) => {
                self.session.seek(0);
                self.request_redraw();
            }
            Key::Named(NamedKey::ArrowRight) if self.modifiers.contains(ModifiersState::SUPER) => {
                self.session.seek(usize::MAX);
                self.request_redraw();
            }
            Key::Named(NamedKey::ArrowLeft) => {
                self.session.rewind();
                self.request_redraw();
            }
            Key::Named(NamedKey::ArrowRight) => {
                self.session.forward();
                self.request_redraw();
            }
            Key::Named(NamedKey::ArrowUp) => {
                self.session.cycle_branch(-1);
                self.request_redraw();
            }
            Key::Named(NamedKey::ArrowDown) => {
                self.session.cycle_branch(1);
                self.request_redraw();
            }
            Key::Named(NamedKey::Home) => {
                self.session.seek(0);
                self.request_redraw();
            }
            Key::Named(NamedKey::End) => {
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
                        '0'..='9' if self.session.is_branch_point() => {
                            let index = command.to_digit(10).expect("an ASCII digit") as usize;
                            self.session.select_branch(index);
                        }
                        'f' | '0' => self.session.fit(),
                        'v' => self.session.flip(),
                        'c' => self.session.toggles.coordinates = !self.session.toggles.coordinates,
                        'm' => {
                            self.session.toggles.move_numbers = !self.session.toggles.move_numbers
                        }
                        'l' => self.session.toggles.last_move = !self.session.toggles.last_move,
                        'w' => self.session.toggles.win_line = !self.session.toggles.win_line,
                        '+' | '=' => {
                            self.session.camera.zoom_about(
                                self.session.viewport,
                                [
                                    self.session.viewport.width * 0.5,
                                    self.session.viewport.height * 0.5,
                                ],
                                1.15,
                            );
                        }
                        '-' => {
                            self.session.camera.zoom_about(
                                self.session.viewport,
                                [
                                    self.session.viewport.width * 0.5,
                                    self.session.viewport.height * 0.5,
                                ],
                                1.0 / 1.15,
                            );
                        }
                        _ => {}
                    }
                }
                self.session.update_hover();
                self.request_redraw();
            }
            _ => {}
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

        let images = BoardImages::load()?;
        let renderer = Renderer::new(
            &device,
            &queue,
            format,
            SAMPLES,
            &images,
            &crate::render::STONE_SHAPE,
        );
        let multisampled = create_multisampled(&device, &config);

        let egui = egui::Context::default();
        crate::theme::install(&egui);
        if let Some(path) = &self.font {
            crate::theme::install_font(&egui, path)?;
        }
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

/// The native save-picker boundary; never reached by headless app tests.
fn pick_save_file(directory: Option<&Path>) -> Option<PathBuf> {
    let mut dialog = rfd::FileDialog::new()
        .add_filter("Gomoku game", &["json"])
        .set_title("Save the game")
        .set_file_name("gomoku.json");
    if let Some(directory) = directory {
        dialog = dialog.set_directory(directory);
    }
    dialog.save_file()
}

/// The native corpus-folder-picker boundary; never reached by headless app
/// tests.
fn pick_corpus_folder(directory: Option<&Path>) -> Option<PathBuf> {
    let mut dialog = rfd::FileDialog::new().set_title("Open a corpus folder");
    if let Some(directory) = directory {
        dialog = dialog.set_directory(directory);
    }
    dialog.pick_folder()
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
    renderer.set_stones(queue, &visible_stone_instances(session));
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
            .game()
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

/// One stone with the material and per-stone grain used by the lens pipeline.
fn stone_instance(
    point: gomoku_core::Move,
    colour: gomoku_core::Color,
    index: usize,
    scale: f32,
) -> StoneInstance {
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
        params: [seed, material.kind, material.cap, material.limb],
        scale,
    }
}

/// The played stones to draw, in play order.
pub fn stone_instances(game: &gomoku_core::Game) -> Vec<StoneInstance> {
    game.stones()
        .enumerate()
        .map(|(index, (point, colour))| stone_instance(point, colour, index, 1.0))
        .collect()
}

/// The played stones plus half-size stones for the visible certified proof.
///
/// The proof alternates sides from its winner, just as the numbered overlay
/// does. Keeping this construction pure makes the GPU input testable without
/// a graphics adapter.
pub fn visible_stone_instances(session: &Session) -> Vec<StoneInstance> {
    let mut stones = stone_instances(session.game());
    if let Some(proof) = session.threat_proof() {
        let first_proof_index = stones.len();
        stones.extend(proof.line.iter().enumerate().map(|(index, point)| {
            let side = if index % 2 == 0 {
                proof.winner
            } else {
                proof.winner.other()
            };
            stone_instance(*point, side, first_proof_index + index, 0.5)
        }));
    }
    stones
}

impl ApplicationHandler<AppEvent> for App {
    fn user_event(&mut self, _event_loop: &ActiveEventLoop, event: AppEvent) {
        self.handle_app_event(event);
    }

    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        if self.state.is_some() {
            return;
        }
        let window = match event_loop.create_window(self.window_attributes()) {
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
        // The interface sees every event first — except the activating
        // click, which belongs to the window alone.
        if let WindowEvent::MouseInput {
            state: button_state,
            ..
        } = &event
        {
            if self
                .focus_click
                .swallows(*button_state, std::time::Instant::now())
            {
                return;
            }
        }
        if let WindowEvent::Focused(gained) = &event {
            self.focus_click.focused(*gained, std::time::Instant::now());
        }
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
        let modal = self.session.dialog().is_some();

        match event {
            WindowEvent::CloseRequested => {
                // Cancel any uncommitted preview before replacing its dialog
                // with a save prompt or writing an autosave at exit.
                self.attempt(After::Quit, &mut || event_loop.exit());
            }
            WindowEvent::Resized(size) => {
                if let Some(state) = &mut self.state {
                    if size.width > 0 && size.height > 0 {
                        state.config.width = size.width;
                        state.config.height = size.height;
                        state.surface.configure(&state.device, &state.config);
                        state.multisampled = create_multisampled(&state.device, &state.config);
                        // The window's size is remembered, so the next sitting
                        // opens the same window. The geometry is kept in
                        // logical pixels, the size arrives in physical ones.
                        let logical = size.to_logical::<f32>(state.window.scale_factor());
                        self.geometry.width = logical.width;
                        self.geometry.height = logical.height;
                        // The area for the board is not known here: the interface
                        // reports it in the next frame. `prepare_frame` keeps the
                        // view inside whatever area that is.
                    }
                }
                self.request_redraw();
            }
            WindowEvent::Focused(_) => {
                self.request_redraw();
            }
            WindowEvent::Moved(position) => {
                let scale_factor = self
                    .state
                    .as_ref()
                    .map(|state| state.window.scale_factor())
                    .unwrap_or(1.0);
                self.record_window_position(position, scale_factor);
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
                        // A hand on the view breaks the tie with the window.
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
                                        self.session.board_click(point);
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
            WindowEvent::ModifiersChanged(modifiers) => {
                self.modifiers = modifiers.state();
            }
            WindowEvent::KeyboardInput { event, .. } => {
                // A dialog owns the keyboard while it is open: the interface
                // handles its keys, so that a shortcut cannot slip past it.
                if event.state != ElementState::Pressed || wants_keyboard || modal {
                    return;
                }
                self.handle_key(&event.logical_key, &mut || event_loop.exit());
            }
            WindowEvent::RedrawRequested => {
                self.redraw_pending = false;
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

    fn about_to_wait(&mut self, event_loop: &ActiveEventLoop) {
        event_loop.set_control_flow(self.prepare_wait());
    }

    fn exiting(&mut self, _event_loop: &ActiveEventLoop) {
        self.close_worker();
        log::info!("closing after {} frames", self.frames);
    }
}
