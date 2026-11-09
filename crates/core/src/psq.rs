//! Piskvorky psq game-record loader.
//!
//! Format: a header line starting with `Piskvorky 15x15`, then one move per
//! line as `x,y,time_millis` with 1-based coordinates (x is column, y is row,
//! (1,1) at the top-left). Trailing metadata lines are ignored.

use std::path::Path;

use engine::Move;

use crate::format::{Confidence, Format, LoadError, RecordUnit};
use crate::game::Game;

/// Loader for Piskvorky psq files.
#[derive(Debug, Clone, Copy)]
pub struct PsqFormat;

impl Format for PsqFormat {
    fn name(&self) -> &'static str {
        "psq"
    }

    fn sniff(&self, path: &Path, head: &[u8]) -> Confidence {
        let extension_ok = path
            .extension()
            .and_then(|e| e.to_str())
            .map(|e| e.eq_ignore_ascii_case("psq"))
            .unwrap_or(false);
        let header_ok = head.starts_with(b"Piskvorky");
        if extension_ok && header_ok {
            Confidence::Yes
        } else if extension_ok || header_ok {
            Confidence::Maybe
        } else {
            Confidence::No
        }
    }

    fn load(&self, path: &Path) -> Result<Vec<RecordUnit>, LoadError> {
        let text = std::fs::read_to_string(path).map_err(|e| LoadError::Io {
            path: path.to_path_buf(),
            source: e,
        })?;
        let mut lines = text.lines();
        let header = lines.next().ok_or_else(|| LoadError::Parse {
            path: path.to_path_buf(),
            reason: "empty file".to_string(),
        })?;
        if !header.starts_with("Piskvorky") {
            return Err(LoadError::Parse {
                path: path.to_path_buf(),
                reason: "missing Piskvorky header".to_string(),
            });
        }

        let mut game = Game::new();
        let mut move_lines: Vec<&str> = Vec::new();
        let mut black_name: Option<String> = None;
        let mut white_name: Option<String> = None;

        for raw in lines {
            let line = raw.trim();
            if line.is_empty() || line == "-1" {
                continue;
            }
            // A metadata line has no third coordinate.
            let parts: Vec<&str> = line.split(',').collect();
            match parts.len() {
                1 => {
                    // Player names appear as two single metadata lines.
                    if black_name.is_none() {
                        black_name = Some(line.to_string());
                    } else if white_name.is_none() {
                        white_name = Some(line.to_string());
                    }
                }
                3 => {
                    move_lines.push(line);
                }
                _ => {
                    // Ignore other metadata lines such as `1,Freestyle15_2`.
                }
            }
        }

        for (index, line) in move_lines.iter().enumerate() {
            let parts: Vec<&str> = line.split(',').collect();
            let x: u8 = parts[0].parse().map_err(|e| LoadError::Parse {
                path: path.to_path_buf(),
                reason: format!("move {index}: bad x: {e}"),
            })?;
            let y: u8 = parts[1].parse().map_err(|e| LoadError::Parse {
                path: path.to_path_buf(),
                reason: format!("move {index}: bad y: {e}"),
            })?;
            if x == 0 || x > 15 || y == 0 || y > 15 {
                return Err(LoadError::Parse {
                    path: path.to_path_buf(),
                    reason: format!("move {index}: ({x},{y}) off board"),
                });
            }
            // Invariant: x and y are in 1..=15 by the check above, so
            // x - 1 and y - 1 are in 0..=14, inside the 15x15 board.
            let mv = Move::new(y - 1, x - 1).expect("coordinates are inside the board");
            if let Err(e) = game.play(mv) {
                return Err(LoadError::Parse {
                    path: path.to_path_buf(),
                    reason: format!("move {index}: {e}"),
                });
            }
        }

        game.set_players(black_name, white_name);
        Ok(vec![RecordUnit::Game(game)])
    }
}
