//! The state of one sitting: the game, the view, the open file, and the dialogs.
//!
//! The application owns one of these, and the interface reads and changes it.
//! Everything that is not a graphics resource lives here, so that the rules of
//! the sitting are in one place and the window code stays about the window.

use std::path::{Path, PathBuf};

use anyhow::{Context as _, Result};
use gomoku_core::{Color, Game, Move, Outcome, Settings};

use crate::camera::Camera;

/// A dialog waiting for an answer, or a message to show.
#[derive(Debug, Clone, PartialEq)]
pub enum Dialog {
    /// A placement would delete the moves after the view cursor.
    Truncate {
        /// How many moves would go.
        count: usize,
        /// The point that was clicked.
        point: Move,
    },
    /// The game has changes that are not saved.
    Unsaved {
        /// What the user asked for, and what happens if they confirm.
        then: After,
    },
    /// An autosaved game is newer than the file it came from.
    Resume {
        /// The file the autosave came from, if any.
        source: Option<PathBuf>,
    },
    /// A message, with nothing to decide.
    Message {
        /// The heading.
        title: String,
        /// The body text.
        body: String,
    },
}

/// What an unsaved-changes answer should do next.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum After {
    /// Start a new game.
    NewGame,
    /// Open a game from a file.
    Open,
    /// Close the application.
    Quit,
}

/// Which overlays are shown.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Toggles {
    /// The coordinate labels around the board.
    pub coordinates: bool,
    /// The move numbers on the stones.
    pub move_numbers: bool,
    /// The marker on the last move.
    pub last_move: bool,
    /// The line through the winning stones.
    pub win_line: bool,
}

impl Default for Toggles {
    fn default() -> Toggles {
        Toggles {
            coordinates: true,
            move_numbers: false,
            last_move: true,
            win_line: true,
        }
    }
}

/// One sitting at the board.
pub struct Session {
    /// The game.
    pub game: Game,
    /// Where the view is looking.
    pub camera: Camera,
    /// The file the game came from or was saved to.
    pub path: Option<PathBuf>,
    /// Counts every change to the game, so that "changed" is one comparison.
    revision: u64,
    /// The revision that was last written to a file.
    saved_revision: u64,
    /// Which overlays are shown.
    pub toggles: Toggles,
    /// The brightness multiplier on the wood photograph.
    pub gain: f32,
    /// True when the stone knock plays.
    pub sound: bool,
    /// The volume, from 0 to 1.
    pub volume: f32,
    /// The dialog that is waiting, if any.
    pub dialog: Option<Dialog>,
    /// A short message for the status line.
    pub notice: Option<String>,
    /// The directory of the last open or save.
    pub last_directory: Option<PathBuf>,
    /// The most recently used files, newest first.
    pub recent: Vec<PathBuf>,
    /// The stone knock to play, as the number of occupied neighbours.
    pub knock: Option<usize>,
    /// True when the window title should be refreshed.
    pub title_stale: bool,
    /// The rectangle the board is drawn into, in physical pixels.
    pub viewport: crate::camera::Viewport,
    /// The same rectangle in interface points, as the interface reports it.
    pub viewport_points: [f32; 4],
    /// The width of the side panel, in points.
    pub panel_width: f32,
    /// True when the side panel is folded away.
    pub panel_collapsed: bool,
    /// The pointer position, in physical pixels.
    pub pointer: [f32; 2],
    /// The intersection under the pointer, when a placement there is legal.
    pub hover: Option<Move>,
    /// The scale of the interface, as physical pixels per point.
    pub pixels_per_point: f32,
    /// An action the user has already confirmed, to perform without asking again.
    pub perform: Option<After>,
    /// An action to perform once the game has been saved.
    pub after_save: Option<After>,
}

impl Session {
    /// A new, empty sitting.
    pub fn new(settings: &Settings) -> Session {
        let toggles = Toggles {
            coordinates: settings.overlays.coordinates,
            move_numbers: settings.overlays.move_numbers,
            last_move: settings.overlays.last_move,
            win_line: settings.overlays.win_line,
        };
        Session {
            game: Game::new(),
            camera: Camera::fit(crate::camera::Viewport::window(1180, 950)),
            path: None,
            revision: 0,
            saved_revision: 0,
            toggles,
            gain: crate::render::WOOD.gain,
            sound: settings.audio.enabled,
            volume: settings.audio.volume,
            dialog: None,
            notice: None,
            last_directory: settings.files.last_directory.clone(),
            recent: settings.files.recent.clone(),
            knock: None,
            title_stale: true,
            viewport: crate::camera::Viewport::window(1180, 950),
            viewport_points: [0.0, 28.0, 930.0, 922.0],
            panel_width: settings.panel.width,
            panel_collapsed: settings.panel.collapsed,
            pointer: [-1.0, -1.0],
            hover: None,
            pixels_per_point: 1.0,
            perform: None,
            after_save: None,
        }
    }

    /// Fit the whole board into the viewport.
    pub fn fit(&mut self) {
        self.camera.reset(self.viewport);
        self.title_stale = true;
    }

    /// Turn the board around.
    pub fn flip(&mut self) {
        self.camera.flipped = !self.camera.flipped;
        self.title_stale = true;
    }

    /// True when a point in interface points lies over the board.
    ///
    /// The interface does not claim the pointer over the board, because the board
    /// is drawn outside egui. So the application needs its own test, to know when
    /// a click belongs to the game and when it belongs to a panel.
    pub fn on_board(&self) -> bool {
        let [x, y, width, height] = self.viewport_points;
        let [px, py] = self.pointer;
        px >= x && py >= y && px < x + width && py < y + height
    }

    /// Work out which intersection the pointer is over.
    pub fn update_hover(&mut self) {
        let cell = self.camera.intersection(self.viewport, self.pointer);
        self.hover = cell
            .and_then(|cell| gomoku_core::point(cell[1], cell[0]))
            .filter(|point| {
                self.game.stone_at(*point).is_none()
                    && self.game.status() == gomoku_core::Status::Ongoing
            });
    }

    /// Continue the autosaved game.
    pub fn resume_autosave(&mut self) {
        match autosave_path().map(|path| gomoku_core::load(&path)) {
            Ok(Ok(game)) => {
                self.game = game;
                self.path = read_autosave_source();
                self.revision += 1;
                self.notice = Some("continued the autosaved game".to_string());
                self.title_stale = true;
            }
            Ok(Err(error)) => {
                self.fail("The autosaved game could not be read", &error);
            }
            Err(error) => {
                self.fail("The autosaved game could not be found", &error);
            }
        }
    }

    /// Throw the autosave away.
    pub fn discard_autosave(&mut self) {
        for path in [autosave_path(), autosave_state_path()]
            .into_iter()
            .flatten()
        {
            if let Err(error) = std::fs::remove_file(&path) {
                log::warn!("the autosave {} was left behind: {error}", path.display());
            }
        }
        self.notice = Some("the autosave was discarded".to_string());
    }

    /// Write the game in progress, so that it survives a crash or a quit.
    pub fn autosave(&mut self) {
        let Ok(path) = autosave_path() else {
            return;
        };
        if let Err(error) = gomoku_core::save(&path, &self.game) {
            log::warn!("the game could not be autosaved: {error}");
            return;
        }
        let state = format!(
            "{{\"source_path\": {}, \"cursor\": {}}}",
            match &self.path {
                Some(path) => format!("\"{}\"", path.display()),
                None => "null".to_string(),
            },
            self.game.cursor()
        );
        if let Ok(state_path) = autosave_state_path() {
            if let Err(error) = gomoku_core::write_atomic(&state_path, &state) {
                log::warn!("the autosave state could not be written: {error}");
            }
        }
    }

    /// An autosave that is worth offering to continue, if there is one.
    pub fn autosave_to_offer(&self) -> Option<Option<PathBuf>> {
        let path = autosave_path().ok()?;
        let modified = std::fs::metadata(&path).ok()?.modified().ok()?;
        let source = read_autosave_source();
        if let Some(source) = &source {
            // Only offer it when the autosave is newer than the file it came from.
            if let Ok(saved) = std::fs::metadata(source).and_then(|meta| meta.modified()) {
                if modified <= saved {
                    return None;
                }
            }
        }
        let game = gomoku_core::load(&path).ok()?;
        if game.is_empty() {
            return None;
        }
        Some(source)
    }

    // --- the game ---------------------------------------------------------

    /// Place a stone, after the caller has dealt with any truncation.
    ///
    /// When the view is rewound this asks the user first, because the moves after
    /// the cursor would be lost.
    pub fn place(&mut self, point: Move) {
        if self.game.pending_truncation() > 0 {
            self.dialog = Some(Dialog::Truncate {
                count: self.game.pending_truncation(),
                point,
            });
            return;
        }
        self.place_now(point);
    }

    /// Place a stone without asking, which is what the truncation dialog does
    /// once it is confirmed.
    pub fn place_now(&mut self, point: Move) {
        if self.game.play(point).is_err() {
            return;
        }
        self.revision += 1;
        self.knock = Some(neighbours(&self.game, point));
        self.title_stale = true;
    }

    /// Take the last move back. The move is gone.
    pub fn undo(&mut self) {
        if self.game.undo() {
            self.revision += 1;
            self.title_stale = true;
        }
    }

    /// Step the view back one move.
    pub fn rewind(&mut self) {
        if self.game.rewind() {
            self.title_stale = true;
        }
    }

    /// Step the view forward one move.
    pub fn forward(&mut self) {
        if self.game.forward() {
            self.title_stale = true;
        }
    }

    /// Jump to a move number, or to the latest move with `usize::MAX`.
    pub fn seek(&mut self, index: usize) {
        if self.game.seek(index) {
            self.title_stale = true;
        }
    }

    /// A new, empty game. The player names are kept.
    pub fn new_game(&mut self) {
        let created = self.game.meta().created;
        let black = self.game.meta().black.clone();
        let white = self.game.meta().white.clone();
        let mut game = Game::started_at(created);
        game.set_players(black, white);
        self.game = game;
        self.path = None;
        self.revision += 1;
        self.saved_revision = self.revision;
        self.dialog = None;
        self.notice = None;
        self.title_stale = true;
    }

    /// True when the game has changes that are not in a file.
    pub fn changed(&self) -> bool {
        self.revision != self.saved_revision
    }

    // --- files ------------------------------------------------------------

    /// Save to the open file, or ask for one when there is none.
    pub fn save(&mut self) -> bool {
        match self.path.clone() {
            Some(path) => {
                self.save_to(&path);
                false
            }
            None => true,
        }
    }

    /// Save to a given file.
    pub fn save_to(&mut self, path: &Path) {
        match gomoku_core::save(path, &self.game) {
            Ok(()) => {
                self.saved_revision = self.revision;
                self.path = Some(path.to_path_buf());
                self.remember(path);
                self.notice = Some(format!("saved to {}", name_of(path)));
                self.title_stale = true;
            }
            Err(error) => self.fail("The game could not be saved", &error),
        }
    }

    /// Open a game from a file.
    pub fn open(&mut self, path: &Path) {
        match gomoku_core::load(path) {
            Ok(game) => {
                self.game = game;
                self.path = Some(path.to_path_buf());
                self.revision += 1;
                self.saved_revision = self.revision;
                self.dialog = None;
                self.notice = Some(format!("opened {}", name_of(path)));
                self.remember(path);
                self.title_stale = true;
            }
            Err(error) => self.fail("The game could not be opened", &error),
        }
    }

    /// Remember the directory and the file, for the next open or save.
    fn remember(&mut self, path: &Path) {
        if let Some(directory) = path.parent() {
            self.last_directory = Some(directory.to_path_buf());
        }
        self.recent.retain(|entry| entry != path);
        self.recent.insert(0, path.to_path_buf());
        self.recent.truncate(gomoku_core::MAX_RECENT);
    }

    /// Show a failure to the user, with the detail.
    pub fn fail(&mut self, title: &str, error: &dyn std::fmt::Display) {
        log::error!("{title}: {error}");
        self.dialog = Some(Dialog::Message {
            title: title.to_string(),
            body: format!("{error}"),
        });
    }

    // --- the window title --------------------------------------------------

    /// The title of the window.
    pub fn title(&self) -> String {
        let name = match &self.path {
            Some(path) => name_of(path),
            None => "Untitled".to_string(),
        };
        let mark = if self.changed() { " •" } else { "" };
        let status = match self.game.outcome() {
            Outcome::Ongoing => match self.game.to_move() {
                Color::Black => "Black to play",
                Color::White => "White to play",
            },
            Outcome::Won { winner, .. } => match winner {
                Color::Black => "Black wins",
                Color::White => "White wins",
            },
            Outcome::Draw => "Draw",
        };
        format!("Gomoku — {name}{mark} — {status}")
    }

    /// The settings to write at exit.
    pub fn settings(&self, window: &crate::session::WindowGeometry) -> Settings {
        Settings {
            window: gomoku_core::WindowSettings {
                width: window.width,
                height: window.height,
                x: window.x,
                y: window.y,
                maximized: window.maximized,
            },
            view: gomoku_core::ViewSettings {
                pixels_per_cell: self.camera.pixels_per_cell,
                center: self.camera.centre,
                flipped: self.camera.flipped,
            },
            overlays: gomoku_core::OverlaySettings {
                coordinates: self.toggles.coordinates,
                move_numbers: self.toggles.move_numbers,
                last_move: self.toggles.last_move,
                win_line: self.toggles.win_line,
            },
            board: gomoku_core::BoardSettings {
                material: gomoku_core::BoardSettings::default().material,
            },
            panel: gomoku_core::PanelSettings {
                width: self.panel_width,
                collapsed: self.panel_collapsed,
            },
            stones: gomoku_core::StoneSettings::default(),
            audio: gomoku_core::AudioSettings {
                enabled: self.sound,
                volume: self.volume,
            },
            files: gomoku_core::FileSettings {
                last_directory: self.last_directory.clone(),
                recent: self.recent.clone(),
            },
        }
    }

    /// Apply the remembered view, once the window has a size.
    pub fn apply_view(&mut self, settings: &Settings, size: [u32; 2]) {
        let viewport = crate::camera::Viewport::window(size[0], size[1]);
        self.camera = Camera::fit(viewport);
        if settings.view.pixels_per_cell > 0.0 {
            self.camera.centre = settings.view.center;
            self.camera.pixels_per_cell = settings.view.pixels_per_cell;
            self.camera.flipped = settings.view.flipped;
            self.camera.clamp(viewport);
        }
    }
}

/// The window geometry, which only the window code knows.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct WindowGeometry {
    /// Width in logical pixels.
    pub width: f32,
    /// Height in logical pixels.
    pub height: f32,
    /// X position in logical pixels.
    pub x: Option<f32>,
    /// Y position in logical pixels.
    pub y: Option<f32>,
    /// True when maximised.
    pub maximized: bool,
}

impl Default for WindowGeometry {
    fn default() -> WindowGeometry {
        WindowGeometry {
            width: 1180.0,
            height: 950.0,
            x: None,
            y: None,
            maximized: false,
        }
    }
}

/// The file the autosave came from, if the sidecar names one.
fn read_autosave_source() -> Option<PathBuf> {
    let path = autosave_state_path().ok()?;
    let text = std::fs::read_to_string(path).ok()?;
    let value: serde_json::Value = serde_json::from_str(&text).ok()?;
    value.get("source_path")?.as_str().map(PathBuf::from)
}

/// The file name of a path, for messages.
pub fn name_of(path: &Path) -> String {
    path.file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_else(|| path.to_string_lossy().into_owned())
}

/// How many of the four orthogonal neighbours of a point hold a stone.
pub fn neighbours(game: &Game, point: Move) -> usize {
    let mut count = 0;
    for (dx, dy) in [(-1_i8, 0_i8), (1, 0), (0, -1), (0, 1)] {
        let column = point.col() as i8 + dx;
        let row = point.row() as i8 + dy;
        if !(0..15).contains(&column) || !(0..15).contains(&row) {
            continue;
        }
        if let Some(other) = gomoku_core::point(row as u8, column as u8) {
            if game.stone_at(other).is_some() {
                count += 1;
            }
        }
    }
    count
}

/// The winning line, if the game is won: the run of five or more stones that
/// ends at the last move.
pub fn winning_line(game: &Game) -> Option<[Move; 2]> {
    let winner = match game.outcome() {
        Outcome::Won { winner, .. } => winner,
        _ => return None,
    };
    let last = game.last_move()?;
    if game.stone_at(last) != Some(winner) {
        return None;
    }
    for (dx, dy) in [(1_i8, 0_i8), (0, 1), (1, 1), (1, -1)] {
        // Walk back to the start of the run, then forward to its end.
        let start = walk(game, last, winner, -dx, -dy);
        let end = walk(game, last, winner, dx, dy);
        let length = steps(start, end, dx, dy) + 1;
        if length >= 5 {
            return Some([start, end]);
        }
    }
    None
}

/// Walk from a point while the stones belong to `colour`.
fn walk(game: &Game, from: Move, colour: Color, dx: i8, dy: i8) -> Move {
    let mut current = from;
    loop {
        let column = current.col() as i8 + dx;
        let row = current.row() as i8 + dy;
        if !(0..15).contains(&column) || !(0..15).contains(&row) {
            return current;
        }
        match gomoku_core::point(row as u8, column as u8) {
            Some(next) if game.stone_at(next) == Some(colour) => current = next,
            _ => return current,
        }
    }
}

/// The number of steps between two points along a direction.
fn steps(from: Move, to: Move, dx: i8, dy: i8) -> i32 {
    if dx != 0 {
        ((to.col() as i32 - from.col() as i32) / dx as i32).abs()
    } else {
        ((to.row() as i32 - from.row() as i32) / dy as i32).abs()
    }
}

/// The directory for the settings and the autosave.
///
/// # Errors
/// An error when the system does not name a data directory.
pub fn data_directory() -> Result<PathBuf> {
    let base = directories::ProjectDirs::from("", "", "gomoku-gui")
        .context("the system did not name a data directory")?;
    let directory = base.data_dir().to_path_buf();
    std::fs::create_dir_all(&directory)
        .with_context(|| format!("cannot create {}", directory.display()))?;
    Ok(directory)
}

/// The path of the settings file.
///
/// # Errors
/// An error when the system does not name a data directory.
pub fn settings_path() -> Result<PathBuf> {
    Ok(data_directory()?.join("config.toml"))
}

/// The path of the autosaved game.
///
/// # Errors
/// An error when the system does not name a data directory.
pub fn autosave_path() -> Result<PathBuf> {
    Ok(data_directory()?.join("autosave.json"))
}

/// The path of the autosave's sidecar, which records which file it came from.
///
/// # Errors
/// An error when the system does not name a data directory.
pub fn autosave_state_path() -> Result<PathBuf> {
    Ok(data_directory()?.join("autosave-state.json"))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn game() -> Game {
        let mut game = Game::new();
        for (column, row) in [
            (7_u8, 7_u8),
            (7, 8),
            (8, 8),
            (6, 6),
            (8, 6),
            (8, 7),
            (6, 8),
            (9, 7),
            (5, 9),
        ] {
            game.play(gomoku_core::point(row, column).expect("inside the board"))
                .expect("the cell is empty");
        }
        game
    }

    #[test]
    fn the_winning_line_is_found_across_the_board() {
        let mut game = Game::new();
        for column in 0..5_u8 {
            game.play(gomoku_core::point(0, column).expect("inside"))
                .expect("empty");
            if column < 4 {
                game.play(gomoku_core::point(1, column).expect("inside"))
                    .expect("empty");
            }
        }
        let line = winning_line(&game).expect("the game is won");
        let (first, last) = (line[0], line[1]);
        assert_eq!((first.row(), last.row()), (0, 0));
        assert_eq!((first.col(), last.col()), (0, 4));
    }

    #[test]
    fn the_winning_line_is_found_on_a_diagonal() {
        let mut game = Game::new();
        for index in 0..5_u8 {
            game.play(gomoku_core::point(index, index).expect("inside"))
                .expect("empty");
            if index < 4 {
                game.play(gomoku_core::point(index, 10).expect("inside"))
                    .expect("empty");
            }
        }
        let line = winning_line(&game).expect("the game is won");
        assert_eq!((line[0].row(), line[0].col()), (0, 0));
        assert_eq!((line[1].row(), line[1].col()), (4, 4));
    }

    #[test]
    fn an_overline_reports_its_whole_run() {
        // Five already win, so an overline can only appear when one move closes a
        // gap and makes six at once. The white stones are scattered so that they
        // cannot make a line of their own first.
        let mut game = Game::new();
        let black = [0_u8, 1, 2, 3, 5, 4];
        let white = [(2_u8, 0_u8), (2, 2), (2, 4), (4, 0), (4, 2)];
        for index in 0..5 {
            game.play(gomoku_core::point(0, black[index]).expect("inside"))
                .expect("empty");
            game.play(gomoku_core::point(white[index].0, white[index].1).expect("inside"))
                .expect("empty");
        }
        game.play(gomoku_core::point(0, black[5]).expect("inside"))
            .expect("empty");
        assert_eq!(game.len(), 11, "the last move closes the gap");
        let line = winning_line(&game).expect("the game is won");
        assert_eq!(line[0].col(), 0);
        assert_eq!(line[1].col(), 5, "all six stones are marked");
    }

    #[test]
    fn an_open_game_has_no_winning_line() {
        assert_eq!(winning_line(&game()), None);
    }

    #[test]
    fn the_board_area_is_known() {
        let settings = Settings::default();
        let mut session = Session::new(&settings);
        session.viewport_points = [0.0, 28.0, 900.0, 872.0];
        session.pointer = [450.0, 400.0];
        assert!(session.on_board(), "the middle of the board area");
        session.pointer = [950.0, 400.0];
        assert!(!session.on_board(), "the side panel");
        session.pointer = [450.0, 10.0];
        assert!(!session.on_board(), "the menu bar");
        session.pointer = [0.0, 28.0];
        assert!(
            session.on_board(),
            "the top left corner belongs to the board"
        );
        session.pointer = [900.0, 400.0];
        assert!(!session.on_board(), "the far edge belongs to the panel");
    }

    #[test]
    fn a_new_game_keeps_the_player_names() {
        let settings = Settings::default();
        let mut session = Session::new(&settings);
        session
            .game
            .set_players(Some("Ada".into()), Some("Grace".into()));
        session
            .game
            .play(gomoku_core::point(7, 7).expect("inside"))
            .expect("empty");
        session.new_game();
        assert_eq!(session.game.meta().black.as_deref(), Some("Ada"));
        assert_eq!(session.game.meta().white.as_deref(), Some("Grace"));
        assert!(session.game.is_empty());
        assert!(!session.changed(), "a fresh game is not a change to save");
    }

    #[test]
    fn placing_a_stone_while_rewound_asks_first() {
        let settings = Settings::default();
        let mut session = Session::new(&settings);
        for column in 0..3_u8 {
            session.place_now(gomoku_core::point(7, column).expect("inside"));
        }
        session.seek(1);
        session.place(gomoku_core::point(9, 9).expect("inside"));
        match session.dialog {
            Some(Dialog::Truncate { count, .. }) => assert_eq!(count, 2),
            other => panic!("expected the truncation dialog, got {other:?}"),
        }
        assert_eq!(
            session.game.len(),
            3,
            "nothing is deleted until it is confirmed"
        );
    }

    #[test]
    fn a_confirmed_truncation_deletes_the_tail() {
        let settings = Settings::default();
        let mut session = Session::new(&settings);
        for column in 0..3_u8 {
            session.place_now(gomoku_core::point(7, column).expect("inside"));
        }
        session.seek(1);
        session.place_now(gomoku_core::point(9, 9).expect("inside"));
        assert_eq!(session.game.len(), 2);
        assert!(session.changed());
    }
}
