//! The knock of a stone on the board.
//!
//! A stone landing on wood is a short impulse that excites the resonant modes of
//! the board. The model is four damped modes plus a brief noise transient. Six
//! buffers are rendered once at start-up, so the audio callback only mixes: it
//! does not allocate, does not lock, and calls no transcendental function.

use std::sync::Arc;
use std::sync::atomic::{AtomicU32, AtomicUsize, Ordering};

use cpal::traits::{DeviceTrait as _, HostTrait as _, StreamTrait as _};

/// The resonant modes: frequency in Hz, decay time in seconds, amplitude.
const MODES: [(f32, f32, f32); 4] = [
    (420.0, 0.055, 1.00),
    (1150.0, 0.032, 0.60),
    (2600.0, 0.018, 0.32),
    (5400.0, 0.009, 0.18),
];

/// The length of one knock, in seconds.
const CLICK_SECONDS: f32 = 0.180;

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
pub fn render_click(sample_rate: u32, seed: u32, damped: bool) -> Vec<f32> {
    let length = (sample_rate as f32 * CLICK_SECONDS) as usize;
    let mut samples = vec![0.0_f32; length];
    let mut rng = Rng::new(seed);

    let mut modes = [(0.0_f32, 0.0_f32, 0.0_f32); 4];
    for (index, (frequency, decay, amplitude)) in MODES.iter().enumerate() {
        let frequency = frequency * (1.0 + 0.05 * rng.next());
        let decay = decay * (1.0 + 0.12 * rng.next()) * if damped { 0.86 } else { 1.0 };
        let amplitude = amplitude * (1.0 + 0.18 * rng.next()) * if damped { 0.92 } else { 1.0 };
        modes[index] = (frequency, decay, amplitude);
    }

    for (index, sample) in samples.iter_mut().enumerate() {
        let time = index as f32 / sample_rate as f32;
        let mut value = 0.0;
        for (frequency, decay, amplitude) in modes {
            value += amplitude
                * (std::f32::consts::TAU * frequency * time).sin()
                * (-time / decay).exp();
        }
        // The sharp attack: a brief burst of noise, gone in two milliseconds.
        let transient = if time < 0.002 {
            rng.next() * 0.35 * (-time / 0.0006).exp()
        } else {
            0.0
        };
        *sample = value + transient;
    }

    // Normalise to about minus 3 dBFS, then fade both ends so that there is no
    // step at either boundary.
    let peak = samples.iter().fold(0.0_f32, |peak, s| peak.max(s.abs()));
    if peak > 0.0 {
        let gain = 0.7079 / peak;
        for sample in &mut samples {
            *sample *= gain;
        }
    }
    let fade_in = ((0.003 * sample_rate as f32) as usize).min(length);
    let fade_out = ((0.005 * sample_rate as f32) as usize).min(length);
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
            let value = (sum * self.gain).clamp(-1.0, 1.0);
            for sample in frame.iter_mut() {
                *sample = value;
            }
        }
    }
}

/// The sound of the game. Absent when the machine has no output device.
pub struct Audio {
    queue: Arc<Queue>,
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
                )
            })
            .collect();

        let queue = Arc::new(Queue::new());
        let mixer = Mixer {
            buffers,
            voices: [None; VOICES],
            next_voice: 0,
            gain: 0.7,
            queue: Arc::clone(&queue),
        };

        let stream = build_stream(&device, config, format, channels, mixer)?;
        stream.play().ok()?;
        Some(Audio {
            queue,
            next_variant: 0,
            _stream: stream,
        })
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
        let samples = render_click(48_000, 7, false);
        assert_eq!(samples.len(), (48_000.0 * CLICK_SECONDS) as usize);
    }

    #[test]
    fn a_knock_is_inside_the_range_and_quiet_at_its_edges() {
        let samples = render_click(48_000, 7, false);
        assert!(samples.iter().all(|s| s.abs() <= 1.0));
        assert!(samples[0].abs() < 1e-4, "the first sample must be silent");
        assert!(
            samples[samples.len() - 1].abs() < 1e-4,
            "the last sample must be silent"
        );
    }

    #[test]
    fn a_knock_attacks_early_and_decays() {
        let samples = render_click(48_000, 7, false);
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
            render_click(48_000, 11, false),
            render_click(48_000, 11, false)
        );
        assert_ne!(
            render_click(48_000, 11, false),
            render_click(48_000, 12, false)
        );
    }

    #[test]
    fn a_damped_knock_is_quieter() {
        let plain = render_click(48_000, 5, false);
        let damped = render_click(48_000, 5, true);
        let peak = |samples: &[f32]| samples.iter().fold(0.0_f32, |peak, s| peak.max(s.abs()));
        assert!(peak(&damped) < peak(&plain));
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
