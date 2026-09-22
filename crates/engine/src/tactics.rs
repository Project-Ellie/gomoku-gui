//! Alpha-epsilon tactics: immediate wins, forced blocks, double threats.
//! Slice 7. See docs/13-engine-design.md, "Tactics".
//!
//! Pure bitboard truth, no learning: every answer this module gives is
//! a CERTAINTY. How much these certainties shape priors is a
//! training-time `[experiment]` knob — the engine only vouches for what
//! is true.
//!
//! THREE DOCUMENTED v1 SIMPLIFICATIONS (all deliberate, all measured
//! against their cost):
//!
//! 1. **Four-three blind spot.** `double_threats` counts immediate
//!    wins after one move. A four-three has exactly ONE immediate win
//!    now (the three matures next ply), so it escapes — the full
//!    win-in-2 search is out of scope for the engine milestone.
//! 2. **Opponent-wins-first.** A "double threat" on a board where the
//!    OPPONENT has an immediate win is answerable — they win before it
//!    matures. v1 accepts this; self-play checks forced blocks first.
//! 3. **Immediate wins are not double threats.** A move that completes
//!    five ENDS the game; calling it a "threat" would be a category
//!    error, so `double_threats` excludes it and the three sets
//!    partition cleanly: win now / must block / unblockable threat.
//!    (The pragmatic bonus: with the five-already-present case gone,
//!    the fast whole-board scan and the naive through-cell scan answer
//!    the SAME question — see `reference.rs`.)
//!
//! COST: `immediate_wins` is ~225 hypothetical placements ≈ 13k ops.
//! `double_threats` is O(empties × immediate_wins) ≈ 3M ops — fine at
//! leaf expansion, never call it per PUCT step.

use crate::bitboard::{Bitboard, VALID};
use crate::board::{Board, Color};
use crate::moveset::{Move, MoveSet};
use crate::win::has_any_five;

/// Cells where `side` completes five immediately. Answers for EITHER
/// color regardless of `to_move` — that asymmetry with `forced_blocks`
/// (where the turn matters) is what makes the API honest.
pub fn immediate_wins(b: &Board, side: Color) -> MoveSet {
    winning_cells(b.stones(side), clean_empties(b))
}

/// Cells the side to move MUST play — the opponent's immediate wins.
pub fn forced_blocks(b: &Board) -> MoveSet {
    immediate_wins(b, b.to_move().other())
}

/// Moves after which `side` has >= 2 immediate wins: open fours and
/// double fours — unanswerable next move. Immediate wins themselves
/// are EXCLUDED (simplification 3 above). See the module docs for the
/// four-three blind spot.
pub fn double_threats(b: &Board, side: Color) -> MoveSet {
    let stones = b.stones(side);
    let empty = clean_empties(b);
    let mut out = MoveSet::EMPTY;
    for i in empty.iter_set_bits() {
        let hypo = stones.with_bit(i);
        if has_any_five(&hypo) {
            continue; // immediate win: ends the game, not a threat
        }
        if winning_cells(hypo, empty.without_bit(i)).len() >= 2 {
            out.insert(mv_at(i));
        }
    }
    out
}

/// The shared primitive: every empty cell whose hypothetical
/// occupation completes five. `empty` MUST be VALID-masked (padding
/// bits clear) — `clean_empties` guarantees it, and `without_bit`
/// preserves it.
pub(crate) fn winning_cells(stones: Bitboard, empty: Bitboard) -> MoveSet {
    let mut out = MoveSet::EMPTY;
    for i in empty.iter_set_bits() {
        if has_any_five(&stones.with_bit(i)) {
            out.insert(mv_at(i));
        }
    }
    out
}

/// The empty cells as a CLEAN bitboard. `!occupied` sets every padding
/// bit — the complement must be masked immediately (the padding
/// invariant, ch. 13; `Board::empty_moves` documents the same rule).
pub(crate) fn clean_empties(b: &Board) -> Bitboard {
    !(b.stones(Color::Black) | b.stones(Color::White)) & VALID
}

/// Stride-16 bitboard index → logical Move. Only ever called with
/// VALID-masked bits, so the column is < 15 and `new` cannot fail.
pub(crate) fn mv_at(i: usize) -> Move {
    Move::new((i / 16) as u8, (i % 16) as u8).expect("VALID-masked bits are real cells")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::reference::board_from_ascii;

    fn set_of(cells: &[(u8, u8)]) -> MoveSet {
        let mut s = MoveSet::EMPTY;
        for &(r, c) in cells {
            s.insert(Move::new(r, c).unwrap());
        }
        s
    }

    #[test]
    fn open_four_has_exactly_two_winning_cells() {
        let b = board_from_ascii(
            "
            O . O . O . O . . . . . . . .
            . . . . . . . . . . . . . . .
            . . . . . . . . . . . . . . .
            . . . . . . . . . . . . . . .
            . . . . . . . . . . . . . . .
            . . . . . . . . . . . . . . .
            . . . . . . . . . . . . . . .
            . . . . X X X X . . . . . . .
            . . . . . . . . . . . . . . .
            . . . . . . . . . . . . . . .
            . . . . . . . . . . . . . . .
            . . . . . . . . . . . . . . .
            . . . . . . . . . . . . . . .
            . . . . . . . . . . . . . . .
            . . . . . . . . . . . . . . .
            ",
        );
        assert_eq!(immediate_wins(&b, Color::Black), set_of(&[(7, 3), (7, 8)]));
    }

    #[test]
    fn broken_four_has_exactly_one_winning_cell() {
        // X X X _ X with the gap at (7,6): only the gap completes five.
        let b = board_from_ascii(
            "
            O . O . O . O . . . . . . . .
            . . . . . . . . . . . . . . .
            . . . . . . . . . . . . . . .
            . . . . . . . . . . . . . . .
            . . . . . . . . . . . . . . .
            . . . . . . . . . . . . . . .
            . . . . . . . . . . . . . . .
            . . . . X X . X X . . . . . .
            . . . . . . . . . . . . . . .
            . . . . . . . . . . . . . . .
            . . . . . . . . . . . . . . .
            . . . . . . . . . . . . . . .
            . . . . . . . . . . . . . . .
            . . . . . . . . . . . . . . .
            . . . . . . . . . . . . . . .
            ",
        );
        assert_eq!(immediate_wins(&b, Color::Black), set_of(&[(7, 6)]));
    }

    #[test]
    fn immediate_wins_answers_for_either_color() {
        // Same shape as the open-four test, but the FOUR is White's.
        let b = board_from_ascii(
            "
            . . . . . . . . . . . . . . .
            . . . . . . . . . . . . . . .
            . . . . . . . . . . . . . . .
            . . . . . . . . . . . . . . .
            . . . . . . . . . . . . . . .
            . . . . . . . . . . . . . . .
            . . . . . . . . . . . . . . .
            . . . . . . . . . . . . . . .
            . . . . . . . . . . . . . . .
            . . O O O O . . . . . . . . .
            . . . . . . . . . . . . . . .
            . . . . . . . . . . . . . . .
            . . . . . . . . . . . . . . .
            . . . . . . . . . . . . . . .
            X . X . X . X . . . . . . . .
            ",
        );
        assert_eq!(immediate_wins(&b, Color::White), set_of(&[(9, 1), (9, 6)]));
        // ...while Black's scattered stones threaten nothing.
        assert!(immediate_wins(&b, Color::Black).is_empty());
    }

    #[test]
    fn forced_blocks_are_the_opponents_winning_cells() {
        // White has the open four, Black to move (equal counts).
        let b = board_from_ascii(
            "
            . . . . . . . . . . . . . . .
            . . . . . . . . . . . . . . .
            . . . . . . . . . . . . . . .
            . . . . . . . . . . . . . . .
            . . . . . . . . . . . . . . .
            . . . . . . . . . . . . . . .
            . . . . . . . . . . . . . . .
            . . . . . . . . . . . . . . .
            . . . . . . . . . . . . . . .
            . . O O O O . . . . . . . . .
            . . . . . . . . . . . . . . .
            . . . . . . . . . . . . . . .
            . . . . . . . . . . . . . . .
            . . . . . . . . . . . . . . .
            X . X . X . X . . . . . . . .
            ",
        );
        assert_eq!(b.to_move(), Color::Black);
        assert_eq!(forced_blocks(&b), set_of(&[(9, 1), (9, 6)]));
    }

    #[test]
    fn the_classic_fork_is_exactly_one_double_threat() {
        // Two closed threes (blocked by O at the top / on the left).
        // Playing (7,7) opens TWO fours, each with exactly one winning
        // cell: (8,7) and (7,8). No other move creates more than one
        // four, so the set is EXACTLY {(7,7)}. The remaining O stones
        // are scattered filler — chosen to be genuinely quiet (an
        // earlier draft parked them on rank 0 as `O . O . O . O`,
        // which is ITSELF a double threat: (0,3) fills two gapped
        // fours. The oracle does not do "harmless filler".)
        let b = board_from_ascii(
            "
            . . . . . . . . . . . . . . O
            . . . . . . . . . . . . . . .
            . . . . . . . . . . . . . . .
            . . . . . . . O . . . . . . .
            . . . . . . . X . . . . . . .
            . . . . . . . X . . . . . . .
            . . . . . . . X . . . . . . .
            . . . O X X X . . . . . . . .
            . . . . . . . . . . . . . . .
            . . . . . . . . . . . . . . .
            . . . . . . . . . . . . . . .
            . . . . . . . . . . O . . . .
            . . . . . . . . . . . . . . .
            . . . . . . . . . . . . . . .
            O . . . . . . . . . . . . . O
            ",
        );
        assert!(immediate_wins(&b, Color::Black).is_empty(), "no four yet");
        assert_eq!(double_threats(&b, Color::Black), set_of(&[(7, 7)]));
        assert!(double_threats(&b, Color::White).is_empty());
    }

    #[test]
    fn closed_four_forces_a_block_but_is_no_double_threat() {
        // X X X X blocked by O on the left: ONE winning cell (7,7).
        // White to move (X has one more stone) must play it. And the
        // winning move itself is NOT a double threat — the partition.
        let b = board_from_ascii(
            "
            O . O . . . . . . . . . . . .
            . . . . . . . . . . . . . . .
            . . . . . . . . . . . . . . .
            . . . . . . . . . . . . . . .
            . . . . . . . . . . . . . . .
            . . . . . . . . . . . . . . .
            . . . . . . . . . . . . . . .
            . . O X X X X . . . . . . . .
            . . . . . . . . . . . . . . .
            . . . . . . . . . . . . . . .
            . . . . . . . . . . . . . . .
            . . . . . . . . . . . . . . .
            . . . . . . . . . . . . . . .
            . . . . . . . . . . . . . . .
            . . . . . . . . . . . . . . .
            ",
        );
        assert_eq!(b.to_move(), Color::White);
        assert_eq!(immediate_wins(&b, Color::Black), set_of(&[(7, 7)]));
        assert_eq!(forced_blocks(&b), set_of(&[(7, 7)]));
        assert!(double_threats(&b, Color::Black).is_empty());
    }

    #[test]
    fn quiet_positions_have_no_tactics() {
        let b = board_from_ascii(
            "
            . . . . . . . . . . . . . . .
            . . . . . . . . . . . . . . .
            . . X . . . . . . . . . . . .
            . . . . . . . . . . . . . . .
            . . . . . . . . . . . . . . .
            . . . . . . . . . O . . . . .
            . . . . . . . . . . . . . . .
            . . . . . . . . . . . . . . .
            . . . . . . . . . . . . . . .
            . . . . . O . . . . . . . . .
            . . . . . . . . . . . . . . .
            . . . . . . . . . . . . . . .
            . . . . . . . . . . . . X . .
            . . . . . . . . . . . . . . .
            . . . . . . . . . . . . . . .
            ",
        );
        assert!(immediate_wins(&b, Color::Black).is_empty());
        assert!(immediate_wins(&b, Color::White).is_empty());
        assert!(forced_blocks(&b).is_empty());
        assert!(double_threats(&b, Color::Black).is_empty());
        assert!(double_threats(&b, Color::White).is_empty());
    }

    #[test]
    fn immediate_win_along_the_top_edge() {
        // Open four on row 0: the board edge is no obstacle for the
        // in-row winning cells (0,0) and (0,5).
        let b = board_from_ascii(
            "
            . X X X X . . . . . . . . . .
            . . . . . . . . . . . . . . .
            . . . . . . . . . . . . . . .
            . . . . . . . . . . . . . . .
            . . . . . . . . . . . . . . .
            . . . . . . . . . . . . . . .
            . . . . . . . . . . . . . . .
            . . . . . . . . . . . . . . .
            . . . . . . . . . . . . . . .
            . . . . . . . . . . . . . . .
            . . . . . . . . . . . . . . .
            . . . . . . . . . . . . . . .
            . . . . . . . . . . . . . . .
            . . . . . . . . . . . . . . .
            O . O . O . O . . . . . . . .
            ",
        );
        assert_eq!(immediate_wins(&b, Color::Black), set_of(&[(0, 0), (0, 5)]));
    }

    #[test]
    fn double_threat_accepted_even_when_opponent_wins_first() {
        // v1 simplification 2: a double threat is still reported even
        // though the opponent has an immediate win and would play it
        // before the threat matures. The turn-order truth lives in
        // `forced_blocks`: Black to move is ALREADY in trouble because
        // White has an open four.
        let b = board_from_ascii(
            "
            . . . . . . . . . . . . . . .
            . . . . . . . . . . . . . . .
            . . . . . . . . . . . . . . .
            . . . . . . . . . . . . . . .
            . . . . . . . . . . . . . . .
            . . . . . . . . . . . . . . .
            . . . . . . . . . . . . . . .
            . . . . O O O O . . . . . . .
            . . . . . . . . . . . . . . .
            . . . . . . . . . . . . . . .
            . . . . . . . . . . . . . . .
            . . . . . . . . . . . . . . .
            X . X . X . X . . . . . . . .
            . . . . . . . . . . . . . . .
            . . . . . . . . . . . . . . .
            ",
        );
        assert_eq!(b.to_move(), Color::Black);
        assert_eq!(forced_blocks(&b), set_of(&[(7, 3), (7, 8)]));
        assert_eq!(double_threats(&b, Color::Black), set_of(&[(12, 3)]));
    }

    #[test]
    fn double_threat_in_the_corner() {
        // Two threes hugging the edges. Playing the CORNER (0,0) makes
        // two fours whose only open ends are (0,4) and (4,0) — the
        // edge itself "blocks" the other ends. (0,4) and (4,0) are
        // also double threats (they open a four each... on the edge).
        let b = board_from_ascii(
            "
            . X X X . . . . . . . . . . .
            X . . . . . . . . . . . . . .
            X . . . . . . . . . . . . . .
            X . . . . . . . . . . . . . .
            . . . . . . . . . . . . . . .
            . . . . . . . . . . . . . . .
            . . . . . . . . . . . . . . .
            . . . . . . . . . . . . . . .
            . . . . . . . . . . . . . . .
            . . . . . . . . . . . . . . .
            . . . . . . . . . . . . . . .
            . . . . . . . . . . . . . . .
            . . . . . . . . . . . . . . .
            . . . . . . . . . . . . . . .
            O . O . O . O . O . O . . . .
            ",
        );
        assert!(immediate_wins(&b, Color::Black).is_empty());
        let threats = double_threats(&b, Color::Black);
        assert!(
            threats.contains(Move::new(0, 0).unwrap()),
            "the corner fork"
        );
        assert!(
            threats.contains(Move::new(0, 4).unwrap()),
            "open four on the edge"
        );
        assert!(
            threats.contains(Move::new(4, 0).unwrap()),
            "open four on the edge"
        );
    }
}
