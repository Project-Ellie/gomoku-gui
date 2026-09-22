//! Win detection: staged two/four/five shift-AND, overlines count.
//! Slice 4. See docs/13-engine-design.md, "Win detection".

use crate::bitboard::Bitboard;

pub(crate) const DIRS: [u32; 4] = [1, 16, 15, 17];

pub(crate) fn has_five_in_dir(b: &Bitboard, s: u32) -> bool {
    let two = *b & b.shr(s);
    let four = two & two.shr(2 * s);
    let five = four & four.shr(s);
    !five.is_zero()
}

pub(crate) fn has_any_five(b: &Bitboard) -> bool {
    DIRS.iter().any(|&s| has_five_in_dir(b, s))
}
#[cfg(test)]
mod tests {
    use super::*;
    use crate::bitboard::{Bitboard, VALID, idx};
    use crate::{Board, Move, Status};
    use proptest::{prop_assert, prop_assert_eq, proptest};

    /// scans the board for contiguous lines of 5 with a naive counting approach
    /// returns true if there is at least one such line on the board, false otherwise
    fn count_walk(b: &Bitboard, s: u32) -> bool {
        let (dr, dc) = match s {
            1 => (0i32, 1i32),
            16 => (1, 0),
            15 => (1, -1),
            17 => (1, 1),
            _ => unreachable!("direction is one of DIRS"),
        };
        for r in 0..15i32 {
            for c in 0..15i32 {
                if !b.test(idx(r as u8, c as u8)) {
                    continue;
                }
                let five = (0..5).all(|k| {
                    let (rr, cc) = (r + dr * k, c + dc * k);
                    (0..15).contains(&rr)
                        && (0..15).contains(&cc)
                        && b.test(idx(rr as u8, cc as u8))
                });
                if five {
                    return true;
                }
            }
        }
        false
    }

    fn from_coords(coords: &[(u8, u8)]) -> Bitboard {
        coords
            .iter()
            .fold(Bitboard::EMPTY, |b, &(r, c)| b.with_bit(idx(r, c)))
    }

    #[test]
    fn finds_horizonal_five() {
        let some_five = from_coords(&[(7, 5), (7, 6), (7, 7), (7, 8), (7, 9)]);

        assert!(has_five_in_dir(&some_five, 1));

        assert!(has_any_five(&some_five));
    }

    #[test]
    fn finds_any_five() {
        let cases: &[(&str, &[(u8, u8)])] = &[
            ("horizontal", &[(4, 4), (4, 5), (4, 6), (4, 7), (4, 8)]),
            ("vertical", &[(5, 5), (4, 6), (3, 7), (2, 8), (1, 9)]),
            ("diag down right", &[(4, 4), (5, 5), (6, 6), (7, 7), (8, 8)]),
            ("diag down left", &[(4, 8), (5, 8), (6, 8), (7, 8), (8, 8)]),
        ];
        for (name, coords) in cases {
            assert!(has_any_five(&from_coords(coords)), "{name}");
        }
    }

    #[test]
    fn detects_fives_along_every_edge() {
        let cases: &[(&str, &[(u8, u8)])] = &[
            ("row 0, cols 0-4", &[(0, 0), (0, 1), (0, 2), (0, 3), (0, 4)]),
            (
                "row 14, cols 10-14",
                &[(14, 10), (14, 11), (14, 12), (14, 13), (14, 14)],
            ),
            (
                "col 0, rows 10-14",
                &[(10, 0), (11, 0), (12, 0), (13, 0), (14, 0)],
            ),
            (
                "col 14, rows 0-4",
                &[(0, 14), (1, 14), (2, 14), (3, 14), (4, 14)],
            ),
            (
                "corner diagonal (0,0)-(4,4)",
                &[(0, 0), (1, 1), (2, 2), (3, 3), (4, 4)],
            ),
            (
                "corner diagonal (0,14)-(4,10)",
                &[(0, 14), (1, 13), (2, 12), (3, 11), (4, 10)],
            ),
        ];
        for (name, cells) in cases {
            assert!(has_any_five(&from_coords(cells)), "{name}");
        }
    }

    #[test]
    fn overlines_count_and_near_misses_do_not() {
        let six: Vec<(u8, u8)> = (3..9).map(|c| (7u8, c)).collect();
        let nine: Vec<(u8, u8)> = (0..9).map(|c| (7u8, c)).collect();
        assert!(has_any_five(&from_coords(&six)), "six in a row");
        assert!(has_any_five(&from_coords(&nine)), "nine in a row");
        assert!(
            !has_any_five(&from_coords(&[(7, 3), (7, 4), (7, 5), (7, 6)])),
            "four in a row"
        );
        assert!(
            !has_any_five(&from_coords(&[(7, 3), (7, 4), (7, 6), (7, 7), (7, 8)])),
            "five with a gap"
        );
        assert!(!has_any_five(&Bitboard::EMPTY), "empty board");
        assert!(has_any_five(&VALID), "every cell filled");
    }

    #[test]
    fn wrap_pattern_is_not_a_five() {
        let wrap = &[(7, 12), (7, 13), (7, 14), (8, 0), (8, 1)];
        assert!(!has_any_five(&from_coords(wrap)));
    }

    #[test]
    fn wrap_attack_is_not_a_win() {
        let mut board = Board::new();
        let script = [
            (7, 12),
            (0, 0),
            (7, 13),
            (0, 1),
            (7, 14),
            (0, 2),
            (8, 0),
            (0, 3),
            (8, 1),
        ];
        for (r, c) in script {
            board.play(Move::new(r, c).unwrap()).unwrap();
        }
        assert_eq!(board.status(), Status::Ongoing);
    }

    proptest! {
        /// Plant a run of five inside the window where it fits, and a
        /// four-run: the four must NOT be a five, the five must be found,
        /// and the walk-based oracle must agree.
        ///
        /// Note the sampling: `a`/`b` are mapped into the legal start
        /// window instead of sampled freely plus `prop_assume!`. Assuming
        /// away ~45% of draws aborts the test at proptest's 1024 global
        /// rejects ("Too many global rejects") long before any real bug
        /// shows up.
        #[test]
        fn planted_runs_are_found(dir in 0usize..4, a in 0u8..11, b in 0u8..11) {
            let (dr, dc) = [(0i32, 1i32), (1, 0), (1, -1), (1, 1)][dir];
            let (r0, c0) = (a, b + 4 * u8::from(dc != 1));
            let cells: Vec<(u8, u8)> = (0..5)
                .map(|k| ((r0 as i32 + dr * k) as u8, (c0 as i32 + dc * k) as u8))
                .collect();
            prop_assert!(cells.iter().all(|&(r, c)| r < 15 && c < 15));

            let four = from_coords(&cells[..4]);
            let five = from_coords(&cells);
            prop_assert!(!has_any_five(&four), "four in a row is not a five");
            prop_assert!(has_any_five(&five));
            prop_assert_eq!(has_any_five(&five), count_walk(&five, DIRS[dir]));
        }

        /// Random stone sets: the bit trick and the walk-based oracle
        /// must agree on every one of them.
        #[test]
        fn agrees_with_the_slow_oracle(cells in proptest::collection::hash_set(0u8..225, 0..30)) {
            let bb = cells.iter().fold(Bitboard::EMPTY, |b, &i| {
                b.with_bit(((i / 15) as usize) * 16 + (i % 15) as usize)
            });
            prop_assert_eq!(
                has_any_five(&bb),
                DIRS.iter().any(|&s| count_walk(&bb, s)),
                "bitboard says {}, oracle says {}",
                has_any_five(&bb),
                DIRS.iter().any(|&s| count_walk(&bb, s))
            );
        }
    }
}
