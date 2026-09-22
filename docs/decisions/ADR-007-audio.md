# ADR-007: The stone knock is synthesised at start-up and played through cpal

## Status

Accepted (owner-approved, 2026-09-22)

## Context

The owner asked for the sound of a stone placed on a solid wooden board. The
game has no captures, so that is the only sound.

| Option | Assessment |
|---|---|
| Bundle a recorded sample | Best realism. Needs an asset file, a licence that permits redistribution, and the sample still sounds identical on every move. |
| Synthesise in the audio callback | Flexible, but a callback must not call `sin`, allocate, or lock. |
| Synthesise at start-up, play from a buffer | No per-callback synthesis, no asset, full control of the model. |
| `rodio` | A small player layer, but its job is decoding and mixing files, which we do not need. |

## Decision

- One sound, synthesised on the CPU at start-up: a contact, the body of the
  board, and the stone, rendered into six fixed buffers (one default, three
  damped variants for occupied neighbours, two extra default variants). The model
  is described in [06_AUDIO](../architecture/06_AUDIO.md) and
  [ADR-009](ADR-009-voicing-model.md).
- Audio output goes through `cpal` directly, not `rodio`, because we generate
  samples rather than decode files.
- The audio callback only mixes from a pre-rendered buffer with a moving
  playhead. It does not allocate, does not lock, does not log, and does not
  call a transcendental function.
- Voice hand-off from the user-interface thread uses a fixed 16-slot lock-free
  ring of atomics. A full queue drops a click instead of blocking.
- The sound is chosen by ear, not by a setting: `--knock` renders the candidates
  and the measurements beside them.
- A missing or lost audio device disables sound, logs one warning, and never
  stops the application or shows a dialog.

## Consequences

- No asset file, no licence question, and per-move variation, so 200 moves do
  not sound like one sample replayed 200 times.
- A damped-board variant is a few lines of parameter change, and it is audible
  in a real way.
- A real recording still sounds better than a model. The mitigation is that the
  audio layer exposes `play_click(near_neighbour: bool)` as its entire
  interface, so swapping in a sample later touches one module.
- The synthesis is a pure function and is tested without an audio device.
