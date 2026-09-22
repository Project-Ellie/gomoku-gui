//! Naive array-based reference engine — the differential-testing oracle.
//! Slice 2. Compiled only for tests and the `testutil` feature.

use crate::board::{Color, PlayError, Status};
use crate::moveset::{Move, MoveSet};

/// One board cell. Private - the outside world sees `Option<Color`
/// through `stone_at`; the `Cell` representation is our business.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Cell {
    Empty,
    Stone(Color),
}

/// the naive board 15x15 cells + full move history
pub struct Board {
    cells: [[Cell; 15]; 15],
    to_move: Color,
    status: Status,
    moves: Vec<Move>,
}

/// The four axes as (drow, dcol) step vectors. Each axis is walked in
/// both directions, for four entries cover all eight rays
const DIRECTIONS: [(i32, i32); 4] = [(0, 1), (1, 0), (1, 1), (1, -1)];

impl Board {
    pub fn new() -> Board {
        Board {
            cells: [[Cell::Empty; 15]; 15],
            to_move: Color::Black,
            status: Status::Ongoing,
            moves: Vec::new(),
        }
    }

    pub fn status(&self) -> Status {
        self.status
    }

    pub fn to_move(&self) -> Color {
        self.to_move
    }

    /// `None` = empty cell. Translates our private `Cell` into the
    /// public `Option<Color>` vocabulary.
    pub fn stone_at(&self, mv: Move) -> Option<Color> {
        match self.cells[mv.row() as usize][mv.col() as usize] {
            Cell::Empty => None,
            Cell::Stone(color) => Some(color),
        }
    }

    pub fn play(&mut self, mv: Move) -> Result<(), PlayError> {
        if self.status != Status::Ongoing {
            return Err(PlayError::GameOver);
        }
        if self.stone_at(mv).is_some() {
            return Err(PlayError::Occupied);
        }
        self.cells[mv.row() as usize][mv.col() as usize] = Cell::Stone(self.to_move);
        self.moves.push(mv);
        if self.wins_from(mv.row() as usize, mv.col() as usize, self.to_move) {
            self.status = Status::Won(self.to_move);
        } else if self.moves.len() == 225 {
            self.status = Status::Draw;
        }
        self.to_move = self.to_move.other();
        Ok(())
    }

    fn wins_from(&self, r: usize, c: usize, color: Color) -> bool {
        wins_from_cells(&self.cells, r, c, color)
    }

    pub fn count_dir(&self, r: usize, c: usize, dr: i32, dc: i32, color: Color) -> usize {
        count_dir_cells(&self.cells, r, c, dr, dc, color)
    }

    pub fn moves(&self) -> &[Move] {
        &self.moves
    }

    /// All legal empty cells, in row-major order.
    pub fn empty_moves(&self) -> impl Iterator<Item = Move> + '_ {
        (0..15u8).flat_map(move |r| {
            (0..15u8).filter_map(move |c| {
                if self.cells[r as usize][c as usize] == Cell::Empty {
                    Some(Move::new(r, c).unwrap())
                } else {
                    None
                }
            })
        })
    }

    /// Undoes the last move by REPLAYING the remaining history onto a
    /// fresh board. O(n) where the fast undo is O(1) — and obviously
    /// correct, which is the only virtue an oracle needs.
    pub fn undo(&mut self) {
        self.moves.pop().expect("undo with non-empty history");
        // A prefix of a legal game is always replayable: a game ends
        // only ON its winning/drawing move, and we just removed it.
        let history = self.moves.clone();
        *self = Board::new();
        for mv in history {
            self.play(mv).expect("replaying a valid history");
        }
    }
}

impl Default for Board {
    fn default() -> Self {
        Self::new()
    }
}

// --- Slice 7: naive tactics oracle + the ASCII puzzle parser ---------

/// Free-function twin of `Board::wins_from`, so the naive tactics can
/// scan HYPOTHETICAL cell arrays. Counts the cell (r, c) itself as 1 —
/// exactly the "place a stone here" semantics.
fn wins_from_cells(cells: &[[Cell; 15]; 15], r: usize, c: usize, color: Color) -> bool {
    DIRECTIONS.iter().any(|&(dr, dc)| {
        1 + count_dir_cells(cells, r, c, dr, dc, color)
            + count_dir_cells(cells, r, c, -dr, -dc, color)
            >= 5
    })
}

fn count_dir_cells(
    cells: &[[Cell; 15]; 15],
    r: usize,
    c: usize,
    dr: i32,
    dc: i32,
    color: Color,
) -> usize {
    let mut n = 0;
    let mut nr = r as i32 + dr;
    let mut nc = c as i32 + dc;
    while (0..15).contains(&nr)
        && (0..15).contains(&nc)
        && cells[nr as usize][nc as usize] == Cell::Stone(color)
    {
        n += 1;
        nr += dr;
        nc += dc;
    }
    n
}

/// Replays a fast board's history into the naive representation.
fn naive_from(b: &crate::board::Board) -> Board {
    let mut nb = Board::new();
    for &mv in b.moves() {
        nb.play(mv).expect("a valid fast history replays naively");
    }
    nb
}

/// Naive `immediate_wins`: line scans through each empty cell.
///
/// VALID ON NON-TERMINAL BOARDS ONLY. Once a five exists, the fast
/// whole-board scan (`has_any_five`) and this through-cell scan answer
/// different questions — that divergence is precisely why
/// `double_threats` excludes immediate wins (see tactics.rs).
pub fn naive_immediate_wins(b: &crate::board::Board, side: Color) -> MoveSet {
    let nb = naive_from(b);
    let mut out = MoveSet::EMPTY;
    for r in 0..15 {
        for c in 0..15 {
            if nb.cells[r][c] == Cell::Empty && wins_from_cells(&nb.cells, r, c, side) {
                out.insert(Move::new(r as u8, c as u8).unwrap());
            }
        }
    }
    out
}

/// Naive `forced_blocks`. Same non-terminal precondition.
pub fn naive_forced_blocks(b: &crate::board::Board) -> MoveSet {
    naive_immediate_wins(b, b.to_move().other())
}

/// Naive `double_threats`: place each candidate, count cells that
/// would complete five, keep the candidates with two or more.
/// Immediate wins are skipped — same exclusion as the fast version.
/// Same non-terminal precondition.
pub fn naive_double_threats(b: &crate::board::Board, side: Color) -> MoveSet {
    let nb = naive_from(b);
    let mut out = MoveSet::EMPTY;
    for r in 0..15usize {
        for c in 0..15usize {
            if nb.cells[r][c] != Cell::Empty {
                continue;
            }
            if wins_from_cells(&nb.cells, r, c, side) {
                continue; // immediate win: ends the game, not a threat
            }
            let mut hypo = nb.cells;
            hypo[r][c] = Cell::Stone(side);
            let mut wins = 0;
            for r2 in 0..15 {
                for c2 in 0..15 {
                    if hypo[r2][c2] == Cell::Empty && wins_from_cells(&hypo, r2, c2, side) {
                        wins += 1;
                    }
                }
            }
            if wins >= 2 {
                out.insert(Move::new(r as u8, c as u8).unwrap());
            }
        }
    }
    out
}

/// Parses rows of `X` / `O` / `.` into a fast Board (Black = X,
/// White = O): 15 whitespace-separated cells per row, 15 non-blank
/// rows (blank lines are tolerated, so raw-string literals can
/// indent). Stone counts must be reachable by alternating play from
/// Black: equal counts (Black to move — Black wins ties) or one more
/// X (White to move). The parser is strict: it replays stones in
/// parse order and rejects counts that are not alternating-replay
/// reachable (e.g., 2 Black with 0 White). The position must be
/// non-terminal; a completed five (overlines included) aborts the
/// replay with a panic. Panics on malformed input — test code is
/// allowed to panic.
pub fn board_from_ascii(rows: &str) -> crate::board::Board {
    let mut xs: Vec<(u8, u8)> = Vec::new();
    let mut os: Vec<(u8, u8)> = Vec::new();
    let mut nrows = 0usize;
    for line in rows.lines() {
        let tokens: Vec<&str> = line.split_whitespace().collect();
        if tokens.is_empty() {
            continue;
        }
        let r = nrows;
        assert!(r < 15, "more than 15 rows");
        assert_eq!(
            tokens.len(),
            15,
            "row {r} has {} cells, expected 15",
            tokens.len()
        );
        for (c, t) in tokens.iter().enumerate() {
            match *t {
                "X" => xs.push((r as u8, c as u8)),
                "O" => os.push((r as u8, c as u8)),
                "." => {}
                other => panic!("row {r}, col {c}: unexpected token {other:?} (want X, O, or .)"),
            }
        }
        nrows += 1;
    }
    assert_eq!(nrows, 15, "expected 15 rows, got {nrows}");
    assert!(
        xs.len() == os.len() || xs.len() == os.len() + 1,
        "unreachable stone counts: {} X vs {} O (alternating play from Black first)",
        xs.len(),
        os.len()
    );

    // Replay in parse order, strictly alternating X/O. Cells are
    // distinct by construction, so the only way `play` can fail is a
    // completed five mid-history — i.e. a broken puzzle, which is
    // exactly what a test helper should report.
    let mut b = crate::board::Board::new();
    for k in 0..os.len() {
        let (r, c) = xs[k];
        b.play(Move::new(r, c).unwrap())
            .expect("puzzle contains a five (X)?");
        let (r, c) = os[k];
        b.play(Move::new(r, c).unwrap())
            .expect("puzzle contains a five (O)?");
    }
    if xs.len() > os.len() {
        let &(r, c) = xs.last().unwrap();
        b.play(Move::new(r, c).unwrap())
            .expect("puzzle contains a five (X)?");
    }
    b
}

// --- Slice 8: depth-bounded adjudicator (differential oracle) ------

/// Depth-bounded, threat-space adjudicator using the line-scan oracle.
/// Returns `Some(side)` if `side` can force a win within `max_depth`
/// plies, `Some(side.other())` if the defender can refute, and `None`
/// if the answer is beyond the horizon. This is intentionally a
/// separate implementation from `tss::prove_forced_win`: it validates
/// the bitboard prover against the naive board + cell-walk tactics.
pub fn adjudicate(b: &crate::board::Board, side: Color, max_depth: u8) -> Option<Color> {
    let mut nb = naive_from(b);
    adjudicate_naive(&mut nb, side, max_depth)
}

/// Immediate wins on the naive `reference::Board` (no conversion).
fn immediate_wins_naive(b: &Board, side: Color) -> MoveSet {
    let mut out = MoveSet::EMPTY;
    for r in 0..15 {
        for c in 0..15 {
            if b.cells[r][c] == Cell::Empty && wins_from_cells(&b.cells, r, c, side) {
                out.insert(Move::new(r as u8, c as u8).unwrap());
            }
        }
    }
    out
}

fn adjudicate_naive(b: &mut Board, side: Color, depth: u8) -> Option<Color> {
    match b.status() {
        Status::Won(c) => return Some(c),
        Status::Draw => return None,
        Status::Ongoing => {}
    }
    if depth == 0 {
        return None;
    }

    let to_move = b.to_move();
    if to_move == side {
        // Attacker: immediate win, then threat-generating moves.
        let wins = immediate_wins_naive(b, side);
        if !wins.is_empty() {
            return Some(side);
        }

        let empties: Vec<Move> = b.empty_moves().collect();
        for mv in empties {
            b.play(mv).ok()?;

            if b.status() == Status::Won(side) {
                b.undo();
                return Some(side);
            }

            let defender_wins = immediate_wins_naive(b, side.other());
            let attacker_wins = immediate_wins_naive(b, side);

            if !defender_wins.is_empty() {
                b.undo();
                continue;
            }
            if attacker_wins.len() >= 2 {
                b.undo();
                return Some(side);
            }
            if attacker_wins.is_empty() {
                b.undo();
                continue;
            }

            // Exactly one threat: defender must block every winning cell.
            let blocks: Vec<Move> = attacker_wins.iter().collect();
            let mut all_win = true;
            for block in &blocks {
                b.play(*block).ok()?;
                let sub = adjudicate_naive(b, side, depth - 1);
                b.undo();
                if sub != Some(side) {
                    all_win = false;
                    break;
                }
            }
            b.undo();
            if all_win {
                return Some(side);
            }
        }
        None
    } else {
        // Defender: replies are the attacker's immediate wins.
        let blocks = immediate_wins_naive(b, side);
        if blocks.is_empty() {
            // No immediate threat to address; the position is quiet.
            return None;
        }

        // If every block still lets the attacker force a win, defender loses.
        for block in blocks.iter() {
            b.play(block).ok()?;
            let sub = adjudicate_naive(b, side, depth - 1);
            b.undo();
            if sub != Some(side) {
                return sub;
            }
        }
        Some(side)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::moveset::Move;

    #[test]
    fn new_board_is_empty_ongoing_black_to_move() {
        let b = Board::new();
        assert_eq!(b.status(), Status::Ongoing);
        assert_eq!(b.to_move(), Color::Black);
        assert_eq!(b.stone_at(Move::new(7, 7).unwrap()), None);
        assert!(b.moves().is_empty());
    }

    #[test]
    fn play_places_stone_and_flips_to_move() {
        let mut b = Board::new();
        let mv = Move::new(7, 7).unwrap();
        b.play(mv).unwrap();
        assert_eq!(b.stone_at(mv), Some(Color::Black));
        assert_eq!(b.to_move(), Color::White);
    }

    #[test]
    fn play_on_occupied_cell_is_rejected() {
        let mut b = Board::new();
        let mv = Move::new(5, 5).unwrap();
        b.play(mv).unwrap();

        assert_eq!(b.play(mv), Err(PlayError::Occupied));
        assert_eq!(b.stone_at(mv), Some(Color::Black));
        assert_eq!(b.to_move(), Color::White);
        assert_eq!(b.moves().len(), 1);
    }

    #[test]
    fn horizontal_five_wins_for_black() {
        let mut b = Board::new();
        let script = [
            (7, 3),
            (0, 0),
            (7, 4),
            (0, 2),
            (7, 5),
            (0, 4),
            (7, 6),
            (0, 6),
            (7, 7),
        ];
        for (r, c) in script {
            b.play(Move::new(r, c).unwrap()).unwrap();
        }
        assert_eq!(b.status(), Status::Won(Color::Black));
    }

    #[test]
    fn horizontal_five_wins_for_white() {
        let mut b = Board::new();
        // White wins, so White makes the last move: the script ends
        // on a White stone. Black's junk sits on rank 12 with gaps.
        let script = [
            (12, 0),
            (4, 4),
            (12, 2),
            (4, 5),
            (12, 4),
            (4, 6),
            (12, 6),
            (4, 7),
            (12, 8),
            (4, 8),
        ];
        for (r, c) in script {
            b.play(Move::new(r, c).unwrap()).unwrap();
        }
        assert_eq!(b.status(), Status::Won(Color::White));
    }

    #[test]
    fn vertical_five_wins() {
        let mut b = Board::new();
        let script = [
            (2, 5),
            (0, 0),
            (3, 5),
            (0, 2),
            (4, 5),
            (0, 4),
            (5, 5),
            (0, 6),
            (6, 5),
        ];
        for (r, c) in script {
            b.play(Move::new(r, c).unwrap()).unwrap();
        }
        assert_eq!(b.status(), Status::Won(Color::Black));
    }

    #[test]
    fn diagonal_down_right_five_wins() {
        let mut b = Board::new();
        let script = [
            (2, 2),
            (0, 0),
            (3, 3),
            (0, 2),
            (4, 4),
            (0, 4),
            (5, 5),
            (0, 6),
            (6, 6),
        ];
        for (r, c) in script {
            b.play(Move::new(r, c).unwrap()).unwrap();
        }
        assert_eq!(b.status(), Status::Won(Color::Black));
    }

    #[test]
    fn diagonal_down_left_five_wins() {
        let mut b = Board::new();
        let script = [
            (2, 8),
            (0, 0),
            (3, 7),
            (0, 2),
            (4, 6),
            (0, 4),
            (5, 5),
            (0, 6),
            (6, 4),
        ];
        for (r, c) in script {
            b.play(Move::new(r, c).unwrap()).unwrap();
        }
        assert_eq!(b.status(), Status::Won(Color::Black));
    }

    #[test]
    fn overline_six_counts_as_a_win() {
        let mut b = Board::new();
        // Black builds the two arms (7,3)-(7,4) and (7,6)-(7,8),
        // then bridges them with (7,5). White junks on rank 0, gapped.
        let script = [
            (7, 3),
            (0, 0),
            (7, 4),
            (0, 2),
            (7, 6),
            (0, 4),
            (7, 7),
            (0, 6),
            (7, 8),
            (0, 8),
            // Black's runs so far: length 2 and 3. No five yet!
            (7, 5), // the bridge — six in a row, all at once
        ];
        for (r, c) in script {
            b.play(Move::new(r, c).unwrap()).unwrap();
        }
        // Freestyle rules: overlines count (ch. 13, decision 1).
        assert_eq!(b.status(), Status::Won(Color::Black));
    }

    #[test]
    fn four_in_a_row_is_still_ongoing() {
        let mut b = Board::new();
        let script = [
            (7, 3),
            (0, 0),
            (7, 4),
            (0, 2),
            (7, 5),
            (0, 4),
            (7, 6),
            (0, 6),
        ];
        for (r, c) in script {
            b.play(Move::new(r, c).unwrap()).unwrap();
        }
        assert_eq!(b.status(), Status::Ongoing);
        assert_eq!(b.to_move(), Color::Black); // and the game continues
    }

    #[test]
    fn five_in_the_top_left_corner_wins() {
        let mut b = Board::new();
        // Black fills rank 0 starting at column 0: the win walk runs
        // into the left edge and must stop cleanly.
        let script = [
            (0, 0),
            (7, 7),
            (0, 1),
            (7, 9),
            (0, 2),
            (9, 7),
            (0, 3),
            (9, 9),
            (0, 4),
        ];
        for (r, c) in script {
            b.play(Move::new(r, c).unwrap()).unwrap();
        }
        assert_eq!(b.status(), Status::Won(Color::Black));
    }

    #[test]
    fn five_on_the_bottom_edge_wins_for_white() {
        let mut b = Board::new();
        // White wins along rank 14, ending in the bottom-right corner.
        let script = [
            (0, 0),
            (14, 10),
            (0, 2),
            (14, 11),
            (0, 4),
            (14, 12),
            (0, 6),
            (14, 13),
            (0, 8),
            (14, 14),
        ];
        for (r, c) in script {
            b.play(Move::new(r, c).unwrap()).unwrap();
        }
        assert_eq!(b.status(), Status::Won(Color::White));
    }

    #[test]
    fn full_board_without_five_is_a_draw() {
        let mut b = Board::new();

        // Split the 225 cells into black/white by the stripe pattern:
        //   row pattern BBWW BBWW …, inverted on odd rows.
        let mut blacks = Vec::new(); // 113 cells
        let mut whites = Vec::new(); // 112 cells
        for r in 0..15u8 {
            for c in 0..15u8 {
                let stripe = (c % 4) < 2; // BBWW repeating
                let black = stripe != (r % 2 == 1); // inverted on odd rows
                if black {
                    blacks.push(Move::new(r, c).unwrap());
                } else {
                    whites.push(Move::new(r, c).unwrap());
                }
            }
        }

        // Alternate strictly: Black, White, Black, White, …
        for i in 0..112 {
            b.play(blacks[i]).unwrap();
            b.play(whites[i]).unwrap();
        }
        assert_eq!(b.status(), Status::Ongoing); // 224 moves: still playing

        b.play(blacks[112]).unwrap(); // move 225 — board full
        assert_eq!(b.status(), Status::Draw);
        assert_eq!(b.moves().len(), 225);

        // A decided game rejects further moves — even though no cell
        // is free, GameOver (checked first) is the honest answer.
        assert_eq!(b.play(blacks[0]), Err(PlayError::GameOver));
    }

    #[test]
    fn play_after_a_won_game_is_rejected() {
        let mut b = Board::new();
        let script = [
            (7, 3),
            (0, 0),
            (7, 4),
            (0, 2),
            (7, 5),
            (0, 4),
            (7, 6),
            (0, 6),
            (7, 7),
        ];
        for (r, c) in script {
            b.play(Move::new(r, c).unwrap()).unwrap();
        }
        assert_eq!(b.status(), Status::Won(Color::Black));

        // The board is full of empty cells — but the game is over.
        assert_eq!(b.play(Move::new(13, 13).unwrap()), Err(PlayError::GameOver));
    }

    #[test]
    fn moves_returns_history_in_play_order() {
        let mut b = Board::new();
        let script = [(7, 7), (3, 3), (7, 8), (3, 4)];
        for (r, c) in script {
            b.play(Move::new(r, c).unwrap()).unwrap();
        }

        let expected: Vec<Move> = [(7, 7), (3, 3), (7, 8), (3, 4)]
            .iter()
            .map(|&(r, c)| Move::new(r, c).unwrap())
            .collect();
        assert_eq!(b.moves(), expected.as_slice());
    }

    #[test]
    fn undo_replays_history_without_the_last_move() {
        let mut b = Board::new();
        let script = [
            (7, 3),
            (0, 0),
            (7, 4),
            (0, 2),
            (7, 5),
            (0, 4),
            (7, 6),
            (0, 6),
            (7, 7),
        ];
        for (r, c) in script {
            b.play(Move::new(r, c).unwrap()).unwrap();
        }
        assert_eq!(b.status(), Status::Won(Color::Black));

        b.undo();
        assert_eq!(b.status(), Status::Ongoing);
        assert_eq!(b.stone_at(Move::new(7, 7).unwrap()), None);
        assert_eq!(b.moves().len(), 8);
    }

    #[test]
    fn ascii_parser_infers_black_to_move_on_equal_counts() {
        let b = board_from_ascii(
            "
            X . . . . . . . . . . . . . .
            . . . . . . . . . . . . . . .
            . . . . . . . . . . . . . . .
            . . . . . . . . . . . . . . .
            . . . . . . . . . . . . . . .
            . . . . . . . . . . . . . . .
            . . . . . . . . . . . . . . .
            . . . . O . . . . . . . . . .
            . . . . . . . . . . . . . . .
            . . . . . . . . . . . . . . .
            . . . . . . . . . . . . . . .
            . . . . . . . . . . . . . . .
            . . . . . . . . . . . . . . .
            . . . . . . . . . . . . . . .
            . . . . . . . . . . . . . . .
            ",
        );
        assert_eq!(b.to_move(), Color::Black);
        assert_eq!(b.stone_at(Move::new(0, 0).unwrap()), Some(Color::Black));
        assert_eq!(b.stone_at(Move::new(7, 4).unwrap()), Some(Color::White));
        assert_eq!(b.stone_at(Move::new(1, 1).unwrap()), None);
    }

    #[test]
    fn ascii_parser_infers_white_to_move_when_black_leads() {
        let b = board_from_ascii(
            "
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
            . . . . . . . . . . . . . . .
            . . . . . . . . . . . . . . .
            . . . . . . . . . . . . . . .
            . . . . . . . . . . . . . . .
            ",
        );
        assert_eq!(b.to_move(), Color::White);
        assert_eq!(b.stone_at(Move::new(0, 0).unwrap()), Some(Color::Black));
    }

    #[test]
    #[should_panic(expected = "expected 15")]
    fn ascii_parser_rejects_a_short_row() {
        board_from_ascii(
            "
            X . . . . . . . . . . . . . .
            . . . . . . . . . . . . . .
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
            ",
        );
    }

    #[test]
    #[should_panic(expected = "unreachable stone counts")]
    fn ascii_parser_rejects_impossible_counts() {
        board_from_ascii(
            "
            X X . . . . . . . . . . . . .
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
            . . . . . . . . . . . . . . .
            ",
        );
    }
}
