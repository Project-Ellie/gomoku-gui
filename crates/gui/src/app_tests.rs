//! Headless application tests: real egui requests and app dispatch, fake only
//! the native save picker, event-loop exit, and filesystem destinations.
//!
//! `real_acceptance` adds opt-in release tests using the genuine pinned player.
use super::*;
use std::path::{Path, PathBuf};

struct Files(PathBuf);
impl Files {
    fn new(name: &str) -> Self {
        let path = std::env::temp_dir().join(format!("gomoku-app-{name}-{}", std::process::id()));
        std::fs::create_dir(&path).unwrap();
        Self(path)
    }
    fn path(&self, name: &str) -> PathBuf {
        self.0.join(name)
    }
    fn write_settings(&self, app: &mut App) {
        app.write_settings(
            Ok(self.path("settings.toml")),
            Ok(self.path("recovery.json")),
            Ok(self.path("recovery-state.json")),
        );
    }
}
impl Drop for Files {
    fn drop(&mut self) {
        std::fs::remove_dir_all(&self.0).unwrap();
    }
}

#[test]
fn idle_worker_completion_requests_redraw_without_pointer_input() {
    use std::sync::mpsc;
    let mut app = app();
    let (tx, rx) = mpsc::channel();
    app.worker = Some(Ok(crate::ai_worker::Worker::spawn(
        || {
            Ok(|_: &crate::ai::AiRequest| {
                Ok(crate::ai::AiPlayerDecision {
                    action: crate::ai::DecisionAction::Play(gomoku_core::point(7, 8).unwrap()),
                    search: None,
                    guard: None,
                })
            })
        },
        move |reply| tx.send(reply).is_ok(),
    )
    .unwrap()));
    app.session.release_seat(gomoku_core::Color::White);
    app.session.place(gomoku_core::point(7, 7).unwrap());
    app.dirty = false;
    // A future repaint deadline must not defer scheduling.
    app.wake_at = Some(std::time::Instant::now() + std::time::Duration::from_secs(60));
    assert!(matches!(app.prepare_wait(), ControlFlow::WaitUntil(_)));
    assert!(app.session.next_ai_request().is_none());
    app.redraw_pending = true; // A completion must not churn an existing redraw.
    app.handle_app_event(AppEvent::AiCompleted(
        rx.recv_timeout(std::time::Duration::from_secs(30)).unwrap(),
    ));
    assert_eq!(app.session.game().len(), 2);
    assert!(app.dirty);
    assert!(app.redraw_pending);
    assert!(app.session.knock.is_some());
}

#[test]
fn closing_ignores_already_queued_completion_and_never_schedules() {
    let mut app = app();
    app.session.release_seat(gomoku_core::Color::White);
    app.session.place(gomoku_core::point(7, 7).unwrap());
    app.session.knock.take();
    let request = app.session.next_ai_request().unwrap();
    let reply = request.reply(Ok(crate::ai::AiPlayerDecision {
        action: crate::ai::DecisionAction::Play(gomoku_core::point(7, 8).unwrap()),
        search: None,
        guard: None,
    }));
    app.close_worker();
    app.dirty = false;
    app.handle_app_event(AppEvent::AiCompleted(reply));
    assert_eq!(app.prepare_wait(), ControlFlow::Wait);
    assert_eq!(app.session.game().len(), 1);
    assert!(app.session.knock.is_none());
    assert!(!app.dirty);
}

#[test]
fn failed_thread_start_retires_reservation_and_pauses_without_retry() {
    let mut app = app();
    app.session.release_seat(gomoku_core::Color::White);
    app.session.place(gomoku_core::point(7, 7).unwrap());
    app.worker = Some(Err("thread unavailable".into()));
    app.prepare_wait();
    assert!(!app.session.automation_running());
    assert!(
        app.session
            .notice
            .as_ref()
            .unwrap()
            .contains("thread unavailable")
    );
    app.prepare_wait();
    assert!(app.session.next_ai_request().is_none());
    assert_eq!(app.session.game().len(), 1);
}

// Hold completion publication open to model preemption before worker cleanup.
// The replacement must fail even though the failed worker has not exited yet.
fn stale_terminal_failure_rejects_replacement(initialization: bool, panics: bool) {
    use std::sync::mpsc;
    let mut app = app();
    let (tx, rx) = mpsc::channel();
    let (release, latch) = mpsc::channel();
    app.worker = Some(Ok(crate::ai_worker::Worker::spawn(
        move || {
            if initialization {
                assert!(!panics, "initialization panic");
                return Err("initialization failed".into());
            }
            Ok(move |_: &crate::ai::AiRequest| {
                assert!(!panics, "decision panic");
                Err("decision failed".into())
            })
        },
        move |reply| {
            let delivered = tx.send(reply).is_ok();
            // Test-only blocking models preemption just after publication.
            // Sender drop also releases this latch if an assertion unwinds.
            let _ = latch.recv();
            delivered
        },
    )
    .unwrap()));
    app.session.release_seat(gomoku_core::Color::White);
    app.session.place(gomoku_core::point(7, 7).unwrap());
    app.prepare_wait();
    let reply = rx.recv_timeout(std::time::Duration::from_secs(30)).unwrap();
    assert!(reply.result.is_err());
    app.session
        .set_ai_options(
            gomoku_core::Color::White,
            crate::ai::AiOptions {
                simulations: 7,
                ..Default::default()
            },
        )
        .unwrap();
    assert!(app.session.next_ai_request().is_none());
    app.handle_app_event(AppEvent::AiCompleted(reply));
    assert!(app.session.automation_running(), "stale failure is ignored");
    assert!(app.session.notice.is_none());
    app.dirty = false;

    // Reconcile while the completion sink is still latched, not after exit.
    app.prepare_wait();
    assert!(
        !app.session.automation_running(),
        "terminal worker must reject rather than abandon the replacement"
    );
    assert!(
        app.session
            .notice
            .as_ref()
            .unwrap()
            .starts_with("AI paused: worker disconnected")
    );
    assert!(
        app.dirty,
        "submission failure must be visible without input"
    );
    assert_eq!(app.session.game().len(), 1);
    assert!(app.session.next_ai_request().is_none());
    app.session.resume_automation();
    assert!(
        app.session.next_ai_request().is_some(),
        "rejected replacement's reservation must have been retired"
    );
    release.send(()).unwrap();
}

#[test]
fn stale_factory_error_rejects_replacement_before_worker_exit() {
    stale_terminal_failure_rejects_replacement(true, false);
}

#[test]
fn stale_factory_panic_rejects_replacement_before_worker_exit() {
    stale_terminal_failure_rejects_replacement(true, true);
}

#[test]
fn stale_decision_error_rejects_replacement_before_worker_exit() {
    stale_terminal_failure_rejects_replacement(false, false);
}

#[test]
fn stale_decision_panic_rejects_replacement_before_worker_exit() {
    stale_terminal_failure_rejects_replacement(false, true);
}

#[test]
fn dirty_interface_redraw_is_not_deferred_by_a_future_wake() {
    let mut app = app();
    app.dirty = true;
    app.wake_at = Some(std::time::Instant::now() + std::time::Duration::from_secs(60));

    assert_eq!(app.prepare_wait(), ControlFlow::Wait);
    assert!(app.dirty, "the redraw request remains outstanding");
}

#[test]
fn headless_construction_does_not_start_worker_or_reserve_a_flight() {
    let mut app = App::new(Settings::default(), gomoku_core::Game::new(), None, None);
    app.session.place(gomoku_core::point(7, 7).unwrap());
    app.prepare_wait();
    assert!(app.worker.is_none());
    assert!(app.session.next_ai_request().is_some());
}

fn app() -> App {
    let mut app = App::new(Settings::default(), gomoku_core::Game::new(), None, None);
    app.session.take_seat(gomoku_core::Color::White);
    app
}

fn frame(
    app: &mut App,
    context: &egui::Context,
    events: Vec<egui::Event>,
    pick: &mut impl FnMut(Option<&Path>) -> Option<PathBuf>,
    exit: &mut impl FnMut(),
) -> egui::FullOutput {
    let mut requests = Requests::default();
    let output = context.run_ui(
        egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(
                egui::Pos2::ZERO,
                egui::vec2(1200.0, 900.0),
            )),
            events,
            ..Default::default()
        },
        |ui| requests = ui::draw(ui, &mut app.session),
    );
    app.run_requests(requests, pick, &mut |_| None, exit);
    output
}

fn save_first(
    app: &mut App,
    pick: &mut impl FnMut(Option<&Path>) -> Option<PathBuf>,
    exit: &mut impl FnMut(),
) -> egui::Context {
    click_button(app, pick, exit, "Save first")
}

fn click_button(
    app: &mut App,
    pick: &mut impl FnMut(Option<&Path>) -> Option<PathBuf>,
    exit: &mut impl FnMut(),
    label: &str,
) -> egui::Context {
    let context = egui::Context::default();
    frame(app, &context, vec![], pick, exit)
        .textures_delta
        .clear();
    let mut output = frame(app, &context, vec![], pick, exit);
    let position = output
        .shapes
        .iter()
        .find_map(|shape| match &shape.shape {
            egui::epaint::Shape::Text(text) if text.galley.job.text == label => {
                Some(text.pos + text.galley.rect.center().to_vec2())
            }
            _ => None,
        })
        .expect("the requested confirmation button is visible");
    output.textures_delta.clear();
    for pressed in [true, false] {
        frame(
            app,
            &context,
            vec![
                egui::Event::PointerMoved(position),
                egui::Event::PointerButton {
                    pos: position,
                    button: egui::PointerButton::Primary,
                    pressed,
                    modifiers: egui::Modifiers::NONE,
                },
            ],
            pick,
            exit,
        )
        .textures_delta
        .clear();
    }
    context
}

#[test]
fn save_first_existing_and_untitled_execute_pending_quit_exactly_once() {
    let files = Files::new("save-success");
    for existing in [false, true] {
        let mut app = app();
        let path = files.path(if existing {
            "existing.json"
        } else {
            "untitled.json"
        });
        app.session.place(gomoku_core::point(7, 7).unwrap());
        if existing {
            app.session.save_to(&path);
            app.session.place(gomoku_core::point(8, 8).unwrap());
        }
        let expected = app.session.game().clone();
        let mut exits = 0;
        let mut exit = || exits += 1;
        let mut picks = 0;
        let mut pick = |_: Option<&Path>| {
            assert!(!existing, "an existing destination must bypass the picker");
            picks += 1;
            Some(path.clone())
        };
        app.run_requests(
            Requests {
                quit: true,
                ..Default::default()
            },
            &mut pick,
            &mut |_| None,
            &mut exit,
        );
        let context = save_first(&mut app, &mut pick, &mut exit);
        assert!(app.closing);
        assert!(!app.session.changed());
        assert_eq!(gomoku_core::load(&path).unwrap(), expected);
        // Settling frames and an independent later save must not replay Quit.
        frame(&mut app, &context, vec![], &mut pick, &mut exit)
            .textures_delta
            .clear();
        app.run_requests(
            Requests {
                save: true,
                ..Default::default()
            },
            &mut pick,
            &mut |_| None,
            &mut exit,
        );
        assert_eq!(exits, 1);
        assert_eq!(picks, usize::from(!existing));
    }
}

#[test]
fn save_first_existing_and_untitled_start_new_game_without_replaying_it() {
    let files = Files::new("new-game-success");
    for existing in [false, true] {
        let mut app = app();
        let path = files.path(if existing {
            "existing.json"
        } else {
            "untitled.json"
        });
        if existing {
            app.session.save_to(&path);
        }
        app.session.place(gomoku_core::point(7, 7).unwrap());
        let expected = app.session.game().clone();
        let mut pick = |_: Option<&Path>| Some(path.clone());
        let mut exit = || panic!("New Game must not exit");
        app.run_requests(
            Requests {
                new_game: true,
                ..Default::default()
            },
            &mut pick,
            &mut |_| None,
            &mut exit,
        );
        let context = save_first(&mut app, &mut pick, &mut exit);
        assert!(app.session.is_fresh_game());
        assert!(!app.session.changed());
        assert_eq!(gomoku_core::load(&path).unwrap(), expected);
        app.session.place(gomoku_core::point(3, 3).unwrap());
        let sentinel = app.session.game().clone();
        frame(&mut app, &context, vec![], &mut pick, &mut exit)
            .textures_delta
            .clear();
        app.run_requests(
            Requests {
                save: true,
                ..Default::default()
            },
            &mut pick,
            &mut |_| None,
            &mut exit,
        );
        assert_eq!(
            app.session.game(),
            &sentinel,
            "later saves must not replay New Game"
        );
        assert_eq!(gomoku_core::load(&path).unwrap(), sentinel);
    }
}

#[test]
fn save_first_cancel_preserves_game_and_never_replays_pending_action() {
    let files = Files::new("save-cancel");
    for requests in [
        Requests {
            new_game: true,
            ..Default::default()
        },
        Requests {
            quit: true,
            ..Default::default()
        },
    ] {
        let mut app = app();
        app.session.place(gomoku_core::point(7, 7).unwrap());
        let expected = app.session.game().clone();
        let mut exits = 0;
        let mut exit = || exits += 1;
        app.run_requests(requests, &mut |_| None, &mut |_| None, &mut exit);
        let context = save_first(&mut app, &mut |_| None, &mut exit);
        assert_eq!(app.session.game(), &expected);
        assert!(app.session.changed());
        assert!(app.session.dialog().is_none());
        assert!(!app.closing);
        // A later successful, unrelated save must not resurrect the cancelled action.
        frame(&mut app, &context, vec![], &mut |_| None, &mut exit)
            .textures_delta
            .clear();
        app.run_requests(
            Requests {
                save: true,
                ..Default::default()
            },
            &mut |_| Some(files.path("later.json")),
            &mut |_| None,
            &mut exit,
        );
        assert_eq!(exits, 0);
        assert_eq!(app.session.game(), &expected);
        assert!(!app.session.changed());
        assert!(!app.closing);
    }
}

#[test]
fn save_first_existing_and_untitled_failure_keeps_error_and_never_continues() {
    let files = Files::new("save-failure");
    for existing in [false, true] {
        for quit in [false, true] {
            let mut app = app();
            let parent = files.path("destination");
            let path = parent.join("game.json");
            if existing {
                std::fs::create_dir(&parent).unwrap();
                app.session.save_to(&path);
                std::fs::remove_file(&path).unwrap();
                std::fs::remove_dir(&parent).unwrap();
            }
            std::fs::write(&parent, "not a directory").unwrap();
            app.session.place(gomoku_core::point(7, 7).unwrap());
            let expected = app.session.game().clone();
            let mut exits = 0;
            let mut exit = || exits += 1;
            let mut pick = |_: Option<&Path>| {
                assert!(
                    !existing,
                    "existing path failure must not fall back to a picker"
                );
                Some(path.clone())
            };
            app.run_requests(
                Requests {
                    quit,
                    new_game: !quit,
                    ..Default::default()
                },
                &mut pick,
                &mut |_| None,
                &mut exit,
            );
            let context = save_first(&mut app, &mut pick, &mut exit);
            let error = app.session.dialog().cloned();
            assert!(
                matches!(&error, Some(Dialog::Message { title, .. }) if title == "The game could not be saved")
            );
            assert_eq!(app.session.game(), &expected);
            assert!(app.session.changed());
            assert!(!app.closing);
            for _ in 0..2 {
                frame(&mut app, &context, vec![], &mut pick, &mut exit)
                    .textures_delta
                    .clear();
                assert_eq!(
                    app.session.dialog(),
                    error.as_ref(),
                    "settling frames must retain the failure message"
                );
            }
            app.session.dismiss_dialog();
            std::fs::remove_file(&parent).unwrap();
            std::fs::create_dir(&parent).unwrap();
            // Retry succeeds, but the failed transaction's continuation is gone.
            app.run_requests(
                Requests {
                    save: true,
                    ..Default::default()
                },
                &mut pick,
                &mut |_| None,
                &mut exit,
            );
            assert_eq!(exits, 0);
            assert_eq!(app.session.game(), &expected);
            assert!(!app.session.changed());
            assert!(!app.closing);
            std::fs::remove_file(&path).unwrap();
            std::fs::remove_dir(&parent).unwrap();
        }
    }
}

#[test]
fn explicit_save_as_uses_new_destination_and_completes_pending_quit_once() {
    let files = Files::new("save-as");
    let mut app = app();
    let original = files.path("original.json");
    let destination = files.path("save-as.json");
    app.session.save_to(&original);
    let original_game = gomoku_core::load(&original).unwrap();
    app.session.place(gomoku_core::point(7, 7).unwrap());
    let expected = app.session.game().clone();
    app.session.defer_after_save(After::Quit);
    let mut exits = 0;
    let mut picks = 0;
    let mut exit = || exits += 1;
    let mut pick = |directory: Option<&Path>| {
        assert_eq!(directory, Some(files.0.as_path()));
        picks += 1;
        Some(destination.clone())
    };
    app.run_requests(
        Requests {
            save_as: true,
            ..Default::default()
        },
        &mut pick,
        &mut |_| None,
        &mut exit,
    );
    assert!(app.closing);
    assert!(!app.session.changed());
    assert_eq!(gomoku_core::load(&original).unwrap(), original_game);
    assert_eq!(gomoku_core::load(&destination).unwrap(), expected);
    app.run_requests(Requests::default(), &mut pick, &mut |_| None, &mut exit);
    app.run_requests(
        Requests {
            save: true,
            ..Default::default()
        },
        &mut pick,
        &mut |_| None,
        &mut exit,
    );
    assert_eq!(exits, 1);
    assert_eq!(picks, 1);
}

#[test]
fn zero_move_resigned_exit_writes_recovery_but_pristine_exit_discards_it() {
    let files = Files::new("exit-recovery");
    let mut resigned = app();
    resigned.session.release_seat(gomoku_core::Color::Black);
    let request = resigned.session.next_ai_request().unwrap();
    assert!(
        resigned
            .session
            .apply_ai_reply(request.reply(Ok(player::Decision {
                action: player::DecisionAction::Resign(player::ResignReason::VerifiedOpponentVcf {
                    candidates: 1,
                }),
                search: None,
                guard: None,
            })))
    );
    let expected = resigned.session.game().clone();
    assert!(expected.is_empty());
    assert!(resigned.session.changed());
    let mut exits = 0;
    let mut exit = || exits += 1;
    let mut no_picker = |_: Option<&Path>| panic!("exit does not need a file picker");
    resigned.run_requests(
        Requests {
            quit: true,
            ..Default::default()
        },
        &mut no_picker,
        &mut |_| None,
        &mut exit,
    );
    click_button(
        &mut resigned,
        &mut no_picker,
        &mut exit,
        "Discard the changes",
    );
    assert!(resigned.closing);
    files.write_settings(&mut resigned);
    let recovery = files.path("recovery.json");
    let state = files.path("recovery-state.json");
    assert_eq!(gomoku_core::load(&recovery).unwrap(), expected);
    assert!(state.exists());
    assert!(files.path("settings.toml").exists());
    assert_eq!(Session::autosave_to_offer_from(&recovery, None), Some(None));
    let mut reader = Session::new(&Settings::default());
    reader.resume_autosave_from(&recovery, None);
    assert_eq!(reader.game(), &expected);

    // A clean, saved zero-move resignation must not be mistaken for a pristine
    // empty sitting and delete the existing recovery files on exit either.
    let recovery_bytes = std::fs::read(&recovery).unwrap();
    let state_bytes = std::fs::read(&state).unwrap();
    let mut saved = app();
    saved.session.open(&recovery);
    assert!(!saved.session.changed());
    saved.run_requests(
        Requests {
            quit: true,
            ..Default::default()
        },
        &mut no_picker,
        &mut |_| None,
        &mut exit,
    );
    files.write_settings(&mut saved);
    assert_eq!(std::fs::read(&recovery).unwrap(), recovery_bytes);
    assert_eq!(std::fs::read(&state).unwrap(), state_bytes);

    let mut pristine = app();
    assert!(!pristine.session.changed());
    pristine.run_requests(
        Requests {
            quit: true,
            ..Default::default()
        },
        &mut no_picker,
        &mut |_| None,
        &mut exit,
    );
    files.write_settings(&mut pristine);
    assert!(!recovery.exists());
    assert!(!state.exists());
    assert_eq!(exits, 3);
}

/// A corpus directory holding one tiny psq game record.
fn psq_corpus(files: &Files) -> PathBuf {
    let corpus = files.path("corpus");
    std::fs::create_dir(&corpus).unwrap();
    std::fs::write(
        corpus.join("tiny.psq"),
        "Piskvorky 15x15, 0:0, 0\n8,8,0\n7,7,0\n9,9,0\n6,6,0\n10,7,0\n\
         BlackPlayer\nWhitePlayer\n-1\n1,Freestyle15_2\n",
    )
    .unwrap();
    corpus
}

#[test]
fn explorer_request_reaches_session() {
    let files = Files::new("explorer-request");
    let corpus = psq_corpus(&files);
    let mut app = app();
    let requests = ui::Requests {
        open_explorer: true,
        ..Default::default()
    };
    let mut pick_folder = |_: Option<&Path>| Some(corpus.clone());
    let mut exits = 0;
    app.run_requests(requests, &mut |_| None, &mut pick_folder, &mut || {
        exits += 1
    });
    assert!(app.session.explorer().is_some());
    assert_eq!(app.session.explorer().unwrap().records.len(), 1);
    assert_eq!(exits, 0);
}

#[test]
fn explorer_rejects_a_non_corpus_folder_without_changing_state() {
    let files = Files::new("explorer-non-corpus");
    let folder = files.path("not-a-corpus");
    std::fs::create_dir(&folder).unwrap();
    std::fs::write(folder.join("notes.txt"), "not a game record").unwrap();
    let previous = files.path("previous-corpus");
    let mut app = app();
    app.session.last_corpus_directory = Some(previous.clone());

    app.run_requests(
        ui::Requests {
            open_explorer: true,
            ..Default::default()
        },
        &mut |_| None,
        &mut |_| Some(folder.clone()),
        &mut || {},
    );

    assert!(app.session.explorer().is_none());
    assert!(matches!(
        app.session.dialog(),
        Some(Dialog::Message { title, body })
            if title == "This folder doesn't look like a Gomoku corpus"
                && body.contains("no recognized record files")
    ));
    assert_eq!(app.session.last_corpus_directory, Some(previous));
}

#[test]
fn explorer_picker_remembers_the_last_successful_corpus() {
    let files = Files::new("explorer-remembered");
    let corpus = psq_corpus(&files);
    let fallback = files.path("last-game-directory");
    let mut app = app();
    app.session.last_directory = Some(fallback.clone());
    let requests = ui::Requests {
        open_explorer: true,
        ..Default::default()
    };

    app.run_requests(
        requests,
        &mut |_| None,
        &mut |directory| {
            assert_eq!(directory, Some(fallback.as_path()));
            Some(corpus.clone())
        },
        &mut || {},
    );
    assert_eq!(app.session.last_corpus_directory, Some(corpus.clone()));

    app.run_requests(
        ui::Requests {
            open_explorer: true,
            ..Default::default()
        },
        &mut |_| None,
        &mut |directory| {
            assert_eq!(directory, Some(corpus.as_path()));
            None
        },
        &mut || {},
    );
    assert_eq!(app.session.last_corpus_directory, Some(corpus));
}

#[test]
fn explorer_selection_loads_the_record_onto_the_board() {
    let files = Files::new("explorer-select");
    let corpus = psq_corpus(&files);
    let mut app = app();
    let requests = ui::Requests {
        open_explorer: true,
        ..Default::default()
    };
    let mut pick_folder = |_: Option<&Path>| Some(corpus.clone());
    app.run_requests(requests, &mut |_| None, &mut pick_folder, &mut || {});
    app.run_requests(
        ui::Requests {
            explorer_select: Some(0),
            ..Default::default()
        },
        &mut |_| None,
        &mut |_| None,
        &mut || {},
    );
    assert_eq!(app.session.game().len(), 5);
    assert!(
        !app.session.changed(),
        "a loaded record has no unsaved changes"
    );
    assert_eq!(app.session.explorer().unwrap().selected, Some(0));
}

#[test]
fn explorer_arrows_bypass_game_seek_and_moves_hand_over_restores_right_arrow() {
    let mut app = app();
    app.session.place(gomoku_core::point(7, 7).unwrap());
    app.session.place(gomoku_core::point(8, 8).unwrap());
    app.session.seek(0);
    app.session
        .open_explorer_corpus(vec![gomoku_core::IndexEntry {
            index: 0,
            kind: gomoku_core::RecordKind::Game,
            name: "tiny.psq".into(),
            path: PathBuf::from("tiny.psq"),
        }]);
    let reviewing_start = app.session.game().clone();

    app.handle_key(&Key::Named(NamedKey::ArrowDown), &mut || {});
    assert_eq!(app.session.game(), &reviewing_start);
    app.handle_key(&Key::Named(NamedKey::ArrowUp), &mut || {});
    assert_eq!(app.session.game(), &reviewing_start);

    app.session.hand_over_explorer_to_moves();
    app.handle_key(&Key::Named(NamedKey::ArrowRight), &mut || {});
    assert_eq!(app.session.game().cursor(), 1);
}

/// Slow acceptance through the actual App scheduler and owned worker sink.
/// Run: cargo test --release -p gomoku-gui --locked app::tests::real_acceptance \
///     -- --ignored --nocapture --test-threads=1
mod real_acceptance {
    use super::*;
    use crate::ai::{AiReply, AiRequest, DecisionAction};
    use gomoku_core::{Color, Game, Outcome, WinMethod, point};
    use rand::SeedableRng;
    use std::sync::mpsc::{self, Receiver, Sender};
    use std::time::{Duration, Instant};

    const REPLY_DEADLINE: Duration = Duration::from_secs(60);
    const GAME_DEADLINE: Duration = Duration::from_secs(600);

    struct EvaluationLatch {
        entered: Sender<()>,
        release: Receiver<()>,
    }

    /// Delegates every evaluation unchanged. The one-shot handshake holds an
    /// actual MCTS evaluation open, not a fabricated decision or completion.
    struct Evaluator {
        baseline: baseline::BaselineEvaluator,
        latch: Option<EvaluationLatch>,
    }

    impl mcts::Evaluator for Evaluator {
        fn evaluate(&mut self, request: mcts::EvalRequest) -> mcts::EvalResult {
            let result = self.baseline.evaluate(request);
            if let Some(latch) = self.latch.take() {
                latch.entered.send(()).unwrap();
                latch
                    .release
                    .recv_timeout(REPLY_DEADLINE)
                    .expect("release the genuine evaluation before its deadline");
            }
            result
        }
    }

    #[allow(
        clippy::assertions_on_constants,
        reason = "ignored tests must compile in debug but execute only in release"
    )]
    fn real_app(seed: u64, game: Game, latch: Option<EvaluationLatch>) -> (App, Receiver<AiReply>) {
        assert!(!cfg!(debug_assertions), "run acceptance with --release");
        let mut app = App::new(Settings::default(), game, None, None);
        let (tx, replies) = mpsc::channel();
        let origin = std::thread::current().id();
        app.worker = Some(Ok(Worker::spawn(
            move || {
                assert_ne!(origin, std::thread::current().id());
                // All non-Send evaluator/player/RNG state originates on worker.
                let evaluator = Evaluator {
                    baseline: baseline::BaselineEvaluator::new(baseline::Device::flex()),
                    latch,
                };
                let mut player = player::AiPlayer::new(Box::new(evaluator));
                let mut rng = rand::rngs::StdRng::seed_from_u64(seed);
                Ok(move |request: &AiRequest| {
                    assert_eq!(request.options.simulations, 400);
                    assert_eq!(request.options.temperature, 0.0);
                    assert_eq!(request.options.noise_epsilon, 0.0);
                    assert_eq!(request.options.own, player::OwnTssConfig::baseline());
                    assert_eq!(request.options.guard, player::GuardConfig::baseline());
                    assert!(request.options.resign);
                    player
                        .decide(&request.board, &request.options, &mut rng)
                        .map_err(|error| error.to_string())
                })
            },
            move |reply| tx.send(reply).is_ok(),
        )
        .unwrap()));
        (app, replies)
    }

    fn receive<T>(receiver: &Receiver<T>) -> T {
        receiver
            .recv_timeout(REPLY_DEADLINE)
            .expect("worker deadline")
    }

    fn check_search(reply: &AiReply, side: Color) -> bool {
        let decision = reply.result.as_ref().expect("genuine player succeeded");
        if let Some(search) = &decision.search {
            assert_eq!(search.side_to_move, side);
            assert_eq!(search.visits.iter().map(|(_, n)| n).sum::<u32>(), 400);
            true
        } else {
            assert!(matches!(decision.action, DecisionAction::Play(_)));
            assert!(matches!(
                decision.guard.as_ref().unwrap().outcome,
                player::GuardOutcome::ProvenWin
            ));
            false
        }
    }

    /// No Step, direct Worker::submit, or Session::apply_ai_reply shortcuts.
    fn automatic_turn(app: &mut App, replies: &Receiver<AiReply>) -> bool {
        let before = app.session.game().clone();
        assert_eq!(before.outcome(), Outcome::Ongoing);
        app.session.knock.take();
        app.dirty = false;
        app.prepare_wait();
        assert!(app.session.next_ai_request().is_none(), "one flight only");
        assert_eq!(app.session.game(), &before, "search owns a detached board");
        let reply = receive(replies);
        let searched = check_search(&reply, before.to_move());
        let mut expected = before;
        let knock = match reply.result.as_ref().unwrap().action {
            DecisionAction::Play(mv) => {
                assert!(expected.board_snapshot().is_legal(mv));
                expected.play(mv).expect("every actual move is legal");
                Some(crate::session::neighbours(&expected, mv))
            }
            DecisionAction::Resign(_) => {
                expected.resign().unwrap();
                None
            }
        };
        app.handle_app_event(AppEvent::AiCompleted(reply));
        assert_eq!(app.session.game(), &expected, "one exact action committed");
        assert_eq!(app.session.knock.take(), knock);
        assert!(app.dirty, "completion wakes the App without pointer input");
        assert!(app.session.notice.is_none());
        searched
    }

    fn stopped(app: &mut App, replies: &Receiver<AiReply>) {
        assert_ne!(app.session.game().outcome(), Outcome::Ongoing);
        let terminal = app.session.game().clone();
        for _ in 0..3 {
            app.prepare_wait();
            assert!(app.session.next_ai_request().is_none());
            assert_eq!(app.session.game(), &terminal);
            assert!(matches!(replies.try_recv(), Err(mpsc::TryRecvError::Empty)));
        }
    }

    fn roundtrip(app: &mut App, files: &Files) -> std::path::PathBuf {
        let expected = app.session.game().clone();
        assert!(
            !expected.is_from_position(),
            "a playable record, not a puzzle"
        );
        let path = files.path("terminal.json");
        app.session.save_to(&path);
        assert!(!app.session.changed());
        assert_eq!(gomoku_core::load(&path).unwrap(), expected);
        let json: serde_json::Value =
            serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
        assert_eq!(json["version"], 2);
        // Independently replay the entire history, rejecting any illegal move.
        let mut replay = Game::started_at(expected.meta().created);
        for &mv in expected.moves() {
            replay.play(mv).expect("legal full history replay");
        }
        if matches!(
            expected.outcome(),
            Outcome::Won {
                method: WinMethod::Resignation,
                ..
            }
        ) {
            replay.resign().unwrap();
        }
        assert_eq!(replay, expected);
        path
    }

    #[test]
    #[ignore = "release acceptance: full seeded Flex default-400 game"]
    fn both_ai_default_400_finish_automatically_and_roundtrip() {
        let files = Files::new("real-both-ai");
        let start = Instant::now();
        let (mut app, replies) = real_app(0x6006_1a00, Game::new(), None);
        app.session.release_seat(Color::Black);
        let mut decisions = 0;
        let mut searches = 0;
        while app.session.game().outcome() == Outcome::Ongoing {
            assert!(start.elapsed() < GAME_DEADLINE, "whole-game deadline");
            assert!(decisions < 226, "225 stones plus optional resignation");
            searches += usize::from(automatic_turn(&mut app, &replies));
            decisions += 1;
            assert!(app.session.game().len() <= 225);
        }
        assert!(searches > 0, "a genuine 400-simulation search ran");
        stopped(&mut app, &replies);
        roundtrip(&mut app, &files);
        eprintln!(
            "both-AI: elapsed={:?}, plies={}, decisions={decisions}, searches={searches}, outcome={:?}",
            start.elapsed(),
            app.session.game().len(),
            app.session.game().outcome()
        );
        app.close_worker();
    }

    #[test]
    #[ignore = "release acceptance: scripted human versus real Flex default-400"]
    fn scripted_human_exchange_uses_session_and_app_dispatch() {
        let start = Instant::now();
        let (mut app, replies) = real_app(0x6006_1a01, Game::new(), None);
        // Widely separated human moves avoid manufacturing a tactical puzzle.
        for (turn, (row, col)) in [(7, 7), (2, 2), (12, 12), (2, 12)].into_iter().enumerate() {
            let mv = point(row, col).unwrap();
            assert_eq!(app.session.game().to_move(), Color::Black);
            assert!(app.session.game().board_snapshot().is_legal(mv));
            app.session.place(mv);
            assert_eq!(app.session.game().len(), turn * 2 + 1);
            automatic_turn(&mut app, &replies);
            assert_eq!(app.session.game().len(), (turn + 1) * 2);
            assert_eq!(app.session.game().outcome(), Outcome::Ongoing);
            app.prepare_wait();
            assert!(app.session.next_ai_request().is_none(), "human owns Black");
        }
        eprintln!(
            "human/AI: elapsed={:?}, plies={}, outcome={:?}",
            start.elapsed(),
            app.session.game().len(),
            app.session.game().outcome()
        );
        app.close_worker();
    }

    #[test]
    #[ignore = "release acceptance: Take inside real evaluation, stale discard, undo/Resume"]
    fn take_during_real_search_discards_silently_then_undo_resume_searches() {
        let start = Instant::now();
        let (entered, evaluating) = mpsc::channel();
        let (release, latch) = mpsc::channel();
        let (mut app, replies) = real_app(
            0x6006_1a02,
            Game::new(),
            Some(EvaluationLatch {
                entered,
                release: latch,
            }),
        );
        app.session.place(point(7, 7).unwrap());
        app.session.knock.take();
        app.prepare_wait();
        receive(&evaluating); // A genuine Flex evaluation is inside MCTS now.
        app.session.take_seat(Color::White);
        let human = point(0, 0).unwrap();
        app.session.place(human); // Take acts before the old search can finish.
        assert_eq!(app.session.game().len(), 2);
        app.session.knock.take();
        let taken = app.session.game().clone();
        app.prepare_wait();
        assert!(app.session.next_ai_request().is_none());
        release.send(()).unwrap();
        let stale = receive(&replies);
        assert!(check_search(&stale, Color::White));
        app.handle_app_event(AppEvent::AiCompleted(stale));
        assert_eq!(app.session.game(), &taken);
        assert!(app.session.knock.is_none(), "stale work makes no sound");
        assert!(app.session.notice.is_none());

        // White is non-current: Release does not start another flight. Undo
        // returns to its turn and pauses, despite White now being AI-owned.
        app.session.release_seat(Color::White);
        app.session.undo();
        assert_eq!(app.session.game().len(), 1);
        assert!(!app.session.automation_running());
        app.prepare_wait();
        assert!(app.session.next_ai_request().is_none());
        assert!(matches!(replies.try_recv(), Err(mpsc::TryRecvError::Empty)));
        app.session.resume_automation();
        assert!(automatic_turn(&mut app, &replies));
        assert_eq!(app.session.game().len(), 2);
        assert_eq!(app.session.game().outcome(), Outcome::Ongoing);
        eprintln!(
            "Take/stale/undo/Resume: elapsed={:?}, plies={}, outcome={:?}",
            start.elapsed(),
            app.session.game().len(),
            app.session.game().outcome()
        );
        app.close_worker();
    }

    #[test]
    #[ignore = "release acceptance: playable demo real resignation, v2 save/load, result-only undo"]
    fn real_resignation_replayed_game_saves_loads_and_undo_keeps_stones() {
        let files = Files::new("real-resignation");
        let start = Instant::now();
        // Exactly the native --demo history, using its normal legal Game::play
        // construction rather than the unsavable imported-position fixture.
        let demo = crate::demo_game();
        assert_eq!(demo.len(), 11);
        assert_eq!(demo.outcome(), Outcome::Ongoing);
        assert_eq!(demo.to_move(), Color::White);
        let stones = demo.stones().collect::<Vec<_>>();
        let (mut app, replies) = real_app(0x6006_1a03, demo.clone(), None);
        assert!(automatic_turn(&mut app, &replies));
        assert_eq!(app.session.game().moves(), demo.moves());
        assert_eq!(app.session.game().stones().collect::<Vec<_>>(), stones);
        assert_eq!(
            app.session.game().outcome(),
            Outcome::Won {
                winner: Color::Black,
                method: WinMethod::Resignation
            }
        );
        stopped(&mut app, &replies);
        let path = roundtrip(&mut app, &files);
        app.session.open(&path);
        assert!(!app.session.automation_running());
        stopped(&mut app, &replies);
        app.session.undo();
        assert_eq!(app.session.game(), &demo, "undo clears only the result");
        assert_eq!(app.session.game().stones().collect::<Vec<_>>(), stones);
        assert!(!app.session.automation_running());
        assert!(app.session.changed());
        assert!(app.session.knock.is_none());
        app.prepare_wait();
        assert!(app.session.next_ai_request().is_none());
        eprintln!(
            "real resignation/save/load/undo: elapsed={:?}, plies={}, terminal=Black/Resignation, after_undo={:?}",
            start.elapsed(),
            app.session.game().len(),
            app.session.game().outcome()
        );
        app.close_worker();
    }
}
