//! The game record: writes JSON schema v3, reads linear v1/v2 and tree v3.

use engine::{Color, Move};
use serde::{Deserialize, Serialize};
use time::OffsetDateTime;

use crate::error::{GameError, RecordError};
use crate::game::{Game, Outcome, WinMethod};

/// The value of the `format` field of a record.
pub const FORMAT_MARKER: &str = "gomoku-gui/record";
/// The record schema version this build writes. It also reads v1 and v2.
pub const FORMAT_VERSION: u32 = 3;
/// The board size this application plays.
pub const BOARD_SIZE: u32 = 15;
/// The rule set this application plays.
pub const RULESET: &str = "freestyle";
/// The number of cells on a 15x15 board.
const CELLS: usize = 225;
/// A defensive bound on total branches, not on a single 225-cell line.
const MAX_TREE_NODES: usize = 10_000;

/// The player names of a record.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Players {
    /// The name of the Black player, if given.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub black: Option<String>,
    /// The name of the White player, if given.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub white: Option<String>,
}

/// A stone color in a record.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum StoredColor {
    /// Black, the first player.
    Black,
    /// White, the second player.
    White,
}

/// A way to win a game in a record.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum StoredMethod {
    /// Five or more stones in a line.
    Five,
    /// The other player resigned.
    Resignation,
}

/// The result of a game in a record.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "lowercase")]
pub enum StoredOutcome {
    /// The game is still open.
    Ongoing,
    /// A player has won.
    Won {
        /// The winner.
        winner: StoredColor,
        /// How the game was won.
        method: StoredMethod,
    },
    /// The board is full with no line.
    Draw,
}

impl From<Color> for StoredColor {
    fn from(color: Color) -> StoredColor {
        match color {
            Color::Black => StoredColor::Black,
            Color::White => StoredColor::White,
        }
    }
}

impl From<StoredColor> for Color {
    fn from(color: StoredColor) -> Color {
        match color {
            StoredColor::Black => Color::Black,
            StoredColor::White => Color::White,
        }
    }
}

impl From<Outcome> for StoredOutcome {
    fn from(outcome: Outcome) -> StoredOutcome {
        match outcome {
            Outcome::Ongoing => StoredOutcome::Ongoing,
            Outcome::Draw => StoredOutcome::Draw,
            Outcome::Won { winner, method } => StoredOutcome::Won {
                winner: winner.into(),
                method: match method {
                    WinMethod::Five => StoredMethod::Five,
                    WinMethod::Resignation => StoredMethod::Resignation,
                },
            },
        }
    }
}

impl From<StoredOutcome> for Outcome {
    fn from(outcome: StoredOutcome) -> Outcome {
        match outcome {
            StoredOutcome::Ongoing => Outcome::Ongoing,
            StoredOutcome::Draw => Outcome::Draw,
            StoredOutcome::Won { winner, method } => Outcome::Won {
                winner: winner.into(),
                method: match method {
                    StoredMethod::Five => WinMethod::Five,
                    StoredMethod::Resignation => WinMethod::Resignation,
                },
            },
        }
    }
}

/// One node in a version 3 move tree. Node zero is the empty-board root.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct NodeRecord {
    /// Parent node ID, or null for the root.
    pub parent: Option<u32>,
    /// The move as `[column, row]`, or null for the root.
    pub point: Option<[u8; 2]>,
    /// The child followed by redo, or null for a leaf.
    pub current: Option<u32>,
    /// The side that resigned at this node, if this branch ended by resignation.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub resigned: Option<StoredColor>,
}

/// A game record as it is stored on disk.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Record {
    /// The format marker. Always [`FORMAT_MARKER`].
    pub format: String,
    /// The schema version: 1, 2 or 3 on read; [`FORMAT_VERSION`] for new records.
    pub version: u32,
    /// The board size. Always [`BOARD_SIZE`].
    pub size: u32,
    /// The rule set. Always `freestyle`.
    pub ruleset: String,
    /// The player names.
    #[serde(default)]
    pub players: Players,
    /// When the game started, in UTC.
    #[serde(with = "time::serde::rfc3339")]
    pub created: OffsetDateTime,
    /// The result at the selected branch's tip.
    pub result: StoredOutcome,
    /// The selected branch as `[column, row]` pairs. In v1/v2 this is the
    /// complete linear game; in v3 it is checked against `tree`.
    pub moves: Vec<[u8; 2]>,
    /// All nodes, in parent-before-child order. Empty in linear v1/v2 files.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub tree: Vec<NodeRecord>,
}

impl Record {
    /// Build a v3 record of the full tree and current branch, even during review.
    ///
    /// # Errors
    /// `RecordError::FromPosition` if the game was loaded from a puzzle
    /// position, because the moves-only record format cannot represent the
    /// starting stones.
    pub fn from_game(game: &Game) -> Result<Record, RecordError> {
        if game.is_from_position() {
            return Err(RecordError::FromPosition);
        }
        if game.tree_nodes().len() > MAX_TREE_NODES {
            return Err(RecordError::TooManyNodes {
                count: game.tree_nodes().len(),
            });
        }
        let meta = game.meta();
        Ok(Record {
            format: FORMAT_MARKER.to_string(),
            version: FORMAT_VERSION,
            size: BOARD_SIZE,
            ruleset: RULESET.to_string(),
            players: Players {
                black: meta.black.clone(),
                white: meta.white.clone(),
            },
            created: meta.created,
            result: meta.outcome.into(),
            moves: game.moves().iter().map(|mv| [mv.col(), mv.row()]).collect(),
            tree: game
                .tree_nodes()
                .iter()
                .map(|node| NodeRecord {
                    parent: node.parent.map(|id| id as u32),
                    point: node.point.map(|mv| [mv.col(), mv.row()]),
                    current: node.current_child.map(|id| id as u32),
                    resigned: node.resigned.map(StoredColor::from),
                })
                .collect(),
        })
    }

    /// Encode the record as pretty JSON with a trailing newline.
    ///
    /// # Errors
    /// `RecordError::Parse` if the record cannot be encoded.
    pub fn to_json(&self) -> Result<String, RecordError> {
        let mut text = serde_json::to_string_pretty(self)
            .map_err(|err| RecordError::Parse(err.to_string()))?;
        text.push('\n');
        Ok(text)
    }

    /// Parse a record from JSON text.
    ///
    /// This checks the syntax only. `into_game` checks the contents.
    ///
    /// # Errors
    /// `RecordError::Parse` if the text is not a record.
    pub fn from_json(text: &str) -> Result<Record, RecordError> {
        serde_json::from_str(text).map_err(|err| RecordError::Parse(err.to_string()))
    }

    /// Turn the record into a game, after checking every field.
    ///
    /// # Errors
    /// A `RecordError` when the header, moves, tree links or result are invalid.
    pub fn into_game(self) -> Result<Game, RecordError> {
        self.check_header()?;
        if self.version == 3 {
            self.into_tree_game()
        } else {
            self.into_linear_game()
        }
    }

    fn into_linear_game(self) -> Result<Game, RecordError> {
        let mut game = Game::started_at(self.created);
        game.set_players(self.players.black, self.players.white);
        for (index, &[col, row]) in self.moves.iter().enumerate() {
            let point = Move::new(row, col).ok_or(RecordError::PointOutOfRange { index })?;
            if game.stone_at(point).is_some() {
                return Err(RecordError::RepeatedPoint { index });
            }
            game.play(point)
                .map_err(|_| RecordError::IllegalMove { index })?;
        }
        if let StoredOutcome::Won {
            winner,
            method: StoredMethod::Resignation,
        } = self.result
        {
            if self.version == 1 || Color::from(winner) != game.to_move().other() {
                return Err(RecordError::ResultMismatch);
            }
            game.resign().map_err(|_| RecordError::ResultMismatch)?;
        }
        if Outcome::from(self.result) != game.outcome() {
            return Err(RecordError::ResultMismatch);
        }
        Ok(game)
    }

    fn into_tree_game(self) -> Result<Game, RecordError> {
        if self.tree.is_empty() {
            return Err(RecordError::InvalidTree("the root node is missing".into()));
        }
        if self.tree[0].parent.is_some() || self.tree[0].point.is_some() {
            return Err(RecordError::InvalidTree(
                "node zero must be the root".into(),
            ));
        }
        let mut game = Game::started_at(self.created);
        game.set_players(self.players.black, self.players.white);
        for (index, node) in self.tree.iter().enumerate().skip(1) {
            let parent = node
                .parent
                .ok_or_else(|| RecordError::InvalidTree(format!("node {index} has no parent")))?
                as usize;
            if parent >= index {
                return Err(RecordError::InvalidTree(format!(
                    "node {index} has a parent that does not precede it"
                )));
            }
            let [col, row] = node
                .point
                .ok_or_else(|| RecordError::InvalidTree(format!("node {index} has no move")))?;
            let point = Move::new(row, col).ok_or(RecordError::PointOutOfRange { index })?;
            let restored = game
                .restore_child(parent, point)
                .map_err(|error| match error {
                    GameError::Occupied => RecordError::RepeatedPoint { index },
                    _ => RecordError::IllegalMove { index },
                })?;
            if restored != index {
                return Err(RecordError::InvalidTree(format!(
                    "node {index} repeats its sibling's move"
                )));
            }
        }
        for (index, node) in self.tree.iter().enumerate() {
            let child_count = game.tree_nodes()[index].children.len();
            let current = node.current.map(|id| id as usize);
            if (child_count == 0) != current.is_none()
                || current.is_some_and(|child| {
                    self.tree
                        .get(child)
                        .is_none_or(|child| child.parent != Some(index as u32))
                })
            {
                return Err(RecordError::InvalidTree(format!(
                    "node {index} has an invalid current child"
                )));
            }
            if !game.restore_current_child(index, current) {
                return Err(RecordError::InvalidTree(format!(
                    "node {index} has an invalid current child"
                )));
            }
            if let Some(resigned) = node.resigned {
                let board = game.board_at_node(index).ok_or_else(|| {
                    RecordError::InvalidTree(format!("node {index} has no legal board"))
                })?;
                if child_count != 0
                    || board.status() != engine::Status::Ongoing
                    || Color::from(resigned) != board.to_move()
                {
                    return Err(RecordError::ResultMismatch);
                }
                if !game.restore_resignation(index, resigned.into()) {
                    return Err(RecordError::InvalidTree(format!(
                        "node {index} has an invalid resignation"
                    )));
                }
            }
        }
        if !game.finish_restore() {
            return Err(RecordError::InvalidTree(
                "the selected branch cannot be restored".into(),
            ));
        }
        let selected: Vec<[u8; 2]> = game.moves().iter().map(|mv| [mv.col(), mv.row()]).collect();
        if selected != self.moves || Outcome::from(self.result) != game.outcome() {
            return Err(RecordError::ResultMismatch);
        }
        Ok(game)
    }

    /// Check the header fields, which do not depend on the moves.
    fn check_header(&self) -> Result<(), RecordError> {
        if self.format != FORMAT_MARKER {
            return Err(RecordError::WrongFormat {
                found: self.format.clone(),
            });
        }
        if !matches!(self.version, 1..=3) {
            return Err(RecordError::UnsupportedVersion(self.version));
        }
        if self.size != BOARD_SIZE {
            return Err(RecordError::UnsupportedSize(self.size));
        }
        if self.ruleset != RULESET {
            return Err(RecordError::UnsupportedRuleset(self.ruleset.clone()));
        }
        if self.moves.len() > CELLS {
            return Err(RecordError::TooManyMoves {
                count: self.moves.len(),
            });
        }
        if self.tree.len() > MAX_TREE_NODES {
            return Err(RecordError::TooManyNodes {
                count: self.tree.len(),
            });
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use time::macros::datetime;

    fn point(row: u8, col: u8) -> Move {
        Move::new(row, col).expect("row and col are inside the board")
    }

    fn played_game() -> Game {
        let mut game = Game::started_at(datetime!(2026-09-22 18:04:11 UTC));
        game.set_players(Some("Wolfie".to_string()), Some("Anna".to_string()));
        for (row, col) in [(7, 7), (7, 8), (8, 8), (6, 6)] {
            game.play(point(row, col)).expect("the cell is empty");
        }
        game
    }

    fn won_game() -> Game {
        let mut game = Game::started_at(datetime!(2026-09-22 18:04:11 UTC));
        for index in 0..4 {
            game.play(point(7, 3 + index)).expect("the cell is empty");
            game.play(point(3 + index, index))
                .expect("the cell is empty");
        }
        game.play(point(7, 7)).expect("the cell is empty");
        game
    }

    fn record() -> Record {
        Record::from_game(&played_game()).expect("a normal game encodes")
    }

    #[test]
    fn a_record_round_trips_through_json() {
        let original = record();
        let text = original.to_json().expect("the record encodes");
        assert!(
            text.contains("\"created\": \"2026-09-22T18:04:11Z\""),
            "the created field must be RFC 3339 in UTC, got:\n{text}"
        );
        let parsed = Record::from_json(&text).expect("the text parses");
        assert_eq!(parsed, original);

        let game = parsed.into_game().expect("the record is valid");
        assert_eq!(game.moves(), played_game().moves());
        assert_eq!(game.cursor(), 4);
        assert_eq!(game.meta().black.as_deref(), Some("Wolfie"));
        assert_eq!(game.meta().white.as_deref(), Some("Anna"));
        assert_eq!(game.meta().created, datetime!(2026-09-22 18:04:11 UTC));
        assert_eq!(game.outcome(), Outcome::Ongoing);
    }

    #[test]
    fn a_won_game_round_trips_with_its_result() {
        let record = Record::from_game(&won_game()).expect("a normal game encodes");
        assert_eq!(
            record.result,
            StoredOutcome::Won {
                winner: StoredColor::Black,
                method: StoredMethod::Five
            }
        );
        let game = Record::from_json(&record.to_json().expect("encodes"))
            .expect("parses")
            .into_game()
            .expect("valid");
        assert_eq!(
            game.outcome(),
            Outcome::Won {
                winner: Color::Black,
                method: WinMethod::Five
            }
        );
    }

    #[test]
    fn a_draw_maps_both_ways() {
        assert_eq!(Outcome::from(StoredOutcome::Draw), Outcome::Draw);
        assert_eq!(StoredOutcome::from(Outcome::Draw), StoredOutcome::Draw);
    }

    #[test]
    fn the_moves_are_stored_column_first() {
        let list = record().moves;
        assert_eq!(list, vec![[7, 7], [8, 7], [8, 8], [6, 6]]);
    }

    #[test]
    fn an_unknown_field_is_ignored() {
        let original = record();
        let text = original.to_json().expect("encodes");
        let mut value: serde_json::Value = serde_json::from_str(&text).expect("parses");
        value["a_field_from_a_later_version"] = serde_json::json!(42);
        let text = serde_json::to_string(&value).expect("encodes");
        let parsed = Record::from_json(&text).expect("parses");
        assert_eq!(parsed, original);
    }

    #[test]
    fn a_record_refuses_another_format_marker() {
        let mut record = record();
        record.format = "something-else".to_string();
        assert!(matches!(
            record.into_game(),
            Err(RecordError::WrongFormat { .. })
        ));
    }

    #[test]
    fn a_record_refuses_a_newer_version() {
        let mut record = record();
        record.version = FORMAT_VERSION + 1;
        assert!(matches!(
            record.into_game(),
            Err(RecordError::UnsupportedVersion(4))
        ));
    }

    #[test]
    fn a_record_refuses_another_board_size() {
        let mut record = record();
        record.size = 19;
        assert!(matches!(
            record.into_game(),
            Err(RecordError::UnsupportedSize(19))
        ));
    }

    #[test]
    fn a_record_refuses_another_rule_set() {
        let mut record = record();
        record.ruleset = "renju".to_string();
        assert!(matches!(
            record.into_game(),
            Err(RecordError::UnsupportedRuleset(_))
        ));
    }

    #[test]
    fn a_record_refuses_more_moves_than_the_board_has_cells() {
        let mut record = record();
        record.moves = vec![[0, 0]; 226];
        assert!(matches!(
            record.into_game(),
            Err(RecordError::TooManyMoves { count: 226 })
        ));
    }

    #[test]
    fn a_record_refuses_a_point_outside_the_board() {
        let mut record = record();
        record.version = 2;
        record.moves.push([15, 0]);
        assert!(matches!(
            record.into_game(),
            Err(RecordError::PointOutOfRange { index: 4 })
        ));
    }

    #[test]
    fn a_record_refuses_a_repeated_point() {
        let mut record = record();
        record.version = 2;
        record.moves.push([7, 7]);
        assert!(matches!(
            record.into_game(),
            Err(RecordError::RepeatedPoint { index: 4 })
        ));
    }

    #[test]
    fn a_record_refuses_a_move_after_the_game_ended() {
        let mut record = Record::from_game(&won_game()).expect("a normal game encodes");
        record.version = 2;
        record.moves.push([10, 10]);
        assert!(matches!(
            record.into_game(),
            Err(RecordError::IllegalMove { index: 9 })
        ));
    }

    #[test]
    fn a_record_refuses_a_result_that_disagrees_with_the_moves() {
        let mut record = record();
        record.result = StoredOutcome::Won {
            winner: StoredColor::Black,
            method: StoredMethod::Five,
        };
        assert!(matches!(
            record.into_game(),
            Err(RecordError::ResultMismatch)
        ));
    }

    #[test]
    fn damaged_text_is_a_parse_error() {
        assert!(matches!(
            Record::from_json("{ not json"),
            Err(RecordError::Parse(_))
        ));
    }
}
