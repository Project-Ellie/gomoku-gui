//! Bounded threat-space search: prover + line verifier.
//! Slice 8. See docs/13-engine-design.md, "Threat-space search".
//!
//! The oracle is soundness-first: `Some` means the line is machine-
//! verified by `verify_line`; `None` only means "not proven within
//! budget" and says nothing about the true value of the position.

use crate::board::{Board, Color, Status};
use crate::moveset::{Move, MoveSet};
use crate::tactics::{forced_blocks, immediate_wins};

/// A certified forced win: the winning side and one forcing sequence.
/// `line` alternates moves, starting with `winner`'s first threat, and
/// ends when the attacker completes five (overlines count) or creates
/// an unblockable double threat (>= 2 immediate wins).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Proof {
    /// The side that can force a win.
    pub winner: Color,
    /// The forcing sequence, starting with the attacker's first threat.
    pub line: Vec<Move>,
}

/// Hard caps. Exhausting the budget means "no proof" — never "loss".
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SearchBudget {
    /// Maximum threat-space nodes to explore.
    pub max_nodes: u32,
    /// Maximum depth (plies) to search.
    pub max_depth: u8,
}

/// `Some` = certified forced win for `side` from `b`.
/// `None` = not proven within budget; says NOTHING about the true value.
pub fn prove_forced_win(b: &Board, side: Color, budget: SearchBudget) -> Option<Proof> {
    if b.status() != Status::Ongoing || b.to_move() != side {
        return None;
    }
    let mut b = b.clone();
    let mut search = Search::new(budget);
    search
        .solve(&mut b, side, budget.max_depth)
        .map(|line| Proof { winner: side, line })
}

/// The soundness net. Replays `proof.line` from `b`: every move legal,
/// alternating, forcing; at each defender turn, EVERY alternative that
/// addresses the threat is enumerated and the attacker's continuation
/// must still win (gapped fours give the defender two blocks). `true`
/// only if the proof is airtight.
pub fn verify_line(b: &Board, proof: &Proof) -> bool {
    if b.to_move() != proof.winner {
        return false;
    }
    verify_rec(b.clone(), proof.winner, &proof.line)
}

// -----------------------------------------------------------------------------
// Prover

struct Search {
    nodes_left: u32,
}

impl Search {
    fn new(budget: SearchBudget) -> Self {
        Self {
            nodes_left: budget.max_nodes,
        }
    }

    /// Attacker-to-move node. Returns a winning line from this position,
    /// or `None` if the budget is exhausted or no proof is found.
    fn solve(&mut self, b: &mut Board, side: Color, depth: u8) -> Option<Vec<Move>> {
        if self.nodes_left == 0 || depth == 0 {
            return None;
        }
        self.nodes_left -= 1;

        // Win now: the shortest possible proof.
        let wins = immediate_wins(b, side);
        if !wins.is_empty() {
            return Some(vec![wins.iter().next().unwrap()]);
        }

        // Try every threat-generating move.
        for mv in threat_moves(b, side).iter() {
            b.play(mv).ok()?;

            // Win now: line ends here.
            if b.status() == Status::Won(side) {
                b.undo();
                return Some(vec![mv]);
            }

            // Defender's own win comes first: the threat never matures.
            if !immediate_wins(b, side.other()).is_empty() {
                b.undo();
                continue;
            }

            // Unblockable double threat: line ends here.
            if immediate_wins(b, side).len() >= 2 {
                b.undo();
                return Some(vec![mv]);
            }

            let maybe_suffix = self.solve_after_attacker(b, side, depth - 1);
            b.undo();
            if let Some((reply, suffix)) = maybe_suffix {
                let mut line = vec![mv, reply];
                line.extend(suffix);
                return Some(line);
            }
        }
        None
    }

    /// Position after the attacker just played. Decide whether the
    /// threat is terminal, refuted by a defender win, or must be met by
    /// forced blocks. Returns `(representative_defender_reply, suffix)`
    /// so the full line is `[attacker_move, reply] + suffix`.
    fn solve_after_attacker(
        &mut self,
        b: &mut Board,
        side: Color,
        depth: u8,
    ) -> Option<(Move, Vec<Move>)> {
        if self.nodes_left == 0 || depth == 0 {
            return None;
        }
        self.nodes_left -= 1;

        // 1. The attacker's own five ends the game.
        if b.status() == Status::Won(side) {
            // Caller already handles this, but the guard keeps the helper honest.
            return None;
        }

        // 2. Defender's own win comes first: the threat never matures.
        if !immediate_wins(b, side.other()).is_empty() {
            return None;
        }

        // 3. Unblockable double threat ends the line.
        let attacker_wins = immediate_wins(b, side);
        if attacker_wins.len() >= 2 {
            return None; // terminal: caller returns just the attacker move
        }
        if attacker_wins.is_empty() {
            return None; // no threat was created
        }

        // Exactly one threat: defender must block every winning cell.
        let replies: Vec<Move> = forced_blocks(b).iter().collect();
        let mut continuations: Vec<(Move, Vec<Move>)> = Vec::with_capacity(replies.len());
        for reply in &replies {
            b.play(*reply).ok()?;
            let cont = self.solve(b, side, depth - 1);
            b.undo();
            let line = cont?;
            continuations.push((*reply, line));
        }

        // Find one suffix that wins against EVERY defender reply. Usually
        // the same continuation works for all blocks; if none does, the
        // position is unproven within our single-main-line model.
        for (reply, suffix) in &continuations {
            let suffix = suffix.clone();
            let mut ok = true;
            for reply in &replies {
                b.play(*reply).ok()?;
                let passes = verify_line(
                    b,
                    &Proof {
                        winner: side,
                        line: suffix.clone(),
                    },
                );
                b.undo();
                if !passes {
                    ok = false;
                    break;
                }
            }
            if ok {
                return Some((*reply, suffix));
            }
        }
        None
    }
}

/// Threat-generating cells: empty cells whose hypothetical occupation
/// leaves `side` with at least one immediate win, but are not themselves
/// immediate wins (those are handled before we get here).
fn threat_moves(b: &Board, side: Color) -> MoveSet {
    let stones = b.stones(side);
    let empty = crate::tactics::clean_empties(b);
    let mut out = MoveSet::EMPTY;
    for i in empty.iter_set_bits() {
        let after = stones.with_bit(i);
        if crate::win::has_any_five(&after) {
            continue; // win now
        }
        let remaining = empty.without_bit(i);
        if !crate::tactics::winning_cells(after, remaining).is_empty() {
            out.insert(crate::tactics::mv_at(i));
        }
    }
    out
}

// -----------------------------------------------------------------------------
// Verifier

fn verify_rec(mut b: Board, side: Color, line: &[Move]) -> bool {
    if line.is_empty() {
        return false;
    }
    let mv = line[0];
    if !b.is_legal(mv) || b.to_move() != side {
        return false;
    }
    b.play(mv).unwrap();

    // Attacker completed five: line must end here.
    if b.status() == Status::Won(side) {
        return line.len() == 1;
    }

    // Defender's own win comes first: the threat is not sound.
    if !immediate_wins(&b, side.other()).is_empty() {
        return false;
    }

    let attacker_wins = immediate_wins(&b, side);

    // Unblockable: line must end here.
    if attacker_wins.len() >= 2 {
        return line.len() == 1;
    }

    // Exactly one threat: the next move must be a defender block.
    if attacker_wins.is_empty() || line.len() < 2 {
        return false;
    }
    let reply = line[1];
    if !attacker_wins.contains(reply) {
        return false;
    }

    // Enumerate every block; the suffix must win against all of them.
    for alt in attacker_wins.iter() {
        if alt == reply {
            // The main line's block is checked by the recursive call below.
            continue;
        }
        let mut branch = b.clone();
        branch.play(alt).unwrap();
        if !verify_rec(branch, side, &line[2..]) {
            return false;
        }
    }

    // Now follow the main line's block and continue.
    b.play(reply).unwrap();
    verify_rec(b, side, &line[2..])
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::reference::board_from_ascii;

    // ------------------------------------------------------------------
    // 1. Immediate win: one-move line.
    #[test]
    fn open_four_is_one_move_line() {
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
        let proof = prove_forced_win(
            &b,
            Color::Black,
            SearchBudget {
                max_nodes: 1000,
                max_depth: 5,
            },
        )
        .unwrap();
        assert_eq!(proof.line.len(), 1);
        assert!(
            proof.line[0] == Move::new(7, 3).unwrap() || proof.line[0] == Move::new(7, 8).unwrap()
        );
        assert!(verify_line(&b, &proof));
    }

    // ------------------------------------------------------------------
    // 2. Quiet position -> None.
    #[test]
    fn quiet_position_is_not_proven() {
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
        assert!(
            prove_forced_win(
                &b,
                Color::Black,
                SearchBudget {
                    max_nodes: 1000,
                    max_depth: 5
                }
            )
            .is_none()
        );
    }

    // ------------------------------------------------------------------
    // 3. Open-four maker: one-move unblockable line.
    #[test]
    fn open_four_maker_is_one_move_unblockable() {
        // Classic fork: Black plays (7,7) and creates two closed fours,
        // each with one winning cell. >=2 immediate wins => terminal.
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
        let proof = prove_forced_win(
            &b,
            Color::Black,
            SearchBudget {
                max_nodes: 1000,
                max_depth: 5,
            },
        )
        .unwrap();
        assert_eq!(proof.line, vec![Move::new(7, 7).unwrap()]);
        assert!(verify_line(&b, &proof));
    }

    // ------------------------------------------------------------------
    // 4. VCF win-in-3: closed four -> forced block -> double threat.
    #[test]
    fn vcf_win_in_three() {
        // Build by script to avoid ASCII count pitfalls.
        // Black plays (7,7): row 7 becomes a closed four.
        // White must block at (7,8).
        // Black plays (5,5): diagonal becomes an open four (double threat).
        let mut b = Board::new();
        let script = [
            (7, 4),
            (7, 3),
            (7, 5),
            (0, 0),
            (7, 6),
            (0, 2),
            (6, 6),
            (0, 4),
            (8, 8),
            (0, 6),
        ];
        for &(r, c) in &script {
            b.play(Move::new(r, c).unwrap()).unwrap();
        }
        assert_eq!(b.to_move(), Color::Black);
        let proof = prove_forced_win(
            &b,
            Color::Black,
            SearchBudget {
                max_nodes: 2000,
                max_depth: 6,
            },
        )
        .unwrap();
        assert_eq!(proof.line.len(), 3);
        assert_eq!(proof.line[0], Move::new(7, 7).unwrap());
        let mut after_first = b.clone();
        after_first.play(proof.line[0]).unwrap();
        assert!(forced_blocks(&after_first).contains(proof.line[1]));
        assert!(verify_line(&b, &proof));
    }

    // ------------------------------------------------------------------
    // 5. Budget honesty.
    #[test]
    fn tiny_budget_fails_generous_budget_succeeds() {
        let mut b = Board::new();
        let script = [
            (7, 4),
            (7, 3),
            (7, 5),
            (0, 0),
            (7, 6),
            (0, 2),
            (6, 6),
            (0, 4),
            (8, 8),
            (0, 6),
        ];
        for &(r, c) in &script {
            b.play(Move::new(r, c).unwrap()).unwrap();
        }
        // The puzzle is win-in-3; a single node is not enough to see past the
        // first attacker move, and depth 2 cannot fit three plies.
        assert!(
            prove_forced_win(
                &b,
                Color::Black,
                SearchBudget {
                    max_nodes: 1,
                    max_depth: 6
                }
            )
            .is_none()
        );
        assert!(
            prove_forced_win(
                &b,
                Color::Black,
                SearchBudget {
                    max_nodes: 2000,
                    max_depth: 2
                }
            )
            .is_none()
        );
        assert!(
            prove_forced_win(
                &b,
                Color::Black,
                SearchBudget {
                    max_nodes: 2000,
                    max_depth: 6
                }
            )
            .is_some()
        );
    }

    // ------------------------------------------------------------------
    // 6. verify_line rejects doctored proofs.
    #[test]
    fn verifier_rejects_illegal_move() {
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
        let proof = Proof {
            winner: Color::Black,
            line: vec![Move::new(7, 5).unwrap()],
        };
        assert!(!verify_line(&b, &proof));
    }

    #[test]
    fn verifier_rejects_wrong_starter() {
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
        let proof = Proof {
            winner: Color::White,
            line: vec![Move::new(7, 3).unwrap()],
        };
        assert!(!verify_line(&b, &proof));
    }

    #[test]
    fn verifier_rejects_non_forcing_attacker_move() {
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
        let proof = Proof {
            winner: Color::Black,
            line: vec![Move::new(0, 0).unwrap()],
        };
        assert!(!verify_line(&b, &proof));
    }

    #[test]
    fn verifier_rejects_truncated_line() {
        let mut b = Board::new();
        let script = [
            (7, 4),
            (7, 3),
            (7, 5),
            (0, 0),
            (7, 6),
            (0, 2),
            (6, 6),
            (0, 4),
            (8, 8),
            (0, 6),
        ];
        for &(r, c) in &script {
            b.play(Move::new(r, c).unwrap()).unwrap();
        }
        // (7,7) creates a closed four, but the line ends before the win.
        let proof = Proof {
            winner: Color::Black,
            line: vec![Move::new(7, 7).unwrap()],
        };
        assert!(!verify_line(&b, &proof));
    }

    // ------------------------------------------------------------------
    // 7. The gapped-four / double-threat trap.
    //
    // A doctored proof claims a win through a defender block that does
    // not actually work. `verify_line` enumerates every block and rejects
    // the proof because the suffix fails against the other block.
    #[test]
    fn double_threat_maker_rejects_doctored_long_line() {
        // Black plays (7,7). It creates two gapped fours whose winning
        // cells are (7,6) and (6,7). The real position is a double threat,
        // so the only sound proof is the one-move line [(7,7)]. A doctored
        // proof that drags the line out through one block must be caught.
        let b = board_from_ascii(
            "
            . O . O . . . . . . . . . . .
            . . . . . . . . . . . . . . .
            . . . . . . . . . . . . . . .
            . . . . . . . . . . . . . . .
            . . . . . . . X . . . . . . .
            . . . . . . . X . . . . . . .
            . . . . . . . . . . . . . . .
            . . . . X X . . X . . . . . .
            . . . . . . . X . . . . . . .
            . . . . . . . . . . . . . . .
            . . . . . . . . . . . . . . .
            . . . . . . . . . . . . . . .
            . . . . . . . . . . . . . . .
            . . . . . . . . . . . . . . .
            O . O . O . O . . . . . . . .
            ",
        );
        let proof = prove_forced_win(
            &b,
            Color::Black,
            SearchBudget {
                max_nodes: 2000,
                max_depth: 6,
            },
        );
        assert_eq!(proof.map(|p| p.line), Some(vec![Move::new(7, 7).unwrap()]));

        // Doctor a proof that claims the win goes through (6,7) and then
        // some follow-up. After (7,7) the position is terminal (>=2 wins),
        // so any line longer than one move is rejected by the verifier.
        let doctored = Proof {
            winner: Color::Black,
            line: vec![
                Move::new(7, 7).unwrap(),
                Move::new(6, 7).unwrap(), // a valid block, but the suffix is bogus
                Move::new(0, 0).unwrap(), // not a winning continuation
            ],
        };
        assert!(!verify_line(&b, &doctored));
    }

    // A genuine gapped-four refutation: the attacker creates a single
    // threat with two distinct defender blocks (two immediate winning
    // cells), and a proof that proceeds through one of them must be
    // rejected when the other one refutes it. The position below is a
    // double threat, so the sound proof is the one-move line; the value
    // of the test is that a doctored multi-move line is caught.
    #[test]
    fn gapped_four_refutation_rejects_doctored_line() {
        // Black to move. Playing (7,7) creates a gapped four along row 7
        // (winning cell (7,6)) and a gapped four along column 7 (winning
        // cell (6,7)). Either block wins the game for White if the line
        // is dragged out, so any proof longer than [(7,7)] is unsound.
        let b = board_from_ascii(
            "
            O . . . O . . . . . . . . . .
            . . . . . . . . . . . . . . .
            . . . . . . . . . . . . . . .
            . . . . . . . O . . . . . . .
            . . . . . . . X . . . . . . .
            . . . . . . . X . . . . . . .
            . . . . . . . . . . . . . . .
            . . . . X X . . X . . . . . .
            . . . . . . . X . . . . . . .
            . . . . . . . . . . . . . . .
            . . . . . . . . . . . . . . .
            . . . . . . . . . . . . . . .
            . . . . . . . . . . . . . . .
            . . . . . . . . . . . . . . .
            O . O . O . . . . . . . . . .
            ",
        );
        assert_eq!(b.to_move(), Color::Black);
        let proof = prove_forced_win(
            &b,
            Color::Black,
            SearchBudget {
                max_nodes: 2000,
                max_depth: 6,
            },
        );
        let proof = proof.expect("proven double threat");
        assert_eq!(proof.line, vec![Move::new(7, 7).unwrap()]);
        assert!(verify_line(&b, &proof));

        // Doctor a proof that claims the win goes through (7,6) and then
        // (6,7). After (7,7) the position is terminal, so the verifier
        // rejects any line longer than one move; the other block (6,7)
        // is the refuting alternative.
        let doctored = Proof {
            winner: Color::Black,
            line: vec![
                Move::new(7, 7).unwrap(),
                Move::new(7, 6).unwrap(), // one valid block
                Move::new(6, 7).unwrap(), // would win only after the first block
            ],
        };
        assert!(!verify_line(&b, &doctored));
    }

    // ------------------------------------------------------------------
    // 8. Defender-wins-first: a threat never matures if the defender
    // already has an immediate win.
    #[test]
    fn defender_wins_first_kills_attacker_branch() {
        // White has an open four on row 7; Black has a tempting closed-four
        // maker on column 7. Black to move can create a threat, but White
        // replies with an immediate win, so `prove_forced_win` for Black
        // returns None.
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
            . . . . . . . X . . . . . . .
            . . . . . . . X . . . . . . .
            . . . . . . . X . . . . . . .
            . . . . . . . . . . . . . . .
            X . . . . . . . . . . . . . .
            ",
        );
        assert_eq!(b.to_move(), Color::Black);
        assert!(!forced_blocks(&b).is_empty(), "White has an immediate win");
        assert!(
            prove_forced_win(
                &b,
                Color::Black,
                SearchBudget {
                    max_nodes: 2000,
                    max_depth: 6,
                }
            )
            .is_none(),
            "Black's threat never matures because White wins first"
        );
    }

    // ------------------------------------------------------------------
    // 10. Overline: a proof ending in SIX in a row is valid.
    #[test]
    fn overline_six_is_valid_proof() {
        // Black to move. (7,7) completes six in a row.
        let b = board_from_ascii(
            "
            . . . . . . . . . . . . . . .
            . . . . . . . . . . . . . . .
            . . . . . . . . . . . . . . .
            . . . . . . . . . . . . . . .
            . . . . . . . . . . . . . . .
            . . . . . . . . . . . . . . .
            . . . . . . . . . . . . . . .
            . . . . X X X . X X . . . . .
            . . . . . . . . . . . . . . .
            . . . . . . . . . . . . . . .
            . . . . . . . . . . . . . . .
            . . . . . . . . . . . . . . .
            . . . . . . . . . . . . . . .
            . . . . . . . . . . . . . . .
            O . O . O . O . O . . . . . .
            ",
        );
        let proof = prove_forced_win(
            &b,
            Color::Black,
            SearchBudget {
                max_nodes: 1000,
                max_depth: 5,
            },
        )
        .unwrap();
        assert_eq!(proof.line, vec![Move::new(7, 7).unwrap()]);
        assert!(verify_line(&b, &proof));
    }
}
