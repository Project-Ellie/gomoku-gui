//! Tests for the Piskvorky psq loader.

use std::path::PathBuf;

use gomoku_core::{Color, Confidence, Format, LoadError, RecordUnit, Registry, point};

fn fixture(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/psq")
        .join(name)
}

#[test]
fn psq_format_sniffs_by_extension_and_header() {
    let registry = Registry::builder().add(gomoku_core::PsqFormat).build();
    let yes = registry.sniff_path(PathBuf::from("game.psq").as_path(), b"Piskvorky 15x15");
    assert_eq!(yes, Confidence::Yes);
    let no = registry.sniff_path(PathBuf::from("game.txt").as_path(), b"not a psq");
    assert_eq!(no, Confidence::No);
}

#[test]
fn tiny_psq_loads_as_a_game() {
    let format = gomoku_core::PsqFormat;
    let units = format
        .load(&fixture("tiny.psq"))
        .expect("tiny fixture loads");
    assert_eq!(units.len(), 1);
    let RecordUnit::Game(game) = &units[0] else {
        panic!("expected a game record");
    };
    assert_eq!(game.len(), 5);
    assert_eq!(game.meta().black.as_deref(), Some("BlackPlayer"));
    assert_eq!(game.meta().white.as_deref(), Some("WhitePlayer"));
    // The psq x is the column and y is the row, both 1-based.
    // A diagonal move: psq (8,8) is row 7, col 7, played by Black.
    assert_eq!(
        game.stone_at(point(7, 7).expect("inside the board")),
        Some(Color::Black)
    );
    // psq (7,7) is row 6, col 6, played by White.
    assert_eq!(
        game.stone_at(point(6, 6).expect("inside the board")),
        Some(Color::White)
    );
    // The off-diagonal move: psq (10,7) is row 6, col 9, played by Black.
    // A transposed mapping would place it at row 9, col 6 instead.
    assert_eq!(
        game.stone_at(point(6, 9).expect("inside the board")),
        Some(Color::Black)
    );
    assert_eq!(game.stone_at(point(9, 6).expect("inside the board")), None);
}

#[test]
fn corrupt_psq_reports_the_offending_line() {
    let format = gomoku_core::PsqFormat;
    let err = format
        .load(&fixture("bad-move.psq"))
        .expect_err("bad move fails");
    assert!(matches!(err, LoadError::Parse { .. }), "got {err}");
    // The duplicate (8,8) is the third move line, index 2.
    assert!(err.to_string().contains("move 2"), "got {err}");
}
