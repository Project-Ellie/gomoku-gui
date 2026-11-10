//! Tree-shaped history, branch navigation, and record compatibility.

use gomoku_core::{Game, Move, Record};

fn point(row: u8, col: u8) -> Move {
    Move::new(row, col).expect("inside the board")
}

#[test]
fn a_linear_game_has_no_moves_with_siblings() {
    let mut game = Game::new();
    for mv in [point(7, 7), point(7, 8), point(8, 8)] {
        game.play(mv).unwrap();
    }

    assert_eq!(game.moves_with_siblings(), vec![false; 3]);
}

#[test]
fn only_moves_with_sibling_nodes_are_marked_on_the_selected_branch() {
    let mut game = branched_game();

    assert_eq!(game.moves_with_siblings(), vec![false, true, false]);

    game.seek(1);
    assert!(game.select_branch(1));
    assert_eq!(game.moves_with_siblings(), vec![false, true]);
}

#[test]
fn deleting_a_sibling_removes_its_move_marker() {
    let mut game = branched_game();
    assert!(game.undo(), "delete the old branch's tip");
    assert_eq!(game.moves_with_siblings(), vec![false, true]);

    assert!(game.undo(), "delete one of the two second moves");

    assert_eq!(game.moves(), &[point(7, 7), point(3, 3)]);
    assert_eq!(game.moves_with_siblings(), vec![false, false]);
}

#[test]
fn playing_from_review_creates_a_sibling_without_losing_the_old_line() {
    let mut game = Game::new();
    for mv in [point(7, 7), point(7, 8), point(8, 8)] {
        game.play(mv).unwrap();
    }
    game.seek(1);

    game.play(point(3, 3)).unwrap();

    assert_eq!(game.moves(), &[point(7, 7), point(3, 3)]);
    game.seek(1);
    assert!(game.is_branch_point());
    assert_eq!(game.branch_options(), vec![point(7, 8), point(3, 3)]);

    assert!(game.select_branch(0));
    assert_eq!(game.moves(), &[point(7, 7), point(7, 8), point(8, 8)]);
    assert_eq!(game.cursor(), 2);
    assert!(game.forward());
    assert_eq!(game.stone_at(point(8, 8)), Some(gomoku_core::Color::Black));
}

#[test]
fn undo_at_the_live_tip_deletes_the_move_instead_of_allowing_redo() {
    let mut game = Game::new();
    game.play(point(7, 7)).unwrap();
    game.play(point(7, 8)).unwrap();

    assert!(game.undo());

    assert_eq!(game.moves(), &[point(7, 7)]);
    assert_eq!(game.cursor(), 1);
    assert_eq!(game.stone_at(point(7, 8)), None);
    assert!(!game.forward(), "the deleted move cannot be replayed");
    game.play(point(3, 3)).unwrap();
    assert_eq!(game.moves(), &[point(7, 7), point(3, 3)]);
    game.seek(1);
    assert!(!game.is_branch_point(), "the old child was removed");
}

#[test]
fn changing_the_current_child_does_not_enter_it_and_forward_uses_the_choice() {
    let mut game = Game::new();
    game.play(point(7, 7)).unwrap();
    game.play(point(7, 8)).unwrap();
    game.play(point(8, 8)).unwrap();
    game.seek(1);
    game.play(point(3, 3)).unwrap();
    game.seek(1);
    assert_eq!(game.selected_branch(), Some(1));

    assert!(game.set_current_child(0));

    assert_eq!(game.cursor(), 1, "selection alone does not advance");
    assert_eq!(game.last_move(), Some(point(7, 7)));
    assert_eq!(game.selected_branch(), Some(0));
    assert_eq!(
        game.moves(),
        &[point(7, 7), point(7, 8), point(8, 8)],
        "the selected continuation changes immediately"
    );
    assert!(game.forward());
    assert_eq!(game.last_move(), Some(point(7, 8)));
    assert!(game.rewind());
    assert_eq!(game.selected_branch(), Some(0), "the choice is remembered");
    assert!(!game.set_current_child(2));
    assert_eq!(game.selected_branch(), Some(0));
}

#[test]
fn destructive_undo_cleans_up_branch_points_and_preserves_sibling_nodes() {
    let mut game = Game::new();
    for mv in [point(7, 7), point(7, 8), point(8, 8)] {
        game.play(mv).unwrap();
    }
    game.seek(1);
    game.play(point(3, 3)).unwrap();
    game.seek(1);
    assert!(game.select_branch(0));
    game.live();

    assert!(game.undo(), "delete the old line's tip, not its sibling");
    assert_eq!(game.moves(), &[point(7, 7), point(7, 8)]);
    assert_eq!(game.cursor(), 2);
    assert!(!game.is_branch_point(), "the deleted tip had no children");
    assert!(game.undo(), "delete the now-childless middle node");
    assert_eq!(game.cursor(), 1);
    assert_eq!(game.branch_options(), vec![point(3, 3)]);
    assert_eq!(game.selected_branch(), Some(0));
    assert!(!game.is_branch_point(), "one sibling is not a branch point");
    let record = Record::from_game(&game).unwrap();
    assert_eq!(
        record.tree.len(),
        3,
        "only root and the two survivors remain"
    );
    let loaded = Record::from_json(&record.to_json().unwrap())
        .unwrap()
        .into_game()
        .unwrap();
    let mut live = game.clone();
    live.live();
    assert_eq!(loaded, live, "deleted nodes do not return through a record");
    assert!(game.forward());
    assert_eq!(game.last_move(), Some(point(3, 3)));
    assert!(
        !game.forward(),
        "the deleted old continuation is really gone"
    );

    assert!(game.undo());
    assert_eq!(game.moves(), &[point(7, 7)]);
    assert!(game.undo());
    assert!(game.is_empty());
    assert_eq!(game.cursor(), 0);
    assert!(game.stones().next().is_none());
    assert!(!game.undo());
}

#[test]
fn undo_while_reviewing_only_rewinds_and_preserves_the_continuation() {
    let mut game = Game::new();
    game.play(point(7, 7)).unwrap();
    game.play(point(7, 8)).unwrap();
    game.seek(1);

    assert!(game.undo());

    assert_eq!(game.cursor(), 0);
    assert_eq!(game.len(), 2);
    assert!(game.seek(usize::MAX));
    assert_eq!(game.last_move(), Some(point(7, 8)));
}

fn branched_game() -> Game {
    let mut game = Game::new();
    game.play(point(7, 7)).unwrap();
    game.play(point(7, 8)).unwrap();
    game.play(point(8, 8)).unwrap();
    game.seek(1);
    game.play(point(3, 3)).unwrap();
    game.seek(1);
    assert!(game.select_branch(0));
    game.live();
    game
}

#[test]
fn child_choices_survive_switching_to_an_outer_sibling() {
    let mut game = Game::new();
    for mv in [point(7, 7), point(7, 8), point(8, 8)] {
        game.play(mv).unwrap();
    }
    game.seek(2);
    game.play(point(9, 9)).unwrap();
    game.seek(0);
    game.play(point(3, 3)).unwrap();

    game.seek(0);
    assert!(game.select_branch(0));
    assert_eq!(
        game.moves(),
        &[point(7, 7), point(7, 8), point(9, 9)],
        "the inner branch remembers its own last-taken child"
    );
    assert!(game.forward());
    assert_eq!(game.selected_branch(), Some(1));
    assert!(game.forward());
    assert_eq!(game.last_move(), Some(point(9, 9)));

    game.seek(2);
    game.play(point(9, 9)).unwrap();
    game.seek(2);
    assert_eq!(
        game.branch_options().len(),
        2,
        "following an existing child adds no duplicate"
    );
}

#[test]
fn a_branched_record_round_trips_the_whole_tree_and_branch_choices() {
    let game = branched_game();
    let record = Record::from_game(&game).unwrap();
    assert_eq!(record.version, 3);
    let text = record.to_json().unwrap();
    let loaded = Record::from_json(&text).unwrap().into_game().unwrap();

    assert_eq!(loaded, game);
    assert_eq!(Record::from_game(&loaded).unwrap(), record);
}

#[test]
fn resignation_on_a_sibling_branch_survives_the_record_round_trip() {
    let mut game = Game::new();
    for mv in [point(7, 7), point(7, 8), point(8, 8)] {
        game.play(mv).unwrap();
    }
    game.resign().unwrap();
    let resigned = game.outcome();
    game.seek(2);
    game.play(point(3, 3)).unwrap();
    game.seek(2);
    assert!(game.select_branch(0));
    game.live();

    let record = Record::from_game(&game).unwrap();
    let mut loaded = Record::from_json(&record.to_json().unwrap())
        .unwrap()
        .into_game()
        .unwrap();
    assert_eq!(loaded, game);
    assert_eq!(loaded.outcome(), resigned);
    loaded.seek(2);
    assert!(loaded.select_branch(1));
    assert_eq!(loaded.outcome(), gomoku_core::Outcome::Ongoing);
    assert_eq!(loaded.last_move(), Some(point(3, 3)));
}

#[test]
fn an_old_linear_file_loads_as_a_degenerate_tree() {
    let text = include_str!("golden/hotseat-v1.json");
    let loaded = Record::from_json(text).unwrap().into_game().unwrap();
    assert_eq!(loaded.len(), 4);
    assert_eq!(loaded.cursor(), 4);

    let tree = Record::from_game(&loaded).unwrap();
    assert_eq!(tree.version, 3);
    assert_eq!(tree.tree.len(), 5);
    for (index, node) in tree.tree.iter().enumerate() {
        assert_eq!(node.parent, index.checked_sub(1).map(|id| id as u32));
        assert_eq!(
            node.current,
            (index + 1 < tree.tree.len()).then_some((index + 1) as u32)
        );
    }
}

#[test]
fn damaged_tree_links_and_moves_are_errors_not_panics() {
    let original = Record::from_game(&branched_game()).unwrap();
    let mut damaged = Vec::new();

    let mut record = original.clone();
    record.tree.clear();
    damaged.push(record);
    let mut record = original.clone();
    record.tree[0].parent = Some(0);
    damaged.push(record);
    let mut record = original.clone();
    record.tree[1].parent = None;
    damaged.push(record);
    let mut record = original.clone();
    record.tree[1].point = None;
    damaged.push(record);
    let mut record = original.clone();
    record.tree[1].parent = Some(1);
    damaged.push(record);
    let mut record = original.clone();
    record.tree[0].current = Some(2);
    damaged.push(record);
    let mut record = original.clone();
    record.tree[1].current = Some(99);
    damaged.push(record);
    let mut record = original.clone();
    record.tree[4].point = record.tree[2].point;
    damaged.push(record);
    let mut record = original.clone();
    record.tree[3].point = record.tree[1].point;
    damaged.push(record);
    let mut record = original;
    record.moves.pop();
    damaged.push(record);

    for record in damaged {
        let text = record.to_json().unwrap();
        let parsed = Record::from_json(&text).unwrap();
        assert!(parsed.into_game().is_err());
    }
}
