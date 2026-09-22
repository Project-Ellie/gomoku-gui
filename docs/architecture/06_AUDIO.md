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

A stone landing on wood is a short contact. It excites the board under it, and
the board answers with modes of its own. The model has three parts:

| Part | What it is | What it sounds like |
|---|---|---|
| Contact | A burst of noise, band passed around a centre frequency, with a rise and a decay | The click of the stone |
| Body | Two or three low modes, quickly damped | The board answering |
| Stone | One or two short modes above the body | The stone itself |

A `Voicing` says how much of each part there is, and three further things:

- **Glide.** Every mode can fall in pitch over its life, by a fraction of its
  frequency. A real contact spreads and settles as it lands, so the pitch drops.
  A mode that holds its pitch sounds synthetic, which is what the owner heard in
  the first version.
- **Spread.** Each mode is three partials a little apart, which beat against one
  another. One partial on its own rings like a tuning fork; a cluster sounds like
  a material.
- **Whisper.** A breath of noise that the wood radiates along with its modes.

Two more numbers shape the whole knock. The **tone** is a low pass over
everything, which is what "warm" and "dark" mean here, because wood eats the top
end. The **attack** is how long the knock takes to reach full strength: at zero
the knock is at its full level in its first sample, which is what a blow sounds
like, and a stone set down arrives over a few milliseconds.

A knock is normalised to about minus 3 dBFS, and both ends are faded. The fade at
the start is a tenth of a millisecond on purpose: the contact is the loudest part
of a knock and it is over in a millisecond or two, so a longer fade would ramp
away the very thing that makes it a knock.

### A sound can be a mix

A sound is one voicing, or several mixed, each at its own level. A part is
normalised before it is mixed, so a level is the share of the sound rather than
the strength of its numbers, and the mix is normalised again afterwards.

The sound the game plays is a mix of two, which are opposed:

| Part | Share | Character |
|---|---|---|
| Set down | 80% | Quiet, low, dark, arrives over two milliseconds |
| Ringing stone | 20% | Bright, holds its pitch, rings for thirty milliseconds |

The shares are written out rather than worked out from one another, so that what
the game plays is exactly the file that was listened to.

### Choosing the sound

The sound was chosen by ear, over several rounds, and the code that made it is
still there:

```sh
cargo run -p gomoku-gui -- --knock /tmp/knock   # writes the sounds and a page that plays them
```

`--knock` writes the sound in use and the two parts it is made from, together
with a table of measurements: how loud the contact is against the peak, when the
knock reaches its peak, where the energy sits, and how long it takes to fall 20
decibels. Those numbers are how a sound is compared without ears, and they are
how the difference between two candidates was checked before either was played.

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
when one or more of the four orthogonal neighbours is occupied, the response uses
`tau * 0.86` and loses 8 percent of its level. Variant selection walks the
variants in a shuffled order, so two identical placements in a row do not sound
identical.

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
