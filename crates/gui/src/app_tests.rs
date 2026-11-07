//! Headless application tests: real egui requests and app dispatch, fake only
//! the native save picker, event-loop exit, and filesystem destinations.
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
    app.run_requests(requests, pick, exit);
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
        app.run_requests(requests, &mut |_| None, &mut exit);
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
        &mut exit,
    );
    assert!(app.closing);
    assert!(!app.session.changed());
    assert_eq!(gomoku_core::load(&original).unwrap(), original_game);
    assert_eq!(gomoku_core::load(&destination).unwrap(), expected);
    app.run_requests(Requests::default(), &mut pick, &mut exit);
    app.run_requests(
        Requests {
            save: true,
            ..Default::default()
        },
        &mut pick,
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
        &mut exit,
    );
    files.write_settings(&mut pristine);
    assert!(!recovery.exists());
    assert!(!state.exists());
    assert_eq!(exits, 3);
}
