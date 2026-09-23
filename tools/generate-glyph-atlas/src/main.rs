//! Offline generator for the board's engraved coordinate-label atlas.
//!
//! The atlas is a single-channel signed-distance field: 0.5 marks the glyph
//! edge, values above 0.5 are inside the glyph, values below are outside.  The
//! board shader samples it with derivative-based anti-aliasing and a directional
//! highlight so the labels look engraved into the wood like the grid lines.
//!
//! Usage:
//!     cargo run --bin generate-glyph-atlas -- \
//!         /System/Library/Fonts/Menlo.ttc:1 crates/gui/assets/glyphs.png
//!
//! A font path may end with `:N` to select face `N` from a TrueType collection
//! (`.ttc`/`.dfont`).
//!
//! The PNG is committed; the generator is only needed when the glyph set,
//! resolution, spread, or weight changes.

use signed_distance_field::prelude::*;
use std::path::PathBuf;

/// Glyphs in atlas order.  Letters occupy row 0, individual digits row 1.
/// Numbers 10-15 are composed in the shader from two digit glyphs, so every
/// letter and every digit is rendered at the same font size.
const LETTERS: &str = "ABCDEFGHIJKLMNO";
const DIGITS: &str = "0123456789";

/// Resolution of one glyph cell, in pixels.  A larger cell gives crisper labels
/// when the board is viewed close up; the shader anti-aliases the SDF for free.
const CELL: usize = 128;

/// How far the SDF is clamped, in pixels.  The shader sees 0.5 as the edge and
/// can render a smooth lip a few pixels outside it.
const SPREAD: f32 = 8.0;

/// Font size as a fraction of the cell.  All glyphs use the same scale so that
/// letters and digits share one size in the shader.
const GLYPH_SCALE: f32 = 0.78;

/// Positive bias applied to the normalized SDF before it is encoded.  This
/// moves the encoded edge outward, so a glyph rendered at the shader's 0.5
/// threshold looks heavier than the source face.  A value of 0.1 shifts the
/// edge by roughly 1.6 px inside a 128 px cell (the SDF spans `2 * SPREAD`
/// pixels, mapped to [0, 1]).
const SDF_DILATION: f32 = 0.10;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<String> = std::env::args().collect();
    if args.len() != 3 {
        eprintln!(
            "Usage: {} <font.ttf[:index]> <output.png>",
            args.first()
                .map(String::as_str)
                .unwrap_or("generate-glyph-atlas")
        );
        std::process::exit(1);
    }

    let (font_path, collection_index) = parse_font_arg(&args[1]);
    let output_path = PathBuf::from(&args[2]);

    let font_bytes = std::fs::read(&font_path)?;
    let font = fontdue::Font::from_bytes(
        font_bytes,
        fontdue::FontSettings {
            collection_index,
            ..Default::default()
        },
    )?;

    let atlas_width = 16 * CELL;
    let atlas_height = 2 * CELL;
    let mut atlas = vec![0u8; atlas_width * atlas_height];

    // Row 0: letters A-O.
    for (col, ch) in LETTERS.chars().enumerate() {
        let cell_sdf = render_glyph_sdf(&font, &ch.to_string());
        blit(&mut atlas, atlas_width, atlas_height, col, 0, &cell_sdf);
    }

    // Row 1: digits 0-9.
    for (col, ch) in DIGITS.chars().enumerate() {
        let cell_sdf = render_glyph_sdf(&font, &ch.to_string());
        blit(&mut atlas, atlas_width, atlas_height, col, 1, &cell_sdf);
    }

    write_png(&output_path, &atlas, atlas_width, atlas_height)?;
    println!(
        "Wrote {}x{} glyph atlas to {}",
        atlas_width,
        atlas_height,
        output_path.display()
    );
    Ok(())
}

/// Render a string into a square `CELL x CELL` signed-distance field.
fn render_glyph_sdf(font: &fontdue::Font, text: &str) -> Vec<u8> {
    let px = CELL as f32 * GLYPH_SCALE;

    // Rasterize each character and collect metrics.
    let mut images: Vec<(fontdue::Metrics, Vec<u8>)> = Vec::new();
    let mut total_advance = 0.0f32;
    for ch in text.chars() {
        let index = font.lookup_glyph_index(ch);
        let (metrics, bitmap) = font.rasterize_indexed(index, px);
        total_advance += metrics.advance_width;
        images.push((metrics, bitmap));
    }

    // Composite into a binary mask.
    let mut mask = vec![0u8; CELL * CELL];
    let mut pen_x = ((CELL as f32 - total_advance) * 0.5).floor() as i32;
    for (metrics, bitmap) in images {
        let off_x = pen_x + metrics.xmin;
        let off_y = ((CELL as i32 - metrics.height as i32) / 2) + metrics.ymin;
        for y in 0..metrics.height {
            for x in 0..metrics.width {
                let src = bitmap[y * metrics.width + x];
                let dst_x = off_x + x as i32;
                let dst_y = off_y + y as i32;
                if dst_x >= 0 && dst_x < CELL as i32 && dst_y >= 0 && dst_y < CELL as i32 {
                    let idx = dst_y as usize * CELL + dst_x as usize;
                    // Max-composite in case glyphs overlap (they should not for
                    // this glyph set, but it is safer).
                    mask[idx] = mask[idx].max(src);
                }
            }
        }
        pen_x += metrics.advance_width.round() as i32;
    }

    // Convert to signed distance field.
    let binary = binary_image::of_byte_slice(&mask, CELL as u16, CELL as u16);
    let sdf = compute_f32_distance_field(&binary);
    let normalized = sdf
        .normalize_clamped_distances(-SPREAD, SPREAD)
        .expect("non-empty glyph");

    // Map [0, 1] with edge at 0.5 to u8.  The positive dilation shifts the
    // encoded edge outward, so the glyph renders heavier at the shader's 0.5
    // threshold without changing the source raster or the glyph cell size.
    normalized
        .distances
        .iter()
        .map(|d| ((d + SDF_DILATION).clamp(0.0, 1.0) * 255.0).round() as u8)
        .collect()
}

/// Split a font argument into path and optional collection index, e.g.
/// `/System/Library/Fonts/Menlo.ttc:1`.
fn parse_font_arg(arg: &str) -> (PathBuf, u32) {
    let (path, index) = match arg.rsplit_once(':') {
        Some((path, index)) => match index.parse::<u32>() {
            Ok(index) => (path, index),
            Err(_) => (arg, 0),
        },
        None => (arg, 0),
    };
    (PathBuf::from(path), index)
}

/// Copy one cell into the atlas.
fn blit(
    atlas: &mut [u8],
    atlas_width: usize,
    atlas_height: usize,
    cell_col: usize,
    cell_row: usize,
    cell: &[u8],
) {
    assert_eq!(cell.len(), CELL * CELL);
    assert!((cell_col + 1) * CELL <= atlas_width);
    assert!((cell_row + 1) * CELL <= atlas_height);

    for y in 0..CELL {
        let src_row = &cell[y * CELL..(y + 1) * CELL];
        let dst_y = cell_row * CELL + y;
        let dst_start = dst_y * atlas_width + cell_col * CELL;
        atlas[dst_start..dst_start + CELL].copy_from_slice(src_row);
    }
}

/// Write a grayscale PNG.
fn write_png(
    path: &std::path::Path,
    data: &[u8],
    width: usize,
    height: usize,
) -> Result<(), Box<dyn std::error::Error>> {
    let file = std::fs::File::create(path)?;
    let w = &mut std::io::BufWriter::new(file);
    let mut encoder = png::Encoder::new(w, width as u32, height as u32);
    encoder.set_color(png::ColorType::Grayscale);
    encoder.set_depth(png::BitDepth::Eight);
    let mut writer = encoder.write_header()?;
    writer.write_image_data(data)?;
    Ok(())
}
