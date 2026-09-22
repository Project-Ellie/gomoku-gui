# 06 — Audio

## Goal

One sound: a stone placed on a wooden board. No music, no capture sound (the
game has no captures), no UI sounds.

## Why synthesis instead of a sample

A click is short, repeatable, and easy to shape with a physical model, and a
model can vary each placement so that 200 moves do not sound like one sample
played 200 times. There is also no asset file to license, ship, or lose.

Honest limitation: a real recording of a stone on a kaya board still sounds
better than a physical model. The audio layer therefore sits behind one small
interface (`play_click(near_neighbour: bool)`), so replacing the synthesis with
a sample later touches one module.

## The model

A stone striking a board is an impulse that excites the resonant modes of the
board and the stone. The model is a noise burst through four damped resonators.

```
excite(t) = noise(t) * exp(-t / 0.0015)          // 1.5 ms attack
mode_k(t) = a_k * sin(2*pi*f_k*t + phase_k) * exp(-t / tau_k)
click(t)  = soft_clip(sum_k mode_k(t) * excite(t))
```

| Mode | Frequency | Decay `tau` | Amplitude |
|---|---|---|---|
| 1 | 420 Hz | 0.055 s | 1.00 |
| 2 | 1150 Hz | 0.032 s | 0.60 |
| 3 | 2600 Hz | 0.018 s | 0.32 |
| 4 | 5400 Hz | 0.009 s | 0.18 |

The first mode gives the body of the knock, the higher modes give the sharp
attack. Total duration is 180 ms, with a 3 ms fade-in to avoid a DC step and a
5 ms fade-out so the tail never clicks.

This is a starting point, not a truth. The tuning panel exposes the four
frequencies, the four decays, the noise length, and the soft-clip amount, so
the sound is adjusted by ear on the real machine.

## Variation

Six buffers are rendered at start-up, once each:

| Variant | Use |
|---|---|
| 0 | Default placement |
| 1..3 | Placements with one, two, or three or more occupied neighbours |
| 4, 5 | Two extra default variants |

Rules for a variant:

- Frequencies vary by up to `±5` percent.
- Decays vary by up to `±12` percent.
- Level varies by up to `±18` percent.
- The noise phase is random.

A damped board makes a different sound, which is a real effect a player hears:
when one or more of the four orthogonal neighbours is occupied, the response
uses `tau * 0.86`, loses 8 percent of its level, and drops the first mode by a
little more. Variant selection walks the variants in a shuffled order, so two
identical placements in a row do not sound identical.

Per-move variation uses a small deterministic PRNG seeded at start-up. The
audio thread never calls `rand`.

## Real-time rules

The audio callback is a hard real-time context. It must not:

- allocate or free memory,
- lock a mutex or wait on a channel with a blocking call,
- log, print, or format a string,
- call `sin`, `cos`, or any other transcendental function (all four modes are
  pre-rendered, so this is free).

The callback reads from a lock-free single-producer single-consumer ring:

```rust
struct ClickQueue {
    slots: [AtomicUsize; 16],   // variant index plus 1, 0 means empty
    head:  AtomicUsize,          // written by the UI thread
    tail:  AtomicUsize,          // written by the audio thread
}
```

The UI thread writes a variant index and advances `head`. The audio thread
reads and advances `tail`. Full and empty are detected by the index distance.
The queue drops a click rather than blocking, which is the correct behaviour
for a sound effect.

Voices: eight fixed voices, each `{ buffer: u8, position: u32, gain: f32 }`.
Mixing is `out += buffer[position] * gain`, and a finished voice is marked free.
Exceeding eight simultaneous clicks is not possible in this game.

## Stream setup

```
1. Find the default output device. If there is none, disable sound with a warning.
2. Take the default output config. Prefer F32 sample format; otherwise convert
   to I16 in the callback with a fixed scale factor.
3. Render the six click buffers at the device sample rate.
4. Build the output stream with the queue in its data closure.
5. Play the stream. Keep the stream handle alive for the life of the app.
```

A device that disappears (headphones unplugged) causes a stream error. The
error is logged once, the stream is dropped, and the application continues
without sound. No dialog appears for an audio failure.

## Volume

`volume` from the config scales every voice gain. The range is `0.0..=1.0`,
default `0.7`. The View menu has a Sound toggle and a volume slider. Setting
the volume to zero is equivalent to disabling sound and does not stop the
stream, which keeps the code path simple.

## Latency

Only one click plays at a time in the common case, so buffer size follows the
device default. No attempt is made to reach a low-latency configuration,
because a 20 ms delay on a stone knock is not perceptible in this context.

## Testability

The synthesis itself is a pure function:

```rust
pub fn render_click(sample_rate: u32, params: &ClickParams, rng: &mut SmallRng) -> Vec<f32>
```

It is tested without an audio device:

| Test | Assertion |
|---|---|
| Length | `render_click` returns `sample_rate * 0.180` samples |
| Bounds | every sample is inside `-1.0..=1.0` |
| Envelope | the peak is inside the first 15 ms, and the level at 150 ms is below 1 percent of the peak |
| Silence at the edges | the first and last samples are within `1e-4` of zero |
| Variation | two different seeds give different buffers, and the same seed gives the same buffer |
| Damping | a damped variant has a smaller peak than the default variant |

The queue is tested on two threads with a simple producer and consumer, and
with a full queue to prove that a click is dropped instead of blocking.
