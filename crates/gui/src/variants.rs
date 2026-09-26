//! The variation sheets: `--variants DIR`.
//!
//! Ten takes on the slate stone, ten on the shell stone, and ten on the knock,
//! written into one directory with a page that shows and plays them side by
//! side, so the look and the sound are chosen with eyes and ears rather than
//! with numbers.
//!
//! The stone takes move along the dials the shader exposes: how tall the lens
//! is, how much the light and the room shine back, how far the texture
//! modulates the body, and how dark the body is. The knock takes move along
//! the voicing dials: how long the modes ring, how long the file runs, how
//! long the landing takes, and how loud the contact is. Every sheet also shows
//! the look and the sound the game ships with, as the reference the takes are
//! measured against.

use std::path::Path;

use anyhow::{Context as _, Result};

use crate::audio::{self, Mode, Modes, Voicing};
use crate::camera;
use crate::preview::HeadlessGpu;
use crate::render::{
    self, GlyphTexture, Renderer, SHELL, SLATE, StoneInstance, StoneShape, WoodTexture,
};
use crate::sound;

/// The rate the sounds are written at.
const RATE: u32 = 48_000;

/// The side of one sheet image, in pixels.
const IMAGE: u32 = 512;

/// The two zooms a take is rendered at, in pixels per cell: the zoom the game
/// is played at, and a close look at the material.
const ZOOMS: [(&str, f32); 2] = [("play", 64.0), ("close", 300.0)];

/// How many samples each pixel takes.
const SAMPLES: u32 = 4;

/// One take on a stone: what to move against the look the game ships with.
struct StoneVariant {
    /// The label on the page: "B3" or "W7".
    name: String,
    /// What this take changes, in words.
    intent: String,
    /// The lens shape.
    shape: StoneShape,
    /// The base roughness, replacing the material's own.
    roughness: f32,
    /// How dull the stone may go; 0.0 keeps the cap the game ships with.
    cap: f32,
    /// Shine scale, texture contrast scale, albedo scale, room reflection
    /// scale. Ones are the look the game ships with.
    knobs: [f32; 4],
}

/// One take on the drilled crossings: how dark the cup still is at the rim,
/// where the sharp cut comes. At one the drilling is as dark and as sharp as
/// the grid lines.
struct DimpleVariant {
    /// The label on the page: "D2".
    name: String,
    /// What this take changes, in words.
    intent: String,
    /// The darkness at the rim, from 0 to 1.
    floor: f32,
}

/// The takes on the drilled crossings, sharper and sharper.
fn dimple_variants() -> Vec<DimpleVariant> {
    [
        ("D1", "the first sharpening: a third dark at the rim", 0.34),
        ("D2", "two thirds dark at the rim", 0.65),
        ("D3", "nearly full dark at the rim", 0.85),
        ("D4", "as dark and as sharp as the grid", 1.0),
    ]
    .into_iter()
    .map(|(name, intent, floor)| DimpleVariant {
        name: name.to_string(),
        intent: intent.to_string(),
        floor,
    })
    .collect()
}

/// One take on the knock.
struct KnockVariant {
    /// The label on the page: "K4".
    name: String,
    /// What this take changes, in words.
    intent: String,
    /// The sound to render.
    parts: audio::Parts,
}

/// The lentil proportion every take shares: the widest point lower and the
/// dome flatter than the shipped stone, which is the shape of a real stone.
const fn lentil(height: f32) -> StoneShape {
    StoneShape {
        height,
        widest: 0.18,
        shoulder: 0.55,
        dome: 0.82,
    }
}

/// The shape the game played with before the choice, kept so the sheet can
/// compare the chosen stones against it.
const OLD_SHAPE: StoneShape = StoneShape {
    height: 0.40,
    widest: 0.22,
    shoulder: 0.62,
    dome: 0.86,
};

/// The quiet half of the sound before the K10 retuning, kept so the sheet can
/// compare the chosen sound against it.
const OLD_SET_DOWN: Voicing = Voicing {
    contact: 0.4292,
    contact_centre: 1258.125,
    contact_decay: 0.002808,
    contact_rise: 0.00105,
    attack: 0.002011,
    body: Modes::new(&[(136.64, 0.039725, 0.70), (302.56, 0.0227, 0.35)]),
    stone: Modes::new(&[(2000.0, 0.014, 0.15)]),
    tone: 3200.0,
    roughness: 0.34,
    glide: 0.018,
    spread: 0.03,
    whisper: 0.09,
    length: 0.15609,
};

/// The ringing half of the sound before the K10 retuning.
const OLD_STONE_RING: Voicing = Voicing {
    contact: 0.9,
    contact_centre: 2000.0,
    contact_decay: 0.0012,
    contact_rise: 0.0,
    attack: 0.0,
    body: Modes::new(&[(200.0, 0.020, 0.45)]),
    stone: Modes::new(&[(2700.0, 0.030, 0.40), (4100.0, 0.018, 0.20)]),
    tone: 6500.0,
    roughness: 0.2,
    glide: 0.0,
    spread: 0.0,
    whisper: 0.0,
    length: 0.110,
};

/// The sound the game played with before the choice.
fn old_sound() -> audio::Parts {
    vec![
        (audio::SET_DOWN_SHARE, OLD_SET_DOWN),
        (audio::STONE_RING_SHARE, OLD_STONE_RING),
    ]
}

/// Ten takes on the slate stone: flatter, and drier and drier. The bright spot
/// and the room's reflection are what reads as wet, so those fall; and the
/// speckle that breaks the highlight into glitter is smoothed away, which is
/// the contrast dial driven below one.
fn slate_variants() -> Vec<StoneVariant> {
    (0..10)
        .map(|index| {
            let t = index as f32 / 9.0;
            let height = 0.34 - 0.04 * t;
            let shine = 0.72 - 0.47 * t;
            let env = 0.55 - 0.43 * t;
            let albedo = 1.00 + 0.25 * t;
            let roughness = 0.58 + 0.14 * t;
            let glitter = 1.00 - 0.40 * t;
            let cap = 0.45 + 0.45 * t;
            let chosen = if index == 9 {
                " — chosen, now the game's stone"
            } else {
                ""
            };
            StoneVariant {
                name: format!("B{}", index + 1),
                intent: format!(
                    "{height:.2} high, shine at {:.0}%, the room at {:.0}%, \
                     dullness up to {cap:.2}, the body {:.0}% as dark{chosen}",
                    shine * 100.0,
                    env * 100.0,
                    albedo * 100.0
                ),
                shape: lentil(height),
                roughness,
                cap,
                knobs: [shine, glitter, albedo, env],
            }
        })
        .collect()
}

/// Ten takes on the shell stone: flatter, with the streaks and speckle
/// standing out more and more, and the body a little darker to give the
/// texture something to stand against.
fn shell_variants() -> Vec<StoneVariant> {
    (0..10)
        .map(|index| {
            let t = index as f32 / 9.0;
            let height = 0.34 - 0.04 * t;
            let contrast = 1.5 + 2.5 * t;
            let albedo = 0.99 - 0.13 * t;
            let chosen = if index == 8 {
                " — chosen, now the game's stone"
            } else {
                ""
            };
            StoneVariant {
                name: format!("W{}", index + 1),
                intent: format!(
                    "{height:.2} high, texture at {contrast:.1}x, the body {:.0}% as bright{chosen}",
                    albedo * 100.0
                ),
                shape: lentil(height),
                roughness: SHELL.roughness,
                cap: 0.45 + 0.25 * t,
                knobs: [1.0, contrast, albedo, 1.0],
            }
        })
        .collect()
}

/// Revoice a knock: the modes ring for `decay` of their time, the file runs
/// for `length` of its length, the landing takes `attack` seconds, and the
/// contact is at `contact` of its level.
fn revoice(base: &Voicing, decay: f32, length: f32, attack: f32, contact: f32) -> Voicing {
    let shorten = |modes: &Modes| {
        let scaled: Vec<Mode> = modes
            .used()
            .iter()
            .map(|&(frequency, decay_time, level)| (frequency, decay_time * decay, level))
            .collect();
        Modes::new(&scaled)
    };
    Voicing {
        attack,
        contact: base.contact * contact,
        body: shorten(&base.body),
        stone: shorten(&base.stone),
        length: base.length * length,
        ..*base
    }
}

/// Ten takes on the knock, each shorter in the tail and flatter in its arrival
/// than the one before.
fn knock_variants() -> Vec<KnockVariant> {
    (0..10)
        .map(|index| {
            let t = (index + 1) as f32 / 10.0;
            let decay = 1.0 - 0.55 * t;
            let length = 1.0 - 0.45 * t;
            let landing = 0.002 + 0.004 * t;
            let contact = 1.0 - 0.35 * t;
            let set_down = revoice(&OLD_SET_DOWN, decay, length, landing, contact);
            // The ringing part keeps no arrival of its own, but a short one
            // flattens the whole sound, which is what is asked for.
            let ring = revoice(&OLD_STONE_RING, decay, length, landing * 0.4, contact);
            let chosen = if index == 9 {
                " — chosen, now the game's sound"
            } else {
                ""
            };
            KnockVariant {
                name: format!("K{}", index + 1),
                intent: format!(
                    "the tail {:.0}% as long, the landing over {:.1} ms, the contact at {:.0}%{chosen}",
                    decay * 100.0,
                    landing * 1000.0,
                    contact * 100.0
                ),
                parts: vec![
                    (audio::SET_DOWN_SHARE, set_down),
                    (audio::STONE_RING_SHARE, ring),
                ],
            }
        })
        .collect()
}

/// One stone on an empty board, at the centre crossing.
fn lone_stone(kind: f32, roughness: f32, cap: f32) -> Vec<StoneInstance> {
    let material = if kind > 0.5 { SHELL } else { SLATE };
    vec![StoneInstance {
        centre: [7.0, 7.0],
        colour: [
            material.albedo[0],
            material.albedo[1],
            material.albedo[2],
            roughness,
        ],
        // A fixed seed, so that every take shows the same grain of stone.
        params: [0.618, kind, cap, 0.0],
    }]
}

/// Write tightly packed RGBA as a PNG.
fn write_png(path: &Path, width: u32, height: u32, rgba: &[u8]) -> Result<()> {
    let file = std::fs::File::create(path)
        .with_context(|| format!("cannot create {}", path.display()))?;
    let mut encoder = png::Encoder::new(std::io::BufWriter::new(file), width, height);
    encoder.set_color(png::ColorType::Rgba);
    encoder.set_depth(png::BitDepth::Eight);
    let mut writer = encoder.write_header().context("the PNG header failed")?;
    writer
        .write_image_data(rgba)
        .with_context(|| format!("cannot write {}", path.display()))
}

/// One rendered row of a sheet: the label, the intent, and the file names of
/// the play-zoom and close-up images.
struct SheetRow {
    name: String,
    intent: String,
    play: String,
    close: String,
}

/// Render one take at both zooms and return its row.
fn render_take(
    gpu: &HeadlessGpu,
    wood: &WoodTexture,
    glyphs: &GlyphTexture,
    kind: f32,
    take: &StoneVariant,
    directory: &Path,
) -> Result<SheetRow> {
    let mut renderer = Renderer::new(
        &gpu.device,
        &gpu.queue,
        crate::preview::PREVIEW_FORMAT,
        SAMPLES,
        wood,
        glyphs,
        &take.shape,
    );
    renderer
        .set_stones(&gpu.queue, &lone_stone(kind, take.roughness, take.cap));

    let mut files = Vec::new();
    for (label, pixels_per_cell) in ZOOMS {
        let viewport = camera::Viewport::window(IMAGE, IMAGE);
        let mut globals = Renderer::globals_with_wood(viewport, render::WOOD);
        globals.view = [7.0, 7.0, pixels_per_cell, 0.0];
        globals.slate_knobs = take.knobs;
        globals.shell_knobs = take.knobs;
        renderer.set_globals(&gpu.queue, &globals);

        let pixels = gpu.read_frame(IMAGE, IMAGE, &renderer)?;
        let file = format!("{}-{label}.png", take.name.to_lowercase());
        write_png(&directory.join(&file), IMAGE, IMAGE, &pixels)?;
        files.push(file);
    }
    let [play, close] = files.try_into().expect("two zooms were rendered");
    Ok(SheetRow {
        name: take.name.clone(),
        intent: take.intent.clone(),
        play,
        close,
    })
}

/// Render one dimple take at both zooms, on an empty board centred on a
/// crossing, and return its row.
fn render_dimple(
    gpu: &HeadlessGpu,
    wood: &WoodTexture,
    glyphs: &GlyphTexture,
    take: &DimpleVariant,
    directory: &Path,
) -> Result<SheetRow> {
    let mut renderer = Renderer::new(
        &gpu.device,
        &gpu.queue,
        crate::preview::PREVIEW_FORMAT,
        SAMPLES,
        wood,
        glyphs,
        &render::STONE_SHAPE,
    );
    renderer.set_stones(&gpu.queue, &[]);

    let mut files = Vec::new();
    for (label, pixels_per_cell) in ZOOMS {
        let viewport = camera::Viewport::window(IMAGE, IMAGE);
        let mut globals = Renderer::globals_with_wood(viewport, render::WOOD);
        globals.view = [6.0, 6.0, pixels_per_cell, 0.0];
        globals.toggles = [0.0, take.floor, 0.0, 0.0];
        renderer.set_globals(&gpu.queue, &globals);

        let pixels = gpu.read_frame(IMAGE, IMAGE, &renderer)?;
        let file = format!("{}-{label}.png", take.name.to_lowercase());
        write_png(&directory.join(&file), IMAGE, IMAGE, &pixels)?;
        files.push(file);
    }
    let [play, close] = files.try_into().expect("two zooms were rendered");
    Ok(SheetRow {
        name: take.name.clone(),
        intent: take.intent.clone(),
        play,
        close,
    })
}

/// One measured knock, for the page and the terminal.
struct KnockRow {
    name: String,
    intent: String,
    file: String,
    analysis: sound::Analysis,
}

/// Render one knock take and measure it.
fn render_knock(take: &KnockVariant, directory: &Path) -> Result<KnockRow> {
    let samples = audio::render_mix(RATE, 7, false, &take.parts);
    let file = format!("{}.wav", take.name.to_lowercase());
    sound::write_wav(&directory.join(&file), &samples, RATE)?;
    Ok(KnockRow {
        name: take.name.clone(),
        intent: take.intent.clone(),
        file,
        analysis: sound::analyse(&samples, RATE),
    })
}

/// Render all the sheets into `directory`, with a page that shows them.
pub fn audition(directory: &Path) -> Result<()> {
    std::fs::create_dir_all(directory)
        .with_context(|| format!("cannot create {}", directory.display()))?;

    let gpu = HeadlessGpu::new()?;
    let wood = WoodTexture::load_wood()?;
    let glyphs = GlyphTexture::load_glyphs()?;

    // The reference: the look the game ships with, rendered like a take.
    let slate_now = StoneVariant {
        name: "B0".to_string(),
        intent: "the stone before the choice, for reference".to_string(),
        shape: OLD_SHAPE,
        roughness: 0.58,
        cap: 0.0,
        knobs: [1.0, 1.0, 1.0, 1.0],
    };
    let shell_now = StoneVariant {
        name: "W0".to_string(),
        intent: "the stone before the choice, for reference".to_string(),
        shape: OLD_SHAPE,
        roughness: SHELL.roughness,
        cap: 0.0,
        knobs: [1.0, 1.0, 1.0, 1.0],
    };

    let mut slate = vec![render_take(&gpu, &wood, &glyphs, 0.0, &slate_now, directory)?];
    for take in slate_variants() {
        slate.push(render_take(&gpu, &wood, &glyphs, 0.0, &take, directory)?);
        log::info!("slate {} rendered", take.name);
    }
    let mut shell = vec![render_take(&gpu, &wood, &glyphs, 1.0, &shell_now, directory)?];
    for take in shell_variants() {
        shell.push(render_take(&gpu, &wood, &glyphs, 1.0, &take, directory)?);
        log::info!("shell {} rendered", take.name);
    }

    let mut dimples = Vec::new();
    for take in dimple_variants() {
        dimples.push(render_dimple(&gpu, &wood, &glyphs, &take, directory)?);
        log::info!("dimple {} rendered", take.name);
    }

    let knock_now = KnockVariant {
        name: "K0".to_string(),
        intent: "the sound before the choice".to_string(),
        parts: old_sound(),
    };
    let mut knocks = vec![render_knock(&knock_now, directory)?];
    for take in knock_variants() {
        knocks.push(render_knock(&take, directory)?);
        log::info!("knock {} rendered", take.name);
    }

    let page = directory.join("index.html");
    std::fs::write(&page, html(&slate, &shell, &dimples, &knocks))
        .with_context(|| format!("cannot write {}", page.display()))?;

    println!("{}", table(&knocks));
    println!("page: {}", page.display());
    Ok(())
}

/// A table of the knock measurements, for the terminal.
fn table(knocks: &[KnockRow]) -> String {
    let mut text = String::from(
        "  name  impact   rises  centroid    low    mid   high  to -20 dB\n",
    );
    for knock in knocks {
        text.push_str(&format!(
            "  {:4} {:5.1} dB {:5.2} ms {:6.0} Hz {:4.0}% {:4.0}% {:4.0}%  {:5.0} ms\n",
            knock.name,
            knock.analysis.hit_db,
            knock.analysis.rise_ms,
            knock.analysis.centroid,
            knock.analysis.low * 100.0,
            knock.analysis.mid * 100.0,
            knock.analysis.high * 100.0,
            knock.analysis.decay_ms,
        ));
    }
    text
}

/// The page that shows and plays the sheets.
fn html(slate: &[SheetRow], shell: &[SheetRow], dimples: &[SheetRow], knocks: &[KnockRow]) -> String {
    let mut page = String::from(
        "<!doctype html>\n<html lang=\"en\">\n<head>\n<meta charset=\"utf-8\">\n\
         <title>Gomoku: stone and knock variations</title>\n<style>\n\
         body { background: #17181b; color: #e6e6ea; font: 15px/1.5 -apple-system, system-ui, sans-serif;\n\
         max-width: 1180px; margin: 40px auto; padding: 0 20px; }\n\
         h1 { font-size: 22px; font-weight: 600; }\n\
         h2 { font-size: 18px; font-weight: 600; margin-top: 48px; }\n\
         .row { display: flex; align-items: center; gap: 18px; background: #23252a;\n\
         border-radius: 10px; padding: 14px 16px; margin: 12px 0; }\n\
         .row.reference { outline: 1px solid #4a4d55; }\n\
         .meta { width: 320px; flex: none; }\n\
         .name { font-weight: 700; font-size: 17px; }\n\
         .intent { color: #b6b6bf; font-size: 14px; }\n\
         .numbers { color: #85858f; font-variant-numeric: tabular-nums; font-size: 13px; }\n\
         img { width: 340px; height: 340px; border-radius: 8px; display: block; }\n\
         figure { margin: 0; text-align: center; color: #85858f; font-size: 12px; }\n\
         audio { width: 100%; margin-top: 10px; }\n\
         .sound { flex: 1; }\n\
         </style>\n</head>\n<body>\n\
         <h1>Stones and knock: the variations</h1>\n\
         <p>Every take is one stone on the centre crossing, at the zoom the game is played at\n\
         and up close. B0, W0 and K0 are the look and the sound the game ships with today.\n\
         The drilled crossings now end at a sharp rim instead of fading out; the play-zoom\n         images show it.</p>\n",
    );

    let section = |title: &str, note: &str, rows: &[SheetRow], page: &mut String| {
        page.push_str(&format!("<h2>{title}</h2>\n<p>{note}</p>\n"));
        for row in rows {
            page.push_str(&format!(
                "<div class=\"row{reference}\"><div class=\"meta\">\
                 <div class=\"name\">{name}</div><div class=\"intent\">{intent}</div></div>\n\
                 <figure><img src=\"{play}\" alt=\"{name} at play zoom\"><figcaption>play zoom</figcaption></figure>\n\
                 <figure><img src=\"{close}\" alt=\"{name} up close\"><figcaption>close up</figcaption></figure>\n\
                 </div>\n",
                reference = if row.name.ends_with('0') { " reference" } else { "" },
                name = row.name,
                intent = row.intent,
                play = row.play,
                close = row.close,
            ));
        }
    };

    section(
        "Black stones: flatter, and dry rather than wet",
        "The bright spot and the reflected room are what reads as wet, so they fall from B1 to \
         B10. The body lightens a little along the way, which takes the hardness off the spot \
         that is left. All ten are lower and wider in the shoulder than today: a lentil, not \
         a ball.",
        slate,
        &mut page,
    );
    section(
        "White stones: flatter, with the texture standing out",
        "The streaks and speckle of the shell gain contrast from W1 to W10, and the body \
         darkens a little along the way to give the texture something to stand against.",
        shell,
        &mut page,
    );
    section(
        "The drilled crossings: a sharp rim, not a fade",
        "How dark the cup still is where it ends. The cut itself is one pixel in every take; \
         what changes is how much dark the cut drops from, which is what the eye reads as \
         the edge. D4 ends as dark and as sharp as the grid lines.",
        dimples,
        &mut page,
    );

    page.push_str(
        "<h2>The knock: shorter in the tail, flatter in the landing</h2>\n\
         <p>From K1 to K10 the modes ring shorter and shorter, the landing takes longer, and \
         the contact quiets. All files are at the same peak level, so what changes is the \
         sound, not the loudness.</p>\n",
    );
    for knock in knocks {
        page.push_str(&format!(
            "<div class=\"row{reference}\"><div class=\"meta\">\
             <div class=\"name\">{name}</div><div class=\"intent\">{intent}</div>\
             <div class=\"numbers\">impact {impact:.1} dB &middot; rises in {rise:.2} ms &middot; \
             to -20 dB in {decay:.0} ms</div></div>\n\
             <div class=\"sound\"><audio controls preload=\"none\" src=\"{file}\"></audio></div>\n\
             </div>\n",
            reference = if knock.name == "K0" { " reference" } else { "" },
            name = knock.name,
            intent = knock.intent,
            impact = knock.analysis.hit_db,
            rise = knock.analysis.rise_ms,
            decay = knock.analysis.decay_ms,
            file = knock.file,
        ));
    }

    page.push_str(
        "<p style=\"margin-top:48px\">Name the letters and numbers you like. If one is nearly \
         right, say which way to move it.</p>\n</body>\n</html>\n",
    );
    page
}
