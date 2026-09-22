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

use std::path::{Path, PathBuf};

use anyhow::{Context as _, Result};
use gomoku_core::Game;

use crate::camera::Camera;
use crate::render::Renderer;

/// How many samples each pixel takes.
const SAMPLES: u32 = 4;

fn main() -> Result<()> {
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info")).init();

    let mut preview: Option<PathBuf> = None;
    let mut size: u32 = 1100;
    let mut frames: Option<u32> = None;
    let mut demo = false;
    let mut pixels_per_cell: Option<f32> = None;
    let mut centre: Option<[f32; 2]> = None;

    let mut arguments = std::env::args().skip(1);
    while let Some(argument) = arguments.next() {
        match argument.as_str() {
            "--preview" => {
                preview = Some(PathBuf::from(
                    arguments.next().context("--preview needs a file name")?,
                ));
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

    if let Some(path) = preview {
        return write_preview(&path, size, &game, pixels_per_cell, centre);
    }

    let audio = audio::Audio::start();
    if audio.is_none() {
        log::warn!("no audio output device was found, so the knock is off");
    }
    app::App::new(game, audio, frames).run()
}

fn print_help() {
    println!(
        "Gomoku

  (no arguments)      open the board
  --demo              start from a short opening, so the stones are visible
  --frames N          quit after N frames (a smoke test)
  --preview FILE.bmp  render one frame to a file and exit
  --size N            the window or preview size in pixels (default 1100)
  --pixels-per-cell N zoom for a preview (default: fit the board)
  --centre X,Y        the board point at the preview centre (default 7,7)

Place a stone with the left mouse button. Drag to pan, scroll to zoom.
u or backspace takes a move back, f fits the board, v turns it around,
escape quits."
    );
}

/// A short opening, so that a preview shows stones and shadows.
fn demo_game() -> Game {
    let mut game = Game::new();
    for (column, row) in [
        (7_u8, 7_u8),
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
    ] {
        if let Some(point) = gomoku_core::point(row, column) {
            let _ = game.play(point);
        }
    }
    game
}

fn write_preview(
    path: &Path,
    size: u32,
    game: &Game,
    pixels_per_cell: Option<f32>,
    centre: Option<[f32; 2]>,
) -> Result<()> {
    let gpu = preview::HeadlessGpu::new()?;
    let mut renderer = Renderer::new(&gpu.device, &gpu.queue, preview::PREVIEW_FORMAT, SAMPLES);
    let board_size = [size, size];
    let mut camera = Camera::fit(board_size);
    if let Some(pixels) = pixels_per_cell {
        camera.pixels_per_cell = pixels;
    }
    if let Some(centre) = centre {
        camera.centre = centre;
    }
    renderer.set_globals(&gpu.queue, &app::globals_for(&camera, board_size));
    renderer.set_stones(&gpu.queue, &app::stone_instances(game));
    gpu.write_frame(path, size, size, &renderer)?;
    println!("wrote {}", path.display());
    Ok(())
}
