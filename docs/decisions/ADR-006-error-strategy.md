# ADR-006: thiserror in the core crate, anyhow in the binary

## Status

Accepted (owner-approved, 2026-09-22)

## Context

The core crate has three error families: a rejected placement, a rejected
record, and a rejected config. Each has a small, closed set of causes that the
caller must distinguish, and each cause has a message written for the user.

The binary has one error path: something failed, add the context, tell the
user, and continue or exit.

## Decision

- `gomoku-core` defines `GameError`, `RecordError`, and `ConfigError` with
  `thiserror`. Each implements `Display`, `Error`, `Debug`, `PartialEq`, and
  `Eq`, so tests compare errors directly.
- `gomoku-gui` uses `anyhow` with `.context()` at every propagation, and reports
  at the boundary: a log line plus a dialog for a user-visible failure, or a
  non-zero exit for a failure during start-up.
- No `unwrap` and no `expect` in a production path. The only exception is a
  proven invariant with a comment, for example indexing a fixed array of
  presets.
- `Display` messages are written for the user and are safe to show in a dialog.
  Internal detail goes into `Debug` and the log.
- Every io error is wrapped with the path that produced it, so the message
  names the file.

## Consequences

- The caller can decide what to do from the error variant, which the user
  interface needs in order to offer the right action.
- A test can assert `Err(RecordError::UnsupportedVersion(2))` instead of a
  string match.
- The binary stays free of error-mapping boilerplate.
- Error messages are part of the user-visible surface and are written in plain
  language, not in Rust vocabulary.
