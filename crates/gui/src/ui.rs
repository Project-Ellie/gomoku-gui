//! The interface: the menus, the side panel, the board overlays, and the dialogs.
//!
//! It is drawn with egui, on top of the board, in one pass after the resolve.
//! Text and markers are placed by projecting board points through the camera, so
//! they stay attached to the board at any zoom.

use egui::{Align2, Color32, FontId, Frame, RichText, Stroke, Vec2};
use gomoku_core::{Color, Outcome, label};

use crate::render::WOOD;
use crate::session::{After, Dialog, Session, name_of, winning_line};

/// What the interface asks the application to do once the frame is drawn. The
/// application owns the file dialogs and the window, so it performs these.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct Requests {
    /// Start a new game, after the unsaved-changes check.
    pub new_game: bool,
    /// Ask for a file and open it.
    pub open: bool,
    /// Save to the open file, or ask for one.
    pub save: bool,
    /// Ask for a file and save to it.
    pub save_as: bool,
    /// Close the application.
    pub quit: bool,
}

/// How far outside the grid the coordinates are drawn, in cells.
///
/// The wood of the board reaches 0.85 of a cell beyond the last line, so a label
/// closer than this would sit on the dark edge of the board and could not be
/// read.
const OUTSIDE: f32 = 1.15;

/// The colours of the interface.
const INK: Color32 = Color32::from_rgb(226, 226, 230);
const MUTED: Color32 = Color32::from_rgb(150, 150, 158);
const ACCENT: Color32 = Color32::from_rgb(240, 196, 90);

/// Draw the whole interface and return what the application should do next.
///
/// `ui` covers the whole window: the panels take their edges from it and the
/// board is drawn by the passes underneath everything that is left.
pub fn draw(ui: &mut egui::Ui, session: &mut Session) -> Requests {
    let mut requests = Requests::default();

    egui::Panel::top("bar").exact_size(28.0).show(ui, |ui| {
        egui::MenuBar::new().ui(ui, |ui| {
            ui.menu_button("File", |ui| {
                if ui.button("New game").clicked() {
                    requests.new_game = true;
                    ui.close();
                }
                if ui.button("Open...").clicked() {
                    requests.open = true;
                    ui.close();
                }
                ui.separator();
                if ui.button("Save").clicked() {
                    requests.save = true;
                    ui.close();
                }
                if ui.button("Save as...").clicked() {
                    requests.save_as = true;
                    ui.close();
                }
                ui.separator();
                if ui.button("Quit").clicked() {
                    requests.quit = true;
                    ui.close();
                }
            });
            ui.menu_button("Edit", |ui| {
                if ui.button("Undo, and the move is gone").clicked() {
                    session.undo();
                    ui.close();
                }
            });
            ui.menu_button("View", |ui| {
                if ui.button("Fit the board").clicked() {
                    session.fit();
                    ui.close();
                }
                if ui.button("Turn the board around").clicked() {
                    session.flip();
                    ui.close();
                }
                ui.separator();
                ui.checkbox(&mut session.toggles.coordinates, "Coordinates");
                ui.checkbox(&mut session.toggles.move_numbers, "Move numbers");
                ui.checkbox(&mut session.toggles.last_move, "Last move");
                ui.checkbox(&mut session.toggles.win_line, "Winning line");
                ui.separator();
                ui.checkbox(&mut session.sound, "Sound");
                ui.add(
                    egui::Slider::new(&mut session.volume, 0.0..=1.0)
                        .text("Volume")
                        .show_value(false),
                );
                ui.add(
                    egui::Slider::new(&mut session.gain, 1.0..=6.0)
                        .text("Wood lightness")
                        .show_value(false),
                );
                if ui.button("Reset the wood lightness").clicked() {
                    session.gain = WOOD.gain;
                    ui.close();
                }
            });
            ui.menu_button("Go", |ui| {
                if ui.button("To the start").clicked() {
                    session.seek(0);
                    ui.close();
                }
                if ui.button("Back one move").clicked() {
                    session.rewind();
                    ui.close();
                }
                if ui.button("Forward one move").clicked() {
                    session.forward();
                    ui.close();
                }
                if ui.button("To the latest move").clicked() {
                    session.seek(usize::MAX);
                    ui.close();
                }
            });
            ui.separator();
            ui.label(RichText::new(status_line(session)).color(MUTED));
        });
    });

    let panel = egui::Panel::right("panel")
        .resizable(true)
        .default_size(session.panel_width)
        .size_range(180.0..=420.0)
        .show(ui, |ui| {
            names(ui, session);
            ui.separator();
            move_list(ui, session);
            ui.separator();
            recents(ui, session);
        });
    session.panel_width = panel.response.rect.width();

    egui::CentralPanel::default()
        .frame(Frame::NONE)
        .show(ui, |ui| {
            let rect = ui.max_rect();
            let scale = ui.ctx().pixels_per_point();
            session.viewport_points = [rect.min.x, rect.min.y, rect.width(), rect.height()];
            // The interface is the only code that knows where the panels end, so
            // it owns the viewport. In physical pixels, because the board and the
            // camera work in those.
            session.viewport = crate::camera::Viewport {
                x: rect.min.x * scale,
                y: rect.min.y * scale,
                width: rect.width() * scale,
                height: rect.height() * scale,
            };
            session.pixels_per_point = scale;
            if !session.fitted {
                // The first frame that knows the area is the one that places the
                // view, so that the overlays below already use it.
                session.place_view(session.viewport);
            }
            let painter = ui.painter().clone();
            overlays(&painter, session, scale);
            // Nothing here senses the pointer: a click on the board is the game's,
            // and the application tests for it with `Session::on_board`.
        });

    dialogs(ui.ctx(), session, &mut requests);
    requests
}

/// The line of text that says what is happening.
fn status_line(session: &Session) -> String {
    if let Some(notice) = &session.notice {
        return notice.clone();
    }
    match session.game.outcome() {
        Outcome::Ongoing => {
            let side = match session.game.to_move() {
                Color::Black => "Black",
                Color::White => "White",
            };
            let view = if session.game.is_rewound() {
                format!(
                    " — looking at move {} of {}",
                    session.game.cursor(),
                    session.game.len()
                )
            } else {
                String::new()
            };
            format!("{side} to play — move {}{view}", session.game.len() + 1)
        }
        Outcome::Won { winner, .. } => {
            let side = match winner {
                Color::Black => "Black",
                Color::White => "White",
            };
            format!("{side} wins in {} moves", session.game.len())
        }
        Outcome::Draw => "A draw".to_string(),
    }
}

/// The player names.
fn names(ui: &mut egui::Ui, session: &mut Session) {
    let mut black = session.game.meta().black.clone().unwrap_or_default();
    let mut white = session.game.meta().white.clone().unwrap_or_default();
    let mut changed = false;
    ui.horizontal(|ui| {
        ui.label(RichText::new("Black").color(MUTED));
        let edit = egui::TextEdit::singleline(&mut black).desired_width(120.0);
        if ui.add(edit).changed() {
            changed = true;
        }
    });
    ui.horizontal(|ui| {
        ui.label(RichText::new("White").color(MUTED));
        let edit = egui::TextEdit::singleline(&mut white).desired_width(120.0);
        if ui.add(edit).changed() {
            changed = true;
        }
    });
    if changed {
        let black = (!black.trim().is_empty()).then(|| black.trim().to_string());
        let white = (!white.trim().is_empty()).then(|| white.trim().to_string());
        session.game.set_players(black, white);
    }
}

/// The list of moves, one row each, with a click to look at that position.
fn move_list(ui: &mut egui::Ui, session: &mut Session) {
    ui.label(RichText::new("Moves").color(MUTED));
    let cursor = session.game.cursor();
    let mut seek = None;
    // The whole game, so that the moves ahead of the cursor stay visible. They
    // are dimmed, because they are not on the board yet.
    let record: Vec<(gomoku_core::Move, Color)> = session.game.record().collect();
    egui::ScrollArea::vertical()
        .auto_shrink([false, false])
        .max_height((ui.available_height() - 150.0).max(80.0))
        .show(ui, |ui| {
            for (index, (point, colour)) in record.into_iter().enumerate() {
                let number = index + 1;
                let here = number == cursor;
                let glyph = match colour {
                    Color::Black => "●",
                    Color::White => "○",
                };
                let text = format!("{number:>4}  {glyph}  {}", label(point));
                let rich = if here {
                    RichText::new(text).color(ACCENT).strong()
                } else if number > cursor {
                    RichText::new(text).color(MUTED)
                } else {
                    RichText::new(text).color(INK)
                };
                if ui.selectable_label(here, rich).clicked() {
                    seek = Some(number);
                }
            }
        });
    if let Some(number) = seek {
        session.seek(number);
    }
}

/// The recently used files.
fn recents(ui: &mut egui::Ui, session: &mut Session) {
    if session.recent.is_empty() {
        return;
    }
    ui.label(RichText::new("Recent").color(MUTED));
    let mut open = None;
    for path in session.recent.clone().into_iter().take(6) {
        if ui.link(name_of(&path)).clicked() {
            open = Some(path);
        }
    }
    if let Some(path) = open {
        session.open(&path);
    }
}

/// The coordinates, the markers, and the winning line, drawn on the board.
fn overlays(painter: &egui::Painter, session: &Session, points_per_pixel: f32) {
    let ppc = session.camera.pixels_per_cell;
    let font = FontId::monospace((ppc * 0.40 / points_per_pixel).clamp(9.0, 28.0));
    let to_screen = |board: [f32; 2]| -> egui::Pos2 {
        let pixel = session.camera.to_screen(session.viewport, board);
        egui::Pos2::new(pixel[0] / points_per_pixel, pixel[1] / points_per_pixel)
    };

    if session.toggles.coordinates {
        for index in 0..15_u8 {
            let along = index as f32;
            let letter = label(gomoku_core::point(0, index).expect("inside"))
                .chars()
                .next()
                .unwrap_or('?');
            let number = 15 - index;
            // Above and below the board.
            painter.text(
                to_screen([along, -OUTSIDE]),
                Align2::CENTER_CENTER,
                format!("{letter}"),
                font.clone(),
                MUTED,
            );
            painter.text(
                to_screen([along, 14.0 + OUTSIDE]),
                Align2::CENTER_CENTER,
                format!("{letter}"),
                font.clone(),
                MUTED,
            );
            // Left and right of the board.
            painter.text(
                to_screen([-OUTSIDE, along]),
                Align2::CENTER_CENTER,
                format!("{number}"),
                font.clone(),
                MUTED,
            );
            painter.text(
                to_screen([14.0 + OUTSIDE, along]),
                Align2::CENTER_CENTER,
                format!("{number}"),
                font.clone(),
                MUTED,
            );
        }
    }

    if session.toggles.win_line {
        if let Some([from, to]) = winning_line(&session.game) {
            let start = to_screen([from.col() as f32, from.row() as f32]);
            let end = to_screen([to.col() as f32, to.row() as f32]);
            let width = (ppc * 0.10 / points_per_pixel).max(2.0);
            painter.line_segment(
                [start, end],
                Stroke::new(width, Color32::from_rgba_unmultiplied(240, 90, 70, 170)),
            );
            for point in [from, to] {
                let centre = to_screen([point.col() as f32, point.row() as f32]);
                painter.circle_stroke(
                    centre,
                    ppc * 0.44 / points_per_pixel,
                    Stroke::new(
                        width * 0.6,
                        Color32::from_rgba_unmultiplied(240, 90, 70, 200),
                    ),
                );
            }
        }
    }

    if session.toggles.move_numbers {
        for (index, (point, colour)) in session.game.stones().enumerate() {
            let centre = to_screen([point.col() as f32, point.row() as f32]);
            let ink = match colour {
                Color::Black => Color32::from_rgb(235, 235, 240),
                Color::White => Color32::from_rgb(30, 30, 36),
            };
            painter.text(
                centre + Vec2::new(0.0, 0.0),
                Align2::CENTER_CENTER,
                format!("{}", index + 1),
                font.clone(),
                ink,
            );
        }
    }

    if session.toggles.last_move {
        if let Some(point) = session.game.last_move() {
            let centre = to_screen([point.col() as f32, point.row() as f32]);
            let width = (ppc * 0.045 / points_per_pixel).max(1.0);
            painter.circle_stroke(
                centre,
                ppc * 0.34 / points_per_pixel,
                Stroke::new(width, ACCENT),
            );
        }
    }

    // The intersection under the pointer, when a placement there is legal.
    if let Some(point) = session.hover {
        let centre = to_screen([point.col() as f32, point.row() as f32]);
        let width = (ppc * 0.035 / points_per_pixel).max(1.0);
        painter.circle_stroke(
            centre,
            ppc * 0.44 / points_per_pixel,
            Stroke::new(width, Color32::from_rgba_unmultiplied(255, 255, 255, 90)),
        );
    }
}

/// The dialogs that wait for an answer.
fn dialogs(ctx: &egui::Context, session: &mut Session, requests: &mut Requests) {
    let Some(dialog) = session.dialog.clone() else {
        return;
    };
    let mut close = false;
    match dialog {
        Dialog::Truncate { count, point } => {
            let response = egui::Modal::new(egui::Id::new("truncate")).show(ctx, |ui| {
                ui.set_max_width(360.0);
                ui.heading("Delete the moves after this one?");
                ui.label(format!(
                    "You are looking at an earlier position. Playing here deletes {count} \
                     move{}. The move cannot be brought back.",
                    if count == 1 { "" } else { "s" }
                ));
                ui.add_space(8.0);
                ui.horizontal(|ui| {
                    if ui.button("Place, and delete them").clicked() {
                        session.place_now(point);
                        close = true;
                    }
                    if ui.button("Cancel").clicked() {
                        close = true;
                    }
                });
            });
            if response.should_close() {
                close = true;
            }
        }
        Dialog::Unsaved { then } => {
            let response = egui::Modal::new(egui::Id::new("unsaved")).show(ctx, |ui| {
                ui.set_max_width(380.0);
                ui.heading("This game has changes that are not saved");
                ui.label(match then {
                    After::NewGame => "Start a new game anyway?",
                    After::Open => "Open another game anyway?",
                    After::Quit => "Quit anyway?",
                });
                ui.add_space(8.0);
                ui.horizontal(|ui| {
                    if ui.button("Save first").clicked() {
                        session.after_save = Some(then);
                        requests.save = true;
                        close = true;
                    }
                    if ui.button("Discard the changes").clicked() {
                        session.perform = Some(then);
                        close = true;
                    }
                });
            });
            if response.should_close() {
                close = true;
            }
        }
        Dialog::Resume { source } => {
            let response = egui::Modal::new(egui::Id::new("resume")).show(ctx, |ui| {
                ui.set_max_width(380.0);
                ui.heading("An unfinished game was found");
                ui.label(match &source {
                    Some(path) => format!(
                        "{} was not finished, and there is a newer autosave of it.",
                        name_of(path)
                    ),
                    None => {
                        "There is an autosaved game that was never saved to a file.".to_string()
                    }
                });
                ui.add_space(8.0);
                ui.horizontal(|ui| {
                    if ui.button("Continue it").clicked() {
                        session.resume_autosave();
                        close = true;
                    }
                    if ui.button("Start fresh").clicked() {
                        session.discard_autosave();
                        close = true;
                    }
                });
            });
            if response.should_close() {
                close = true;
            }
        }
        Dialog::Message { title, body } => {
            let response = egui::Modal::new(egui::Id::new("message")).show(ctx, |ui| {
                ui.set_max_width(420.0);
                ui.heading(title);
                ui.label(body);
                ui.add_space(8.0);
                if ui.button("OK").clicked() {
                    close = true;
                }
            });
            if response.should_close() {
                close = true;
            }
        }
    }
    if close {
        session.dialog = None;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    /// Run the interface over a window of a fixed size, with a pointer in a
    /// given place, and report what it drew and what it claimed.
    fn run(session: &mut Session, pointer: egui::Pos2) -> (Requests, Claim) {
        let context = egui::Context::default();
        let mut requests = Requests::default();
        let mut claim = Claim::default();
        // The application records the pointer as the cursor moves; a test must
        // do the same, or the board hit test sees no pointer.
        session.pointer = [pointer.x, pointer.y];
        // Two passes: egui decides who owns the pointer from the pass before.
        for _ in 0..2 {
            let input = egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(
                    egui::Pos2::ZERO,
                    egui::vec2(1200.0, 900.0),
                )),
                events: vec![egui::Event::PointerMoved(pointer)],
                ..Default::default()
            };
            let mut output = context.run_ui(input, |ui| {
                requests = draw(ui, session);
            });
            // Nothing renders in a test, so the atlas is thrown away on purpose.
            output.textures_delta.clear();
            claim = Claim {
                over_egui: context.egui_wants_pointer_input(),
                using: context.egui_is_using_pointer(),
            };
        }
        (requests, claim)
    }

    /// What the interface says about the pointer, in the two ways it can say it.
    #[derive(Debug, Default)]
    struct Claim {
        /// The pointer is over the interface, or the interface is dragging.
        over_egui: bool,
        /// The interface is dragging something right now.
        using: bool,
    }

    /// The application blocks the pointer when the interface is dragging, or when
    /// the pointer is not over the board. Testing both halves here means that a
    /// change to either one cannot quietly make the board unclickable.
    fn pointer_blocked(session: &Session, claim: &Claim) -> bool {
        claim.using || !session.on_board()
    }

    #[test]
    fn a_click_on_the_board_reaches_the_game() {
        let mut session = Session::new(&gomoku_core::Settings::default());
        let (_, claim) = run(&mut session, egui::pos2(400.0, 400.0));
        assert!(
            session.on_board(),
            "the application must know the pointer is over the board: {:?}",
            session.viewport_points
        );
        assert!(
            !pointer_blocked(&session, &claim),
            "the click must reach the game: {claim:?}"
        );
        assert!(
            claim.over_egui,
            "the interface reports the pointer as its own over the whole window, \
             which is why the application uses the narrower test"
        );
    }

    #[test]
    fn a_click_on_the_panel_does_not() {
        let mut session = Session::new(&gomoku_core::Settings::default());
        let (_, claim) = run(&mut session, egui::pos2(1150.0, 450.0));
        assert!(!session.on_board(), "the side panel is not the board");
        assert!(
            pointer_blocked(&session, &claim),
            "a click on the panel is the interface's"
        );
    }

    #[test]
    fn every_dialog_draws() {
        let point = gomoku_core::point(7, 7).expect("inside the board");
        let dialogs = [
            Dialog::Truncate { count: 3, point },
            Dialog::Unsaved { then: After::Quit },
            Dialog::Resume {
                source: Some(PathBuf::from("/tmp/game.json")),
            },
            Dialog::Resume { source: None },
            Dialog::Message {
                title: "The game could not be opened".to_string(),
                body: "the record has no marker".to_string(),
            },
        ];
        for dialog in dialogs {
            let mut session = Session::new(&gomoku_core::Settings::default());
            session.dialog = Some(dialog);
            // The interface must draw it without complaint, over the board.
            let (_, claim) = run(&mut session, egui::pos2(600.0, 450.0));
            assert!(
                claim.over_egui,
                "a dialog takes the pointer while it is open"
            );
        }
    }
}
