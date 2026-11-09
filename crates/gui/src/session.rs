//! The state of one sitting: the game, the view, the open file, and the dialogs.
//!
//! The application owns one of these, and the interface reads and changes it.
//! Everything that is not a graphics resource lives here, so that the rules of
//! the sitting are in one place and the window code stays about the window.

use std::path::{Path, PathBuf};

use anyhow::{Context as _, Result};
use gomoku_core::{Color, Game, Move, Outcome, PositionError, Puzzle, Settings, WinMethod};

use crate::ai::{
    AiOptions, AiReply, AiRequest, Controller, DecisionAction, InvalidAiOptions, SeatOwner,
};
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
    /// More than one puzzle is in the file; pick one. While the picker is
    /// open the board previews the selected puzzle, so the game as it was is
    /// kept, to be restored when the picker is cancelled.
    PickPuzzle {
        /// The loaded puzzles.
        puzzles: Vec<Puzzle>,
        /// The index the user has selected.
        index: usize,
        /// The game as it was before the picker opened.
        before: Box<BeforePuzzle>,
    },
}

/// The right-panel section that is currently open.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PanelSection {
    /// Player names and seat ownership.
    Players,
    /// The game's move list.
    Moves,
    /// The open record corpus.
    Explorer,
}

/// State for the Lab Explorer while a corpus is open.
#[derive(Debug, Clone, PartialEq)]
pub struct ExplorerSession {
    /// The indexed records.
    pub records: Vec<gomoku_core::IndexEntry>,
    /// The record currently shown, if any.
    pub selected: Option<usize>,
    /// Filter text from the user.
    pub filter: String,
    /// Jump-to-index input.
    pub jump: String,
}

/// The game as it was before the puzzle picker opened. While the picker is
/// open the board shows the selected puzzle, and cancelling the picker must
/// bring back what was there, with no trace in the revision counters.
#[derive(Debug, Clone, PartialEq)]
pub struct BeforePuzzle {
    /// The game on the board.
    pub(crate) game: Game,
    /// The file the game came from.
    pub(crate) path: Option<PathBuf>,
    /// The puzzle-start cursor ring.
    pub(crate) puzzle_cursor: Option<Move>,
    /// The revision counters, so cancelling does not pretend unsaved changes.
    pub(crate) revision: u64,
    /// The revision that was last written to a file.
    pub(crate) saved_revision: u64,
    /// Seats, options, and running intent; never generation or outstanding work.
    automation: crate::ai::AutomationIntent,
}

impl From<&Session> for BeforePuzzle {
    fn from(session: &Session) -> Self {
        Self {
            game: session.game.clone(),
            path: session.path.clone(),
            puzzle_cursor: session.puzzle_cursor,
            revision: session.revision,
            saved_revision: session.saved_revision,
            automation: session.ai.intent(),
        }
    }
}

/// What an unsaved-changes answer should do next.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum After {
    /// Start a new game.
    NewGame,
    /// Open a game from a file.
    Open,
    /// Open a recently used game, after the same unsaved check as Open.
    OpenRecent(PathBuf),
    /// Open a puzzle from a file.
    OpenPuzzle,
    /// Close the application.
    Quit,
}

/// The result of a complete save attempt, including native-picker cancellation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SaveOutcome {
    /// The current revision reached disk.
    Saved,
    /// No destination was selected.
    Cancelled,
    /// The write failed; a message explains why.
    Failed,
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
    game: Game,
    ai: Controller,
    native_dialog_open: bool,
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
    dialog: Option<Dialog>,
    /// A short message for the status line.
    pub notice: Option<String>,
    /// The directory of the last open or save.
    pub last_directory: Option<PathBuf>,
    /// The directory of the last successfully opened Explorer corpus.
    pub last_corpus_directory: Option<PathBuf>,
    /// The most recently used files, newest first.
    pub recent: Vec<PathBuf>,
    /// The stone knock to play, as the number of occupied neighbours.
    pub knock: Option<usize>,
    /// True when the window title should be refreshed.
    pub title_stale: bool,
    /// The cell to mark with the puzzle-start cursor ring, if a puzzle is
    /// loaded and its claimed solution names a first move.
    ///
    /// Cleared when a stone is placed, a new or opened game starts, or
    /// another puzzle is loaded.
    pub puzzle_cursor: Option<Move>,
    /// The rectangle the board is drawn into, in physical pixels.
    pub viewport: crate::camera::Viewport,
    /// The same rectangle in interface points, as the interface reports it.
    pub viewport_points: [f32; 4],
    /// The view from the settings, until a viewport is known.
    remembered: Option<gomoku_core::ViewSettings>,
    /// True once the view has been set with a known viewport.
    pub fitted: bool,
    /// True while the board's size belongs to the window: the whole board is
    /// fitted into the area it is drawn into, and follows that area as the
    /// area grows and shrinks. Zooming or panning by hand breaks the tie;
    /// fitting the board restores it.
    pub follows_window: bool,
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
    perform: Option<After>,
    /// An action to perform once the game has been saved.
    after_save: Option<After>,
    /// Lab Explorer state, when a corpus is open.
    explorer: Option<ExplorerSession>,
    /// The one right-panel section that is open.
    open_panel_section: PanelSection,
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
            ai: Controller::default(),
            native_dialog_open: false,
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
            last_corpus_directory: settings.files.last_corpus_directory.clone(),
            recent: settings.files.recent.clone(),
            knock: None,
            title_stale: true,
            puzzle_cursor: None,
            viewport: crate::camera::Viewport::window(1180, 950),
            viewport_points: [0.0, 28.0, 930.0, 922.0],
            remembered: Some(settings.view.clone()),
            fitted: false,
            follows_window: true,
            panel_width: settings.panel.width,
            panel_collapsed: settings.panel.collapsed,
            pointer: [-1.0, -1.0],
            hover: None,
            pixels_per_point: 1.0,
            perform: None,
            after_save: None,
            explorer: None,
            open_panel_section: PanelSection::Moves,
        }
    }

    /// The one right-panel section that is open.
    ///
    /// Explorer falls back to Moves when no corpus is open.
    pub fn open_panel_section(&self) -> PanelSection {
        match self.open_panel_section {
            PanelSection::Explorer if self.explorer.is_none() => PanelSection::Moves,
            section => section,
        }
    }

    /// Open one right-panel section, closing the previously open section.
    ///
    /// Requesting Explorer without a corpus opens Moves instead.
    pub fn toggle_panel_section(&mut self, section: PanelSection) {
        self.open_panel_section = match section {
            PanelSection::Explorer if self.explorer.is_none() => PanelSection::Moves,
            _ => section,
        };
    }

    /// Read-only game access; all changes pass through the session.
    pub fn game(&self) -> &Game {
        &self.game
    }

    /// Open an indexed corpus in the Explorer.
    pub fn open_explorer_corpus(&mut self, records: Vec<gomoku_core::IndexEntry>) {
        self.explorer = Some(ExplorerSession {
            records,
            selected: None,
            filter: String::new(),
            jump: String::new(),
        });
        self.open_panel_section = PanelSection::Explorer;
    }

    /// Read-only access to Explorer state.
    pub fn explorer(&self) -> Option<&ExplorerSession> {
        self.explorer.as_ref()
    }

    /// Keep the Explorer filter text the user typed. A no-op without a corpus.
    pub fn set_explorer_filter(&mut self, filter: String) {
        if let Some(explorer) = &mut self.explorer {
            explorer.filter = filter;
        }
    }

    /// Keep the Explorer jump-to-index text the user typed. A no-op without a
    /// corpus.
    pub fn set_explorer_jump(&mut self, jump: String) {
        if let Some(explorer) = &mut self.explorer {
            explorer.jump = jump;
        }
    }

    /// The record index the jump field names, when it is valid.
    ///
    /// # Errors
    /// An error message when no corpus is open, the text is not a number, or
    /// the number is outside the corpus.
    pub fn explorer_jump_index(&self) -> Result<usize, String> {
        let Some(explorer) = self.explorer.as_ref() else {
            return Err("no explorer".to_string());
        };
        let index: usize = explorer
            .jump
            .parse()
            .map_err(|_| "not a number".to_string())?;
        if index >= explorer.records.len() {
            return Err("index out of range".to_string());
        }
        Ok(index)
    }

    /// Select an Explorer record without loading it onto the board.
    ///
    /// An index outside the corpus is ignored. This is a no-op without a corpus.
    pub fn select_explorer_record(&mut self, index: usize) {
        if let Some(explorer) = &mut self.explorer {
            if index < explorer.records.len() {
                explorer.selected = Some(index);
            }
        }
    }

    /// Load a record from the Explorer into the board.
    ///
    /// The record comes through the registry, so whatever format it is in is
    /// the registry's concern. Only a game can go on the board; anything else
    /// is reported and the sitting is kept. An index outside the corpus is
    /// ignored.
    pub fn load_explorer_record(&mut self, registry: &gomoku_core::Registry, index: usize) {
        let Some(entry) = self
            .explorer
            .as_ref()
            .and_then(|explorer| explorer.records.get(index))
        else {
            return;
        };
        let path = entry.path.clone();
        match registry.open(&path) {
            Ok(gomoku_core::OpenedThing::SingleRecord(gomoku_core::RecordUnit::Game(game))) => {
                self.pause_automation();
                self.dismiss_dialog();
                self.game = game;
                self.path = None;
                self.puzzle_cursor = None;
                self.revision += 1;
                self.saved_revision = self.revision;
                self.title_stale = true;
                if let Some(explorer) = &mut self.explorer {
                    explorer.selected = Some(index);
                }
            }
            Ok(_) => self.fail("The record is not a game", &"expected a game record"),
            Err(error) => self.fail("The record could not be loaded", &error),
        }
    }

    /// Install startup/demo content before delivery is connected.
    pub(crate) fn install_game(&mut self, game: Game) {
        self.dismiss_dialog();
        self.game = game;
        self.ai.invalidate();
        self.clear_board_effects();
        self.title_stale = true;
    }

    /// Change record metadata through the same save-revision boundary as moves.
    pub fn set_players(&mut self, black: Option<String>, white: Option<String>) {
        self.game.set_players(black, white);
        self.revision += 1;
        self.title_stale = true;
    }

    /// Bracket synchronous native pickers; cancellation must also invalidate work.
    pub fn begin_native_dialog(&mut self) {
        self.ai.invalidate();
        self.native_dialog_open = true;
        self.clear_board_effects();
    }
    /// End a native picker, including its cancellation path.
    pub fn end_native_dialog(&mut self) {
        self.ai.invalidate();
        self.native_dialog_open = false;
        self.clear_board_effects();
    }
    /// Read the current application modal without exposing mutable game state.
    pub fn dialog(&self) -> Option<&Dialog> {
        self.dialog.as_ref()
    }
    /// Replace a modal, first restoring any uncommitted puzzle preview.
    pub fn set_dialog(&mut self, dialog: Dialog) {
        self.dismiss_dialog();
        self.ai.invalidate();
        self.dialog = Some(dialog);
        self.clear_board_effects();
    }
    /// Start the same unsaved transaction for menus, recents, and window close.
    pub fn attempt(&mut self, action: After) {
        self.dismiss_dialog();
        if self.changed() {
            self.set_dialog(Dialog::Unsaved { then: action });
        } else {
            self.defer_action(action);
        }
    }

    /// Block automatic turns until the confirmed action has been consumed.
    pub fn defer_action(&mut self, action: After) {
        self.ai.invalidate();
        self.clear_board_effects();
        self.perform = Some(action);
    }
    /// Block automatic turns while an action awaits its save transaction.
    pub fn defer_after_save(&mut self, action: After) {
        self.ai.invalidate();
        self.clear_board_effects();
        self.after_save = Some(action);
    }
    /// Hand one confirmed action to the application.
    pub fn take_action(&mut self) -> Option<After> {
        self.perform.take()
    }
    /// Finish a save transaction. Failure and cancellation consume, but never
    /// execute, its continuation. A successful action can be returned only once.
    pub fn complete_save(&mut self, outcome: SaveOutcome) -> Option<After> {
        self.after_save
            .take()
            .filter(|_| outcome == SaveOutcome::Saved)
    }

    /// Who currently controls a color.
    pub fn owner(&self, side: Color) -> SeatOwner {
        self.ai.owner(side)
    }
    /// Retained AI settings, also available while the seat is human.
    pub fn ai_options(&self, side: Color) -> AiOptions {
        self.ai.options(side)
    }
    /// Replace one seat's settings without changing its ownership or save state.
    ///
    /// # Errors
    /// Returns [`InvalidAiOptions`] without changing anything for invalid input.
    pub fn set_ai_options(
        &mut self,
        side: Color,
        options: AiOptions,
    ) -> Result<(), InvalidAiOptions> {
        self.ai.set_options(side, options)
    }
    /// Current invalidation generation, independent of saved-game revisions.
    pub fn generation(&self) -> u64 {
        self.ai.generation
    }
    /// Whether automatic turns are enabled (eligibility can still block them).
    pub fn automation_running(&self) -> bool {
        self.ai.running
    }
    /// Take a color immediately, retaining its AI settings and invalidating work.
    pub fn take_seat(&mut self, side: Color) {
        self.ai.set_owner(side, SeatOwner::Human);
        self.update_hover();
    }
    /// Give a color to AI; releasing the current color also resumes automation.
    pub fn release_seat(&mut self, side: Color) {
        self.ai.set_owner(side, SeatOwner::Ai);
        if side == self.game.to_move() {
            self.resume_automation();
        }
        self.update_hover();
    }
    /// Explicitly resume after review or an error, without bypassing modals.
    pub fn resume_automation(&mut self) {
        self.ai.invalidate();
        self.ai.running = true;
        self.notice = None;
    }
    fn clear_board_effects(&mut self) {
        self.knock = None;
        self.hover = None;
    }
    fn pause_automation(&mut self) {
        self.ai.invalidate();
        self.ai.running = false;
        self.clear_board_effects();
    }
    fn unblocked(&self) -> bool {
        !self.native_dialog_open
            && self.dialog.is_none()
            && self.perform.is_none()
            && self.after_save.is_none()
    }
    fn ai_eligible(&self) -> bool {
        self.unblocked()
            && self.ai.running
            && !self.game.is_rewound()
            && self.game.outcome() == Outcome::Ongoing
            && self.game.can_play()
            && self.owner(self.game.to_move()) == SeatOwner::Ai
    }
    /// Reserve one flight; stale work still occupies the slot until replied to.
    pub fn next_ai_request(&mut self) -> Option<AiRequest> {
        if !self.ai_eligible() {
            return None;
        }
        self.ai.reserve(self.game.board_snapshot())
    }
    /// Retire only matching work, and commit only a still-current decision.
    pub fn apply_ai_reply(&mut self, reply: AiReply) -> bool {
        if !self.ai.retire(reply.flight)
            || reply.flight.generation != self.ai.generation
            || reply.flight.side != self.game.to_move()
            || !self.ai_eligible()
        {
            return false;
        }
        let result = match reply.result {
            Ok(decision) => match decision.action {
                DecisionAction::Play(point) => self.commit_move(point),
                DecisionAction::Resign(_) => self.game.resign().map(|()| {
                    self.revision += 1;
                    self.ai.invalidate();
                    self.knock = None;
                    self.hover = None;
                    self.title_stale = true;
                }),
            },
            Err(error) => {
                self.ai_error(&error);
                return false;
            }
        };
        if let Err(error) = result {
            self.ai_error(&error);
            return false;
        }
        true
    }
    fn ai_error(&mut self, error: &dyn std::fmt::Display) {
        self.pause_automation();
        self.notice = Some(format!("AI paused: {error}"));
    }

    /// Fit the whole board into the viewport.
    pub fn fit(&mut self) {
        self.camera.reset(self.viewport);
        self.follows_window = true;
        self.title_stale = true;
    }

    /// Turn the board around.
    pub fn flip(&mut self) {
        self.camera.flipped = !self.camera.flipped;
        self.title_stale = true;
    }

    /// True when the pointer lies over the board.
    ///
    /// The interface does not claim the pointer over the board, because the board
    /// is drawn outside egui. So the application needs its own test, to know when
    /// a click belongs to the game and when it belongs to a panel.
    ///
    /// The pointer arrives in physical pixels, the same as the viewport, so those
    /// two are compared. Comparing the pointer with the area in interface points
    /// instead throws away every click past the half of the board on a display
    /// with a scale factor of two.
    pub fn on_board(&self) -> bool {
        let pointer = self.pointer;
        pointer[0] >= self.viewport.x
            && pointer[1] >= self.viewport.y
            && pointer[0] < self.viewport.x + self.viewport.width
            && pointer[1] < self.viewport.y + self.viewport.height
    }

    /// Work out which intersection the pointer is over.
    pub fn update_hover(&mut self) {
        let cell = self.camera.intersection(self.viewport, self.pointer);
        self.hover = cell
            .and_then(|cell| gomoku_core::point(cell[1], cell[0]))
            .filter(|point| {
                self.on_board()
                    && self.game.stone_at(*point).is_none()
                    && self.game.can_play()
                    && self.owner(self.game.to_move()) == SeatOwner::Human
                    && self.unblocked()
            });
    }

    /// Continue the autosaved game.
    pub fn resume_autosave(&mut self) {
        match autosave_path() {
            Ok(path) => self.resume_autosave_from(&path, read_autosave_source()),
            Err(error) => self.fail("The autosaved game could not be found", &error),
        }
    }

    /// Recover from an explicit autosave location, without touching global files.
    /// A read failure preserves the sitting and replaces its modal with an error.
    pub fn resume_autosave_from(&mut self, path: &Path, source: Option<PathBuf>) {
        match gomoku_core::load(path) {
            Ok(game) => {
                self.dismiss_dialog();
                self.pause_automation();
                self.game = game;
                self.path = source;
                self.puzzle_cursor = None;
                self.revision += 1;
                self.notice = Some("continued the autosaved game".to_string());
                self.title_stale = true;
            }
            Err(error) => self.fail("The autosaved game could not be read", &error),
        }
    }

    /// An untouched, ongoing empty game has nothing to recover. A zero-move
    /// resignation (or an imported root) is not such a game.
    pub fn is_fresh_game(&self) -> bool {
        fresh_game(&self.game)
    }

    /// Throw the autosave away.
    pub fn discard_autosave(&mut self) {
        self.discard_autosave_from(
            [autosave_path(), autosave_state_path()]
                .into_iter()
                .flatten(),
        );
    }

    /// Remove recovery files at explicit filesystem locations.
    pub(crate) fn discard_autosave_from(&mut self, paths: impl IntoIterator<Item = PathBuf>) {
        for path in paths {
            match std::fs::remove_file(&path) {
                Ok(()) => {}
                // There is usually nothing to throw away, which is not a problem.
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
                Err(error) => {
                    log::warn!("the autosave {} was left behind: {error}", path.display())
                }
            }
        }
        self.notice = Some("the autosave was discarded".to_string());
    }

    /// Write the game in progress, so that it survives a crash or a quit.
    pub fn autosave(&mut self) {
        let (Ok(path), Ok(state_path)) = (autosave_path(), autosave_state_path()) else {
            return;
        };
        self.autosave_to(&path, &state_path);
    }

    /// Write recovery files at explicit locations (also usable by headless hosts).
    pub fn autosave_to(&mut self, path: &Path, state_path: &Path) {
        if let Err(error) = gomoku_core::save(path, &self.game) {
            log::warn!("the game could not be autosaved: {error}");
            // A puzzle position cannot be saved as a moves-only record. Throw
            // any older autosave away so the next launch does not offer a stale
            // normal-game autosave in place of the lost puzzle game.
            if matches!(error, gomoku_core::RecordError::FromPosition) {
                let _ = std::fs::remove_file(path);
                let _ = std::fs::remove_file(state_path);
                log::trace!("discarded stale autosave after FromPosition error");
            }
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
        if let Err(error) = gomoku_core::write_atomic(state_path, &state) {
            log::warn!("the autosave state could not be written: {error}");
        }
    }

    /// An autosave that is worth offering to continue, if there is one.
    pub fn autosave_to_offer(&self) -> Option<Option<PathBuf>> {
        Self::autosave_to_offer_from(&autosave_path().ok()?, read_autosave_source())
    }

    /// Inspect an explicit recovery file using the same startup-offer policy.
    pub fn autosave_to_offer_from(path: &Path, source: Option<PathBuf>) -> Option<Option<PathBuf>> {
        let modified = std::fs::metadata(path).ok()?.modified().ok()?;
        if let Some(source) = &source {
            // Only offer it when the autosave is newer than the file it came from.
            if let Ok(saved) = std::fs::metadata(source).and_then(|meta| meta.modified()) {
                if modified <= saved {
                    log::trace!("autosave is not newer than its source; not offering");
                    return None;
                }
            }
        }
        let game = gomoku_core::load(path).ok()?;
        if fresh_game(&game) {
            log::trace!("autosave is empty; not offering");
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
        if !self.human_can_place(point) {
            return;
        }
        if self.game.pending_truncation() > 0 {
            self.set_dialog(Dialog::Truncate {
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
        if self.human_can_place(point) && self.commit_move(point).is_ok() {
            self.ai.running = true;
            self.notice = None;
        }
    }

    fn human_can_place(&self, point: Move) -> bool {
        self.unblocked()
            && self.owner(self.game.to_move()) == SeatOwner::Human
            && self.game.can_play()
            && self.game.stone_at(point).is_none()
    }

    /// Confirm only the pending placement, rechecking ownership at confirmation.
    pub fn confirm_truncation(&mut self) {
        if let Some(Dialog::Truncate { point, .. }) = self.dialog {
            self.dismiss_dialog();
            self.place_now(point);
        }
    }

    fn commit_move(&mut self, point: Move) -> Result<(), gomoku_core::GameError> {
        self.game.play(point)?;
        self.ai.invalidate();
        self.revision += 1;
        self.knock = Some(neighbours(&self.game, point));
        self.puzzle_cursor = None;
        self.hover = None;
        self.title_stale = true;
        Ok(())
    }

    /// Take the last move back. The move is gone.
    pub fn undo(&mut self) {
        self.pause_automation();
        if self.game.undo() {
            self.revision += 1;
            self.title_stale = true;
        }
    }

    /// Step the view back one move.
    pub fn rewind(&mut self) {
        self.pause_automation();
        if self.game.rewind() {
            self.title_stale = true;
        }
    }

    /// Step the view forward one move, keeping automation paused even at root.
    pub fn forward(&mut self) {
        self.pause_automation();
        if self.game.forward() {
            self.title_stale = true;
        }
    }

    /// Jump to a move number, or to the latest move with `usize::MAX`.
    /// Navigation always pauses; returning live is not a request to resume.
    pub fn seek(&mut self, index: usize) {
        self.pause_automation();
        if self.game.seek(index) {
            self.title_stale = true;
        }
    }

    /// A new, empty game. The player names are kept.
    pub fn new_game(&mut self) {
        self.dismiss_dialog();
        self.ai.invalidate();
        self.ai.running = true;
        self.clear_board_effects();
        let created = self.game.meta().created;
        let black = self.game.meta().black.clone();
        let white = self.game.meta().white.clone();
        let mut game = Game::started_at(created);
        game.set_players(black, white);
        self.game = game;
        self.path = None;
        self.puzzle_cursor = None;
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

    /// Save to the open file, asking the supplied picker only for an untitled
    /// game. The picker boundary is injectable so headless tests never open UI.
    pub fn save(&mut self, pick: impl FnOnce() -> Option<PathBuf>) -> SaveOutcome {
        match self.path.clone() {
            Some(path) => self.save_to(&path),
            None => self.save_as(pick),
        }
    }

    /// Save to a chosen destination, bracketing even picker cancellation.
    pub fn save_as(&mut self, pick: impl FnOnce() -> Option<PathBuf>) -> SaveOutcome {
        self.begin_native_dialog();
        let path = pick();
        self.end_native_dialog();
        match path {
            Some(path) => self.save_to(&path),
            None => SaveOutcome::Cancelled,
        }
    }

    /// Save to a given file and report whether the write actually succeeded.
    pub fn save_to(&mut self, path: &Path) -> SaveOutcome {
        match gomoku_core::save(path, &self.game) {
            Ok(()) => {
                self.saved_revision = self.revision;
                self.path = Some(path.to_path_buf());
                self.remember(path);
                self.notice = Some(format!("saved to {}", name_of(path)));
                self.title_stale = true;
                SaveOutcome::Saved
            }
            Err(error) => {
                self.fail("The game could not be saved", &error);
                SaveOutcome::Failed
            }
        }
    }

    /// Open a game from a file.
    pub fn open(&mut self, path: &Path) {
        match gomoku_core::load(path) {
            Ok(game) => {
                self.dismiss_dialog();
                self.pause_automation();
                self.game = game;
                self.path = Some(path.to_path_buf());
                self.puzzle_cursor = None;
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

    /// Load a puzzle file and either open the single puzzle or ask the user
    /// to pick one.
    pub fn open_puzzle(&mut self, path: &Path) {
        match gomoku_core::load_puzzles(path) {
            Ok(puzzles) if puzzles.is_empty() => {
                self.fail("No puzzles in file", &"the file contains no puzzles");
            }
            Ok(puzzles) if puzzles.len() == 1 => {
                if self.load_puzzle(&puzzles[0]).is_ok() {
                    self.notice = Some(format!("loaded puzzle {}", puzzles[0].name));
                    self.remember(path);
                }
            }
            Ok(puzzles) => {
                self.dismiss_dialog();
                let before = Box::new(BeforePuzzle::from(&*self));
                // Show the first puzzle right away: the picker chooses with
                // the eyes, not with an index.
                self.preview_puzzle(&puzzles[0]);
                self.dialog = Some(Dialog::PickPuzzle {
                    puzzles,
                    index: 0,
                    before,
                });
                self.title_stale = true;
                self.remember(path);
            }
            Err(error) => self.fail("The puzzle could not be loaded", &error),
        }
    }

    /// Change the current picker selection without replacing its saved game.
    pub fn select_puzzle(&mut self, index: usize) {
        let puzzle = match self.dialog.as_mut() {
            Some(Dialog::PickPuzzle {
                puzzles,
                index: selected,
                ..
            }) if index < puzzles.len() => {
                if index == *selected {
                    return;
                }
                *selected = index;
                puzzles[index].clone()
            }
            _ => return,
        };
        self.preview_puzzle(&puzzle);
    }

    /// Show a puzzle position on the board while the picker is open, without
    /// committing to it. `cancel_puzzle_picker` undoes the preview;
    /// `load_puzzle` commits it.
    fn preview_puzzle(&mut self, puzzle: &Puzzle) {
        if let Ok(game) =
            Game::from_position(puzzle.black.clone(), puzzle.white.clone(), puzzle.to_move)
        {
            self.pause_automation();
            self.game = game;
            self.puzzle_cursor = puzzle
                .solution
                .as_ref()
                .and_then(|line| line.first())
                .copied();
            self.hover = None;
        }
    }

    /// Close the puzzle picker without loading, restoring the game the picker
    /// has been previewing over.
    pub fn cancel_puzzle_picker(&mut self, before: BeforePuzzle) {
        self.restore_before(before);
        self.dialog = None;
    }

    /// Close the open dialog, whatever it is. The puzzle picker first
    /// restores the game it has been previewing over.
    pub fn dismiss_dialog(&mut self) {
        if self.dialog.is_some() {
            self.ai.invalidate();
        }
        if let Some(Dialog::PickPuzzle { before, .. }) = self.dialog.take() {
            self.restore_before(*before);
        }
    }

    /// Bring back the game as it was before the puzzle picker opened.
    fn restore_before(&mut self, before: BeforePuzzle) {
        self.ai.restore_intent(before.automation);
        self.knock = None;
        self.hover = None;
        self.game = before.game;
        self.path = before.path;
        self.puzzle_cursor = before.puzzle_cursor;
        self.revision = before.revision;
        self.saved_revision = before.saved_revision;
        self.title_stale = true;
    }

    /// Build the game for the selected puzzle and clear any previous puzzle
    /// cursor.
    ///
    /// Returns `Ok(())` when the puzzle was loaded, or `Err` when the
    /// position was rejected with a core-owned [`PositionError`]. On error a
    /// message dialog has already been opened; the current game is unchanged.
    pub fn load_puzzle(&mut self, puzzle: &Puzzle) -> Result<(), PositionError> {
        match Game::from_position(puzzle.black.clone(), puzzle.white.clone(), puzzle.to_move) {
            Ok(game) => {
                self.dismiss_dialog();
                self.pause_automation();
                self.ai.set_owner(Color::Black, SeatOwner::Human);
                self.ai.set_owner(Color::White, SeatOwner::Human);
                self.game = game;
                self.path = None;
                self.puzzle_cursor = puzzle
                    .solution
                    .as_ref()
                    .and_then(|line| line.first())
                    .copied();
                self.revision += 1;
                self.saved_revision = self.revision;
                self.dialog = None;
                self.title_stale = true;
                Ok(())
            }
            Err(error) => {
                self.fail("The puzzle position is invalid", &error);
                Err(error)
            }
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

    /// Remember a successfully opened Explorer corpus directory.
    pub fn remember_corpus_directory(&mut self, directory: &Path) {
        self.last_corpus_directory = Some(directory.to_path_buf());
    }

    /// Show a failure to the user, with the detail.
    pub fn fail(&mut self, title: &str, error: &dyn std::fmt::Display) {
        log::error!("{title}: {error}");
        self.set_dialog(Dialog::Message {
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
            Outcome::Won {
                winner,
                method: gomoku_core::WinMethod::Resignation,
            } => match winner {
                Color::Black => "White resigned — Black wins",
                Color::White => "Black resigned — White wins",
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
                // Zero means "fit the board at start-up". The board's size
                // belongs to the window, so no zoom is remembered: whatever
                // the camera happens to show, the next sitting starts fitted.
                pixels_per_cell: 0.0,
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
                last_corpus_directory: self.last_corpus_directory.clone(),
                recent: self.recent.clone(),
            },
        }
    }

    /// Set the view, now that the area for the board is known.
    ///
    /// The area is not the window: the menu bar and the side panel take their
    /// space first, and the board must fit in what is left. So the first frame
    /// of the interface is what places the view.
    ///
    /// The board's size belongs to the window, so the view always starts
    /// fitted, whatever zoom an earlier sitting stored. Only the side the
    /// board is seen from is remembered.
    pub fn place_view(&mut self, viewport: crate::camera::Viewport) {
        self.fitted = true;
        self.follows_window = true;
        self.camera.reset(viewport);
        if let Some(view) = self.remembered.take() {
            self.camera.flipped = view.flipped;
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

fn fresh_game(game: &Game) -> bool {
    game.is_empty() && game.outcome() == Outcome::Ongoing && !game.is_from_position()
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

/// The winning line for a five-in-a-row result: the run of five or more
/// stones that ends at the last move. Resignation has no winning line.
pub fn winning_line(game: &Game) -> Option<[Move; 2]> {
    let winner = match game.outcome() {
        Outcome::Won {
            winner,
            method: WinMethod::Five,
        } => winner,
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

    /// A file of our own, so that the tests do not touch the user's files.
    fn scratch(name: &str) -> PathBuf {
        std::env::temp_dir().join(format!("gomoku-test-{name}-{}.json", std::process::id()))
    }

    fn remove(path: &Path) {
        assert!(
            std::fs::remove_file(path).is_ok(),
            "the test file {} is removed",
            path.display()
        );
    }

    #[test]
    fn save_first_only_releases_the_action_once_after_success() {
        let path = scratch("save-first");
        let mut session = Session::new(&Settings::default());
        session.place(gomoku_core::point(7, 7).unwrap());
        session.defer_after_save(After::NewGame);
        assert_eq!(session.complete_save(SaveOutcome::Cancelled), None);
        assert_eq!(session.complete_save(SaveOutcome::Saved), None);
        session.defer_after_save(After::Quit);
        let blocker = scratch("save-first-blocker");
        std::fs::write(&blocker, "not a directory").unwrap();
        let outcome = session.save_to(&blocker.join("game.json"));
        remove(&blocker);
        assert_eq!(outcome, SaveOutcome::Failed);
        assert_eq!(session.complete_save(outcome), None);
        assert_eq!(session.complete_save(SaveOutcome::Saved), None);
        assert!(session.changed());
        assert!(matches!(session.dialog(), Some(Dialog::Message { .. })));
        session.dismiss_dialog();
        session.defer_after_save(After::NewGame);
        let outcome = session.save_to(&path);
        assert_eq!(outcome, SaveOutcome::Saved);
        assert_eq!(session.complete_save(outcome), Some(After::NewGame));
        assert_eq!(session.complete_save(outcome), None);
        remove(&path);
    }

    #[test]
    fn save_picker_cancellation_and_failure_never_leak_a_continuation() {
        let mut session = Session::new(&Settings::default());
        session.place(gomoku_core::point(7, 7).unwrap());
        for action in [After::NewGame, After::Open, After::OpenPuzzle, After::Quit] {
            session.defer_after_save(action);
            let outcome = session.save(|| None);
            assert_eq!(outcome, SaveOutcome::Cancelled);
            assert_eq!(session.complete_save(outcome), None);
            assert_eq!(session.complete_save(SaveOutcome::Saved), None);
            assert!(session.automation_running());
        }
        let path = scratch("save-picker");
        session.defer_after_save(After::NewGame);
        let outcome = session.save(|| Some(path.clone()));
        assert_eq!(session.complete_save(outcome), Some(After::NewGame));
        assert_eq!(session.complete_save(outcome), None);
        session.defer_after_save(After::Quit);
        let outcome = session.save(|| panic!("existing path must not open picker"));
        assert_eq!(session.complete_save(outcome), Some(After::Quit));
        remove(&path);
    }

    #[test]
    fn failure_replacing_a_picker_restores_the_real_game_and_keeps_the_error() {
        let mut session = Session::new(&Settings::default());
        session.place(gomoku_core::point(7, 7).unwrap());
        let expected = session.game().clone();
        let puzzle = Puzzle {
            name: "preview".into(),
            black: vec![gomoku_core::point(3, 3).unwrap()],
            white: vec![],
            to_move: Color::White,
            solution: None,
            depth: None,
        };
        let before = Box::new(BeforePuzzle::from(&session));
        session.set_dialog(Dialog::PickPuzzle {
            puzzles: vec![puzzle.clone()],
            index: 0,
            before,
        });
        session.preview_puzzle(&puzzle);
        let path = scratch("failed-picker-open");
        std::fs::write(&path, "not a game").unwrap();
        session.open(&path);
        remove(&path);
        assert_eq!(session.game(), &expected);
        assert!(session.automation_running());
        assert!(session.changed());
        assert!(matches!(session.dialog(), Some(Dialog::Message { .. })));
    }

    #[test]
    fn recent_open_uses_the_same_unsaved_transaction_as_the_picker() {
        let mut session = Session::new(&Settings::default());
        session.place(gomoku_core::point(7, 7).unwrap());
        let before = session.game().clone();
        for action in [After::Open, After::OpenRecent(scratch("recent"))] {
            session.attempt(action.clone());
            assert_eq!(session.dialog(), Some(&Dialog::Unsaved { then: action }));
            assert!(session.take_action().is_none());
            assert_eq!(session.game(), &before);
            session.dismiss_dialog();
            assert!(session.automation_running());
        }
        session.new_game();
        let action = After::OpenRecent(scratch("recent"));
        session.attempt(action.clone());
        assert_eq!(session.take_action(), Some(action));
        assert_eq!(session.take_action(), None);
    }

    #[test]
    fn zero_move_resignation_is_recoverable_not_a_fresh_game() {
        let path = scratch("zero-move-autosave");
        let state_path = scratch("zero-move-state");
        let mut session = Session::new(&Settings::default());
        assert!(session.is_fresh_game());
        session.release_seat(Color::Black);
        let request = session.next_ai_request().unwrap();
        assert!(
            session.apply_ai_reply(request.reply(Ok(crate::ai::AiPlayerDecision {
                action: DecisionAction::Resign(player::ResignReason::VerifiedOpponentVcf {
                    candidates: 1
                }),
                search: None,
                guard: None,
            })))
        );
        assert!(session.game().is_empty());
        assert!(!session.is_fresh_game());
        session.autosave_to(&path, &state_path);
        assert_eq!(Session::autosave_to_offer_from(&path, None), Some(None));
        let mut reader = Session::new(&Settings::default());
        reader.resume_autosave_from(&path, None);
        assert_eq!(reader.game().outcome(), session.game().outcome());
        assert!(!reader.automation_running());
        assert!(!reader.is_fresh_game());
        remove(&path);
        remove(&state_path);
    }

    #[test]
    fn failed_existing_path_and_save_as_preserve_game_and_cancel_the_action() {
        let blocker = scratch("blocked-save-parent");
        std::fs::write(&blocker, "not a directory").unwrap();
        for save_as in [false, true] {
            let mut session = Session::new(&Settings::default());
            session.place(gomoku_core::point(7, 7).unwrap());
            let before = session.game().clone();
            let failed_path = blocker.join("game.json");
            if !save_as {
                session.path = Some(failed_path.clone());
            }
            let original_path = session.path.clone();
            session.defer_after_save(After::OpenRecent(scratch("never-opened")));
            let outcome = if save_as {
                session.save_as(|| Some(failed_path))
            } else {
                session.save(|| panic!("an existing path must not ask for a destination"))
            };
            assert_eq!(outcome, SaveOutcome::Failed);
            assert_eq!(session.complete_save(outcome), None);
            assert_eq!(session.complete_save(SaveOutcome::Saved), None);
            assert_eq!(session.path, original_path);
            assert_eq!(session.game(), &before);
            assert!(session.changed());
            assert!(matches!(session.dialog(), Some(Dialog::Message { .. })));
            assert!(session.automation_running());
            session.dismiss_dialog();
            assert!(
                session.next_ai_request().is_some(),
                "failure must not leave an invisible blocker"
            );
        }
        remove(&blocker);
    }

    #[test]
    fn saving_clears_the_change_mark() {
        let path = scratch("saving");
        let settings = Settings::default();
        let mut session = Session::new(&settings);
        session.take_seat(Color::Black);
        session.take_seat(Color::White);
        session.place_now(gomoku_core::point(7, 7).expect("inside"));
        assert!(session.changed(), "a new move is a change to save");
        session.save_to(&path);
        assert!(!session.changed(), "the game is in a file now");
        assert_eq!(session.path.as_deref(), Some(path.as_path()));
        assert!(path.exists(), "the file is written");
        session.place_now(gomoku_core::point(8, 8).expect("inside"));
        assert!(session.changed(), "and the next move changes it again");
        remove(&path);
    }

    #[test]
    fn a_saved_game_opens_exactly_as_it_was_saved() {
        let path = scratch("round-trip");
        let settings = Settings::default();
        let mut writer = Session::new(&settings);
        writer.game = game();
        writer.save_to(&path);

        let mut reader = Session::new(&settings);
        reader.open(&path);
        assert_eq!(reader.game.len(), 9, "every move came back");
        assert_eq!(
            reader.game.stones().collect::<Vec<_>>(),
            writer.game.stones().collect::<Vec<_>>(),
            "in the same places, with the same colours"
        );
        assert!(!reader.changed(), "an opened game is not a change to save");
        remove(&path);
    }

    #[test]
    fn a_file_that_is_not_a_game_is_reported() {
        let path = scratch("rubbish");
        assert!(std::fs::write(&path, b"this is not a game").is_ok());
        let settings = Settings::default();
        let mut session = Session::new(&settings);
        session.place_now(gomoku_core::point(7, 7).expect("inside"));
        session.open(&path);
        match session.dialog {
            Some(Dialog::Message { ref title, .. }) => {
                assert_eq!(title, "The game could not be opened");
            }
            ref other => panic!("expected a message dialog, got {other:?}"),
        }
        assert_eq!(session.game.len(), 1, "the game in hand is untouched");
        assert!(session.path.is_none(), "and it still has no name");
        remove(&path);
    }

    #[test]
    fn looking_back_does_not_change_the_game() {
        let settings = Settings::default();
        let mut session = Session::new(&settings);
        session.place_now(gomoku_core::point(7, 7).expect("inside"));
        session.save_to(&scratch("looking"));
        assert!(!session.changed());
        session.rewind();
        assert!(!session.changed(), "a view is not a change");
        session.seek(0);
        assert!(!session.changed());
        session.forward();
        assert!(!session.changed());
        session.undo();
        assert!(session.changed(), "but taking a move back is one");
        remove(&scratch("looking"));
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
    fn resignation_has_no_winning_line_even_in_review() {
        let mut game = game();
        game.resign().unwrap();
        assert_eq!(winning_line(&game), None);
        game.rewind();
        assert_eq!(winning_line(&game), None);
    }

    #[test]
    fn the_view_starts_fitted_whatever_zoom_was_stored() {
        let mut settings = Settings::default();
        settings.view.pixels_per_cell = 240.0;
        settings.view.center = [3.0, 11.0];
        let mut session = Session::new(&settings);
        let viewport = crate::camera::Viewport::window(900, 872);
        session.place_view(viewport);
        assert!(
            (session.camera.pixels_per_cell - Camera::fit_scale(viewport)).abs() < 1e-3,
            "the stored zoom is not restored: {}",
            session.camera.pixels_per_cell
        );
        assert!(session.follows_window, "the board follows the window");
    }

    #[test]
    fn fitting_by_hand_reengages_the_window_tie() {
        let settings = Settings::default();
        let mut session = Session::new(&settings);
        session.viewport = crate::camera::Viewport::window(900, 872);
        session.follows_window = false;
        session.fit();
        assert!(session.follows_window);
    }

    #[test]
    fn no_zoom_is_written_at_exit() {
        let settings = Settings::default();
        let session = Session::new(&settings);
        let written = session.settings(&WindowGeometry::default());
        assert_eq!(
            written.view.pixels_per_cell, 0.0,
            "zero means fit the board at start-up"
        );
    }

    #[test]
    fn the_board_area_is_known() {
        let settings = Settings::default();
        let mut session = Session::new(&settings);
        session.viewport = crate::camera::Viewport {
            frame: [1200.0, 900.0],
            x: 0.0,
            y: 28.0,
            width: 900.0,
            height: 872.0,
        };
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
    fn the_board_area_is_known_on_a_scaled_display() {
        // On a display with a scale factor of two the area for the board is 940
        // by 872 interface points, which is 1880 by 1744 physical pixels, and the
        // pointer arrives in physical pixels. Comparing those two throws away
        // every click in the lower half of the board.
        let settings = Settings::default();
        let mut session = Session::new(&settings);
        session.pixels_per_point = 2.0;
        session.viewport_points = [0.0, 28.0, 940.0, 872.0];
        session.viewport = crate::camera::Viewport {
            frame: [2360.0, 1800.0],
            x: 0.0,
            y: 56.0,
            width: 1880.0,
            height: 1744.0,
        };
        session.pointer = [940.0, 872.0];
        assert!(
            session.on_board(),
            "the middle of the board, in physical pixels"
        );
        session.pointer = [940.0, 1700.0];
        assert!(
            session.on_board(),
            "past the height of the area in points, but still on the board"
        );
        session.pointer = [2200.0, 900.0];
        assert!(!session.on_board(), "the side panel, in physical pixels");
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
        session.take_seat(Color::Black);
        session.take_seat(Color::White);
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
        session.take_seat(Color::Black);
        session.take_seat(Color::White);
        for column in 0..3_u8 {
            session.place_now(gomoku_core::point(7, column).expect("inside"));
        }
        session.seek(1);
        session.place_now(gomoku_core::point(9, 9).expect("inside"));
        assert_eq!(session.game.len(), 2);
        assert!(session.changed());
    }

    #[test]
    fn load_puzzle_builds_the_position_and_sets_cursor() {
        let settings = Settings::default();
        let mut session = Session::new(&settings);
        let first_solution = gomoku_core::point(5, 7).expect("inside the board");
        let puzzle = gomoku_core::Puzzle {
            name: "test puzzle".to_string(),
            black: vec![gomoku_core::point(7, 7).expect("inside the board")],
            white: vec![gomoku_core::point(6, 7).expect("inside the board")],
            to_move: gomoku_core::Color::Black,
            solution: Some(vec![first_solution]),
            depth: Some(3),
        };
        assert!(session.load_puzzle(&puzzle).is_ok());
        assert!(session.game.is_from_position());
        assert_eq!(session.game.to_move(), gomoku_core::Color::Black);
        assert_eq!(
            session.game.stone_at(puzzle.black[0]),
            Some(gomoku_core::Color::Black)
        );
        assert_eq!(session.puzzle_cursor, Some(first_solution));
        assert!(session.path.is_none());
        assert!(session.dialog.is_none());
    }

    #[test]
    fn rejected_puzzles_report_core_errors_without_replacing_the_game() {
        let a = gomoku_core::point(3, 3).expect("inside");
        let b = gomoku_core::point(4, 3).expect("inside");
        let mut session = Session::new(&Settings::default());
        session.place_now(gomoku_core::point(7, 7).expect("inside"));
        session.path = Some(PathBuf::from("existing.json"));
        session.puzzle_cursor = Some(b);
        let before = session.game.clone();
        let revision = session.revision;
        let saved_revision = session.saved_revision;
        for (black, white, expected) in [
            (vec![a, a], vec![b], PositionError::DuplicateStone),
            (vec![a], vec![a], PositionError::OverlappingColors),
            (vec![a, b], vec![], PositionError::InvalidCounts),
        ] {
            let puzzle = Puzzle {
                name: "invalid".into(),
                black,
                white,
                to_move: Color::Black,
                solution: None,
                depth: None,
            };
            let message = expected.to_string();
            assert_eq!(session.load_puzzle(&puzzle), Err(expected));
            assert_eq!(session.game, before);
            assert_eq!(session.path, Some(PathBuf::from("existing.json")));
            assert_eq!(session.puzzle_cursor, Some(b));
            assert_eq!(session.revision, revision);
            assert_eq!(session.saved_revision, saved_revision);
            assert!(matches!(
                &session.dialog,
                Some(Dialog::Message { body, .. }) if body == &message
            ));
        }
    }

    #[test]
    fn dismissing_a_picker_keeps_the_original_game_available_for_saving() {
        let path = scratch("dismiss-picker");
        let mut session = Session::new(&Settings::default());
        session.take_seat(Color::Black);
        session.take_seat(Color::White);
        session.place_now(gomoku_core::point(7, 7).expect("inside"));
        session.save_to(&path);
        session.place_now(gomoku_core::point(8, 8).expect("inside"));
        session.rewind();
        let before = BeforePuzzle::from(&session);
        let expected = before.clone();
        let puzzle = Puzzle {
            name: "preview".to_string(),
            black: vec![gomoku_core::point(3, 3).expect("inside")],
            white: vec![],
            to_move: Color::White,
            solution: None,
            depth: None,
        };
        session.preview_puzzle(&puzzle);
        session.dialog = Some(Dialog::PickPuzzle {
            puzzles: vec![puzzle],
            index: 0,
            before: Box::new(before),
        });

        // The window-close path must do this before opening a save prompt.
        session.dismiss_dialog();
        assert_eq!(session.game, expected.game);
        assert_eq!(session.path, expected.path);
        assert_eq!(session.puzzle_cursor, expected.puzzle_cursor);
        assert_eq!(session.revision, expected.revision);
        assert_eq!(session.saved_revision, expected.saved_revision);
        assert!(session.changed());
        assert!(session.dialog.is_none());
        session.save_to(&path);
        assert!(!session.changed());
        assert_eq!(
            gomoku_core::load(&path)
                .expect("the real game is saved")
                .len(),
            2
        );
        remove(&path);
    }

    #[test]
    fn a_cancelled_picker_restores_the_game_without_a_trace() {
        let settings = Settings::default();
        let mut session = Session::new(&settings);
        let home = gomoku_core::point(7, 7).expect("inside the board");
        session.place(home);
        let before = BeforePuzzle::from(&session);
        let puzzle = gomoku_core::Puzzle {
            name: "test puzzle".to_string(),
            black: vec![gomoku_core::point(3, 3).expect("inside the board")],
            white: vec![],
            to_move: gomoku_core::Color::White,
            solution: None,
            depth: None,
        };
        session.preview_puzzle(&puzzle);
        assert_eq!(
            session.game.stone_at(puzzle.black[0]),
            Some(gomoku_core::Color::Black),
            "the preview really replaced the game"
        );
        assert!(
            session.game.stone_at(home).is_none(),
            "the old game is gone while the preview shows"
        );
        session.cancel_puzzle_picker(before);
        assert_eq!(
            session.game.stone_at(home),
            Some(gomoku_core::Color::Black),
            "the old game is back, with its own stone"
        );
        assert!(
            session.game.stone_at(puzzle.black[0]).is_none(),
            "and the puzzle stone is gone again"
        );
        assert!(
            session.changed(),
            "the unsaved stone is still unsaved, no more and no less"
        );
        assert!(session.dialog.is_none());
    }

    #[test]
    fn corpus_directory_is_mirrored_and_written_to_settings() {
        let initial = PathBuf::from("/games/initial-corpus");
        let settings = Settings {
            files: gomoku_core::FileSettings {
                last_corpus_directory: Some(initial.clone()),
                ..gomoku_core::FileSettings::default()
            },
            ..Settings::default()
        };
        let mut session = Session::new(&settings);
        assert_eq!(session.last_corpus_directory, Some(initial));

        let opened = PathBuf::from("/games/opened-corpus");
        session.remember_corpus_directory(&opened);
        assert_eq!(session.last_corpus_directory, Some(opened.clone()));
        assert_eq!(
            session
                .settings(&WindowGeometry::default())
                .files
                .last_corpus_directory,
            Some(opened)
        );
    }

    #[test]
    fn opening_a_panel_section_closes_the_previous_section() {
        let mut session = Session::new(&Settings::default());
        assert_eq!(session.open_panel_section(), PanelSection::Moves);

        session.toggle_panel_section(PanelSection::Explorer);
        assert_eq!(session.open_panel_section(), PanelSection::Moves);

        session.toggle_panel_section(PanelSection::Players);
        assert_eq!(session.open_panel_section(), PanelSection::Players);

        session.toggle_panel_section(PanelSection::Moves);
        assert_eq!(session.open_panel_section(), PanelSection::Moves);
    }

    #[test]
    fn explorer_corpus_is_stored() {
        use gomoku_core::{IndexEntry, RecordKind};
        let mut session = Session::new(&Settings::default());
        let entries = vec![IndexEntry {
            index: 0,
            kind: RecordKind::Game,
            name: "tiny.psq".into(),
            path: std::env::temp_dir().join("nonexistent.psq"),
        }];
        assert_eq!(session.open_panel_section(), PanelSection::Moves);
        session.open_explorer_corpus(entries.clone());
        assert!(session.explorer().is_some());
        assert_eq!(session.explorer().unwrap().records.len(), 1);
        assert_eq!(session.open_panel_section(), PanelSection::Explorer);
    }

    #[test]
    fn explorer_filter_and_jump_are_kept_and_validated() {
        use gomoku_core::{IndexEntry, RecordKind};
        let mut session = Session::new(&Settings::default());
        // Without a corpus the edits are dropped and the jump has no answer.
        session.set_explorer_filter("tiny".into());
        session.set_explorer_jump("0".into());
        assert_eq!(
            session.explorer_jump_index(),
            Err("no explorer".to_string())
        );
        let entries = (0..3)
            .map(|index| IndexEntry {
                index,
                kind: RecordKind::Game,
                name: format!("game-{index}.psq"),
                path: std::env::temp_dir().join(format!("game-{index}.psq")),
            })
            .collect();
        session.open_explorer_corpus(entries);
        session.set_explorer_filter("game-1".into());
        session.set_explorer_jump("2".into());
        assert_eq!(session.explorer().unwrap().filter, "game-1");
        assert_eq!(session.explorer_jump_index(), Ok(2));
        session.set_explorer_jump("3".into());
        assert_eq!(
            session.explorer_jump_index(),
            Err("index out of range".to_string())
        );
        session.set_explorer_jump("x".into());
        assert_eq!(
            session.explorer_jump_index(),
            Err("not a number".to_string())
        );
    }

    #[test]
    fn explorer_record_loads_onto_the_board_and_unknowns_are_ignored() {
        use gomoku_core::{IndexEntry, RecordKind};
        let path = std::env::temp_dir().join(format!(
            "gomoku-test-explorer-record-{}.psq",
            std::process::id()
        ));
        std::fs::write(
            &path,
            "Piskvorky 15x15, 0:0, 0\n8,8,0\n7,7,0\n9,9,0\n6,6,0\n10,7,0\n\
             BlackPlayer\nWhitePlayer\n-1\n1,Freestyle15_2\n",
        )
        .unwrap();
        let registry = gomoku_core::Registry::builder()
            .add(gomoku_core::PsqFormat)
            .build();
        let mut session = Session::new(&Settings::default());
        session.open_explorer_corpus(vec![IndexEntry {
            index: 0,
            kind: RecordKind::Game,
            name: "tiny.psq".into(),
            path: path.clone(),
        }]);
        // An index outside the corpus is a no-op, not an error.
        session.load_explorer_record(&registry, 9);
        assert!(session.game().is_empty());
        assert!(session.dialog().is_none());
        session.load_explorer_record(&registry, 0);
        assert_eq!(session.game().len(), 5);
        assert!(!session.changed(), "a loaded record has no unsaved changes");
        assert_eq!(session.explorer().unwrap().selected, Some(0));
        assert!(!session.automation_running());
        remove(&path);
    }
}
