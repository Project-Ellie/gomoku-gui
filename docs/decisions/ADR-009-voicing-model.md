# ADR-009: A knock is a voicing of three parts, and a sound is a mix of voicings

## Status

Accepted (owner-approved, 2026-09-22). Refines
[ADR-007](ADR-007-audio.md).

## Context

The first knock was four damped sine modes at 420, 1150, 2600, and 5400 Hz, with
decays from 55 down to 9 milliseconds. The owner heard it as an empty tin bucket:
four pure tones that ring for tens of milliseconds are *pitched*, and a pitched
ring sounds like a hollow vessel.

Choosing a replacement by description did not work. "Warm", "dark", and "softer"
each mean several different things to the ear, and the owner could not say which
of them he wanted until he heard it. Four rounds of listening followed, with 6, 20,
10, and 7 sounds in turn.

The owner's own observation moved the work forward more than any of my guesses:
the frequency of the first version does not change over the life of the knock,
and in real wood it would.

## Decision

Model a knock as three parts of a `Voicing`:

- the **contact**, a burst of band-passed noise with a rise and a decay,
- the **body** of the board, a few low modes,
- the **stone**, one or two short modes above the body,

with three further numbers that make it sound like a material rather than a bell:
a **glide** (the modes fall in pitch as the contact settles), a **spread** (each
mode is three partials a little apart, which beat), and a **whisper** (the noise
the wood radiates with its modes). Two numbers shape the whole knock: the **tone**
(a low pass, which is what dark means) and the **attack** (how long the knock
takes to reach full strength, which is the difference between a blow and a stone
set down).

A sound is one voicing or a **mix** of several with levels. A part is normalised
before it is mixed, so a level is the share of the sound rather than the strength
of its numbers.

The sound in use is 80 percent of a stone set down and 20 percent of a stone that
rings, which the owner picked out of the last round.

## Consequences

- The sound is chosen by ear with `--knock`, which renders the sounds and the
  measurements beside them. The measurements make a difference checkable without
  ears, but they do not choose: three attempts at measuring "how hard the contact
  is" were needed before the number agreed with the owner's ear.
- The final sound is two voicings and their shares, which is a few lines of
  numbers rather than a recording. No asset file, no licence, and no sample
  replayed for 200 moves.
- The audition sets that led to it are removed from the code. The sound in use,
  the two parts it is made of, and the tool that renders them remain, and the
  history of the rounds is in the commits.
- The model has eight numbers plus the modes. That is more than four frequencies,
  and it is what it cost to stop sounding like a bell.
