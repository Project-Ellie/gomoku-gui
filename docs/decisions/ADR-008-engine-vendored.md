# ADR-008: The rules engine is a copy, not a dependency

## Status

Accepted (owner-approved, 2026-09-22). Supersedes
[ADR-002](ADR-002-engine-reuse.md).

## Context

ADR-002 made the Gomoku rules engine a path dependency on a sibling checkout,
`../rust-ml/gomoku/crates/engine`. That decision kept one source of truth for
the rules, but it made this repository unbuildable on its own: a clone without
the sibling fails at `cargo build`.

The owner has decided that this repository must stand alone. He also stated that
he does not intend to develop the interface further, so the engine needs no
channel for future changes.

Three forms were considered:

1. A path dependency on a sibling checkout. Rejected: the repository does not
   build on its own.
2. A git dependency on `Project-Ellie/rust-ml`, pinned to a revision. This was
   tested and it works: Cargo finds the crate inside the nested workspace at
   `gomoku/Cargo.toml`. Rejected: it still depends on another repository of the
   owner's, and the engine would not build if that repository changes or moves.
3. A copy of the crate inside this repository.

## Decision

Copy the whole engine crate to `crates/engine`, make it a member of this
workspace, and depend on it by path inside the repository.

The whole crate is copied, not only the parts the interface uses. The crate is
about 4,000 lines and carries its own property-test differential suite, its
reference implementation, and its benchmarks. Trimming it would remove the tests
that make the rules trustworthy, for no gain in a repository that already builds
in fifteen seconds.

The copy is a snapshot of the engine as it stands in the rust-ml working tree,
which is ahead of that repository's last commit. The rules code that this
interface has been using since the first run is that working-tree version.

## Consequences

- The repository builds and tests on its own. Verified by copying the repository
  without the sibling and running `cargo check --workspace --offline`.
- The engine's own suite runs here: `cargo test -p engine --features testutil`
  runs the differential tests against the reference implementation.
- The engine is now ours to maintain. An engine fix in rust-ml will not arrive
  here by itself, and a fix made here will not reach rust-ml. The owner accepts
  this because the interface is finished.
- `crates/core/src/game.rs` still calls the engine through one module, as
  ADR-002 set out. The facade is unchanged; only the source of the crate moved.
- The engine keeps its tests, benches, and the `testutil` feature, so its
  benchmark harness is available: `cargo bench -p engine`.
