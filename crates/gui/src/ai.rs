//! Headless automatic-turn requests and seat settings. No worker or event loop.

use engine::{Board, Color};
pub use player::{Decision as AiPlayerDecision, DecisionAction};

/// Who may place stones for a color.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SeatOwner {
    /// Board clicks may place stones.
    Human,
    /// Only a current automatic-turn reply may place stones.
    Ai,
}

/// Retained per-seat settings. Session validates these before accepting them.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct AiOptions {
    /// Simulations per decision, from 1 through 10,000.
    pub simulations: u32,
    /// Sampling temperature, from 0 through 2 in steps of 0.1.
    pub temperature: f32,
    /// Enable the owner's baseline own-side proof probe.
    pub own_tss: bool,
    /// Enable the owner's baseline defensive guard.
    pub guard: bool,
    /// Allow the owner's heuristic concession decision.
    pub resign: bool,
}

impl Default for AiOptions {
    fn default() -> Self {
        Self {
            simulations: 400,
            temperature: 0.0,
            own_tss: true,
            guard: true,
            resign: true,
        }
    }
}

impl AiOptions {
    /// Check user-editable numeric bounds before replacing retained settings.
    ///
    /// # Errors
    /// Returns [`InvalidAiOptions`] for out-of-range simulations, non-finite
    /// temperature, or a temperature off the 0.1 grid.
    pub fn validate(self) -> Result<(), InvalidAiOptions> {
        let tenths = self.temperature * 10.0;
        if !(1..=10_000).contains(&self.simulations)
            || !self.temperature.is_finite()
            || !(0.0..=2.0).contains(&self.temperature)
            || (tenths - tenths.round()).abs() > 0.00001
        {
            return Err(InvalidAiOptions);
        }
        Ok(())
    }

    fn player_options(self) -> player::PlayerOptions {
        player::PlayerOptions {
            simulations: self.simulations,
            temperature: self.temperature,
            c_puct: 1.5,
            noise_epsilon: 0.0,
            own: player::OwnTssConfig {
                enabled: self.own_tss,
                ..player::OwnTssConfig::baseline()
            },
            guard: player::GuardConfig {
                enabled: self.guard,
                ..player::GuardConfig::baseline()
            },
            resign: self.resign,
            ..player::PlayerOptions::default()
        }
    }
}

/// Numeric AI settings cannot be used safely by this application.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct InvalidAiOptions;

impl std::fmt::Display for InvalidAiOptions {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("AI requires 1..=10000 simulations and temperature 0..=2 in steps of 0.1")
    }
}
impl std::error::Error for InvalidAiOptions {}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Flight {
    pub generation: u64,
    pub side: Color,
    pub id: u64,
}

/// An owned request. Mutating its board cannot mutate the session.
#[derive(Debug, Clone)]
pub struct AiRequest {
    /// The actual visible board, including any imported puzzle root.
    pub board: Board,
    /// Validated settings captured when this flight was reserved.
    pub options: player::PlayerOptions,
    /// Monotonic session invalidation generation, not the save revision.
    pub generation: u64,
    /// The color whose turn was reserved.
    pub side: Color,
    pub(crate) flight: Flight,
}

impl AiRequest {
    /// Preserve the exact flight identity when returning a result.
    pub fn reply(&self, result: Result<AiPlayerDecision, String>) -> AiReply {
        AiReply {
            flight: self.flight,
            result,
        }
    }
}

/// A reply can retire only the request that produced it.
#[derive(Debug, Clone)]
pub struct AiReply {
    pub(crate) flight: Flight,
    pub(crate) result: Result<AiPlayerDecision, String>,
}

#[derive(Debug, Clone, Copy)]
struct Seat {
    owner: SeatOwner,
    options: AiOptions,
}

/// Bookkeeping only: the session supplies eligibility and commits decisions.
pub(crate) struct Controller {
    seats: [Seat; 2],
    pub running: bool,
    pub generation: u64,
    outstanding: Option<Flight>,
    next_id: u64,
}

fn index(side: Color) -> usize {
    match side {
        Color::Black => 0,
        Color::White => 1,
    }
}

impl Default for Controller {
    fn default() -> Self {
        Self {
            seats: [
                Seat {
                    owner: SeatOwner::Human,
                    options: AiOptions::default(),
                },
                Seat {
                    owner: SeatOwner::Ai,
                    options: AiOptions::default(),
                },
            ],
            running: true,
            generation: 0,
            outstanding: None,
            next_id: 0,
        }
    }
}

impl Controller {
    pub fn owner(&self, side: Color) -> SeatOwner {
        self.seats[index(side)].owner
    }
    pub fn options(&self, side: Color) -> AiOptions {
        self.seats[index(side)].options
    }
    pub fn set_owner(&mut self, side: Color, owner: SeatOwner) {
        self.seats[index(side)].owner = owner;
        self.invalidate();
    }
    pub fn set_options(&mut self, side: Color, options: AiOptions) -> Result<(), InvalidAiOptions> {
        options.validate()?;
        self.seats[index(side)].options = options;
        self.invalidate();
        Ok(())
    }
    pub fn invalidate(&mut self) {
        self.generation = self
            .generation
            .checked_add(1)
            .expect("session generation exhausted");
    }
    pub fn reserve(&mut self, board: Board) -> Option<AiRequest> {
        if self.outstanding.is_some() {
            return None;
        }
        self.next_id = self
            .next_id
            .checked_add(1)
            .expect("flight identities exhausted");
        let side = board.to_move();
        let flight = Flight {
            side,
            generation: self.generation,
            id: self.next_id,
        };
        self.outstanding = Some(flight);
        Some(AiRequest {
            board,
            options: self.options(side).player_options(),
            generation: self.generation,
            side,
            flight,
        })
    }
    pub fn retire(&mut self, flight: Flight) -> bool {
        if self.outstanding != Some(flight) {
            return false;
        }
        self.outstanding = None;
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::session::Session;
    use gomoku_core::{Color, Settings, point};

    fn play(request: &AiRequest, row: u8, col: u8) -> AiReply {
        request.reply(Ok(AiPlayerDecision {
            action: DecisionAction::Play(point(row, col).unwrap()),
            search: None,
            guard: None,
        }))
    }

    fn resign(request: &AiRequest) -> AiReply {
        request.reply(Ok(AiPlayerDecision {
            action: DecisionAction::Resign(player::ResignReason::VerifiedOpponentVcf {
                candidates: 5,
            }),
            search: None,
            guard: None,
        }))
    }

    fn white_turn() -> Session {
        let mut session = Session::new(&Settings::default());
        session.place(point(7, 7).unwrap());
        session.knock.take();
        session
    }

    #[test]
    fn a_valid_reply_commits_exactly_one_move_and_knock() {
        let mut session = white_turn();
        let mut request = session.next_ai_request().unwrap();
        let original = session.game().board_snapshot();
        request.board.play(point(0, 0).unwrap()).unwrap();
        assert_eq!(
            session.game().board_snapshot(),
            original,
            "snapshot is detached"
        );
        let reply = play(&request, 7, 8);
        assert!(session.apply_ai_reply(reply.clone()));
        assert_eq!(session.game().len(), 2);
        assert_eq!(session.game().to_move(), Color::Black);
        assert_eq!(session.knock.take(), Some(1));
        assert!(!session.apply_ai_reply(reply));
        assert!(session.knock.is_none());
        assert!(session.next_ai_request().is_none());
    }

    #[test]
    fn taking_a_seat_rejects_play_and_resign_without_retiring_early() {
        for concession in [false, true] {
            let mut session = white_turn();
            let request = session.next_ai_request().unwrap();
            let reply = if concession {
                resign(&request)
            } else {
                play(&request, 7, 8)
            };
            let board = session.game().clone();
            session.take_seat(Color::White);
            assert!(!session.apply_ai_reply(reply));
            assert_eq!(session.game(), &board);
            assert!(session.knock.is_none());
            assert!(session.next_ai_request().is_none());
            session.place(point(7, 8).unwrap());
            assert_eq!(session.game().len(), 2, "taking is immediate");
        }
    }

    #[test]
    fn take_release_same_board_waits_for_stale_flight_and_preserves_new_flight() {
        let mut session = white_turn();
        let old = session.next_ai_request().unwrap();
        session.take_seat(Color::White);
        session.release_seat(Color::White);
        assert!(session.generation() > old.generation);
        assert!(
            session.next_ai_request().is_none(),
            "stale still occupies slot"
        );
        let stale = resign(&old);
        assert!(!session.apply_ai_reply(stale.clone()));
        let newest = session.next_ai_request().unwrap();
        assert_eq!(newest.board, old.board);
        assert!(newest.generation > old.generation);
        assert!(!session.apply_ai_reply(stale));
        assert!(
            session.next_ai_request().is_none(),
            "duplicate cannot retire newer work"
        );
        assert!(session.apply_ai_reply(play(&newest, 7, 8)));
        assert!(!session.apply_ai_reply(resign(&old)));
        assert_eq!(
            session.knock,
            Some(1),
            "stale work cannot clear a real knock"
        );
    }

    #[test]
    fn settings_changes_coalesce_while_stale_work_occupies_the_slot() {
        let mut session = white_turn();
        let old = session.next_ai_request().unwrap();
        for simulations in [1, 10_000, 123] {
            session
                .set_ai_options(
                    Color::White,
                    AiOptions {
                        simulations,
                        temperature: 1.2,
                        ..AiOptions::default()
                    },
                )
                .unwrap();
            assert!(session.next_ai_request().is_none());
        }
        assert!(!session.apply_ai_reply(play(&old, 7, 8)));
        let latest = session.next_ai_request().unwrap();
        assert_eq!(latest.options.simulations, 123);
        assert_eq!(latest.options.temperature, 1.2);
        assert!(latest.generation > old.generation);
        session.take_seat(Color::White);
        session.release_seat(Color::White);
        assert_eq!(session.ai_options(Color::White).simulations, 123);
    }

    #[test]
    fn both_ai_schedule_one_turn_at_a_time_and_terminal_resignation_stops() {
        let mut session = Session::new(&Settings::default());
        session.release_seat(Color::Black);
        let black = session.next_ai_request().unwrap();
        assert_eq!(black.side, Color::Black);
        assert!(session.next_ai_request().is_none());
        assert!(session.apply_ai_reply(play(&black, 7, 7)));
        assert_eq!(session.knock.take(), Some(0));
        let path =
            std::env::temp_dir().join(format!("gomoku-ai-resign-{}.json", std::process::id()));
        session.save_to(&path);
        assert!(!session.changed());
        session.title_stale = false;
        let white = session.next_ai_request().unwrap();
        assert_eq!(white.side, Color::White);
        assert!(session.apply_ai_reply(resign(&white)));
        assert_eq!(session.game().len(), 1);
        assert_eq!(
            session.game().outcome(),
            gomoku_core::Outcome::Won {
                winner: Color::Black,
                method: gomoku_core::WinMethod::Resignation
            }
        );
        assert!(session.changed());
        assert!(session.title_stale);
        assert!(session.knock.is_none());
        assert!(session.next_ai_request().is_none());
        session.undo();
        assert_eq!(
            session.game().len(),
            1,
            "undo clears resignation before stones"
        );
        assert_eq!(session.game().outcome(), gomoku_core::Outcome::Ongoing);
        assert!(session.next_ai_request().is_none());
        session.resume_automation();
        assert!(session.next_ai_request().is_some());
        std::fs::remove_file(path).unwrap();
    }

    #[test]
    fn review_and_undo_pause_even_at_root_and_forward_does_not_resume() {
        for review in [
            Session::undo,
            Session::rewind,
            Session::forward,
            |s: &mut Session| s.seek(usize::MAX),
        ] {
            let mut session = Session::new(&Settings::default());
            session.release_seat(Color::Black);
            review(&mut session);
            assert!(!session.automation_running());
            assert!(session.next_ai_request().is_none());
            session.forward();
            session.seek(usize::MAX);
            assert!(session.next_ai_request().is_none());
            session.release_seat(Color::Black);
            assert!(session.next_ai_request().is_some());
        }
        let mut session = white_turn();
        let old = session.next_ai_request().unwrap();
        session.rewind();
        session.forward();
        assert!(!session.apply_ai_reply(play(&old, 7, 8)));
        assert!(session.next_ai_request().is_none());
        session.resume_automation();
        assert!(session.next_ai_request().is_some());
    }

    #[test]
    fn a_human_branch_resumes_but_ai_owned_clicks_and_truncation_do_not() {
        let mut session = white_turn();
        session.place(point(7, 8).unwrap());
        session.place_now(point(7, 8).unwrap());
        assert_eq!(session.game().len(), 1, "White is not human");
        session.rewind();
        session.place(point(8, 8).unwrap());
        assert!(session.dialog().is_some());
        session.release_seat(Color::Black);
        session.confirm_truncation();
        assert_eq!(session.game().moves(), &[point(7, 7).unwrap()]);
        session.take_seat(Color::Black);
        session.place(point(8, 8).unwrap());
        session.confirm_truncation();
        assert!(session.automation_running());
        assert_eq!(session.game().moves(), &[point(8, 8).unwrap()]);
        assert!(session.next_ai_request().is_some());
    }

    #[test]
    fn invalid_options_are_rejected_by_session_without_invalidation_or_replacement() {
        let mut session = white_turn();
        let generation = session.generation();
        for simulations in [0, 10_001, u32::MAX] {
            assert!(
                session
                    .set_ai_options(
                        Color::White,
                        AiOptions {
                            simulations,
                            ..AiOptions::default()
                        }
                    )
                    .is_err()
            );
        }
        for temperature in [
            f32::NAN,
            f32::INFINITY,
            f32::NEG_INFINITY,
            -0.1,
            2.1,
            0.05,
            0.11,
        ] {
            assert!(
                session
                    .set_ai_options(
                        Color::White,
                        AiOptions {
                            temperature,
                            ..AiOptions::default()
                        }
                    )
                    .is_err()
            );
        }
        assert_eq!(session.generation(), generation);
        assert_eq!(session.ai_options(Color::White), AiOptions::default());
        for tenths in 0..=20 {
            session
                .set_ai_options(
                    Color::White,
                    AiOptions {
                        simulations: 1,
                        temperature: tenths as f32 / 10.0,
                        ..AiOptions::default()
                    },
                )
                .unwrap();
        }
    }

    #[test]
    fn current_worker_errors_and_illegal_decisions_pause_visibly_without_retry() {
        for illegal in [false, true] {
            let mut session = white_turn();
            let request = session.next_ai_request().unwrap();
            let reply = if illegal {
                play(&request, 7, 7)
            } else {
                request.reply(Err("worker failed".into()))
            };
            assert!(!session.apply_ai_reply(reply));
            assert!(!session.automation_running());
            assert!(session.notice.as_deref().unwrap().starts_with("AI paused:"));
            assert!(session.next_ai_request().is_none());
            assert!(session.knock.is_none());
            assert_eq!(session.game().len(), 1);
        }
    }

    #[test]
    fn dialogs_and_deferred_actions_block_delivery_and_stale_errors_do_not_pause() {
        use crate::session::{After, Dialog};
        let mut session = white_turn();
        let old = session.next_ai_request().unwrap();
        session.set_dialog(Dialog::Message {
            title: "test".into(),
            body: "blocked".into(),
        });
        assert!(session.next_ai_request().is_none());
        assert!(!session.apply_ai_reply(old.reply(Err("obsolete error".into()))));
        assert!(session.automation_running());
        assert!(session.notice.is_none());
        session.dismiss_dialog();
        let current = session.next_ai_request().unwrap();
        session.defer_action(After::NewGame);
        assert!(!session.apply_ai_reply(play(&current, 7, 8)));
        assert!(session.next_ai_request().is_none());
        assert_eq!(session.take_action(), Some(After::NewGame));
        let current = session.next_ai_request().unwrap();
        session.defer_after_save(After::Quit);
        assert!(!session.apply_ai_reply(resign(&current)));
        assert!(session.next_ai_request().is_none());
    }

    #[test]
    fn new_retains_seats_and_options_running_but_open_is_paused() {
        let mut session = white_turn();
        session.release_seat(Color::Black);
        let options = AiOptions {
            simulations: 7,
            temperature: 2.0,
            resign: false,
            ..AiOptions::default()
        };
        session.set_ai_options(Color::Black, options).unwrap();
        let old = session.next_ai_request().unwrap();
        let path = std::env::temp_dir().join(format!("gomoku-ai-open-{}.json", std::process::id()));
        session.save_to(&path);
        session.undo();
        session.new_game();
        assert!(session.automation_running());
        assert_eq!(session.owner(Color::Black), SeatOwner::Ai);
        assert_eq!(session.ai_options(Color::Black), options);
        assert!(
            session.next_ai_request().is_none(),
            "new game cannot bypass stale work"
        );
        assert!(!session.apply_ai_reply(resign(&old)));
        let fresh = session.next_ai_request().unwrap();
        assert_eq!(fresh.side, Color::Black);
        assert_eq!(fresh.options.simulations, 7);
        assert!(!fresh.options.resign);
        session.open(&path);
        assert_eq!(session.game().len(), 1);
        assert!(!session.automation_running());
        assert!(!session.apply_ai_reply(play(&fresh, 0, 0)));
        assert!(session.next_ai_request().is_none());
        session.seek(usize::MAX);
        assert!(!session.automation_running());
        session.resume_automation();
        assert!(session.next_ai_request().is_some());
        std::fs::remove_file(path).unwrap();
    }

    #[test]
    fn puzzle_uses_actual_imported_board_and_retains_settings_under_human_seats() {
        let mut session = white_turn();
        let old = session.next_ai_request().unwrap();
        let options = AiOptions {
            simulations: 20,
            ..AiOptions::default()
        };
        session.set_ai_options(Color::White, options).unwrap();
        let puzzle = gomoku_core::Puzzle {
            name: "root".into(),
            black: vec![point(3, 3).unwrap()],
            white: vec![],
            to_move: Color::White,
            solution: None,
            depth: None,
        };
        session.load_puzzle(&puzzle).unwrap();
        assert_eq!(session.owner(Color::Black), SeatOwner::Human);
        assert_eq!(session.owner(Color::White), SeatOwner::Human);
        assert_eq!(session.ai_options(Color::White), options);
        assert!(!session.apply_ai_reply(resign(&old)));
        assert!(session.next_ai_request().is_none());
        session.release_seat(Color::White);
        let request = session.next_ai_request().unwrap();
        assert_eq!(request.board, session.game().board_snapshot());
        assert_eq!(
            request.board.stone_at(point(3, 3).unwrap()),
            Some(Color::Black)
        );
        assert_eq!(request.board.stone_at(point(7, 7).unwrap()), None);
        assert_eq!(request.options.simulations, 20);
    }

    #[test]
    fn human_owned_resigned_board_has_no_phantom_hover() {
        let mut session = white_turn();
        let request = session.next_ai_request().unwrap();
        assert!(session.apply_ai_reply(resign(&request)));
        session.take_seat(Color::White);
        session.pointer = session.camera.to_screen(session.viewport, [8.0, 8.0]);
        session.update_hover();
        assert!(session.hover.is_none());
        session.place(point(8, 8).unwrap());
        assert_eq!(session.game().len(), 1);
        session.undo();
        session.update_hover();
        assert_eq!(session.hover, Some(point(8, 8).unwrap()));
    }

    #[test]
    fn seat_and_option_changes_do_not_dirty_the_saved_game() {
        let mut session = Session::new(&Settings::default());
        session.release_seat(Color::Black);
        session.take_seat(Color::White);
        session
            .set_ai_options(
                Color::Black,
                AiOptions {
                    simulations: 500,
                    ..AiOptions::default()
                },
            )
            .unwrap();
        assert!(!session.changed());
    }

    #[test]
    fn native_modal_blocks_requests_and_invalidates_delayed_delivery_after_cancel() {
        let mut session = white_turn();
        let old = session.next_ai_request().unwrap();
        session.begin_native_dialog();
        assert!(session.next_ai_request().is_none());
        assert!(!session.apply_ai_reply(play(&old, 7, 8)));
        assert!(session.next_ai_request().is_none());
        session.end_native_dialog();
        let current = session.next_ai_request().unwrap();
        session.begin_native_dialog();
        session.end_native_dialog();
        assert!(!session.apply_ai_reply(resign(&current)));
        assert!(session.next_ai_request().is_some());
    }

    #[test]
    fn five_in_a_row_stops_automatic_requests() {
        let mut session = Session::new(&Settings::default());
        session.release_seat(Color::Black);
        for col in 0..5 {
            let black = session.next_ai_request().unwrap();
            assert!(session.apply_ai_reply(play(&black, 7, col)));
            if col < 4 {
                let white = session.next_ai_request().unwrap();
                assert!(session.apply_ai_reply(play(&white, 0, col)));
            }
        }
        assert!(matches!(
            session.game().outcome(),
            gomoku_core::Outcome::Won {
                method: gomoku_core::WinMethod::Five,
                ..
            }
        ));
        assert!(session.next_ai_request().is_none());
    }

    #[test]
    fn owner_decisions_with_optional_real_stat_types_are_accepted() {
        let mut session = white_turn();
        let request = session.next_ai_request().unwrap();
        let mv = point(7, 8).unwrap();
        let reply = request.reply(Ok(AiPlayerDecision {
            action: DecisionAction::Play(mv),
            search: Some(player::SearchStats {
                side_to_move: Color::White,
                visits: vec![(mv, 1)],
                prior: vec![(mv, 1.0)],
                value: 0.0,
            }),
            guard: Some(player::GuardDiagnostics {
                proposal: mv,
                refuted: vec![],
                checked: 1,
                outcome: player::GuardOutcome::Unrefuted,
            }),
        }));
        assert!(session.apply_ai_reply(reply));
        assert_eq!(session.game().stone_at(mv), Some(Color::White));
    }

    #[test]
    fn default_white_automatically_requests_one_owned_snapshot() {
        let mut session = Session::new(&Settings::default());
        assert_eq!(session.owner(Color::Black), SeatOwner::Human);
        assert_eq!(session.owner(Color::White), SeatOwner::Ai);
        assert!(session.next_ai_request().is_none());
        session.place(point(7, 7).unwrap());
        let request = session.next_ai_request().expect("White is automatic");
        assert_eq!(request.side, Color::White);
        assert_eq!(request.board, session.game().board_snapshot());
        assert_eq!(request.options.simulations, 400);
        assert_eq!(request.options.temperature, 0.0);
        assert_eq!(request.options.c_puct, 1.5);
        assert_eq!(request.options.noise_epsilon, 0.0);
        assert_eq!(request.options.own, player::OwnTssConfig::baseline());
        assert_eq!(request.options.guard, player::GuardConfig::baseline());
        assert!(request.options.resign);
        assert!(session.next_ai_request().is_none());
    }
}
