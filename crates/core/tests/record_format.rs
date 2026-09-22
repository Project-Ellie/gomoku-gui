//! File-level tests for the record format: the frozen golden file, a round
//! trip through the file system, and the damaged-file cases.

use std::path::PathBuf;

use engine::Move;
use gomoku_core::{Game, Record, RecordError, load, save};
use time::macros::datetime;

/// The golden file is frozen. A change to the schema changes these bytes,
/// and that must be a deliberate decision with a version bump.
const GOLDEN: &str = include_str!("golden/hotseat-v1.json");

fn golden_game() -> Game {
    let mut game = Game::started_at(datetime!(2026-09-22 18:04:11 UTC));
    game.set_players(Some("Wolfie".to_string()), Some("Anna".to_string()));
    for (row, col) in [(7, 7), (7, 8), (8, 8), (6, 6)] {
        game.play(Move::new(row, col).expect("inside the board"))
            .expect("the cell is empty");
    }
    game
}

#[test]
fn the_encoder_still_writes_the_golden_file() {
    let text = Record::from_game(&golden_game())
        .to_json()
        .expect("the record encodes");
    assert_eq!(text, GOLDEN, "the record schema changed");
}

#[test]
fn the_golden_file_loads_and_validates() {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/golden/hotseat-v1.json");
    let game = load(&path).expect("the golden file is a valid record");
    assert_eq!(game.len(), 4);
    assert_eq!(game.moves(), golden_game().moves());
    assert_eq!(game.meta().black.as_deref(), Some("Wolfie"));
}

#[test]
fn a_saved_game_loads_back_unchanged() {
    let directory = tempfile::tempdir().expect("a temporary directory");
    let path = directory.path().join("game.json");
    let game = golden_game();

    save(&path, &game).expect("the game saves");
    let loaded = load(&path).expect("the game loads");

    assert_eq!(loaded.moves(), game.moves());
    assert_eq!(loaded.cursor(), game.cursor());
    assert_eq!(loaded.meta(), game.meta());
}

#[test]
fn a_save_leaves_no_temporary_file_behind() {
    let directory = tempfile::tempdir().expect("a temporary directory");
    let path = directory.path().join("game.json");
    save(&path, &golden_game()).expect("the game saves");

    assert!(path.exists(), "the record file exists");
    let leftovers: Vec<_> = std::fs::read_dir(directory.path())
        .expect("the directory reads")
        .filter_map(Result::ok)
        .map(|entry| entry.file_name().to_string_lossy().into_owned())
        .filter(|name| name.ends_with(".tmp"))
        .collect();
    assert!(
        leftovers.is_empty(),
        "temporary files left behind: {leftovers:?}"
    );
}

#[test]
fn a_save_replaces_an_older_file() {
    let directory = tempfile::tempdir().expect("a temporary directory");
    let path = directory.path().join("game.json");
    let mut game = golden_game();
    save(&path, &game).expect("the first save");

    game.play(Move::new(10, 10).expect("inside the board"))
        .expect("the cell is empty");
    save(&path, &game).expect("the second save");

    let loaded = load(&path).expect("the game loads");
    assert_eq!(loaded.len(), 5);
}

#[test]
fn a_missing_file_is_an_io_error() {
    let directory = tempfile::tempdir().expect("a temporary directory");
    let path = directory.path().join("absent.json");
    let error = load(&path).expect_err("the file is absent");
    assert!(
        matches!(&error, RecordError::Io { path: failed, .. } if failed == &path),
        "the error must name the file that failed, got {error:?}"
    );
}

#[test]
fn a_damaged_file_is_a_parse_error() {
    let directory = tempfile::tempdir().expect("a temporary directory");
    let path = directory.path().join("damaged.json");
    std::fs::write(&path, "{\"format\": \"gomoku-gui/record\",").expect("the file writes");
    let error = load(&path).expect_err("the text is not a record");
    assert!(matches!(error, RecordError::Parse(_)));
}
