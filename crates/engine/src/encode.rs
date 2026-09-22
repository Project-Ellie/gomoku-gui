//! Burn-free 17×17 plane encoding, border ring set in the `you` plane.
//! Slice 6. See docs/13-engine-design.md, "Encoding".
//!
//! DESIGN DECISION (not a trick): transforms live in 15×15 space and
//! the border is added AFTER transformation. The border ring is
//! D4-invariant — rotating a ring gives the same ring — so
//! `encode(transform(b)) == embed(permute(inner_planes(b)))` holds
//! exactly, and no 289-entry permutation tables exist anywhere. The
//! commutation proptest in this file is the property the training
//! pipeline relies on ten thousand times per iteration: augmentation
//! must never change what a position MEANS.

use crate::bitboard::idx;
use crate::board::Board;

/// Side length of the encoded planes (17×17, including the border ring).
pub const EXT: usize = 17;

/// RELATIVE planes: `me` is always the side to move. (The ABSOLUTE
/// colors stay inside `Board` — ch. 13, decision 5; the network
/// always plays "me".)
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Planes {
    /// Stones of the side to move; the border ring is zero.
    pub me: [u8; EXT * EXT],
    /// Opponent stones; the one-cell border ring is set to one.
    pub you: [u8; EXT * EXT],
}

/// Encode a board into two 17×17 `u8` planes.
///
/// # Examples
///
/// ```
/// use engine::{Board, encode};
///
/// let p = encode(&Board::new());
/// assert_eq!(p.me.iter().filter(|&&x| x == 1).count(), 0);
/// assert_eq!(p.you.iter().filter(|&&x| x == 1).count(), 64);
/// ```
pub fn encode(b: &Board) -> Planes {
    let mut me = [0u8; EXT * EXT];
    let mut you = [0u8; EXT * EXT];

    // The border ring: a wall of "opponent" stones one cell thick.
    // A convolution is translation-equivariant — with the ring, the
    // filter sliding past the edge sees a WALL, which is semantically
    // true: no line continues through the border, exactly as through
    // an enemy stone (ch. 12 §7). 17*4 - 4 = 64 cells.
    for k in 0..EXT {
        you[k] = 1; // top row
        you[(EXT - 1) * EXT + k] = 1; // bottom row
        you[k * EXT] = 1; // left column
        you[k * EXT + (EXT - 1)] = 1; // right column
    }

    // Cell (r, c) lands at (r + 1) * 17 + (c + 1) — the one stride-15
    // to stride-17 crossing in the crate.
    let mover = b.to_move();
    for r in 0..15u8 {
        for c in 0..15u8 {
            let i = idx(r, c); // stride-16 bitboard index
            let dst = (r as usize + 1) * EXT + (c as usize + 1);
            if b.stones(mover).test(i) {
                me[dst] = 1;
            } else if b.stones(mover.other()).test(i) {
                you[dst] = 1;
            }
        }
    }
    Planes { me, you }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::board::Color;
    use crate::moveset::Move;
    use crate::symmetry::{Transform, transform_board_for_test};
    use proptest::prelude::*;

    fn mv(r: u8, c: u8) -> Move {
        Move::new(r, c).unwrap()
    }

    #[test]
    fn empty_board_has_empty_me_and_a_64_cell_border_ring() {
        let p = encode(&Board::new());
        assert!(p.me.iter().all(|&x| x == 0), "me must be all zero");
        assert_eq!(p.you.iter().filter(|&&x| x == 1).count(), 64, "17*4 - 4");
        // the ring, and ONLY the ring
        for r in 0..EXT {
            for c in 0..EXT {
                let on_ring = r == 0 || c == 0 || r == EXT - 1 || c == EXT - 1;
                assert_eq!(p.you[r * EXT + c], u8::from(on_ring), "at ({r}, {c})");
            }
        }
    }

    #[test]
    fn stones_land_relative_to_the_side_to_move() {
        // Black to move... so first give Black a stone and White one:
        // B(0,0), W(7,7). Now Black is to move: (0,0) is "me".
        let mut b = Board::new();
        b.play(mv(0, 0)).unwrap();
        b.play(mv(7, 7)).unwrap();
        assert_eq!(b.to_move(), Color::Black);
        let p = encode(&b);
        assert_eq!(p.me[EXT + 1], 1, "mover's (0,0) at index 18 of me");
        assert_eq!(p.you[8 * EXT + 8], 1, "opponent's (7,7) in you");

        // One ply earlier (White to move): Black's (0,0) is "you".
        let mut b1 = Board::new();
        b1.play(mv(0, 0)).unwrap();
        assert_eq!(b1.to_move(), Color::White);
        let p1 = encode(&b1);
        assert_eq!(p1.you[EXT + 1], 1, "opponent's (0,0) at index 18 of you");
        assert_eq!(p1.me[EXT + 1], 0);
    }

    /// The naive reference for the commutation property: extract the
    /// inner 15×15, permute it, re-embed with a fresh border ring.
    /// Deliberately written the dumb way — two implementations that
    /// disagree are a bug report.
    fn permute_planes17(p: &Planes, t: Transform) -> Planes {
        let mut me_inner = [0u8; 225];
        let mut you_inner = [0u8; 225];
        for r in 0..15usize {
            for c in 0..15usize {
                me_inner[r * 15 + c] = p.me[(r + 1) * EXT + (c + 1)];
                you_inner[r * 15 + c] = p.you[(r + 1) * EXT + (c + 1)];
            }
        }
        let me_perm = t.permute(&me_inner);
        let you_perm = t.permute(&you_inner);

        let mut out = Planes {
            me: [0; EXT * EXT],
            you: [0; EXT * EXT],
        };
        for k in 0..EXT {
            out.you[k] = 1;
            out.you[(EXT - 1) * EXT + k] = 1;
            out.you[k * EXT] = 1;
            out.you[k * EXT + (EXT - 1)] = 1;
        }
        for r in 0..15usize {
            for c in 0..15usize {
                out.me[(r + 1) * EXT + (c + 1)] = me_perm[r * 15 + c];
                out.you[(r + 1) * EXT + (c + 1)] = you_perm[r * 15 + c];
            }
        }
        out
    }

    proptest! {
        #![proptest_config(ProptestConfig::with_cases(10_000))]

        /// THE deliverable of this slice: encode commutes with every
        /// transform. Transform the board, then encode == encode, then
        /// permute the inner planes and re-add the border.
        #[test]
        fn encode_commutes_with_every_transform(
            cells in prop::collection::vec(0u16..225, 1..=60),
            t_idx in 0usize..8,
        ) {
            let mut b = Board::new();
            for p in cells {
                let _ = b.play(mv((p / 15) as u8, (p % 15) as u8));
            }
            let t = Transform::ALL[t_idx];
            prop_assert_eq!(
                encode(&transform_board_for_test(&b, t)),
                permute_planes17(&encode(&b), t),
            );
        }
    }
}
