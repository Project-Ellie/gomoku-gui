//! Resignation is a game result, not a stone or an engine board status.

mod common;

use gomoku_core::{Color, Game, GameError, Outcome, Status, WinMethod, point};

fn played_game() -> Game {
    let mut game = Game::new();
    for col in 0..3 {
        game.play(point(7, col).unwrap()).unwrap();
    }
    game
}

#[test]
fn resignation_awards_the_other_side_without_changing_the_position() {
    for count in 0..=2 {
        let mut game = Game::new();
        for col in 0..count {
            game.play(point(7, col).unwrap()).unwrap();
        }
        let before = game.clone();
        assert!(game.can_play());
        game.resign().unwrap();
        assert_eq!(
            game.outcome(),
            Outcome::Won {
                winner: before.to_move().other(),
                method: WinMethod::Resignation,
            }
        );
        assert_eq!(game.status(), Status::Ongoing);
        assert_eq!(game.board_snapshot(), before.board_snapshot());
        assert_eq!(game.moves(), before.moves());
        assert_eq!(game.cursor(), before.cursor());
        assert_eq!(game.to_move(), before.to_move());
        assert!(!game.can_play());
        let resigned = game.clone();
        assert_eq!(game.play(point(0, 0).unwrap()), Err(GameError::GameOver));
        assert_eq!(
            game, resigned,
            "live resignation blocks placement atomically"
        );
    }
}

#[test]
fn undo_clears_resignation_before_removing_any_stone_even_from_review() {
    let mut game = played_game();
    let before = game.clone();
    game.resign().unwrap();
    let resigned = game.clone();
    game.seek(0);
    assert_eq!(
        game.outcome(),
        resigned.outcome(),
        "review keeps the result"
    );
    game.live();
    assert_eq!(game, resigned, "review replay preserves resignation");
    game.seek(1);

    assert!(game.undo());
    assert_eq!(
        game, before,
        "first undo clears only the result and goes live"
    );
    assert!(game.can_play());
    assert!(game.undo());
    assert_eq!(game.len(), before.len() - 1);
    assert!(!game.forward(), "ordinary undo has no redo");
    game.play(*before.moves().last().unwrap()).unwrap();
    assert_eq!(game, before, "the removed move can be played again");
    game.resign().unwrap();
    assert_eq!(
        game, resigned,
        "resignation can be applied again after undo"
    );
}

#[test]
fn resignation_is_rejected_transactionally_in_review_or_after_any_result() {
    let mut review = played_game();
    review.rewind();
    let before = review.clone();
    assert_eq!(review.resign(), Err(GameError::NotLive));
    assert_eq!(review, before);

    let mut resigned = played_game();
    resigned.resign().unwrap();
    let won = common::won_game();
    let drawn = common::drawn_game();
    assert_eq!(won.status(), Status::Won(Color::Black));
    assert_eq!(drawn.status(), Status::Draw);
    for mut game in [resigned, won, drawn] {
        let before = game.clone();
        assert!(!game.can_play());
        assert_eq!(game.resign(), Err(GameError::GameOver));
        assert_eq!(game, before);
        game.rewind();
        let review = game.clone();
        assert_eq!(game.resign(), Err(GameError::NotLive));
        assert_eq!(
            game, review,
            "review of a finished game cannot resign either"
        );
    }
}

#[test]
fn only_a_successful_branch_discards_the_old_resignation() {
    let mut game = played_game();
    let old_moves = game.moves().to_vec();
    game.resign().unwrap();
    let result = game.outcome();
    assert!(game.rewind());
    assert_eq!(game.outcome(), result);
    assert!(game.can_play(), "an ongoing review board can branch");
    let before = game.clone();
    assert_eq!(game.play(old_moves[0]), Err(GameError::Occupied));
    assert_eq!(
        game, before,
        "illegal branch keeps history, cursor and result"
    );
    game.play(point(1, 1).unwrap()).unwrap();
    assert_eq!(game.outcome(), Outcome::Ongoing);
    assert_eq!(
        game.moves(),
        &[old_moves[0], old_moves[1], point(1, 1).unwrap()]
    );
    assert!(!game.is_rewound());
    assert!(game.can_play());
    assert!(!game.forward(), "old history and result cannot be replayed");
}

#[test]
fn resignation_can_be_undone_at_an_empty_or_puzzle_root() {
    let white_root = Game::from_position(vec![point(7, 7).unwrap()], vec![], Color::White).unwrap();
    let black_root = Game::from_position(
        vec![point(7, 7).unwrap()],
        vec![point(6, 7).unwrap()],
        Color::Black,
    )
    .unwrap();
    for mut game in [Game::new(), white_root, black_root] {
        let before = game.clone();
        game.resign().unwrap();
        assert_eq!(
            game.outcome(),
            Outcome::Won {
                winner: before.to_move().other(),
                method: WinMethod::Resignation,
            }
        );
        assert!(game.undo(), "result undo works without moves");
        assert_eq!(game, before, "root stones, turn and history are unchanged");
        assert!(!game.undo(), "ordinary undo stops at the root");
        assert!(!game.forward(), "a cleared resignation has no redo");
        assert!(game.can_play());
    }
}

#[test]
fn undoing_resignation_from_a_reviewed_puzzle_root_preserves_all_played_moves() {
    let mut game = Game::from_position(vec![point(7, 7).unwrap()], vec![], Color::White).unwrap();
    game.play(point(6, 7).unwrap()).unwrap();
    let before = game.clone();
    game.resign().unwrap();
    game.seek(0);
    assert!(
        game.undo(),
        "resignation undo precedes the puzzle-root guard"
    );
    assert_eq!(game, before);
    assert!(game.undo(), "ordinary undo now removes the played move");
    assert_eq!(
        game.stones().collect::<Vec<_>>(),
        vec![(point(7, 7).unwrap(), Color::Black)]
    );
    assert!(!game.undo(), "imported stones cannot be undone");
}

#[test]
fn can_play_allows_branching_behind_a_board_win_or_draw() {
    for mut game in [common::won_game(), common::drawn_game()] {
        assert!(!game.can_play());
        let result = game.outcome();
        game.rewind();
        assert!(game.can_play());
        assert_eq!(game.outcome(), result);
        game.live();
        assert!(!game.can_play());
    }
}
