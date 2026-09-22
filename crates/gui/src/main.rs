//! The Gomoku application.
//!
//! Run it with no arguments to open the board. `--preview FILE.bmp` renders one
//! frame without a window, which is how the look is checked and how the
//! pipeline is exercised on a machine with no display.

mod app;
mod audio;
mod camera;
mod preview;
mod render;
mod session;
mod ui;

use std::path::PathBuf;

use anyhow::{Context as _, Result};
use gomoku_core::Game;

use crate::camera::Camera;
use crate::render::Renderer;

/// How many samples each pixel takes.
const SAMPLES: u32 = 4;

/// What to draw when the application renders a frame to a file instead of to a
/// window. Used to check the board and the interface without opening one.
struct Preview {
    /// Where to write the image.
    path: PathBuf,
    /// The width and height in interface points.
    size: u32,
    /// Interface points per physical pixel, as a scaled display has.
    scale: f32,
    /// A fixed scale for the view, in pixels per cell.
    pixels_per_cell: Option<f32>,
    /// A fixed centre for the view, in cells.
    centre: Option<[f32; 2]>,
    /// A fixed lightness for the wood.
    gain: Option<f32>,
    /// Draw the interface as well as the board.
    with_ui: bool,
}

fn main() -> Result<()> {
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info")).init();

    let mut preview: Option<Preview> = None;
    let mut size: u32 = 1100;
    let mut frames: Option<u32> = None;
    let mut demo = false;
    let mut ui_preview = false;
    let mut scale = 1.0_f32;
    let mut pixels_per_cell: Option<f32> = None;
    let mut gain: Option<f32> = None;
    let mut centre: Option<[f32; 2]> = None;

    let mut arguments = std::env::args().skip(1);
    while let Some(argument) = arguments.next() {
        match argument.as_str() {
            "--preview" => {
                preview = Some(Preview {
                    path: PathBuf::from(arguments.next().context("--preview needs a file name")?),
                    size: 0,
                    scale: 1.0,
                    pixels_per_cell: None,
                    centre: None,
                    gain: None,
                    with_ui: false,
                });
            }
            "--size" => {
                size = arguments
                    .next()
                    .and_then(|value| value.parse().ok())
                    .context("--size needs a number of pixels")?;
            }
            "--frames" => {
                frames = Some(
                    arguments
                        .next()
                        .and_then(|value| value.parse().ok())
                        .context("--frames needs a number")?,
                );
            }
            "--demo" => demo = true,
            "--preview-ui" => ui_preview = true,
            "--scale" => {
                scale = arguments
                    .next()
                    .and_then(|value| value.parse().ok())
                    .context("--scale needs a number")?;
            }
            "--wood-gain" => {
                gain = Some(
                    arguments
                        .next()
                        .and_then(|value| value.parse().ok())
                        .context("--wood-gain needs a number")?,
                );
            }
            "--pixels-per-cell" => {
                pixels_per_cell = Some(
                    arguments
                        .next()
                        .and_then(|value| value.parse().ok())
                        .context("--pixels-per-cell needs a number")?,
                );
            }
            "--centre" => {
                let value = arguments.next().context("--centre needs two numbers")?;
                let (x, y) = value
                    .split_once(',')
                    .context("--centre wants the form X,Y")?;
                centre = Some([
                    x.parse().context("--centre x is not a number")?,
                    y.parse().context("--centre y is not a number")?,
                ]);
            }
            "--help" | "-h" => {
                print_help();
                return Ok(());
            }
            other => anyhow::bail!("unknown argument {other}. Try --help."),
        }
    }

    let game = if demo { demo_game() } else { Game::new() };

    if let Some(mut preview) = preview {
        preview.size = size;
        preview.scale = scale;
        preview.pixels_per_cell = pixels_per_cell;
        preview.centre = centre;
        preview.gain = gain;
        preview.with_ui = ui_preview;
        return write_preview(&preview, &game);
    }

    // The settings, and the game in progress.
    let settings = match session::settings_path() {
        Ok(path) => {
            let loaded = gomoku_core::load_settings(&path);
            if let Some(notice) = &loaded.notice {
                log::info!("settings: {notice:?}");
            }
            loaded.settings
        }
        Err(error) => {
            log::warn!("the settings have no home, so the defaults apply: {error}");
            gomoku_core::Settings::default()
        }
    };
    let audio = audio::Audio::start();
    if audio.is_none() {
        log::warn!("no audio output device was found, so the knock is off");
    }
    app::App::new(settings, game, audio, frames).run()
}

fn print_help() {
    println!(
        "Gomoku

  (no arguments)      open the board
  --demo              start from a short opening, so the stones are visible
  --frames N          quit after N frames (a smoke test)
  --preview FILE.bmp  render one frame to a file and exit
  --preview-ui        include the interface in the preview
  --scale N           interface points per pixel, as a scaled display has
  --size N            the window or preview size in pixels (default 1100)
  --pixels-per-cell N zoom for a preview (default: fit the board)
  --wood-gain F       a brightness multiplier on the wood photograph (default 1)
  --centre X,Y        the board point at the preview centre (default 7,7)

Place a stone with the left mouse button. Drag to pan, scroll to zoom.
u or backspace takes a move back, f fits the board, v turns it around,
escape quits."
    );
}

/// The stones of a short opening, so that a preview shows stones and shadows.
///
/// Every pair is a free square on the board, which is why `play` may be relied
/// on below.
const DEMO: [(u8, u8); 11] = [
    (7, 7),
    (7, 8),
    (8, 8),
    (6, 6),
    (8, 6),
    (8, 7),
    (6, 8),
    (9, 7),
    (5, 9),
    (9, 9),
    (6, 7),
];

/// A short opening, so that a preview shows stones and shadows.
fn demo_game() -> Game {
    let mut game = Game::new();
    for (column, row) in DEMO {
        let point = gomoku_core::point(row, column).expect("the demo moves are on the board");
        game.play(point)
            .expect("the demo moves are on free squares");
    }
    game
}

fn write_preview(preview: &Preview, game: &Game) -> Result<()> {
    let &Preview {
        ref path,
        size,
        scale,
        pixels_per_cell,
        centre,
        gain,
        with_ui,
    } = preview;
    let gpu = preview::HeadlessGpu::new()?;
    let wood = render::WoodTexture::load()?;
    let mut renderer = Renderer::new(
        &gpu.device,
        &gpu.queue,
        preview::PREVIEW_FORMAT,
        SAMPLES,
        &wood,
    );
    let board_size = [size, size];
    let mut camera = Camera::fit(camera::Viewport::window(board_size[0], board_size[1]));
    if let Some(pixels) = pixels_per_cell {
        camera.pixels_per_cell = pixels;
    }
    if let Some(centre) = centre {
        camera.centre = centre;
    }
    let look = render::WoodLook {
        gain: gain.unwrap_or(render::WOOD.gain),
        ..render::WOOD
    };
    let viewport = camera::Viewport::window(board_size[0], board_size[1]);
    let mut globals = Renderer::globals_with_wood(viewport, look);
    globals.view = [
        camera.centre[0],
        camera.centre[1],
        camera.pixels_per_cell,
        if camera.flipped { 1.0 } else { 0.0 },
    ];
    renderer.set_globals(&gpu.queue, &globals);
    renderer.set_stones(&gpu.queue, &app::stone_instances(game));
    if with_ui {
        let settings = gomoku_core::Settings::default();
        let mut session = session::Session::new(&settings);
        session.game = game.clone();
        session.camera = camera;
        session.gain = look.gain;
        session.viewport = viewport;
        session.pixels_per_point = 1.0;
        gpu.write_ui_frame(path, size, scale, &mut renderer, &mut session)?;
    } else {
        gpu.write_frame(path, size, size, &renderer)?;
    }
    println!("wrote {}", path.display());
    Ok(())
}
