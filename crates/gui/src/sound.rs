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

use crate::audio;

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
    /// The highest sample, in the range 0 to 1.
    pub peak: f32,
    /// The root mean square over the whole knock.
    pub rms: f32,
    /// Where the energy sits on average, in Hz. Higher is brighter.
    pub centroid: f32,
    /// The share of the energy below 500 Hz.
    pub low: f32,
    /// The share between 500 Hz and 2 kHz.
    pub mid: f32,
    /// The share above 2 kHz.
    pub high: f32,
    /// The time from the peak until the level is 40 dB down, in milliseconds.
    pub decay_ms: f32,
}

/// Measure one knock.
pub fn analyse(samples: &[f32], rate: u32) -> Analysis {
    let peak = samples.iter().fold(0.0_f32, |peak, s| peak.max(s.abs()));
    let rms = (samples.iter().map(|s| s * s).sum::<f32>() / samples.len().max(1) as f32).sqrt();

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
    for step in 0..BANDS {
        let frequency = BAND_START + step as f32 * BAND_WIDTH;
        let energy = goertzel(&samples[..window], rate, frequency);
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

    Analysis {
        peak,
        rms,
        centroid: if total > 0.0 { weighted / total } else { 0.0 },
        low: share(low),
        mid: share(mid),
        high: share(high),
        decay_ms: decay_ms(samples, rate, peak),
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

/// The time from the peak until the level is 40 dB down, in milliseconds.
fn decay_ms(samples: &[f32], rate: u32, peak: f32) -> f32 {
    let peak_index = samples
        .iter()
        .enumerate()
        .max_by(|a, b| a.1.abs().partial_cmp(&b.1.abs()).expect("no NaN"))
        .map(|(index, _)| index)
        .unwrap_or(0);
    let step = (rate as usize / 200).max(1);
    let threshold = peak * 0.01;
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
    /// The name of the file, and the heading on the page.
    name: String,
    /// What the sound is for.
    intent: String,
    /// What was measured.
    analysis: Analysis,
}

/// Render every voicing into `directory`, and write a page that plays them.
pub fn audition(directory: &Path) -> Result<()> {
    std::fs::create_dir_all(directory)
        .with_context(|| format!("cannot create {}", directory.display()))?;

    // The sound in use first, so that it can be compared against the candidates.
    let mut sounds = Vec::new();
    let samples = audio::render_click(RATE, 7, false, audio::IN_USE);
    write_wav(&directory.join("0-the-sound-now.wav"), &samples, RATE)?;
    sounds.push(Sound {
        name: "0-the-sound-now".to_string(),
        intent: "The sound the game makes at present: four ringing modes.".to_string(),
        analysis: analyse(&samples, RATE),
    });

    for candidate in audio::CANDIDATES {
        let samples = audio::render_click(RATE, 7, false, &candidate.voicing);
        let file = directory.join(format!("{}.wav", candidate.name));
        write_wav(&file, &samples, RATE)?;
        sounds.push(Sound {
            name: candidate.name.to_string(),
            intent: candidate.intent.to_string(),
            analysis: analyse(&samples, RATE),
        });
    }

    let page = directory.join("index.html");
    std::fs::write(&page, html(&sounds))
        .with_context(|| format!("cannot write {}", page.display()))?;

    println!("{}", table(&sounds));
    println!("page: {}", page.display());
    println!(
        "play one on the command line with: afplay {}/1-dry-clack.wav",
        directory.display()
    );
    Ok(())
}

/// A table of the measurements, for the terminal.
fn table(sounds: &[Sound]) -> String {
    let mut text = String::from(
        "  name                    peak   rms  centroid    low   mid  high   decays\n",
    );
    for sound in sounds {
        text.push_str(&format!(
            "  {:22} {:5.2} {:5.2}  {:5.0} Hz {:4.0}% {:4.0}% {:4.0}%  {:5.0} ms\n",
            sound.name,
            sound.analysis.peak,
            sound.analysis.rms,
            sound.analysis.centroid,
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
    for sound in sounds {
        page.push_str(&format!(
            "<li><div class=\"name\">{}</div><div class=\"intent\">{}</div>\
             <div class=\"numbers\">centroid {:.0} Hz &middot; low {:.0}% &middot; mid {:.0}% \
             &middot; high {:.0}% &middot; decays in {:.0} ms</div>\
             <audio controls preload=\"none\" src=\"{}.wav\"></audio></li>\n",
            sound.name,
            sound.intent,
            sound.analysis.centroid,
            sound.analysis.low * 100.0,
            sound.analysis.mid * 100.0,
            sound.analysis.high * 100.0,
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
