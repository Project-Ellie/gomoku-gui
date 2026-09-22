//! Hearing the knock without playing the game.
//!
//! `--knock DIR` renders every voicing in [`crate::audio::CANDIDATES`], and the
//! sound in use, to 16-bit WAV files, measures each of them, and writes a page
//! that plays them side by side.
//!
//! The measurements are how a sound is compared without ears: the spectral
//! centroid says how bright it is, the band shares say where the energy sits, and
//! the decay says how long it rings.

use std::path::Path;

use anyhow::{Context as _, Result};

use crate::audio::{self, Voicing};

/// The rate the files are written at.
const RATE: u32 = 48_000;

/// The bottom of the lowest band, in Hz.
const BAND_START: f32 = 100.0;

/// The width of every band, in Hz.
const BAND_WIDTH: f32 = 100.0;

/// The number of bands that are measured.
const BANDS: usize = 120;

/// What the ear would notice about one knock.
#[derive(Debug, Clone, Copy)]
pub struct Analysis {
    /// How loud the contact is against the loudest moment of the knock, in dB.
    ///
    /// The contact is the part above 800 Hz in the first few milliseconds, which is
    /// what the ear hears as how hard the stone hit. A hard hit is at its full
    /// level at once, so this is near zero; a stone set down arrives far below its
    /// own loudest moment, so this is well down.
    ///
    /// Neither the crest factor nor the level of the whole first milliseconds says
    /// this, because both are dominated by the board's low body, which is loud
    /// whatever the contact does.
    pub hit_db: f32,
    /// The time from the start until the loudest moment, in milliseconds. A hard
    /// hit peaks at once; a soft landing takes a moment to arrive.
    pub rise_ms: f32,
    /// Where the energy sits on average, in Hz. Higher is brighter.
    pub centroid: f32,
    /// How far the energy is spread around the centroid, in Hz. A single tone is
    /// narrow; a material that beats with itself is wide.
    pub width: f32,
    /// The share of the energy below 500 Hz.
    pub low: f32,
    /// The share between 500 Hz and 2 kHz.
    pub mid: f32,
    /// The share above 2 kHz.
    pub high: f32,
    /// The time from the peak until the level is 20 dB down, in milliseconds.
    pub decay_ms: f32,
}

/// Measure one knock.
pub fn analyse(samples: &[f32], rate: u32) -> Analysis {
    let peak = samples.iter().fold(0.0_f32, |peak, s| peak.max(s.abs()));

    // The spectrum, in bands of the same width.
    //
    // Equal width matters. On a logarithmic scale a narrow tone lands in one band
    // while noise of the same power is spread thin over many, so the tone would
    // weigh far more than it sounds, and every knock would measure as dull.
    //
    // A window of 20 ms keeps neighbouring bands overlapping, so that a tone
    // which falls between two of them is not missed.
    let window = samples.len().min(rate as usize / 50);
    let mut total = 0.0_f32;
    let mut weighted = 0.0_f32;
    let mut low = 0.0_f32;
    let mut mid = 0.0_f32;
    let mut high = 0.0_f32;
    let mut energies = [0.0_f32; BANDS];
    for (step, stored) in energies.iter_mut().enumerate() {
        let frequency = BAND_START + step as f32 * BAND_WIDTH;
        let energy = goertzel(&samples[..window], rate, frequency);
        *stored = energy;
        total += energy;
        weighted += energy * frequency;
        if frequency < 500.0 {
            low += energy;
        } else if frequency < 2000.0 {
            mid += energy;
        } else {
            high += energy;
        }
    }
    let share = |part: f32| if total > 0.0 { part / total } else { 0.0 };
    let centroid = if total > 0.0 { weighted / total } else { 0.0 };
    let variance = if total > 0.0 {
        energies
            .iter()
            .enumerate()
            .map(|(step, energy)| {
                let distance = BAND_START + step as f32 * BAND_WIDTH - centroid;
                energy * distance * distance
            })
            .sum::<f32>()
            / total
    } else {
        0.0
    };

    let peak_index = samples
        .iter()
        .enumerate()
        .max_by(|a, b| a.1.abs().partial_cmp(&b.1.abs()).expect("no NaN"))
        .map(|(index, _)| index)
        .unwrap_or(0);

    Analysis {
        hit_db: hit_db(samples, rate, peak),
        rise_ms: peak_index as f32 / rate as f32 * 1000.0,
        centroid,
        width: variance.sqrt(),
        low: share(low),
        mid: share(mid),
        high: share(high),
        decay_ms: decay_ms(samples, rate, peak, peak_index),
    }
}

/// The energy at one frequency, by the Goertzel method.
fn goertzel(samples: &[f32], rate: u32, frequency: f32) -> f32 {
    let coefficient = 2.0 * (std::f32::consts::TAU * frequency / rate as f32).cos();
    let mut previous = 0.0_f32;
    let mut before = 0.0_f32;
    for sample in samples {
        let current = sample + coefficient * previous - before;
        before = previous;
        previous = current;
    }
    (previous * previous + before * before - coefficient * previous * before).max(0.0)
}

/// How loud the contact is, in dB against the peak of the knock.
///
/// The signal is passed through a one-pole high pass at 800 Hz, which keeps the
/// contact and drops most of the board, and the loudest moment of what is left in
/// the first five milliseconds is compared with the peak of the whole knock.
fn hit_db(samples: &[f32], rate: u32, peak: f32) -> f32 {
    if peak <= 0.0 {
        return 0.0;
    }
    // Three poles, because one leaves too much of the board behind: the body sits
    // at a few hundred hertz and a single pole only takes eight decibels off it.
    let corner = 900.0_f32;
    let alpha = 1.0 - (-std::f32::consts::TAU * corner / rate as f32).exp();
    let window = samples.len().min(rate as usize / 200);
    let mut low = [0.0_f32; 3];
    let mut loudest = 0.0_f32;
    for sample in &samples[..window] {
        let mut value = *sample;
        for stage in &mut low {
            *stage += alpha * (value - *stage);
            value -= *stage;
        }
        loudest = loudest.max(value.abs());
    }
    20.0 * (loudest / peak).log10()
}

/// The time from the peak until the level is 20 dB down, in milliseconds.
///
/// Twenty decibels rather than forty, because forty is past the end of a knock:
/// a body that decays over 35 ms needs 160 ms to fall that far, so the number
/// would say nothing about how the knock sounds.
fn decay_ms(samples: &[f32], rate: u32, peak: f32, peak_index: usize) -> f32 {
    let step = (rate as usize / 200).max(1);
    let threshold = peak * 0.1;
    let mut index = peak_index;
    while index + step < samples.len() {
        let level = samples[index..index + step]
            .iter()
            .fold(0.0_f32, |level, s| level.max(s.abs()));
        if level < threshold {
            return (index - peak_index) as f32 / rate as f32 * 1000.0;
        }
        index += step;
    }
    (samples.len() - peak_index) as f32 / rate as f32 * 1000.0
}

/// Write a mono 16-bit WAV file.
pub fn write_wav(path: &Path, samples: &[f32], rate: u32) -> Result<()> {
    let mut bytes = Vec::with_capacity(44 + samples.len() * 2);
    let data_length = (samples.len() * 2) as u32;
    bytes.extend_from_slice(b"RIFF");
    bytes.extend_from_slice(&(36 + data_length).to_le_bytes());
    bytes.extend_from_slice(b"WAVEfmt ");
    bytes.extend_from_slice(&16_u32.to_le_bytes());
    bytes.extend_from_slice(&1_u16.to_le_bytes());
    bytes.extend_from_slice(&1_u16.to_le_bytes());
    bytes.extend_from_slice(&rate.to_le_bytes());
    bytes.extend_from_slice(&(rate * 2).to_le_bytes());
    bytes.extend_from_slice(&2_u16.to_le_bytes());
    bytes.extend_from_slice(&16_u16.to_le_bytes());
    bytes.extend_from_slice(b"data");
    bytes.extend_from_slice(&data_length.to_le_bytes());
    for sample in samples {
        let value = (sample.clamp(-1.0, 1.0) * 32_767.0).round() as i16;
        bytes.extend_from_slice(&value.to_le_bytes());
    }
    std::fs::write(path, bytes).with_context(|| format!("cannot write {}", path.display()))
}

/// One sound that was rendered.
struct Sound {
    /// The group of sounds it belongs to.
    group: String,
    /// The name of the file, and the heading on the page.
    name: String,
    /// What the sound is for.
    intent: String,
    /// What was measured.
    analysis: Analysis,
}

/// Render the sounds that are being decided on into `directory`, with a page.
pub fn audition(directory: &Path) -> Result<()> {
    std::fs::create_dir_all(directory)
        .with_context(|| format!("cannot create {}", directory.display()))?;

    // Each sound, with the group it belongs to and what it is for.
    let mut wanted: Vec<(&str, &str, String, Voicing)> = vec![
        (
            "The sound the game makes now",
            "0-the-sound-now",
            "Four ringing modes, which sound hollow.".to_string(),
            *audio::IN_USE,
        ),
        (
            "Number 13, which you picked",
            "13-wood",
            audio::intent_of(&audio::chosen()),
            audio::chosen(),
        ),
        (
            "Number 3, its parent",
            "3-deep-board",
            "A thick board: lower, and a little longer.".to_string(),
            audio::DEEP_BOARD,
        ),
    ];
    for candidate in audio::candidates() {
        wanted.push((
            "Earlier rounds, for reference",
            Box::leak(candidate.name.into_boxed_str()),
            candidate.intent,
            candidate.voicing,
        ));
    }
    for candidate in audio::quieter() {
        wanted.push((
            "Quieter than 13",
            Box::leak(candidate.name.into_boxed_str()),
            candidate.intent,
            candidate.voicing,
        ));
    }

    let mut sounds = Vec::new();
    for (group, name, intent, voicing) in wanted {
        let samples = audio::render_click(RATE, 7, false, &voicing);
        write_wav(&directory.join(format!("{name}.wav")), &samples, RATE)?;
        sounds.push(Sound {
            group: group.to_string(),
            name: name.to_string(),
            intent,
            analysis: analyse(&samples, RATE),
        });
    }

    let page = directory.join("index.html");
    std::fs::write(&page, html(&sounds))
        .with_context(|| format!("cannot write {}", page.display()))?;

    println!("{}", table(&sounds));
    println!("page: {}", page.display());
    Ok(())
}

/// A table of the measurements, for the terminal.
fn table(sounds: &[Sound]) -> String {
    let mut text = String::from(
        "  name                    impact  rises  centroid  width    low   mid  high  to -20 dB\n",
    );
    let mut group = "";
    for sound in sounds {
        if sound.group != group {
            group = &sound.group;
            text.push_str(&format!("\n  -- {group}\n"));
        }
        text.push_str(&format!(
            "  {:22} {:5.1} dB {:5.2} ms {:5.0} Hz {:4.0} Hz {:4.0}% {:4.0}% {:4.0}%  {:5.0} ms\n",
            sound.name,
            sound.analysis.hit_db,
            sound.analysis.rise_ms,
            sound.analysis.centroid,
            sound.analysis.width,
            sound.analysis.low * 100.0,
            sound.analysis.mid * 100.0,
            sound.analysis.high * 100.0,
            sound.analysis.decay_ms,
        ));
    }
    text
}

/// The page that plays the renderings.
fn html(sounds: &[Sound]) -> String {
    let mut page = String::from(
        "<!doctype html>\n<html lang=\"en\">\n<head>\n<meta charset=\"utf-8\">\n\
         <title>Gomoku stone knock: candidates</title>\n<style>\n\
         body { background: #17181b; color: #e6e6ea; font: 15px/1.5 -apple-system, system-ui, sans-serif;\n\
         max-width: 720px; margin: 40px auto; padding: 0 20px; }\n\
         h1 { font-size: 20px; font-weight: 600; }\n\
         ol { padding-left: 0; list-style: none; }\n\
         li { background: #23252a; border-radius: 10px; padding: 14px 16px; margin: 12px 0; }\n\
         .name { font-weight: 600; }\n\
         .intent { color: #b6b6bf; }\n\
         .numbers { color: #85858f; font-variant-numeric: tabular-nums; font-size: 13px; }\n\
         audio { width: 100%; margin-top: 10px; }\n\
         </style>\n</head>\n<body>\n\
         <h1>Stone on the board: which one is right?</h1>\n\
         <p>Press play on each. Every file is beside this page, so it can be opened anywhere.\n\
         All of them are at the same peak level, so what changes is the sound, not the loudness.</p>\n\
         <ol>\n",
    );
    let mut group = "";
    for sound in sounds {
        if sound.group != group {
            group = &sound.group;
            page.push_str(&format!("</ol><h2>{group}</h2><ol>\n"));
        }
        page.push_str(&format!(
            "<li><div class=\"name\">{}</div><div class=\"intent\">{}</div>\
             <div class=\"numbers\">impact {:.1} dB &middot; rises in {:.2} ms &middot; \
             centroid {:.0} Hz &middot; width {:.0} Hz &middot; to -20 dB in {:.0} ms</div>\
             <audio controls preload=\"none\" src=\"{}.wav\"></audio></li>\n",
            sound.name,
            sound.intent,
            sound.analysis.hit_db,
            sound.analysis.rise_ms,
            sound.analysis.centroid,
            sound.analysis.width,
            sound.analysis.decay_ms,
            sound.name,
        ));
    }
    page.push_str(
        "</ol>\n<p>Name the numbers you like. If one is nearly right, say which way to move it:\n\
         warmer, darker, shorter, more of a clack.</p>\n</body>\n</html>\n",
    );
    page
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tone(rate: u32, frequency: f32, length: f32) -> Vec<f32> {
        (0..(rate as f32 * length) as usize)
            .map(|index| {
                0.5 * (std::f32::consts::TAU * frequency * index as f32 / rate as f32).sin()
            })
            .collect()
    }

    #[test]
    fn the_analyser_finds_where_the_energy_is() {
        let high = analyse(&tone(48_000, 3000.0, 0.2), 48_000);
        assert!(
            (2000.0..4500.0).contains(&high.centroid),
            "a 3 kHz tone must measure near 3 kHz, got {:.0} Hz",
            high.centroid
        );
        assert!(
            high.high > 0.8,
            "and its energy must be in the high band, got {:.2}",
            high.high
        );

        let low = analyse(&tone(48_000, 200.0, 0.2), 48_000);
        assert!(
            low.centroid < 400.0,
            "a 200 Hz tone must measure near 200 Hz, got {:.0} Hz",
            low.centroid
        );
        assert!(
            low.low > 0.8,
            "and its energy must be in the low band, got {:.2}",
            low.low
        );
    }

    #[test]
    fn a_written_file_is_a_wav_of_the_right_length() {
        let path = std::env::temp_dir().join("gomoku-knock-test.wav");
        let samples = tone(48_000, 440.0, 0.05);
        write_wav(&path, &samples, 48_000).expect("the file is written");
        let bytes = std::fs::read(&path).expect("the file is read");
        assert_eq!(&bytes[0..4], b"RIFF");
        assert_eq!(&bytes[8..12], b"WAVE");
        assert_eq!(bytes.len(), 44 + samples.len() * 2);
        std::fs::remove_file(&path).expect("the test file is removed");
    }
}
