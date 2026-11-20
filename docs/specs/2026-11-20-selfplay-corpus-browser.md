# Self-play corpus browser — design

2026-11-20 · status: agreed (Wolfie), implementation in slices C1–C3

## Goal

While self-play corpora generate in rust-ml, Wolfie wants to watch finished
games in the gomoku-gui board: game-by-game browsing, the corpus's manifest
parameters visible, and — the heart of it — the **recorded** per-ply search
data (visits, post-noise priors, root value) rendered as overlays, so he can
see how the generating seat (e.g. `first-corpus` net) actually decided.

## Problem statement

The Lab Explorer's "corpus" is a directory of individual GUI-format record
files (JSON tree records, ADR-003/004). rust-ml self-play corpora are
`manifest.json` + `shard-*.bin` (length-delimited bincode `GameRecord`s) —
the Explorer cannot open them at all. And even for loadable records, the
policy overlay is always computed **live** from the board snapshot
(`session.rs::refresh_policy_rings`, baseline evaluator), while the data
that matters for understanding a self-play game — the visits and
*Dirichlet-noised* root priors the generating seat actually saw — sits
unused in the record. The noised prior is not reproducible after the fact;
live recomputation is both wrong and (at net sims) glacial.

## Decisions

- **D1 — read selfplay corpora natively.** `gomoku-core` gains a `selfplay`
  dependency (same git pin as `neteval`/`baseline`). Rejected alternative:
  an export converter to GUI JSON records — the GUI record format has no
  slots for visits/priors/root value, so conversion would lose exactly the
  data this feature exists for, plus an extra manual step per corpus.
- **D2 — eager parse on open.** Opening a selfplay corpus parses the
  manifest and all shards in one read. At 100–1k games this is trivial;
  revisit if corpora reach ~10k games.
- **D3 — search data rides a side channel.** Loading a selfplay game puts
  the opening + moves on the board as today; the per-ply search data
  (visits, top-16 prior, root value, search-ran flag), the certified
  suffix, and the outcome are stashed on the session parallel to the main
  line. The GUI record tree format (v4) is untouched; selfplay corpora are
  read-only in the GUI.
- **D4 — recorded data wins.** When the viewed node has recorded search
  data, the policy overlay rings render the recorded top-16 prior and the
  headline value is the recorded root value (side-to-move's perspective,
  matching the existing convention). Live baseline computation stays the
  fallback for ordinary games and for plies without search data.
- **D5 — decision badges in the move list.** Plies decided without search
  (own-TSS proof, guard, resignation admission — `search: None`), the start
  of the certified forced-win suffix, and resignations get visible badges.
- **D6 — manifest panel.** The Explorer section gains a header block for
  selfplay corpora: seat pairing (evaluator descriptors + sims), the
  exploration schedule, game count, seed range, workspace version.

## Slices

- **C1 (core):** probe + parse selfplay corpora (`manifest.json` +
  `shard-*.bin`), explorer listing with meaningful names
  (`seed 42 · 67 plies · Black wins`), game loading with the per-ply
  search-data side channel.
- **C2 (gui):** manifest panel in the Explorer section.
- **C3 (gui):** recorded-data overlays (prior rings + headline value) and
  move-list badges.

## Verification

- C1: unit tests with a fabricated corpus fixture (written via
  `selfplay::corpus::write_corpus` in dev-dependencies); open → list →
  load → side channel populated.
- C2/C3: session-level tests assert the rendered state for a fixture
  record; existing gates (`cargo test`, `clippy --locked -D warnings`,
  `fmt`).
- Wolfie's manual acceptance: browse the real `net-vs-net-400` corpus while
  it generates (read-only — safe), check the manifest panel, step through a
  game watching recorded priors/values, badges on proved/resigned plies.
