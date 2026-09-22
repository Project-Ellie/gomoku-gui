//! Dihedral group D4: 8 transforms, const-generated permutation tables
//! over logical 15×15 indices.
//! Slice 6.

use crate::Move;

/// An element of the dihedral group D4 (the symmetries of a square).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Transform {
    /// Identity.
    Id,
    /// 90-degree counter-clockwise rotation.
    Rot90,
    /// 180-degree rotation.
    Rot180,
    /// 270-degree counter-clockwise rotation.
    Rot270,
    /// Reflection across the vertical axis.
    Flip,
    /// Flip followed by 90-degree rotation.
    FlipRot90,
    /// Flip followed by 180-degree rotation.
    FlipRot180,
    /// Flip followed by 270-degree rotation.
    FlipRot270,
}

/// rotate 90 degrees to the left
const fn rot90(i: u8) -> u8 {
    let (r, c) = (i / 15, i % 15);
    (14 - c) * 15 + r
}

/// Flip along the vertial axis
const fn flip(i: u8) -> u8 {
    let (r, c) = (i / 15, i % 15);
    r * 15 + (14 - c)
}

/// The Transforms as 8 tables over 0..225
pub(crate) const TRANSFORMS: [[u8; 225]; 8] = {
    let mut tables = [[0u8; 225]; 8];
    let mut i = 0usize;
    while i < 225 {
        let r90 = rot90(i as u8);
        let r180 = rot90(r90);
        let r270 = rot90(r180);
        let f = flip(i as u8);
        tables[0][i] = i as u8; // ID
        tables[1][i] = r90;
        tables[2][i] = r180;
        tables[3][i] = r270;
        tables[4][i] = f;
        tables[5][i] = rot90(f);
        tables[6][i] = rot90(tables[5][i]);
        tables[7][i] = rot90(tables[6][i]);
        i += 1;
    }
    tables
};

impl Transform {
    /// All eight D4 transforms, in the order used by `Transform::index`.
    pub const ALL: [Transform; 8] = [
        Transform::Id,
        Transform::Rot90,
        Transform::Rot180,
        Transform::Rot270,
        Transform::Flip,
        Transform::FlipRot90,
        Transform::FlipRot180,
        Transform::FlipRot270,
    ];

    fn index(self) -> usize {
        match self {
            Transform::Id => 0,
            Transform::Rot90 => 1,
            Transform::Rot180 => 2,
            Transform::Rot270 => 3,
            Transform::Flip => 4,
            Transform::FlipRot90 => 5,
            Transform::FlipRot180 => 6,
            Transform::FlipRot270 => 7,
        }
    }

    /// The inverse transform: `t.inverse().transform_move(t.transform_move(m)) == m`.
    pub fn inverse(self) -> Transform {
        match self {
            Transform::Id => Transform::Id,
            Transform::Rot90 => Transform::Rot270,
            Transform::Rot180 => Transform::Rot180,
            Transform::Rot270 => Transform::Rot90,
            f @ (Transform::Flip
            | Transform::FlipRot90
            | Transform::FlipRot180
            | Transform::FlipRot270) => f,
        }
    }

    /// Apply this transform to a board cell.
    pub fn transform_move(self, mv: Move) -> Move {
        let i = TRANSFORMS[self.index()][mv.index()];
        Move::new(i / 15, i % 15).expect("permutation tables contain only valid cells")
    }

    /// Permute a flat 15×15 array according to this transform.
    pub fn permute<T: Copy>(self, cells: &[T; 225]) -> [T; 225] {
        let table = &TRANSFORMS[self.index()];
        let mut out = *cells;
        for (i, &dst) in table.iter().enumerate() {
            out[dst as usize] = cells[i];
        }
        out
    }
}

#[cfg(test)]
pub(crate) fn transform_board_for_test(
    b: &crate::board::Board,
    t: Transform,
) -> crate::board::Board {
    let mut out = crate::board::Board::new();
    for &m in b.moves() {
        let _ = out.play(t.transform_move(m));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Board, Color, Status};
    use proptest::prelude::prop;
    use proptest::strategy::Strategy;
    use proptest::{prop_assert_eq, proptest};
    // just a couple of examples to verfiy the naive picture

    fn mv(r: u8, c: u8) -> Move {
        Move::new(r, c).unwrap()
    }
    #[test]
    fn corner_behaviour_is_what_we_expect_naively() {
        let upper_left = 0u8;
        let lower_left = (14 * 15) as u8;
        let upper_right = 14u8;
        let lower_right = lower_left + 14u8;
        let center = (7 * 15 + 7) as u8;

        assert_eq!(rot90(upper_left), lower_left);
        assert_eq!(rot90(upper_right), upper_left);
        assert_eq!(rot90(lower_right), upper_right);
        assert_eq!(rot90(lower_left), lower_right);

        assert_eq!(flip(lower_right), lower_left);
        assert_eq!(flip(upper_right), upper_left);

        assert_eq!(rot90(center), center);
        assert_eq!(flip(center), center);
    }

    #[test]
    fn rot90_four_times_and_flip_twice_are_identity() {
        let m = mv(3, 11);
        let r = Transform::Rot90;
        let res = r.transform_move(r.transform_move(r.transform_move(r.transform_move(m))));
        assert_eq!(m, res);
        let f = Transform::Flip;
        let res = f.transform_move(f.transform_move(m));
        assert_eq!(m, res);
    }

    #[test]
    fn inverse_undoes_every_transform_on_selected_moves() {
        let moves = [mv(0, 0), mv(4, 8), mv(11, 2), mv(5, 0)];
        for mv in moves {
            for t in Transform::ALL {
                assert_eq!(
                    mv,
                    t.inverse().transform_move(t.transform_move(mv)),
                    "{t:?} on {mv:?}"
                );
            }
        }
    }

    proptest! {
        /// Inverse roundtrip for ALL moves and all transforms.
        #[test]
        fn inverse_roundtrip_for_all_moves(cell in 0u8..225, t_idx in 0usize..8) {
            let m = mv(cell / 15, cell % 15);
            let t = Transform::ALL[t_idx];
            prop_assert_eq!(t.inverse().transform_move(t.transform_move(m)), m);
        }

        /// Win preservation, generated not filtered: fillers are drawn
        /// from the COMPLEMENT of the winning line, so no case is ever
        /// rejected. (prop_assume here rejected ~9% of cases — fine at
        /// 10k cases, but it exhausts proptest's global reject budget
        /// and aborts the test once PROPTEST_CASES gets big.)
        #[test]
        fn won_positions_stay_won_under_all_transforms(
            (five, fillers) in (0usize..4, 0u8..11, 0u8..11).prop_flat_map(|(dir, a, b)| {
                let (dr, dc) = [(0i32, 1i32), (1, 0), (1, -1), (1, 1)][dir];
                let (r0, c0) = (a, b + 4 * u8::from(dc != 1));
                let five: Vec<u8> = (0..5)
                    .map(|k| ((r0 as i32 + dr * k) * 15 + (c0 as i32 + dc * k)) as u8)
                    .collect();
                let free: Vec<u8> = (0u8..225).filter(|c| !five.contains(c)).collect();
                prop::collection::hash_set(prop::sample::select(free), 4)
                    .prop_map(move |s| {
                        let mut fillers: Vec<u8> = s.into_iter().collect();
                        fillers.sort_unstable();
                        (five.clone(), fillers)
                    })
            }),
        ) {
            let mut board = Board::new();
            for k in 0..4 {
                board.play(mv(five[k] / 15, five[k] % 15)).unwrap();
                board.play(mv(fillers[k] / 15, fillers[k] % 15)).unwrap();
            }
            board.play(mv(five[4] / 15, five[4] % 15)).unwrap();
            prop_assert_eq!(board.status(), Status::Won(Color::Black));

            for t in Transform::ALL {
                let tb = transform_board_for_test(&board, t);
                prop_assert_eq!(tb.status(), Status::Won(Color::Black), "{:?}", t);
            }
        }


    }
}
