//! Property tests for the invariants of `Game`.
//!
//! I1 cursor <= len
//! I2 the board equals a replay of moves[..cursor]
//! I3 the move list has no repeat and no point off the board
//! I4 the board holds exactly `cursor` moves
//! I5 the side to move follows the cursor parity
//! I6 the outcome follows the board at the latest position

use engine::{Board, Move, Status};
use gomoku_core::{Game, GameError, Outcome};
use proptest::prelude::*;

/// One step of a random walk through the public interface.
#[derive(Debug, Clone, Copy)]
enum Step {
    Play(Move),
    Undo,
    Rewind,
    Forward,
    Seek(u8),
    Live,
}

fn any_move() -> impl Strategy<Value = Move> {
    (0u8..15, 0u8..15).prop_map(|(row, col)| Move::new(row, col).expect("inside the board"))
}

fn any_step() -> impl Strategy<Value = Step> {
    prop_oneof![
        6 => any_move().prop_map(Step::Play),
        1 => Just(Step::Undo),
        1 => Just(Step::Rewind),
        1 => Just(Step::Forward),
        1 => (0u8..230).prop_map(Step::Seek),
        1 => Just(Step::Live),
    ]
}

fn apply(game: &mut Game, step: Step) {
    match step {
        Step::Play(point) => match game.play(point) {
            // A random placement can be refused: the cell may hold a stone,
            // or the board at the cursor may be finished. Both are valid.
            Ok(())
            | Err(GameError::Occupied)
            | Err(GameError::GameOver)
            | Err(GameError::Rejected(_)) => {}
        },
        Step::Undo => {
            game.undo();
        }
        Step::Rewind => {
            game.rewind();
        }
        Step::Forward => {
            game.forward();
        }
        Step::Seek(index) => {
            game.seek(usize::from(index));
        }
        Step::Live => {
            game.live();
        }
    }
}

/// A fresh board with the first `count` moves of `game` replayed.
fn replay(game: &Game, count: usize) -> Board {
    let mut board = Board::new();
    for mv in &game.moves()[..count] {
        board.play(*mv).expect("a recorded move replays");
    }
    board
}

/// The outcome that the board at the latest position implies.
fn expected_outcome(game: &Game) -> Outcome {
    match replay(game, game.len()).status() {
        Status::Ongoing => Outcome::Ongoing,
        Status::Won(winner) => Outcome::Won {
            winner,
            method: gomoku_core::WinMethod::Five,
        },
        Status::Draw => Outcome::Draw,
    }
}

fn check(game: &Game) {
    assert!(
        game.cursor() <= game.len(),
        "I1: the cursor is inside the move list"
    );
    assert_eq!(
        game.pending_truncation(),
        game.len() - game.cursor(),
        "the truncation count is the length of the tail"
    );

    // I2: the visible board is the replay of the first `cursor` moves.
    let visible = replay(game, game.cursor());
    for row in 0..15u8 {
        for col in 0..15u8 {
            let point = Move::new(row, col).expect("row and col are inside the board");
            assert_eq!(
                game.stone_at(point),
                visible.stone_at(point),
                "I2: the visible board differs at row {row} column {col}"
            );
        }
    }
    assert_eq!(
        game.status(),
        visible.status(),
        "I2: the status of the replay"
    );
    assert_eq!(
        game.to_move(),
        visible.to_move(),
        "I5: the side to move of the replay"
    );

    // I6: the outcome describes the game, not the view.
    assert_eq!(
        game.outcome(),
        expected_outcome(game),
        "I6: the outcome of the game"
    );

    // I3: the move list has no repeat.
    let mut seen = Vec::with_capacity(game.len());
    for mv in game.moves() {
        assert!(!seen.contains(mv), "I3: no repeated point");
        seen.push(*mv);
    }
}

proptest! {
    #[test]
    fn every_reachable_state_holds_the_invariants(steps in prop::collection::vec(any_step(), 0..60)) {
        let mut game = Game::new();
        check(&game);
        for step in steps {
            apply(&mut game, step);
            check(&game);
        }
    }

    #[test]
    fn rewind_then_forward_restores_the_same_position(steps in prop::collection::vec(any_step(), 0..40)) {
        let mut game = Game::new();
        for step in steps {
            apply(&mut game, step);
        }
        // Forward stops at the latest move, so the cycle restores the live
        // position. Normalize the snapshot: `apply` can leave a rewound view.
        let mut live_before = game.clone();
        live_before.live();
        while game.rewind() {}
        while game.forward() {}
        prop_assert_eq!(game, live_before);
    }

    #[test]
    fn play_while_rewound_truncates_the_tail(steps in prop::collection::vec(any_step(), 0..40), point in any_move()) {
        let mut game = Game::new();
        for step in steps {
            apply(&mut game, step);
        }
        let cursor = game.cursor();
        if game.stone_at(point).is_none() && game.status() == Status::Ongoing && game.play(point).is_ok() {
            prop_assert_eq!(game.len(), cursor + 1);
            prop_assert_eq!(game.cursor(), cursor + 1);
            prop_assert!(!game.is_rewound());
            prop_assert_eq!(game.last_move(), Some(point));
            prop_assert_eq!(game.outcome(), expected_outcome(&game));
        }
    }
}
