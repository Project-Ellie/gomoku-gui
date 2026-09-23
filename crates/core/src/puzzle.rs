//! Puzzle file loading — parser registry + `vcf-material-v1`.
//!
//! A puzzle data file may only be loaded if a sidecar file sits next to
//! it: for `foo.json` the sidecar is `foo.parser`. The sidecar's first
//! non-comment line names the parser that has been verified for that
//! format. This gate mirrors the project's re-adjudication rule: never
//! silently trust a downloaded format.

use std::fs;
use std::path::{Path, PathBuf};

use engine::{Color, Move};
use serde::Deserialize;

/// One puzzle position, independent of any on-disk format.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Puzzle {
    /// Display name, e.g. "vcf-material #42".
    pub name: String,
    /// Black stones in the starting position.
    pub black: Vec<Move>,
    /// White stones in the starting position.
    pub white: Vec<Move>,
    /// Side to move (the attacker in VCF terminology).
    pub to_move: Color,
    /// Claimed forced-win line, if the format provides one.
    pub solution: Option<Vec<Move>>,
    /// Claimed line depth, if the format provides one.
    pub depth: Option<u32>,
}

/// Everything that can go wrong when loading a puzzle file.
#[derive(Debug, thiserror::Error)]
pub enum PuzzleError {
    /// No `.parser` sidecar was found next to the data file.
    #[error(
        "missing parser sidecar: {path}\n       Create it with the verified parser identifier as the first non-comment line."
    )]
    MissingSidecar {
        /// Path to the expected sidecar file.
        path: PathBuf,
    },

    /// The sidecar named a parser that is not registered.
    #[error("unknown parser identifier '{identifier}' in sidecar {path}")]
    UnknownParser {
        /// Path to the sidecar file.
        path: PathBuf,
        /// The identifier that could not be resolved.
        identifier: String,
    },

    /// I/O failure reading the data or sidecar file.
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),

    /// The data file is not valid JSON for its registered parser.
    #[error("JSON error: {0}")]
    Json(#[from] serde_json::Error),

    /// A coordinate is outside the 15×15 board.
    #[error("puzzle {item}: coordinate ({row}, {col}) is off the board")]
    InvalidCoord {
        /// Puzzle index within the file.
        item: u32,
        /// Row from the file (0-based).
        row: u8,
        /// Column from the file (0-based).
        col: u8,
    },

    /// The same cell appears twice in one color's stone list.
    #[error("puzzle {item}: duplicate stone at ({row}, {col})")]
    DuplicateStone {
        /// Puzzle index within the file.
        item: u32,
        /// Row of the duplicated cell.
        row: u8,
        /// Column of the duplicated cell.
        col: u8,
    },

    /// A cell appears in both Black's and White's stone lists.
    #[error("puzzle {item}: black and white stones overlap")]
    OverlappingColors {
        /// Puzzle index within the file.
        item: u32,
    },

    /// Stone counts violate the Gomoku turn invariant.
    #[error("puzzle {item}: invalid counts (black={black}, white={white}, attacker={attacker:?})")]
    InvalidCounts {
        /// Puzzle index within the file.
        item: u32,
        /// Number of black stones.
        black: usize,
        /// Number of white stones.
        white: usize,
        /// Side to move according to the file.
        attacker: Color,
    },

    /// The top-level `count` field does not match `items.len()`.
    #[error("count mismatch: declared {declared}, actual {actual}")]
    CountMismatch {
        /// Value of the `count` field.
        declared: usize,
        /// Length of the `items` array.
        actual: usize,
    },

    /// The `a` (attacker) field is not "black" or "white".
    #[error("puzzle {item}: invalid attacker '{value}'")]
    InvalidAttacker {
        /// Puzzle index within the file.
        item: u32,
        /// Raw value from the file.
        value: String,
    },

    /// The first move of the claimed solution is not by the attacker.
    #[error("puzzle {item}: solution starts with side {found}, expected {expected:?}")]
    InvalidSolutionSide {
        /// Puzzle index within the file.
        item: u32,
        /// Expected side (the attacker).
        expected: Color,
        /// Side value found in the file.
        found: u8,
    },
}

/// Load all puzzles from `path`. Fails at the sidecar gate first, then
/// dispatches to the registered parser named in the sidecar.
pub fn load_puzzles(path: &Path) -> Result<Vec<Puzzle>, PuzzleError> {
    let sidecar = sidecar_path(path);
    if !sidecar.exists() {
        return Err(PuzzleError::MissingSidecar { path: sidecar });
    }

    let identifier = read_sidecar_identifier(&sidecar)?;
    let text = fs::read_to_string(path)?;

    match identifier.as_str() {
        "vcf-material-v1" => parse_vcf_material_v1(&text),
        other => Err(PuzzleError::UnknownParser {
            path: sidecar,
            identifier: other.to_string(),
        }),
    }
}

fn sidecar_path(path: &Path) -> PathBuf {
    let mut p = path.to_path_buf();
    p.set_extension("parser");
    p
}

fn read_sidecar_identifier(path: &Path) -> Result<String, PuzzleError> {
    let raw = fs::read_to_string(path)?;
    raw.lines()
        .map(str::trim)
        .find(|line| !line.is_empty() && !line.starts_with('#'))
        .map(|s| s.to_string())
        .ok_or_else(|| {
            // Empty or comment-only sidecar is treated as unknown.
            PuzzleError::UnknownParser {
                path: path.to_path_buf(),
                identifier: String::new(),
            }
        })
}

// ------------------------------------------------------------------
// vcf-material-v1

#[derive(Debug, Deserialize)]
struct VcfMaterialFile {
    #[allow(dead_code)]
    v: u32,
    count: usize,
    items: Vec<VcfMaterialItem>,
}

#[derive(Debug, Deserialize)]
struct VcfMaterialItem {
    i: u32,
    a: String,
    d: Option<u32>,
    #[serde(rename = "b")]
    black: Vec<[u8; 2]>,
    #[serde(rename = "w")]
    white: Vec<[u8; 2]>,
    #[serde(rename = "l")]
    line: Vec<[u8; 3]>,
}

fn parse_vcf_material_v1(text: &str) -> Result<Vec<Puzzle>, PuzzleError> {
    let file: VcfMaterialFile = serde_json::from_str(text)?;

    if file.count != file.items.len() {
        return Err(PuzzleError::CountMismatch {
            declared: file.count,
            actual: file.items.len(),
        });
    }

    file.items.into_iter().map(parse_vcf_item).collect()
}

fn parse_vcf_item(item: VcfMaterialItem) -> Result<Puzzle, PuzzleError> {
    let attacker = match item.a.as_str() {
        "black" => Color::Black,
        "white" => Color::White,
        other => {
            return Err(PuzzleError::InvalidAttacker {
                item: item.i,
                value: other.to_string(),
            });
        }
    };

    let black = parse_stone_list(item.i, &item.black)?;
    let white = parse_stone_list(item.i, &item.white)?;

    // No duplicates within a color and no overlap between colors.
    let mut seen = std::collections::HashSet::new();
    for mv in &black {
        if !seen.insert(mv.index()) {
            return Err(PuzzleError::DuplicateStone {
                item: item.i,
                row: mv.row(),
                col: mv.col(),
            });
        }
    }
    for mv in &white {
        if seen.contains(&mv.index()) {
            return Err(PuzzleError::OverlappingColors { item: item.i });
        }
        if !seen.insert(mv.index()) {
            return Err(PuzzleError::DuplicateStone {
                item: item.i,
                row: mv.row(),
                col: mv.col(),
            });
        }
    }

    // Count invariant: attacker is the side with the equal-or-fewer
    // stone count that is to move.
    let counts_ok = match attacker {
        Color::Black => black.len() == white.len(),
        Color::White => black.len() == white.len() + 1,
    };
    if !counts_ok {
        return Err(PuzzleError::InvalidCounts {
            item: item.i,
            black: black.len(),
            white: white.len(),
            attacker,
        });
    }

    let solution = if item.line.is_empty() {
        None
    } else {
        let first_side = item.line[0][2];
        let expected = match attacker {
            Color::Black => 1,
            Color::White => 2,
        };
        if first_side != expected {
            return Err(PuzzleError::InvalidSolutionSide {
                item: item.i,
                expected: attacker,
                found: first_side,
            });
        }
        let mut line = Vec::with_capacity(item.line.len());
        for &[x, y, _] in &item.line {
            let mv = Move::new(y, x).ok_or(PuzzleError::InvalidCoord {
                item: item.i,
                row: y,
                col: x,
            })?;
            line.push(mv);
        }
        Some(line)
    };

    Ok(Puzzle {
        name: format!("vcf-material #{}", item.i),
        black,
        white,
        to_move: attacker,
        solution,
        depth: item.d,
    })
}

fn parse_stone_list(item: u32, coords: &[[u8; 2]]) -> Result<Vec<Move>, PuzzleError> {
    coords
        .iter()
        .map(|&[x, y]| {
            Move::new(y, x).ok_or(PuzzleError::InvalidCoord {
                item,
                row: y,
                col: x,
            })
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicU64, Ordering};

    static COUNTER: AtomicU64 = AtomicU64::new(0);

    /// Unique temp directory per test, removed on drop. The process id
    /// is part of the name because the counter restarts in every test
    /// process while stale directories survive previous runs.
    struct TempDir(PathBuf);

    impl TempDir {
        fn new() -> Self {
            let n = COUNTER.fetch_add(1, Ordering::Relaxed);
            let dir = std::env::temp_dir().join(format!(
                "gomoku-core-puzzle-test-{}-{}",
                std::process::id(),
                n
            ));
            fs::create_dir_all(&dir).unwrap();
            Self(dir)
        }
    }

    impl Drop for TempDir {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    fn make_fixture() -> String {
        r#"{"v":1,"count":2,"items":[
            {"i":0,"a":"black","d":6,"u":0,"f":3,
             "b":[[3,7],[4,2],[4,3],[4,6],[5,2],[5,4],[5,5],[5,7],[6,5],[7,2],[7,7],[8,3],[8,4],[8,6]],
             "w":[[3,1],[3,2],[3,5],[4,4],[5,3],[6,3],[6,4],[6,6],[7,3],[7,4],[7,5],[8,2],[8,7],[9,4]],
             "l":[[2,8,1],[1,9,2],[4,7,1]]},
            {"i":2,"a":"white","d":5,"u":0,"f":0,
             "b":[[2,4],[2,7],[3,3],[3,12],[4,2]],
             "w":[[2,5],[3,4],[4,3],[4,5]],
             "l":[[2,3,2],[3,3,1],[4,4,2]]}
        ]}"#
            .to_string()
    }

    fn temp_pair() -> (TempDir, PathBuf, PathBuf) {
        let dir = TempDir::new();
        let data = dir.0.join("puzzles.json");
        let sidecar = dir.0.join("puzzles.parser");
        (dir, data, sidecar)
    }

    #[test]
    fn loads_two_puzzles_and_maps_coordinates() {
        let (_dir, data, sidecar) = temp_pair();
        fs::write(&data, make_fixture()).unwrap();
        fs::write(&sidecar, b"vcf-material-v1\n").unwrap();

        let puzzles = load_puzzles(&data).unwrap();
        assert_eq!(puzzles.len(), 2);

        let p0 = &puzzles[0];
        assert_eq!(p0.name, "vcf-material #0");
        assert_eq!(p0.to_move, Color::Black);
        assert_eq!(p0.depth, Some(6));
        assert_eq!(p0.black.len(), 14);
        assert_eq!(p0.white.len(), 14);
        // [3,7] -> col 3, row 7
        assert_eq!(p0.black[0], Move::new(7, 3).unwrap());
        // [2,8,1] in solution -> Move::new(8, 2)
        assert_eq!(p0.solution.as_ref().unwrap()[0], Move::new(8, 2).unwrap());

        let p1 = &puzzles[1];
        assert_eq!(p1.name, "vcf-material #2");
        assert_eq!(p1.to_move, Color::White);
        // black==white+1 for white attacker
        assert_eq!(p1.black.len(), 5);
        assert_eq!(p1.white.len(), 4);
    }

    #[test]
    fn missing_sidecar_is_clear() {
        let (_dir, data, _) = temp_pair();
        fs::write(&data, b"{}").unwrap();
        let err = load_puzzles(&data).unwrap_err();
        assert!(matches!(err, PuzzleError::MissingSidecar { .. }), "{err}");
        assert!(err.to_string().contains("missing parser sidecar"), "{err}");
    }

    #[test]
    fn unknown_identifier_is_rejected() {
        let (_dir, data, sidecar) = temp_pair();
        fs::write(&data, b"{}").unwrap();
        fs::write(&sidecar, b"not-a-parser\n").unwrap();
        let err = load_puzzles(&data).unwrap_err();
        assert!(matches!(err, PuzzleError::UnknownParser { .. }), "{err}");
    }

    #[test]
    fn count_mismatch_is_rejected() {
        let (_dir, data, sidecar) = temp_pair();
        fs::write(&data, br#"{"v":1,"count":99,"items":[]}"#).unwrap();
        fs::write(&sidecar, b"vcf-material-v1\n").unwrap();
        let err = load_puzzles(&data).unwrap_err();
        assert!(matches!(err, PuzzleError::CountMismatch { .. }), "{err}");
    }

    #[test]
    fn off_board_coord_is_rejected() {
        let (_dir, data, sidecar) = temp_pair();
        fs::write(
            &data,
            br#"{"v":1,"count":1,"items":[{"i":0,"a":"black","d":1,"u":0,"f":0,"b":[[15,0]],"w":[],"l":[]}]}"#,
        )
        .unwrap();
        fs::write(&sidecar, b"vcf-material-v1\n").unwrap();
        let err = load_puzzles(&data).unwrap_err();
        assert!(matches!(err, PuzzleError::InvalidCoord { .. }), "{err}");
    }

    #[test]
    fn duplicate_stone_is_rejected() {
        let (_dir, data, sidecar) = temp_pair();
        fs::write(
            &data,
            br#"{"v":1,"count":1,"items":[{"i":0,"a":"black","d":1,"u":0,"f":0,"b":[[7,7],[7,7]],"w":[],"l":[]}]}"#,
        )
        .unwrap();
        fs::write(&sidecar, b"vcf-material-v1\n").unwrap();
        let err = load_puzzles(&data).unwrap_err();
        assert!(matches!(err, PuzzleError::DuplicateStone { .. }), "{err}");
    }

    #[test]
    fn overlapping_colors_is_rejected() {
        let (_dir, data, sidecar) = temp_pair();
        fs::write(
            &data,
            br#"{"v":1,"count":1,"items":[{"i":0,"a":"black","d":1,"u":0,"f":0,"b":[[7,7]],"w":[[7,7]],"l":[]}]}"#,
        )
        .unwrap();
        fs::write(&sidecar, b"vcf-material-v1\n").unwrap();
        let err = load_puzzles(&data).unwrap_err();
        assert!(
            matches!(err, PuzzleError::OverlappingColors { .. }),
            "{err}"
        );
    }

    #[test]
    fn invalid_counts_for_attacker_is_rejected() {
        let (_dir, data, sidecar) = temp_pair();
        // Black attacker requires black == white.
        fs::write(
            &data,
            br#"{"v":1,"count":1,"items":[{"i":0,"a":"black","d":1,"u":0,"f":0,"b":[[7,7],[6,6]],"w":[[5,5]],"l":[]}]}"#,
        )
        .unwrap();
        fs::write(&sidecar, b"vcf-material-v1\n").unwrap();
        let err = load_puzzles(&data).unwrap_err();
        assert!(matches!(err, PuzzleError::InvalidCounts { .. }), "{err}");
    }

    #[test]
    fn invalid_solution_start_side_is_rejected() {
        let (_dir, data, sidecar) = temp_pair();
        // Black attacker but solution starts with side 2 (White).
        fs::write(
            &data,
            br#"{"v":1,"count":1,"items":[{"i":0,"a":"black","d":1,"u":0,"f":0,"b":[],"w":[],"l":[[7,7,2]]}]}"#,
        )
        .unwrap();
        fs::write(&sidecar, b"vcf-material-v1\n").unwrap();
        let err = load_puzzles(&data).unwrap_err();
        assert!(
            matches!(err, PuzzleError::InvalidSolutionSide { .. }),
            "{err}"
        );
    }
}
