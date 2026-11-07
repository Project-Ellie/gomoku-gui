//! File-level tests for versioned records, persistence and damaged-file cases.

mod common;

use std::path::PathBuf;

use engine::Move;
use gomoku_core::{
    Game, Outcome, Record, RecordError, StoredColor, StoredMethod, StoredOutcome, WinMethod, load,
    save,
};
use time::macros::datetime;

/// The v1 fixture stays frozen. New writes are pinned separately to v2.
const GOLDEN_V1: &str = include_str!("golden/hotseat-v1.json");
const GOLDEN_V2: &str = include_str!("golden/resignation-v2.json");

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
fn the_encoder_writes_the_v2_resignation_fixture() {
    let mut game = golden_game();
    game.resign().unwrap();
    let text = Record::from_game(&game)
        .expect("the record builds")
        .to_json()
        .expect("the record encodes");
    assert_eq!(text, GOLDEN_V2, "the record schema changed");
}

#[test]
fn both_golden_files_load_and_validate() {
    for (fixture, text) in [
        ("hotseat-v1.json", GOLDEN_V1),
        ("resignation-v2.json", GOLDEN_V2),
    ] {
        let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("tests/golden")
            .join(fixture);
        let game = load(&path).expect("the golden file is a valid record");
        let record = Record::from_json(text).unwrap();
        assert_eq!(
            record.to_json().unwrap(),
            text,
            "frozen wire representation"
        );
        let mut expected = golden_game();
        if record.version == 2 {
            expected.resign().unwrap();
        }
        assert_eq!(game, expected);
        assert_eq!(Record::from_game(&game).unwrap().version, 2);
    }
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
fn saving_a_reviewed_resignation_keeps_the_whole_game_and_loads_finished_live() {
    for count in 0..=4 {
        let directory = tempfile::tempdir().expect("a temporary directory");
        let path = directory.path().join("resigned.json");
        let mut game = golden_game();
        while game.len() > count {
            game.undo();
        }
        game.resign().unwrap();
        let live = game.clone();
        game.seek(0);
        save(&path, &game).unwrap();
        let text = std::fs::read_to_string(&path).unwrap();
        let record = Record::from_json(&text).unwrap();
        assert_eq!(record.version, 2, "all new records use v2");
        let loaded = load(&path).unwrap();
        assert_eq!(
            loaded, live,
            "all history, names, timestamp and result round-trip"
        );
        assert!(!loaded.is_rewound());
        assert!(!loaded.can_play());
        assert!(matches!(
            loaded.outcome(),
            Outcome::Won {
                method: WinMethod::Resignation,
                ..
            }
        ));
    }
}

#[test]
fn v1_cannot_claim_a_resignation() {
    let mut game = golden_game();
    game.resign().unwrap();
    let mut record = Record::from_game(&game).unwrap();
    record.version = 1;
    let parsed = Record::from_json(&record.to_json().unwrap()).unwrap();
    assert!(matches!(
        parsed.into_game(),
        Err(RecordError::ResultMismatch)
    ));
}

#[test]
fn versions_other_than_v1_or_v2_are_rejected() {
    for version in [0, 3, u32::MAX] {
        let mut record = Record::from_game(&golden_game()).unwrap();
        record.version = version;
        let parsed = Record::from_json(&record.to_json().unwrap()).unwrap();
        assert!(
            matches!(parsed.into_game(), Err(RecordError::UnsupportedVersion(found)) if found == version)
        );
    }
}

#[test]
fn resignation_winner_must_be_opposite_the_replayed_side_to_move() {
    for count in 0..=4 {
        let mut game = golden_game();
        while game.len() > count {
            game.undo();
        }
        let mut record = Record::from_game(&game).unwrap();
        record.result = StoredOutcome::Won {
            winner: game.to_move().into(),
            method: StoredMethod::Resignation,
        };
        let parsed = Record::from_json(&record.to_json().unwrap()).unwrap();
        assert!(matches!(
            parsed.into_game(),
            Err(RecordError::ResultMismatch)
        ));
    }
}

#[test]
fn a_terminal_board_cannot_claim_resignation_for_either_winner() {
    for game in [common::won_game(), common::drawn_game()] {
        for winner in [StoredColor::Black, StoredColor::White] {
            let mut record = Record::from_game(&game).unwrap();
            record.result = StoredOutcome::Won {
                winner,
                method: StoredMethod::Resignation,
            };
            let parsed = Record::from_json(&record.to_json().unwrap()).unwrap();
            assert!(matches!(
                parsed.into_game(),
                Err(RecordError::ResultMismatch)
            ));
        }
    }
}

#[test]
fn both_versions_read_ongoing_five_and_draw_results() {
    for game in [golden_game(), common::won_game(), common::drawn_game()] {
        for version in [1, 2] {
            let mut record = Record::from_game(&game).unwrap();
            assert_eq!(record.version, 2, "new records always use v2");
            record.version = version;
            let loaded = Record::from_json(&record.to_json().unwrap())
                .unwrap()
                .into_game()
                .unwrap();
            assert_eq!(loaded, game);
        }
    }
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

#[test]
fn a_from_position_game_refuses_to_save() {
    let black = vec![Move::new(7, 7).expect("inside the board")];
    let white = vec![Move::new(6, 7).expect("inside the board")];
    let game = Game::from_position(black, white, engine::Color::Black).expect("valid position");
    let mut resigned = game.clone();
    resigned.resign().unwrap();
    for game in [game, resigned] {
        assert!(
            matches!(Record::from_game(&game), Err(RecordError::FromPosition)),
            "from-position games must not round-trip through the moves-only format"
        );

        let directory = tempfile::tempdir().expect("a temporary directory");
        let path = directory.path().join("puzzle.json");
        assert!(
            matches!(save(&path, &game), Err(RecordError::FromPosition)),
            "the file path must also be refused"
        );
        assert!(!path.exists(), "no file is written");
    }
}
