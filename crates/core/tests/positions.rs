//! Regression coverage for imported roots at the public game boundary.

use gomoku_core::{Color, Game, Outcome, PositionError, Status, point};

#[test]
fn repeated_stones_within_either_color_are_rejected() {
    let a = point(7, 7).unwrap();
    let b = point(6, 7).unwrap();
    for (black, white, side) in [
        (vec![a, a], vec![b], Color::White),
        (vec![a], vec![b, b], Color::Black),
    ] {
        assert_eq!(
            Game::from_position(black, white, side),
            Err(PositionError::DuplicateStone)
        );
    }
}

#[test]
fn a_cell_shared_by_both_colors_is_overlap_not_a_duplicate() {
    let a = point(7, 7).unwrap();
    assert_eq!(
        Game::from_position(vec![a], vec![a], Color::Black),
        Err(PositionError::OverlappingColors)
    );
}

#[test]
fn invalid_counts_and_side_to_move_are_mapped_to_core_errors() {
    let a = point(7, 7).unwrap();
    let b = point(6, 7).unwrap();
    for (black, white, side) in [
        (vec![a, b], vec![], Color::White),
        (vec![], vec![a, b], Color::Black),
        (vec![a], vec![b], Color::White),
        (vec![a], vec![], Color::Black),
    ] {
        assert_eq!(
            Game::from_position(black, white, side),
            Err(PositionError::InvalidCounts)
        );
    }
}

#[test]
fn snapshot_is_an_independent_board_at_the_cursor_with_the_imported_root() {
    let black = vec![point(7, 7).unwrap(), point(7, 8).unwrap()];
    let white = vec![point(6, 7).unwrap()];
    let root = engine::Board::from_position(&black, &white, Color::White).unwrap();
    let mut game = Game::from_position(black, white, Color::White).unwrap();
    let first = point(5, 7).unwrap();
    game.play(first).unwrap();
    game.play(point(8, 7).unwrap()).unwrap();
    game.rewind();

    let mut expected = root.clone();
    expected.play(first).unwrap();
    let mut snapshot = game.board_snapshot();
    assert_eq!(
        snapshot, expected,
        "includes root, side, history and Zobrist"
    );
    let before = game.clone();
    snapshot.play(point(1, 1).unwrap()).unwrap();
    assert_eq!(
        game, before,
        "mutating a detached snapshot cannot change Game"
    );
    game.seek(0);
    assert_eq!(
        game.board_snapshot(),
        root,
        "seek preserves the puzzle root"
    );
    assert_eq!(
        snapshot.moves().len(),
        2,
        "later Game changes are independent"
    );
}

#[test]
fn imported_nearly_full_root_draws_rewinds_replays_and_undoes() {
    // This pattern has no five in any direction, including after the last
    // placement. Root stones are not part of the engine's move history.
    let last = point(0, 0).unwrap();
    let mut black = Vec::new();
    let mut white = Vec::new();
    for row in 0..15 {
        for col in 0..15 {
            let mv = point(row, col).unwrap();
            if mv == last {
                continue;
            }
            if (row + 2 * col) % 4 < 2 {
                black.push(mv);
            } else {
                white.push(mv);
            }
        }
    }
    assert_eq!((black.len(), white.len()), (112, 112));
    let mut game = Game::from_position(black, white, Color::Black).unwrap();
    let root_stones: Vec<_> = game.stones().collect();
    assert_eq!(game.status(), Status::Ongoing);
    assert!(!game.undo());

    game.play(last).unwrap();
    assert_eq!(game.status(), Status::Draw);
    assert_eq!(game.outcome(), Outcome::Draw);
    assert_eq!(game.stones().count(), 225);
    assert_eq!(game.moves(), &[last]);

    assert!(game.rewind());
    assert_eq!(game.status(), Status::Ongoing);
    assert_eq!(game.outcome(), Outcome::Draw, "review keeps the result");
    assert_eq!(game.stones().collect::<Vec<_>>(), root_stones);
    assert!(!game.rewind());
    assert!(game.forward(), "review redo replays the final move");
    assert_eq!(game.status(), Status::Draw);

    assert!(game.undo(), "undo deletes back to the imported root");
    assert_eq!(game.status(), Status::Ongoing);
    assert_eq!(game.outcome(), Outcome::Ongoing);
    assert_eq!(game.to_move(), Color::Black);
    assert!(game.is_empty());
    assert_eq!(game.stones().collect::<Vec<_>>(), root_stones);
    assert!(!game.undo());
    assert!(!game.forward(), "the deleted move has no redo");
    game.play(last).unwrap();
    assert_eq!(game.status(), Status::Draw);
}
