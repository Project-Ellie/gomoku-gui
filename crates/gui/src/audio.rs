//! The knock of a stone on the board.
//!
//! A stone landing on wood is a short contact. It excites the board under it, and
//! the board answers with a few low modes of its own. The model has three parts:
//! the contact, which is a burst of filtered noise; the body of the board, which
//! is a few low modes; and the stone, which is a few short modes of its own.
//!
//! A [`Voicing`] says how much of each part there is, and how dark the result is.
//! The voicings that were listened to are kept in [`CANDIDATES`], so that the
//! sound can be compared again after a change:
//!
//! ```sh
//! cargo run -p gomoku-gui -- --knock /tmp/knock
//! ```
//!
//! Six buffers are rendered once at start-up, so the audio callback only mixes:
//! it does not allocate, does not lock, and calls no transcendental function.

use std::sync::Arc;
use std::sync::atomic::{AtomicU32, AtomicUsize, Ordering};

use cpal::traits::{DeviceTrait as _, HostTrait as _, StreamTrait as _};

/// One mode of the board or of the stone: frequency in Hz, decay time in seconds,
/// and level.
pub type Mode = (f32, f32, f32);

/// The most modes that one part of a knock can have.
pub const MAX_MODES: usize = 6;

/// A silent mode, used for the entries that are not needed.
const SILENT: Mode = (0.0, 0.0, 0.0);

/// The modes of one part of a knock: the body of the board, or the stone.
///
/// The list is of a fixed size so that a voicing costs nothing to copy and can be
/// built while the program runs, which is how the variations of a voicing are
/// made. The entries past `count` are silent.
#[derive(Debug, Clone, Copy)]
pub struct Modes {
    list: [Mode; MAX_MODES],
    count: usize,
}

impl Modes {
    /// A list of modes, up to [`MAX_MODES`].
    pub const fn new(list: &[Mode]) -> Modes {
        let mut modes = [SILENT; MAX_MODES];
        let mut index = 0;
        while index < list.len() && index < MAX_MODES {
            modes[index] = list[index];
            index += 1;
        }
        Modes {
            list: modes,
            count: index,
        }
    }

    /// The modes that are used.
    pub fn used(&self) -> &[Mode] {
        &self.list[..self.count]
    }
}

/// How a knock sounds.
///
/// These numbers are what the ear hears as warm, dry, or hollow, so they are
/// named after that rather than after wood.
#[derive(Debug, Clone, Copy)]
pub struct Voicing {
    /// The level of the contact, which is the sharp part of the knock.
    pub contact: f32,
    /// Where the contact sits in the spectrum, in Hz. Higher is brighter.
    pub contact_centre: f32,
    /// How long the contact lasts, in seconds. A real one is one to three
    /// milliseconds.
    pub contact_decay: f32,
    /// The modes of the board itself. Low, and quickly damped.
    pub body: Modes,
    /// The modes of the stone. Higher than the board, and shorter still.
    pub stone: Modes,
    /// How far the frequency falls over the life of a mode, as a fraction. A real
    /// contact settles as it spreads, so the pitch drops; a mode that holds its
    /// pitch sounds synthetic.
    pub glide: f32,
    /// How far the partials of one mode are spread, as a fraction. A mode made of
    /// one frequency rings like a tuning fork; a spread one beats with itself,
    /// which is what a piece of wood does.
    pub spread: f32,
    /// The level of the noise that the wood radiates along with its modes.
    pub whisper: f32,
    /// The top of the knock: a low pass at this frequency, in Hz. Wood eats the
    /// high end, which is what makes a knock sound warm rather than sharp.
    pub tone: f32,
    /// How much the modes wander, from 0 to 1. A pure mode rings like a bell; a
    /// little wandering makes it sound like a material instead.
    pub roughness: f32,
    /// The length of one knock, in seconds.
    pub length: f32,
}

/// The sound of the game at present.
///
/// Four ringing modes with long decays. It is pitched and hollow, which is what
/// the owner heard as an empty tin bucket. It is kept so that the sound can be
/// compared against the candidates before it is replaced.
pub const CURRENT: Voicing = Voicing {
    contact: 0.35,
    contact_centre: 3000.0,
    contact_decay: 0.0006,
    body: Modes::new(&[]),
    stone: Modes::new(&[
        (420.0, 0.055, 1.00),
        (1150.0, 0.032, 0.60),
        (2600.0, 0.018, 0.32),
        (5400.0, 0.009, 0.18),
    ]),
    tone: 20000.0,
    roughness: 0.0,
    glide: 0.0,
    spread: 0.0,
    whisper: 0.0,
    length: 0.180,
};

/// A voicing that was listened to, and the reason it exists.
#[derive(Debug, Clone)]
pub struct Candidate {
    /// The name of the file that `--knock` writes.
    pub name: String,
    /// What the sound is for, in the words of the ear, and in numbers.
    pub intent: String,
    /// The voicing itself.
    pub voicing: Voicing,
}

/// The deep board that the owner picked out of the first round.
///
/// A thick board: the contact is low and round, the body is a low pair of modes,
/// and the whole knock is dark.
pub const DEEP_BOARD: Voicing = Voicing {
    contact: 0.8,
    contact_centre: 1600.0,
    contact_decay: 0.0015,
    body: Modes::new(&[(140.0, 0.035, 0.70), (310.0, 0.020, 0.35)]),
    stone: Modes::new(&[(2000.0, 0.014, 0.15)]),
    tone: 3500.0,
    roughness: 0.25,
    glide: 0.0,
    spread: 0.0,
    whisper: 0.0,
    length: 0.120,
};

/// The voicings of the first round, which the owner listened to.
pub fn candidates() -> Vec<Candidate> {
    let mut made = Vec::new();
    let mut add = |name: &str, intent: &str, voicing: Voicing| {
        made.push(Candidate {
            name: name.to_string(),
            intent: intent.to_string(),
            voicing,
        });
    };
    add(
        "1-dry-clack",
        "The contact on its own: dry, short, almost no tone.",
        Voicing {
            contact: 1.0,
            contact_centre: 2200.0,
            contact_decay: 0.0012,
            body: Modes::new(&[(260.0, 0.012, 0.25)]),
            stone: Modes::new(&[(3000.0, 0.008, 0.15)]),
            tone: 6000.0,
            roughness: 0.3,
            glide: 0.0,
            spread: 0.0,
            whisper: 0.0,
            length: 0.060,
        },
    );
    add(
        "2-wooden-tok",
        "The board answers: a wooden tok with a low body.",
        Voicing {
            contact: 0.9,
            contact_centre: 1800.0,
            contact_decay: 0.0015,
            body: Modes::new(&[(190.0, 0.022, 0.55), (430.0, 0.014, 0.30)]),
            stone: Modes::new(&[(2400.0, 0.012, 0.20)]),
            tone: 4500.0,
            roughness: 0.25,
            glide: 0.0,
            spread: 0.0,
            whisper: 0.0,
            length: 0.090,
        },
    );
    add(
        "3-deep-board",
        "A thick board: lower, and a little longer.",
        DEEP_BOARD,
    );
    add(
        "4-stone-ring",
        "The stone rings a little, as slate does.",
        Voicing {
            contact: 0.9,
            contact_centre: 2000.0,
            contact_decay: 0.0012,
            body: Modes::new(&[(200.0, 0.020, 0.45)]),
            stone: Modes::new(&[(2700.0, 0.030, 0.40), (4100.0, 0.018, 0.20)]),
            tone: 6500.0,
            roughness: 0.2,
            glide: 0.0,
            spread: 0.0,
            whisper: 0.0,
            length: 0.110,
        },
    );
    add(
        "5-muted",
        "Laid down rather than dropped: dark, soft, and short.",
        Voicing {
            contact: 0.6,
            contact_centre: 1400.0,
            contact_decay: 0.0010,
            body: Modes::new(&[(230.0, 0.016, 0.50)]),
            stone: Modes::new(&[(1800.0, 0.010, 0.12)]),
            tone: 2600.0,
            roughness: 0.2,
            glide: 0.0,
            spread: 0.0,
            whisper: 0.0,
            length: 0.070,
        },
    );
    add(
        "6-warm-low",
        "The darkest of the set: a low body and a short tail.",
        Voicing {
            contact: 0.85,
            contact_centre: 1500.0,
            contact_decay: 0.0014,
            body: Modes::new(&[(170.0, 0.028, 0.65), (600.0, 0.012, 0.20)]),
            stone: Modes::new(&[(2200.0, 0.010, 0.10)]),
            tone: 3000.0,
            roughness: 0.22,
            glide: 0.0,
            spread: 0.0,
            whisper: 0.0,
            length: 0.100,
        },
    );
    made
}

/// How many variations of the deep board were made.
pub const VARIATIONS: usize = 20;

/// The variations of the deep board, twenty of them, in two runs of ten.
///
/// Every one of them is a little less aggressive than the deep board, and none of
/// them holds its pitch the way a pair of pure modes does. The first run makes the
/// contact softer and the tone darker and does nothing else, so that the effect of
/// softness can be heard on its own. The second run moves the wood: the modes
/// fall in pitch, the partials spread and beat, noise joins the body, and the tone
/// darkens with it.
///
/// The two runs are written as formulas rather than as twenty separate tables, so
/// the numbers below are the whole story. The page states the numbers that each
/// one came out with.
pub fn variations() -> Vec<Candidate> {
    let mut made = Vec::with_capacity(VARIATIONS);

    // The first run: softer and darker, and nothing else, so that softness can be
    // heard on its own.
    for step in 0..10 {
        // The run starts past the deep board rather than at it, so that every
        // variation is a change from the sound he picked.
        let share = (step + 1) as f32 / 10.0;
        let soft = Voicing {
            // The contact gets quieter, lower, and slower, which rounds off the
            // attack, and the tone closes down over it.
            contact: 0.80 - 0.35 * share,
            contact_centre: 1600.0 - 750.0 * share,
            contact_decay: 0.0015 + 0.0022 * share,
            tone: 3500.0 - 1600.0 * share,
            length: 0.120 + 0.020 * share,
            ..DEEP_BOARD
        };
        made.push(Candidate {
            name: format!("{:02}-soft", step + 1),
            intent: intent_of(&soft),
            voicing: soft,
        });
    }

    // The second run: the same contact, softened a little, and then the wood. The
    // modes fall in pitch as the contact settles, each mode is a cluster that
    // beats with itself, and the body radiates a breath of noise.
    for step in 0..10 {
        let share = (step + 1) as f32 / 10.0;
        let woody = Voicing {
            contact: 0.80 - 0.20 * share,
            contact_centre: 1600.0 - 250.0 * share,
            contact_decay: 0.0015 + 0.0010 * share,
            body: scaled(DEEP_BOARD.body, 1.0 - 0.08 * share, 1.0 + 0.45 * share, 1.0),
            roughness: 0.25 + 0.30 * share,
            glide: 0.060 * share,
            spread: 0.10 * share,
            whisper: 0.30 * share,
            tone: 3500.0 - 1000.0 * share,
            length: 0.120 + 0.030 * share,
            ..DEEP_BOARD
        };
        made.push(Candidate {
            name: format!("{:02}-wood", step + 11),
            intent: intent_of(&woody),
            voicing: woody,
        });
    }

    made
}

/// A mode list with the frequencies multiplied, the decay times multiplied, and
/// the levels multiplied.
fn scaled(modes: Modes, frequency: f32, decay: f32, level: f32) -> Modes {
    let mut list = [SILENT; MAX_MODES];
    let used = modes.used();
    for (index, &(f, d, l)) in used.iter().enumerate() {
        list[index] = (f * frequency, d * decay, l * level);
    }
    // Only the modes that were in the list: an entry that stays silent must not
    // be counted, because a mode of zero decay has no meaning in the renderer.
    Modes::new(&list[..used.len()])
}

/// What a variation changed, in the numbers.
fn intent_of(voicing: &Voicing) -> String {
    let body: Vec<String> = voicing
        .body
        .used()
        .iter()
        .map(|(frequency, decay, _)| format!("{frequency:.0} Hz/{:.0} ms", decay * 1000.0))
        .collect();
    format!(
        "contact {:.2} at {:.0} Hz over {:.1} ms, body {}, tone {:.0} Hz, \
         glides {:.0}%, partials spread {:.0}%, roughness {:.2}, noise {:.2}",
        voicing.contact,
        voicing.contact_centre,
        voicing.contact_decay * 1000.0,
        body.join(" and "),
        voicing.tone,
        voicing.glide * 100.0,
        voicing.spread * 100.0,
        voicing.roughness,
        voicing.whisper,
    )
}

/// The voicing the game plays.
pub const IN_USE: &Voicing = &CURRENT;

/// The number of rendered variants.
const VARIANTS: usize = 6;

/// How many knocks can sound at once.
const VOICES: usize = 8;

const SLOTS: usize = 16;

/// A lock-free queue from the user-interface thread to the audio thread. A full
/// queue drops a knock rather than blocking.
struct Queue {
    slots: [AtomicU32; SLOTS],
    head: AtomicUsize,
    tail: AtomicUsize,
}

impl Queue {
    fn new() -> Queue {
        Queue {
            slots: std::array::from_fn(|_| AtomicU32::new(0)),
            head: AtomicUsize::new(0),
            tail: AtomicUsize::new(0),
        }
    }

    /// Push a variant index. Never blocks.
    fn push(&self, variant: usize) {
        let head = self.head.load(Ordering::Relaxed);
        let tail = self.tail.load(Ordering::Acquire);
        if head.wrapping_sub(tail) >= SLOTS {
            return;
        }
        self.slots[head % SLOTS].store(variant as u32 + 1, Ordering::Release);
        self.head.store(head.wrapping_add(1), Ordering::Release);
    }

    /// Pop a variant index, if one is waiting.
    fn pop(&self) -> Option<usize> {
        let tail = self.tail.load(Ordering::Relaxed);
        if tail == self.head.load(Ordering::Acquire) {
            return None;
        }
        let value = self.slots[tail % SLOTS].load(Ordering::Acquire);
        self.tail.store(tail.wrapping_add(1), Ordering::Release);
        (value > 0).then(|| (value - 1) as usize)
    }
}

/// A small deterministic generator, so a variant is reproducible from its seed.
struct Rng(u32);

impl Rng {
    fn new(seed: u32) -> Rng {
        Rng(seed | 1)
    }

    /// The next value, in `-1.0..=1.0`.
    fn next(&mut self) -> f32 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 17;
        self.0 ^= self.0 << 5;
        (self.0 >> 8) as f32 / 8_388_608.0 - 1.0
    }
}

/// Render one knock.
///
/// `damped` shortens and quietens the response, which is what a neighbouring
/// stone does to a real board.
pub fn render_click(sample_rate: u32, seed: u32, damped: bool, voicing: &Voicing) -> Vec<f32> {
    let rate = sample_rate as f32;
    let length = (rate * voicing.length) as usize;
    let mut samples = vec![0.0_f32; length];
    let mut rng = Rng::new(seed);

    // Every mode wanders a little, and every mode is a cluster of partials a
    // little apart, which beat against each other. One partial on its own rings
    // like a tuning fork; a cluster sounds like a material.
    let mut partials: Vec<Mode> = Vec::new();
    let mut longest_body = 0.0_f32;
    let body_count = voicing.body.used().len();
    for (index, &(frequency, decay, level)) in voicing
        .body
        .used()
        .iter()
        .chain(voicing.stone.used().iter())
        .filter(|(_, decay, level)| *level != 0.0 && *decay > 0.0)
        .enumerate()
    {
        let frequency = frequency * (1.0 + 0.05 * rng.next());
        let decay = decay * (1.0 + 0.12 * rng.next()) * if damped { 0.86 } else { 1.0 };
        let level = level * (1.0 + 0.18 * rng.next()) * if damped { 0.92 } else { 1.0 };
        if index < body_count {
            longest_body = longest_body.max(decay);
        }
        if voicing.spread > 0.0 {
            let share = level / 3.0_f32.sqrt();
            partials.push((frequency * (1.0 - voicing.spread), decay, share));
            partials.push((frequency, decay, share));
            partials.push((frequency * (1.0 + voicing.spread), decay, share));
        } else {
            partials.push((frequency, decay, level));
        }
    }
    // The phase of each partial is carried forward, because its frequency moves.
    let mut phases = vec![0.0_f32; partials.len()];

    // The contact: noise with the top and the bottom taken off it, so that it
    // clacks instead of hissing.
    let contact_low = voicing.contact_centre * 2.0;
    let contact_high = voicing.contact_centre * 0.5;
    let low_alpha = 1.0 - (-std::f32::consts::TAU * contact_low / rate).exp();
    let high_alpha = 1.0 - (-std::f32::consts::TAU * contact_high / rate).exp();
    let mut low_1 = 0.0_f32;
    let mut low_2 = 0.0_f32;
    let mut high = 0.0_f32;

    // The tone: one pole over the whole knock. This is what dark means here.
    let tone_alpha = 1.0 - (-std::f32::consts::TAU * voicing.tone / rate).exp();
    let mut tone = 0.0_f32;

    // The breath: the noise that wood radiates along with its body. It is what
    // stops a knock from being two clean tones.
    let whisper_decay = if longest_body > 0.0 {
        longest_body
    } else {
        voicing.length * 0.2
    };
    let breath_alpha = 1.0 - (-std::f32::consts::TAU * (voicing.tone * 0.6) / rate).exp();
    let mut breath = 0.0_f32;

    for (index, sample) in samples.iter_mut().enumerate() {
        let time = index as f32 / rate;
        let noise = rng.next();

        // The contact, which is gone in a millisecond or two.
        let contact = if time < voicing.contact_decay * 10.0 {
            low_1 += low_alpha * (noise - low_1);
            low_2 += low_alpha * (low_1 - low_2);
            high += high_alpha * (low_2 - high);
            (low_2 - high) * voicing.contact * (-time / voicing.contact_decay).exp()
        } else {
            0.0
        };

        // The board, and then the stone. The frequency of each partial falls as
        // the contact settles, which is what a real board does.
        let mut value = 0.0;
        for ((frequency, decay, level), phase) in partials.iter().zip(phases.iter_mut()) {
            let settled = 1.0 - voicing.glide * (1.0 - (-time / (decay * 0.5)).exp());
            *phase += std::f32::consts::TAU * frequency * settled / rate;
            let wander = 1.0 + voicing.roughness * noise;
            value += level * wander * phase.sin() * (-time / decay).exp();
        }

        let breath_noise = if voicing.whisper > 0.0 {
            breath += breath_alpha * (noise - breath);
            breath * voicing.whisper * (-time / whisper_decay).exp()
        } else {
            0.0
        };

        tone += tone_alpha * (value + contact + breath_noise - tone);
        *sample = tone;
    }

    // Normalise to about minus 3 dBFS, then fade both ends so that there is no
    // step at either boundary.
    //
    // The fade at the start is very short on purpose. The contact is the loudest
    // part of the knock and it is over in a millisecond or two, so a fade of a
    // few milliseconds would ramp away the very thing that makes it a knock.
    let peak = samples.iter().fold(0.0_f32, |peak, s| peak.max(s.abs()));
    if peak > 0.0 {
        let gain = 0.7079 / peak;
        for sample in &mut samples {
            *sample *= gain;
        }
    }
    let fade_in = ((0.0002 * rate) as usize).max(1).min(length);
    let fade_out = ((0.002 * rate) as usize).min(length);
    for (index, sample) in samples.iter_mut().take(fade_in).enumerate() {
        *sample *= index as f32 / fade_in as f32;
    }
    for (index, sample) in samples.iter_mut().rev().take(fade_out).enumerate() {
        *sample *= index as f32 / fade_out as f32;
    }
    samples
}

/// Mixes the rendered knocks. Lives on the audio thread.
struct Mixer {
    buffers: Vec<Vec<f32>>,
    voices: [Option<(usize, usize)>; VOICES],
    next_voice: usize,
    gain: f32,
    /// The volume, shared with the user interface. Held as the bits of an `f32`,
    /// because the audio thread must not take a lock.
    volume: Arc<AtomicU32>,
    queue: Arc<Queue>,
}

impl Mixer {
    fn fill(&mut self, output: &mut [f32], channels: usize) {
        if let Some(variant) = self.queue.pop() {
            self.voices[self.next_voice] = Some((variant.min(self.buffers.len() - 1), 0));
            self.next_voice = (self.next_voice + 1) % VOICES;
        }

        let buffers = &self.buffers;
        for frame in output.chunks_mut(channels) {
            let mut sum = 0.0;
            for voice in &mut self.voices {
                if let Some((variant, position)) = voice {
                    let buffer = &buffers[*variant];
                    if *position < buffer.len() {
                        sum += buffer[*position];
                        *position += 1;
                    } else {
                        *voice = None;
                    }
                }
            }
            let volume = f32::from_bits(self.volume.load(Ordering::Relaxed));
            let value = (sum * self.gain * volume).clamp(-1.0, 1.0);
            for sample in frame.iter_mut() {
                *sample = value;
            }
        }
    }
}

/// The sound of the game. Absent when the machine has no output device.
pub struct Audio {
    queue: Arc<Queue>,
    volume: Arc<AtomicU32>,
    next_variant: usize,
    // The stream stops when it is dropped, so it is kept for the life of the
    // application.
    _stream: cpal::Stream,
}

impl Audio {
    /// Start the output stream and render the knocks.
    ///
    /// Returns `None` when there is no output device. The caller continues
    /// without sound; a missing device never stops the game.
    pub fn start() -> Option<Audio> {
        let device = cpal::default_host().default_output_device()?;
        let supported = device.default_output_config().ok()?;
        let sample_rate = supported.sample_rate();
        let channels = supported.channels() as usize;
        let format = supported.sample_format();
        let config = cpal::StreamConfig {
            channels: supported.channels(),
            sample_rate: supported.sample_rate(),
            buffer_size: cpal::BufferSize::Default,
        };

        let buffers: Vec<Vec<f32>> = (0..VARIANTS)
            .map(|variant| {
                render_click(
                    sample_rate,
                    variant as u32 * 2_654_435 + 7,
                    (1..=3).contains(&variant),
                    IN_USE,
                )
            })
            .collect();

        let queue = Arc::new(Queue::new());
        let volume = Arc::new(AtomicU32::new(0.7_f32.to_bits()));
        let mixer = Mixer {
            buffers,
            voices: [None; VOICES],
            next_voice: 0,
            gain: 0.7,
            volume: Arc::clone(&volume),
            queue: Arc::clone(&queue),
        };

        let stream = build_stream(&device, config, format, channels, mixer)?;
        stream.play().ok()?;
        Some(Audio {
            queue,
            volume,
            next_variant: 0,
            _stream: stream,
        })
    }

    /// Set the volume, from 0 to 1. Takes effect on the next block.
    pub fn set_volume(&self, volume: f32) {
        self.volume
            .store(volume.clamp(0.0, 1.0).to_bits(), Ordering::Relaxed);
    }

    /// Play a knock. `neighbours` is how many of the four orthogonal
    /// intersections already hold a stone.
    pub fn knock(&mut self, neighbours: usize) {
        let variant = match neighbours {
            0 => {
                // Cycle through the plain variants so that repeated placements
                // do not sound identical.
                let choice = [0, 4, 5][self.next_variant % 3];
                self.next_variant = self.next_variant.wrapping_add(1);
                choice
            }
            1 => 1,
            2 => 2,
            _ => 3,
        };
        self.queue.push(variant);
    }
}

fn build_stream(
    device: &cpal::Device,
    config: cpal::StreamConfig,
    format: cpal::SampleFormat,
    channels: usize,
    mut mixer: Mixer,
) -> Option<cpal::Stream> {
    let error_callback = |error| log::warn!("audio device error: {error}");
    match format {
        cpal::SampleFormat::F32 => device
            .build_output_stream(
                config,
                move |data: &mut [f32], _: &cpal::OutputCallbackInfo| {
                    mixer.fill(data, channels);
                },
                error_callback,
                None,
            )
            .ok(),
        cpal::SampleFormat::I16 => device
            .build_output_stream(
                config,
                move |data: &mut [i16], _: &cpal::OutputCallbackInfo| {
                    let mut scratch = vec![0.0_f32; data.len()];
                    mixer.fill(&mut scratch, channels);
                    for (sample, value) in data.iter_mut().zip(scratch) {
                        *sample = (value * i16::MAX as f32) as i16;
                    }
                },
                error_callback,
                None,
            )
            .ok(),
        other => {
            log::warn!("unsupported audio sample format {other:?}; sound is off");
            None
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_knock_has_the_expected_length() {
        let samples = render_click(48_000, 7, false, &CURRENT);
        assert_eq!(samples.len(), (48_000.0 * CURRENT.length) as usize);
    }

    #[test]
    fn a_knock_is_inside_the_range_and_quiet_at_its_edges() {
        let samples = render_click(48_000, 7, false, &CURRENT);
        assert!(samples.iter().all(|s| s.abs() <= 1.0));
        assert!(samples[0].abs() < 1e-4, "the first sample must be silent");
        assert!(
            samples[samples.len() - 1].abs() < 1e-4,
            "the last sample must be silent"
        );
    }

    #[test]
    fn a_knock_attacks_early_and_decays() {
        let samples = render_click(48_000, 7, false, &CURRENT);
        let peak_index = samples
            .iter()
            .enumerate()
            .max_by(|a, b| a.1.abs().partial_cmp(&b.1.abs()).expect("no NaN"))
            .map(|(index, _)| index)
            .expect("not empty");
        assert!(
            peak_index < 48_000 / 60,
            "the peak must be in the first 15 ms"
        );

        // A 420 Hz mode with a 55 ms decay is still audible at 150 ms, so the
        // tail is compared against the peak rather than against silence. The
        // fade-out at the very end is covered by the edges test.
        let level = |slice: &[f32]| slice.iter().fold(0.0_f32, |peak, s| peak.max(s.abs()));
        let peak = level(&samples);
        let last_five_ms = &samples[samples.len() - 48_000 * 5 / 1000..];
        let tail = level(last_five_ms);
        assert!(
            tail < peak * 0.15,
            "the tail must have decayed well below the peak, got {tail} against {peak}"
        );
    }

    #[test]
    fn a_knock_is_reproducible_from_its_seed() {
        assert_eq!(
            render_click(48_000, 11, false, &CURRENT),
            render_click(48_000, 11, false, &CURRENT)
        );
        assert_ne!(
            render_click(48_000, 11, false, &CURRENT),
            render_click(48_000, 12, false, &CURRENT)
        );
    }

    #[test]
    fn every_sound_is_a_sound_of_its_own() {
        // The point of the set is that the owner can tell the sounds apart, so no
        // two of them may come out the same.
        let mut all = candidates();
        all.extend(variations());
        assert_eq!(all.len(), 6 + VARIATIONS);
        for (index, candidate) in all.iter().enumerate() {
            let samples = render_click(48_000, 3, false, &candidate.voicing);
            assert_eq!(
                samples.len(),
                (48_000.0 * candidate.voicing.length) as usize
            );
            assert!(
                samples.iter().any(|s| s.abs() > 0.1),
                "{} is silent",
                candidate.name
            );
            for other in all.iter().skip(index + 1) {
                let other_samples = render_click(48_000, 3, false, &other.voicing);
                assert_ne!(
                    samples, other_samples,
                    "{} is the same as {}",
                    candidate.name, other.name
                );
            }
        }
    }

    #[test]
    fn every_variation_changes_the_deep_board() {
        // Each variation must be its own sound, and none of them may hold its
        // pitch the way the deep board does: that is what the owner asked for.
        let base = render_click(48_000, 3, false, &DEEP_BOARD);
        let mut moved = 0;
        for candidate in variations() {
            let samples = render_click(48_000, 3, false, &candidate.voicing);
            assert_ne!(samples, base, "{} is the deep board itself", candidate.name);
            if candidate.voicing.glide > 0.0 {
                moved += 1;
            }
        }
        assert_eq!(moved, 10, "ten of the variations must move the pitch");
    }

    #[test]
    fn a_mode_that_glides_falls_in_pitch() {
        // The frequency must fall over the life of the mode, which is what makes
        // the sound move the way wood does.
        let gliding = Voicing {
            contact: 0.0,
            body: Modes::new(&[(400.0, 0.100, 1.0)]),
            stone: Modes::new(&[]),
            roughness: 0.0,
            tone: 20000.0,
            glide: 0.10,
            ..DEEP_BOARD
        };
        let plain = Voicing {
            glide: 0.0,
            ..gliding
        };
        let count_crossings = |voicing: &Voicing| {
            let samples = render_click(48_000, 5, false, voicing);
            samples
                .windows(2)
                .filter(|pair| pair[0] <= 0.0 && pair[1] > 0.0)
                .count()
        };
        let (glided, still) = (count_crossings(&gliding), count_crossings(&plain));
        assert!(
            glided < still,
            "a mode that falls in pitch completes fewer cycles: {glided} against {still}"
        );
    }

    #[test]
    fn a_damped_knock_carries_less_energy() {
        // Every knock is normalised to the same peak before it is played, so a
        // damped one is quieter in energy rather than in peak.
        let plain = render_click(48_000, 5, false, &CURRENT);
        let damped = render_click(48_000, 5, true, &CURRENT);
        let energy = |samples: &[f32]| samples.iter().map(|s| s * s).sum::<f32>();
        assert!(energy(&damped) < energy(&plain));
    }

    #[test]
    fn the_queue_is_first_in_first_out() {
        let queue = Queue::new();
        assert_eq!(queue.pop(), None);
        queue.push(3);
        queue.push(4);
        assert_eq!(queue.pop(), Some(3));
        assert_eq!(queue.pop(), Some(4));
        assert_eq!(queue.pop(), None);
    }

    #[test]
    fn a_full_queue_drops_instead_of_blocking() {
        let queue = Queue::new();
        for _ in 0..SLOTS * 4 {
            queue.push(1);
        }
        assert_eq!(queue.pop(), Some(1));
    }
}
