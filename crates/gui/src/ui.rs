//! The interface: the menus, the side panel, the board overlays, and the dialogs.
//!
//! It is drawn with egui, on top of the board, in one pass after the resolve.
//! Text and markers are placed by projecting board points through the camera, so
//! they stay attached to the board at any zoom.

use egui::{Align2, Color32, FontId, Frame, RichText, Stroke, Vec2};
use gomoku_core::{Color, Outcome, WinMethod, label};

use crate::ai::SeatOwner;
use crate::render::WOOD;
use crate::session::{After, Dialog, PanelSection, Session, name_of, winning_line};

/// What the interface asks the application to do once the frame is drawn. The
/// application owns the file dialogs and the window, so it performs these.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct Requests {
    /// Start a new game, after the unsaved-changes check.
    pub new_game: bool,
    /// Ask for a file and open it.
    pub open: bool,
    /// Ask for a puzzle file and load it.
    pub open_puzzle: bool,
    /// Ask for a corpus folder and open it in the Lab Explorer.
    pub open_explorer: bool,
    /// Save to the open file, or ask for one.
    pub save: bool,
    /// Ask for a file and save to it.
    pub save_as: bool,
    /// Close the application.
    pub quit: bool,
    /// Load the record with this index from the Explorer into the board.
    pub explorer_select: Option<usize>,
}

/// The colours of the interface.
const MUTED: Color32 = Color32::from_rgb(150, 150, 158);
/// The puzzle-start cursor ring.
const PUZZLE_CURSOR: Color32 = Color32::from_rgb(50, 232, 90);

/// The side panel is middle grey. On the dark background of the window a black
/// stone could not be told from the panel, so the move list could not show both
/// stones as a filled circle of their own colour.
const PANEL: Color32 = Color32::from_rgb(126, 126, 132);
/// The ink for the middle grey panel.
const PANEL_INK: Color32 = Color32::from_rgb(26, 26, 30);
/// The background of the row that the board is showing, lighter than the panel
/// so that both stone colours stay visible on it.
const PANEL_CURRENT: Color32 = Color32::from_rgb(178, 178, 184);
/// The background of a text field on the middle grey panel. The dark theme gives
/// a text field a near-black background, which turns the player names into two
/// black bars.
const PANEL_FIELD: Color32 = Color32::from_rgb(200, 200, 206);
/// The colour of a black stone in the move list.
const SLATE_MARK: Color32 = Color32::from_rgb(22, 22, 26);
/// The colour of a white stone in the move list.
const SHELL_MARK: Color32 = Color32::from_rgb(246, 246, 250);
/// The resignation banner: a strong red, because a concession is the one
/// result the board itself cannot show. Bold white text on it stays readable
/// at a glance.
const BANNER: Color32 = Color32::from_rgb(178, 24, 24);
/// The banner is meant to shout; anything under 24 points is a label.
const BANNER_FONT: f32 = 28.0;

/// A section body's shapes in the paint list.
type PanelBodyPaint = (
    PanelSection,
    egui::LayerId,
    egui::layers::ShapeIdx,
    egui::layers::ShapeIdx,
);

/// Paint a section body and remember its shapes in case a later header supersedes it.
fn paint_panel_body(
    ui: &mut egui::Ui,
    section: PanelSection,
    add_contents: impl FnOnce(&mut egui::Ui),
) -> PanelBodyPaint {
    let layer_id = ui.painter().layer_id();
    let first = ui
        .ctx()
        .graphics_mut(|graphics| graphics.entry(layer_id).next_idx());
    add_contents(ui);
    let last = ui
        .ctx()
        .graphics_mut(|graphics| graphics.entry(layer_id).next_idx());
    (section, layer_id, first, last)
}

/// Draw one accordion header and report whether its section is open.
fn panel_section_header(
    ui: &mut egui::Ui,
    session: &mut Session,
    section: PanelSection,
    title: &str,
    font_size: f32,
) -> bool {
    let indicator = if session.open_panel_section() == section {
        "▾"
    } else {
        "▸"
    };
    let response = ui.add(
        egui::Label::new(
            RichText::new(format!("{indicator} {title}"))
                .font(FontId::proportional(font_size))
                .color(PANEL_INK),
        )
        .sense(egui::Sense::click()),
    );
    if response.clicked() {
        session.toggle_panel_section(section);
    }
    session.open_panel_section() == section
}

/// Draw the whole interface and return what the application should do next.
///
/// `ui` covers the whole window: the panels take their edges from it and the
/// board is drawn by the passes underneath everything that is left.
pub fn draw(ui: &mut egui::Ui, session: &mut Session) -> Requests {
    let mut requests = Requests::default();
    // Floating picker windows do not block the background as egui modals do.
    // Keep menus, recents, and name fields inert until the dialog is answered.
    if session.dialog().is_some() {
        ui.disable();
    }

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
                if ui.button("Load puzzle...").clicked() {
                    requests.open_puzzle = true;
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
            ui.menu_button("Lab", |ui| {
                if ui.button("Explorer...").clicked() {
                    requests.open_explorer = true;
                    ui.close();
                }
            });
            ui.separator();
            ui.label(RichText::new(status_line(session)).color(MUTED));
        });
    });

    // A concession is the one result the board itself cannot show, so it gets
    // the loudest surface there is: a persistent red banner across the top of
    // the board. It takes no space in any other state of the game.
    if let Some(line) = resignation_banner(session.game().outcome()) {
        egui::Panel::top("resignation")
            .frame(Frame::NONE.fill(BANNER).inner_margin(8.0))
            .show(ui, |ui| {
                ui.vertical_centered(|ui| {
                    ui.add(
                        egui::Label::new(
                            RichText::new(line)
                                .font(FontId::proportional(BANNER_FONT))
                                .strong()
                                .color(Color32::WHITE),
                        )
                        .wrap(),
                    );
                });
            });
    }

    let panel = egui::Panel::right("panel")
        .resizable(true)
        .default_size(session.panel_width)
        .size_range(180.0..=420.0)
        .frame(Frame::NONE.fill(PANEL).inner_margin(8.0))
        .show(ui, |ui| {
            // Exactly one section is open. The panel ink is set once here,
            // before any section runs, so headers and uncoloured labels read
            // on the grey panel.
            ui.visuals_mut().override_text_color = Some(PANEL_INK);
            let header_size = ui
                .style()
                .text_styles
                .get(&egui::TextStyle::Body)
                .map_or(FontId::default().size, |font| font.size)
                * 2.0;
            let mut body_paints = Vec::new();
            if panel_section_header(ui, session, PanelSection::Players, "Players", header_size) {
                body_paints.push(paint_panel_body(ui, PanelSection::Players, |ui| {
                    seats(ui, session)
                }));
            }
            ui.separator();
            if panel_section_header(ui, session, PanelSection::Moves, "Moves", header_size) {
                body_paints.push(paint_panel_body(ui, PanelSection::Moves, |ui| {
                    move_list(ui, session);
                }));
            }
            // The Explorer header appears only while its corpus is available.
            if session.explorer().is_some() {
                ui.separator();
                if panel_section_header(
                    ui,
                    session,
                    PanelSection::Explorer,
                    "Lab Explorer",
                    header_size,
                ) {
                    body_paints.push(paint_panel_body(ui, PanelSection::Explorer, |ui| {
                        requests.explorer_select =
                            crate::explorer::draw(ui, session).explorer_select;
                    }));
                }
            }
            // A later header can supersede a body that was already laid out.
            // Keep its space for this frame, but remove its paint so only the
            // newly selected body is visible during the switch.
            for (section, layer_id, first, last) in body_paints {
                if section != session.open_panel_section() {
                    ui.ctx().graphics_mut(|graphics| {
                        let paint_list = graphics.entry(layer_id);
                        for index in first.0..last.0 {
                            paint_list.reset_shape(egui::layers::ShapeIdx(index));
                        }
                    });
                }
            }
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
            // The size of the whole surface is not known here; the application
            // fills that in. Only the area for the board is the interface's.
            let width = rect.width() * scale;
            let height = rect.height() * scale;
            let area_changed = (session.viewport.width - width).abs() > 0.5
                || (session.viewport.height - height).abs() > 0.5;
            session.viewport.x = rect.min.x * scale;
            session.viewport.y = rect.min.y * scale;
            session.viewport.width = width;
            session.viewport.height = height;
            session.pixels_per_point = scale;
            if !session.fitted {
                // The first frame that knows the area is the one that places the
                // view, so that the overlays below already use it.
                session.place_view(session.viewport);
            } else if area_changed && session.follows_window {
                // The board's size belongs to the window: while the tie holds,
                // the board grows and shrinks with the area it is drawn into.
                session.camera.refit(session.viewport);
            }
            // Turn/ownership changes also refresh a stationary pointer.
            session.update_hover();
            let painter = ui.painter().clone();
            overlays(&painter, session, scale);
            // Nothing here senses the pointer: a click on the board is the game's,
            // and the application tests for it with `Session::on_board`.
        });

    dialogs(ui.ctx(), session, &mut requests, Session::resume_autosave);
    requests
}

/// The banner line for a resigned game, and nothing for any other state.
///
/// The text is the same one the status line and the title use. It does not
/// carry the review position: a resignation belongs to the game, not to the
/// move under the cursor, so the banner stays while the game is reviewed.
fn resignation_banner(outcome: Outcome) -> Option<String> {
    if let Outcome::Won {
        winner,
        method: WinMethod::Resignation,
    } = outcome
    {
        let (resigned, wins) = match winner {
            Color::Black => ("White", "Black"),
            Color::White => ("Black", "White"),
        };
        Some(format!("{resigned} resigned — {wins} wins"))
    } else {
        None
    }
}

/// The line of text that says what is happening.
fn status_line(session: &Session) -> String {
    let review = if session.game().is_rewound() {
        format!(
            " — reviewing {} of {}",
            session.game().cursor(),
            session.game().len()
        )
    } else {
        String::new()
    };
    match session.game().outcome() {
        Outcome::Ongoing => {
            let side = if session.game().to_move() == Color::Black {
                "Black"
            } else {
                "White"
            };
            if session.owner(session.game().to_move()) == SeatOwner::Ai
                && session.automation_running()
                && !session.game().is_rewound()
            {
                return format!("{side} · Thinking…");
            }
            let paused = if !session.automation_running() {
                " — AI paused"
            } else {
                ""
            };
            let notice = session
                .notice
                .as_ref()
                .map(|text| format!(" — {text}"))
                .unwrap_or_default();
            format!(
                "{side} to play — move {}{paused}{review}{notice}",
                session.game().len() + 1
            )
        }
        Outcome::Won { winner, method } => {
            let side = if winner == Color::Black {
                "Black"
            } else {
                "White"
            };
            match method {
                WinMethod::Five => format!("{side} wins in {} moves{review}", session.game().len()),
                WinMethod::Resignation => {
                    let resigned = if winner == Color::Black {
                        "White"
                    } else {
                        "Black"
                    };
                    format!("{resigned} resigned — {side} wins{review}")
                }
            }
        }
        Outcome::Draw => format!("A draw{review}"),
    }
}

/// Seat intent is separate from editable player metadata. Only actual widget
/// changes commit options: repainting must never invalidate a search.
fn seats(ui: &mut egui::Ui, session: &mut Session) {
    ui.spacing_mut().interact_size.y = 26.0;
    ui.visuals_mut().override_text_color = Some(PANEL_INK);
    let widgets = &mut ui.visuals_mut().widgets;
    for state in [
        &mut widgets.inactive,
        &mut widgets.hovered,
        &mut widgets.active,
    ] {
        state.weak_bg_fill = PANEL_FIELD;
        state.bg_fill = PANEL_FIELD;
        state.fg_stroke.color = PANEL_INK;
    }
    widgets.hovered.weak_bg_fill = Color32::from_rgb(224, 224, 230);
    widgets.active.weak_bg_fill = PANEL_CURRENT;
    for (side, label) in [(Color::Black, "Black"), (Color::White, "White")] {
        ui.push_id(label, |ui| {
            Frame::NONE.show(ui, |ui| {
                let human = session.owner(side) == SeatOwner::Human;
                ui.horizontal(|ui| {
                    stone_dot(ui, side);
                    ui.label(RichText::new(label).strong().color(PANEL_INK));
                    ui.label(if human { "Human" } else { "AI · baseline" });
                });
                let mut name = match side {
                    Color::Black => session.game().meta().black.clone(),
                    Color::White => session.game().meta().white.clone(),
                }.unwrap_or_default();
                if ui.add(egui::TextEdit::singleline(&mut name)
                    .id_salt("name")
                    .desired_width(ui.available_width())
                    .hint_text("Player name")
                    .text_color(PANEL_INK)
                    .background_color(PANEL_FIELD))
                    .on_hover_text(format!("{label} player name (saved with the game)"))
                    .changed()
                {
                    let name = (!name.trim().is_empty()).then(|| name.trim().to_owned());
                    let meta = session.game().meta();
                    let (black, white) = match side {
                        Color::Black => (name, meta.white.clone()),
                        Color::White => (meta.black.clone(), name),
                    };
                    session.set_players(black, white);
                }
                let action = if human { "Release" } else { "Take" };
                let help = if human {
                    format!("Release {label} to the baseline AI. Turns run automatically; both colors may be AI.")
                } else {
                    format!("Take {label} for human play, even while thinking. Discards any pending decision for this seat.")
                };
                if ui.add_sized([ui.available_width(), 28.0], egui::Button::new(format!("{action} {label}")))
                    .on_hover_cursor(egui::CursorIcon::PointingHand)
                    .on_hover_text(help).clicked()
                {
                    if human { session.release_seat(side); } else { session.take_seat(side); }
                }
                let mut options = session.ai_options(side);
                let before = options;
                let field_width = (ui.available_width() - 12.0) / 2.0;
                egui::Grid::new("options").spacing([12.0, 3.0]).show(ui, |ui| {
                    ui.label("Sims").on_hover_text("Retained for this color, even while human. 1–10,000 simulations per decision.");
                    ui.label("Temp");
                    ui.end_row();
                    ui.add_sized([field_width, 26.0], egui::DragValue::new(&mut options.simulations).range(1..=10_000).speed(1));
                    ui.add_sized([field_width, 26.0], egui::DragValue::new(&mut options.temperature).range(0.0..=2.0)
                        .speed(0.1).fixed_decimals(1))
                        .on_hover_text("Sampling temperature: 0 (deterministic) to 2, in steps of 0.1.");
                    ui.end_row();
                });
                options.temperature = (options.temperature * 10.0).round() / 10.0;
                if options != before {
                    if let Err(error) = session.set_ai_options(side, options) {
                        session.notice = Some(error.to_string());
                    }
                }
            });
        });
        ui.separator();
    }
    ui.label(RichText::new("Resignation ON").small())
        .on_hover_text("Baseline resignation is enabled for both AI seats by default. It is a heuristic concession, not a proof of a forced win.");
    if !session.automation_running() {
        ui.label("AI paused").on_hover_text("Undo and history review pause automatic turns. Return to the latest move, then Resume AI.");
    }
    if !session.automation_running()
        && !session.game().is_rewound()
        && session.game().outcome() == Outcome::Ongoing
        && session.owner(session.game().to_move()) == SeatOwner::Ai
        && ui
            .add_sized([ui.available_width(), 28.0], egui::Button::new("Resume AI"))
            .on_hover_cursor(egui::CursorIcon::PointingHand)
            .on_hover_text("Resume continuous automatic turns, not a single step.")
            .clicked()
    {
        session.resume_automation();
    }
}

/// The list of moves, one row each, with a click to look at that position.
/// The section header above it carries the title.
fn move_list(ui: &mut egui::Ui, session: &mut Session) {
    if session.game().outcome() != Outcome::Ongoing {
        ui.label(RichText::new(format!("Live result: {}", status_line(session))).color(PANEL_INK));
    }
    let cursor = session.game().cursor();
    let mut seek = None;
    // The whole game, so that the moves ahead of the cursor stay visible. They
    // are dimmed, because they are not on the board yet.
    let record: Vec<(gomoku_core::Move, Color)> = session.game().record().collect();
    egui::ScrollArea::vertical()
        .auto_shrink([false, false])
        .max_height((ui.available_height() - 150.0).max(80.0))
        .show(ui, |ui| {
            for (index, (point, colour)) in record.into_iter().enumerate() {
                let number = index + 1;
                let here = number == cursor;
                let ink = if number > cursor {
                    RichText::new(format!("{number:>4}")).color(MUTED)
                } else {
                    RichText::new(format!("{number:>4}")).color(PANEL_INK)
                };
                let place = RichText::new(label(point)).color(PANEL_INK);
                if here {
                    let row = Frame::NONE
                        .fill(PANEL_CURRENT)
                        .inner_margin(2.0)
                        .show(ui, |ui| {
                            ui.horizontal(|ui| {
                                ui.label(ink.strong());
                                stone_dot(ui, colour);
                                ui.label(place.strong());
                            });
                        });
                    if row.response.interact(egui::Sense::click()).clicked() {
                        seek = Some(number);
                    }
                } else {
                    let row = ui.horizontal(|ui| {
                        ui.label(ink);
                        stone_dot(ui, colour);
                        ui.label(place);
                    });
                    if row.response.interact(egui::Sense::click()).clicked() {
                        seek = Some(number);
                    }
                }
            }
        });
    if let Some(number) = seek {
        session.seek(number);
    }
}

/// A filled circle in the colour of a stone.
///
/// It is drawn rather than typed: the interface font has no filled circle glyph,
/// so typing one shows an empty box. A drawn circle can also carry the thin
/// outline that each stone needs against the row behind it.
fn stone_dot(ui: &mut egui::Ui, colour: Color) {
    let size = 13.0;
    let (rect, _) = ui.allocate_exact_size(egui::vec2(size, size), egui::Sense::hover());
    let radius = size * 0.42;
    let centre = rect.center();
    let painter = ui.painter();
    let (fill, outline) = match colour {
        Color::Black => (SLATE_MARK, Color32::from_rgb(150, 150, 158)),
        Color::White => (SHELL_MARK, Color32::from_rgb(64, 64, 70)),
    };
    painter.circle_filled(centre, radius, fill);
    painter.circle_stroke(centre, radius, Stroke::new(0.8, outline));
}

/// The recently used files.
fn recents(ui: &mut egui::Ui, session: &mut Session) {
    if session.recent.is_empty() {
        return;
    }
    // The divider belongs to this section, so that it does not hang in the air
    // when there are no recent files.
    ui.separator();
    ui.label(RichText::new("Recent").color(PANEL_INK));
    let mut open = None;
    for path in session.recent.clone().into_iter().take(6) {
        if ui.link(name_of(&path)).clicked() {
            open = Some(path);
        }
    }
    if let Some(path) = open {
        session.attempt(After::OpenRecent(path));
    }
}

/// The markers and the winning line, drawn on top of the board.
fn overlays(painter: &egui::Painter, session: &Session, pixels_per_point: f32) {
    let ppc = session.camera.pixels_per_cell;
    let font = FontId::monospace((ppc * 0.40 / pixels_per_point).clamp(9.0, 28.0));
    let to_screen = |board: [f32; 2]| -> egui::Pos2 {
        let pixel = session.camera.to_screen(session.viewport, board);
        egui::Pos2::new(pixel[0] / pixels_per_point, pixel[1] / pixels_per_point)
    };

    if session.toggles.win_line {
        if let Some([from, to]) = winning_line(session.game()) {
            let start = to_screen([from.col() as f32, from.row() as f32]);
            let end = to_screen([to.col() as f32, to.row() as f32]);
            let width = (ppc * 0.10 / pixels_per_point).max(2.0);
            painter.line_segment(
                [start, end],
                Stroke::new(width, Color32::from_rgba_unmultiplied(246, 186, 60, 190)),
            );
            for point in [from, to] {
                let centre = to_screen([point.col() as f32, point.row() as f32]);
                painter.circle_stroke(
                    centre,
                    ppc * 0.44 / pixels_per_point,
                    Stroke::new(
                        width * 0.6,
                        Color32::from_rgba_unmultiplied(246, 186, 60, 210),
                    ),
                );
            }
        }
    }

    if session.toggles.move_numbers {
        for (index, (point, colour)) in session.game().stones().enumerate() {
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

    // The puzzle-start cursor ring.  The last-move marker is now rendered by
    // the board shader as a red glow, so it no longer competes with this ring.
    if let Some(point) = session.puzzle_cursor {
        let centre = to_screen([point.col() as f32, point.row() as f32]);
        let width = (ppc * 0.085 / pixels_per_point).max(2.0);
        painter.circle_stroke(
            centre,
            ppc * 0.40 / pixels_per_point,
            Stroke::new(width, PUZZLE_CURSOR),
        );
    }

    // The intersection under the pointer, when a placement there is legal.
    if let Some(point) = session.hover {
        let centre = to_screen([point.col() as f32, point.row() as f32]);
        let width = (ppc * 0.035 / pixels_per_point).max(1.0);
        painter.circle_stroke(
            centre,
            ppc * 0.44 / pixels_per_point,
            Stroke::new(width, Color32::from_rgba_unmultiplied(255, 255, 255, 90)),
        );
    }
}

/// The dialogs that wait for an answer.
fn dialogs(
    ctx: &egui::Context,
    session: &mut Session,
    requests: &mut Requests,
    resume: impl FnOnce(&mut Session),
) {
    let Some(dialog) = session.dialog().cloned() else {
        return;
    };
    let mut close = false;
    match &dialog {
        Dialog::Truncate { count, point: _ } => {
            let response = egui::Modal::new(egui::Id::new("truncate")).show(ctx, |ui| {
                ui.set_max_width(360.0);
                ui.heading("Delete the moves after this one?");
                ui.label(format!(
                    "You are looking at an earlier position. Playing here deletes {count} \
                     move{}. The move cannot be brought back.",
                    if *count == 1 { "" } else { "s" }
                ));
                ui.add_space(8.0);
                ui.horizontal(|ui| {
                    if ui.button("Place, and delete them").clicked() {
                        session.confirm_truncation();
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
                    After::Open | After::OpenRecent(_) => "Open another game anyway?",
                    After::OpenPuzzle => "Load a puzzle anyway?",
                    After::Quit => "Quit anyway?",
                });
                ui.add_space(8.0);
                ui.horizontal(|ui| {
                    if ui.button("Save first").clicked() {
                        session.defer_after_save(then.clone());
                        requests.save = true;
                        close = true;
                    }
                    if ui.button("Discard the changes").clicked() {
                        session.defer_action(then.clone());
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
                ui.label(match source {
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
                        resume(session);
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
                ui.heading(title.as_str());
                ui.label(body.as_str());
                ui.add_space(8.0);
                if ui.button("OK").clicked() {
                    close = true;
                }
            });
            if response.should_close() {
                close = true;
            }
        }
        Dialog::PickPuzzle { puzzles, index, .. } => {
            let mut new_index = *index;
            let mut confirm = false;
            let mut cancel = false;
            // Twenty per row, as asked: a wall of super-mini boards, each just
            // recognizable, so the choice is made with the eyes.
            let columns = 20;
            let thumb = 44.0;
            // An opaque frame: the board shows the preview behind the window,
            // and the thumbnails must not compete with it.
            let style = ctx.global_style();
            let frame = egui::Frame::window(&style).fill(style.visuals.panel_fill);
            egui::Window::new("Choose a puzzle")
                .id(egui::Id::new("pick-puzzle"))
                .collapsible(false)
                .resizable(true)
                .frame(frame)
                .default_size([columns as f32 * (thumb + 7.0) + 44.0, 540.0])
                .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
                .show(ctx, |ui| {
                    // The selection line: what is selected, and a way to jump
                    // straight to a number.
                    let text_id = egui::Id::new("puzzle-number");
                    ui.horizontal(|ui| {
                        let puzzle = &puzzles[new_index];
                        let depth = puzzle
                            .depth
                            .map(|d| format!("depth {d}"))
                            .unwrap_or_else(|| "no depth".to_string());
                        ui.strong(&puzzle.name);
                        ui.label(format!(
                            "— {depth} — {} of {}",
                            new_index + 1,
                            puzzles.len()
                        ));
                        ui.add_space(12.0);
                        ui.label("Number:");
                        let mut input: String = ui
                            .data_mut(|d| d.get_temp(text_id))
                            .unwrap_or_else(|| new_index.to_string());
                        let edit = egui::TextEdit::singleline(&mut input)
                            .desired_width(64.0)
                            .char_limit(7);
                        let response = ui.add(edit);
                        if response.changed() {
                            if let Some(parsed) = parse_puzzle_index(&input, puzzles.len()) {
                                new_index = parsed;
                            }
                        } else if !response.has_focus() && input != new_index.to_string() {
                            // The selection moved by click or key: the field follows.
                            input = new_index.to_string();
                        }
                        ui.data_mut(|d| d.insert_temp(text_id, input));
                    });
                    ui.add_space(6.0);

                    // The keyboard: the arrows walk the grid, Enter starts the
                    // puzzle, Escape cancels. The application leaves every key
                    // to the dialog while it is open.
                    let mut scroll = false;
                    ui.input(|input| {
                        if input.key_pressed(egui::Key::ArrowLeft) {
                            new_index = new_index.saturating_sub(1);
                            scroll = true;
                        }
                        if input.key_pressed(egui::Key::ArrowRight) {
                            new_index = (new_index + 1).min(puzzles.len() - 1);
                            scroll = true;
                        }
                        if input.key_pressed(egui::Key::ArrowUp) {
                            new_index = new_index.saturating_sub(columns);
                            scroll = true;
                        }
                        if input.key_pressed(egui::Key::ArrowDown) {
                            new_index = (new_index + columns).min(puzzles.len() - 1);
                            scroll = true;
                        }
                        if input.key_pressed(egui::Key::Enter) {
                            confirm = true;
                        }
                        if input.key_pressed(egui::Key::Escape) {
                            cancel = true;
                        }
                    });

                    // Shrink to the grid when it is small, scroll when it is
                    // not, so the buttons sit right under the thumbnails.
                    egui::ScrollArea::vertical()
                        .auto_shrink([false, true])
                        .show(ui, |ui| {
                            egui::Grid::new("puzzle-grid")
                                .num_columns(columns)
                                .spacing([7.0, 7.0])
                                .show(ui, |ui| {
                                    for (i, puzzle) in puzzles.iter().enumerate() {
                                        let (rect, response) = ui.allocate_exact_size(
                                            egui::vec2(thumb, thumb),
                                            egui::Sense::click(),
                                        );
                                        paint_puzzle_thumbnail(
                                            ui,
                                            rect,
                                            i,
                                            puzzle,
                                            i == new_index,
                                            response.hovered(),
                                        );
                                        if response.clicked() {
                                            new_index = i;
                                        }
                                        if scroll && i == new_index {
                                            ui.scroll_to_rect(rect, Some(egui::Align::Center));
                                        }
                                        if (i + 1) % columns == 0 {
                                            ui.end_row();
                                        }
                                    }
                                });
                        });

                    ui.add_space(6.0);
                    ui.horizontal(|ui| {
                        if ui.button("Start solving").clicked() {
                            confirm = true;
                        }
                        if ui.button("Cancel").clicked() {
                            cancel = true;
                        }
                    });
                });

            if confirm {
                // On success the dialog is already closed; on failure the
                // message dialog takes over.
                close = session.load_puzzle(&puzzles[new_index]).is_ok();
            } else if cancel {
                session.dismiss_dialog();
                close = true;
            } else {
                // The board shows the selected puzzle right away; only a
                // confirmation turns the preview into a game.
                session.select_puzzle(new_index);
            }
        }
    }
    // A command can replace the original dialog with an error. Only close the
    // dialog that supplied this answer, never a newly created Message.
    if close && session.dialog() == Some(&dialog) {
        session.dismiss_dialog();
    }
}

/// A puzzle position as a super-mini board: the stones as dots on wood, the
/// side to move as a pip in the corner, and the index for the number field.
fn paint_puzzle_thumbnail(
    ui: &egui::Ui,
    rect: egui::Rect,
    index: usize,
    puzzle: &gomoku_core::Puzzle,
    selected: bool,
    hovered: bool,
) {
    let painter = ui.painter_at(rect);
    painter.rect_filled(rect, 3.0, egui::Color32::from_rgb(0xb0, 0x79, 0x48));
    let cell = rect.width() / 15.0;
    let at = |mv: &gomoku_core::Move| {
        egui::pos2(
            rect.min.x + (mv.col() as f32 + 0.5) * cell,
            rect.min.y + (mv.row() as f32 + 0.5) * cell,
        )
    };
    let radius = cell * 0.44;
    for mv in &puzzle.black {
        painter.circle_filled(at(mv), radius, egui::Color32::from_gray(0x14));
    }
    for mv in &puzzle.white {
        painter.circle_filled(at(mv), radius, egui::Color32::from_gray(0xf2));
    }
    // Who moves first: a pip in the top right corner.
    let pip = match puzzle.to_move {
        gomoku_core::Color::Black => egui::Color32::from_gray(0x14),
        gomoku_core::Color::White => egui::Color32::from_gray(0xf2),
    };
    painter.circle(
        rect.right_top() + egui::vec2(-5.0, 5.0),
        3.0,
        pip,
        egui::Stroke::new(1.0, egui::Color32::from_gray(0x50)),
    );
    // The index, small, so a number can be typed as well as a picture clicked.
    painter.text(
        rect.left_bottom() + egui::vec2(2.0, -1.0),
        egui::Align2::LEFT_BOTTOM,
        index.to_string(),
        egui::FontId::proportional(8.5),
        egui::Color32::from_rgba_unmultiplied(0x26, 0x18, 0x0c, 220),
    );
    let (width, colour) = if selected {
        (2.5, egui::Color32::from_rgb(0x7a, 0xb8, 0xff))
    } else if hovered {
        (1.5, egui::Color32::from_rgb(0xd8, 0xd8, 0xde))
    } else {
        (1.0, egui::Color32::from_black_alpha(90))
    };
    painter.rect_stroke(
        rect,
        3.0,
        egui::Stroke::new(width, colour),
        egui::StrokeKind::Inside,
    );
}

/// Parse a puzzle index from user input, refusing values outside `0..len`.
fn parse_puzzle_index(input: &str, len: usize) -> Option<usize> {
    let index: usize = input.parse().ok()?;
    (index < len).then_some(index)
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

    /// Drive a real egui button/link by its visible label, never fixed pixels.
    fn click_text(
        text: &str,
        mut frame: impl FnMut(Vec<egui::Event>) -> egui::FullOutput,
    ) -> egui::FullOutput {
        click_text_timed(text, 1, |events, _| frame(events))
    }

    /// Drive a real egui double-click by its visible label and explicit times.
    fn double_click_text(text: &str, frame: impl FnMut(Vec<egui::Event>, f64) -> egui::FullOutput) {
        click_text_timed(text, 2, frame);
    }

    fn click_text_timed(
        text: &str,
        clicks: usize,
        mut frame: impl FnMut(Vec<egui::Event>, f64) -> egui::FullOutput,
    ) -> egui::FullOutput {
        for _ in 0..8 {
            frame(vec![], 0.0).textures_delta.clear();
        }
        let mut output = frame(vec![], 0.0);
        let position = output
            .shapes
            .iter()
            .find_map(|shape| match &shape.shape {
                egui::epaint::Shape::Text(shape) if shape.galley.job.text == text => {
                    Some(shape.pos + shape.galley.rect.center().to_vec2())
                }
                _ => None,
            })
            .expect("the requested action is drawn");
        output.textures_delta.clear();
        for click in 0..clicks {
            for (step, pressed) in [true, false].into_iter().enumerate() {
                let time = 0.01 + (click * 2 + step) as f64 * 0.01;
                output = frame(
                    vec![
                        egui::Event::PointerMoved(position),
                        egui::Event::PointerButton {
                            pos: position,
                            button: egui::PointerButton::Primary,
                            pressed,
                            modifiers: egui::Modifiers::NONE,
                        },
                    ],
                    time,
                );
                output.textures_delta.clear();
            }
        }
        output
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
    fn a_resigned_game_names_the_win_method() {
        let mut session = Session::new(&gomoku_core::Settings::default());
        session.place(gomoku_core::point(7, 7).unwrap());
        let request = session.next_ai_request().unwrap();
        session.apply_ai_reply(request.reply(Ok(player::Decision {
            action: player::DecisionAction::Resign(player::ResignReason::VerifiedOpponentVcf {
                candidates: 5,
            }),
            search: None,
            guard: None,
        })));
        assert_eq!(status_line(&session), "White resigned — Black wins");
        assert!(session.title().contains("White resigned — Black wins"));

        let mut session = Session::new(&gomoku_core::Settings::default());
        session.release_seat(Color::Black);
        let request = session.next_ai_request().unwrap();
        session.apply_ai_reply(request.reply(Ok(player::Decision {
            action: player::DecisionAction::Resign(player::ResignReason::VerifiedOpponentVcf {
                candidates: 5,
            }),
            search: None,
            guard: None,
        })));
        assert_eq!(status_line(&session), "Black resigned — White wins");
        assert!(session.title().contains("Black resigned — White wins"));
        assert_eq!(session.game().len(), 0);
        assert!(winning_line(session.game()).is_none());
        assert!(session.knock.is_none());
    }

    fn ui_frame(
        context: &egui::Context,
        session: &mut Session,
        events: Vec<egui::Event>,
    ) -> egui::FullOutput {
        ui_frame_sized(context, session, events, egui::vec2(1200.0, 900.0))
    }

    fn ui_frame_sized(
        context: &egui::Context,
        session: &mut Session,
        events: Vec<egui::Event>,
        size: Vec2,
    ) -> egui::FullOutput {
        for event in &events {
            if let egui::Event::PointerMoved(point) = event {
                let scale = context.pixels_per_point();
                session.pointer = [point.x * scale, point.y * scale];
            }
        }
        context.run_ui(
            egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, size)),
                events,
                ..Default::default()
            },
            |ui| {
                draw(ui, session);
            },
        )
    }

    #[test]
    fn seat_buttons_transfer_both_colors_without_renaming_or_paint_churn() {
        use crate::ai::SeatOwner;
        let context = egui::Context::default();
        let mut session = Session::new(&gomoku_core::Settings::default());
        session.toggle_panel_section(PanelSection::Players);
        session.set_players(Some("Wolfie".into()), Some("Shell".into()));
        for (label, side, owner) in [
            ("Release Black", Color::Black, SeatOwner::Ai),
            ("Take Black", Color::Black, SeatOwner::Human),
            ("Take White", Color::White, SeatOwner::Human),
            ("Release White", Color::White, SeatOwner::Ai),
        ] {
            click_text(label, |events| ui_frame(&context, &mut session, events));
            assert_eq!(session.owner(side), owner);
            assert!(!session.on_board(), "a control click is not a board click");
            assert!(session.hover.is_none());
            assert_eq!(session.game().len(), 0);
        }
        assert_eq!(session.game().meta().black.as_deref(), Some("Wolfie"));
        assert_eq!(session.game().meta().white.as_deref(), Some("Shell"));
        let generation = session.generation();
        for _ in 0..10 {
            ui_frame(&context, &mut session, vec![])
                .textures_delta
                .clear();
        }
        assert_eq!(session.generation(), generation);
    }

    fn key(key: egui::Key, modifiers: egui::Modifiers) -> egui::Event {
        egui::Event::Key {
            key,
            physical_key: None,
            pressed: true,
            repeat: false,
            modifiers,
        }
    }

    fn edit_value(context: &egui::Context, session: &mut Session, current: &str, value: &str) {
        click_text(current, |events| ui_frame(context, session, events));
        ui_frame(
            context,
            session,
            vec![
                key(
                    egui::Key::A,
                    egui::Modifiers {
                        command: true,
                        ctrl: true,
                        ..Default::default()
                    },
                ),
                egui::Event::Text(value.into()),
                key(egui::Key::Enter, egui::Modifiers::NONE),
            ],
        )
        .textures_delta
        .clear();
    }

    #[test]
    fn widget_edits_are_validated_retained_and_reach_the_next_request() {
        let context = egui::Context::default();
        let mut session = Session::new(&gomoku_core::Settings::default());
        session.toggle_panel_section(PanelSection::Players);
        edit_value(&context, &mut session, "400", "900");
        edit_value(&context, &mut session, "0.0", "0.7");
        assert_eq!(session.ai_options(Color::Black).simulations, 900);
        assert_eq!(session.ai_options(Color::Black).temperature, 0.7);
        edit_value(&context, &mut session, "400", "700");
        edit_value(&context, &mut session, "0.0", "1.2");
        assert_eq!(session.ai_options(Color::White).simulations, 700);
        assert_eq!(session.ai_options(Color::White).temperature, 1.2);
        assert!(!session.changed(), "AI settings are not game metadata");
        click_text("Take White", |events| {
            ui_frame(&context, &mut session, events)
        });
        click_text("Release White", |events| {
            ui_frame(&context, &mut session, events)
        });
        session.place(gomoku_core::point(7, 7).unwrap());
        let request = session.next_ai_request().unwrap();
        assert_eq!(request.options.simulations, 700);
        assert_eq!(request.options.temperature, 1.2);
        assert!(request.options.resign);
        assert_eq!(session.game().len(), 1);
        let generation = session.generation();
        for _ in 0..10 {
            ui_frame(&context, &mut session, vec![])
                .textures_delta
                .clear();
        }
        assert_eq!(session.generation(), generation);
        assert!(session.apply_ai_reply(request.reply(Ok(player::Decision {
            action: player::DecisionAction::Play(gomoku_core::point(7, 8).unwrap()),
            search: None,
            guard: None,
        }))));
        click_text("Release Black", |events| {
            ui_frame(&context, &mut session, events)
        });
        let request = session.next_ai_request().unwrap();
        assert_eq!(request.options.simulations, 900);
        assert_eq!(request.options.temperature, 0.7);
    }

    #[test]
    fn resume_is_continuous_and_take_is_active_while_thinking_but_not_modal() {
        let context = egui::Context::default();
        let mut session = Session::new(&gomoku_core::Settings::default());
        session.toggle_panel_section(PanelSection::Players);
        session.place(gomoku_core::point(7, 7).unwrap());
        session.seek(0);
        assert!(status_line(&session).contains("AI paused"));
        session.seek(usize::MAX);
        click_text("Resume AI", |events| {
            ui_frame(&context, &mut session, events)
        });
        session.notice = Some("stale file notice".into());
        assert_eq!(status_line(&session), "White · Thinking…");
        let flight = session.next_ai_request().unwrap();
        session.set_dialog(Dialog::Message {
            title: "Wait".into(),
            body: "Modal".into(),
        });
        click_text("Take White", |events| {
            ui_frame(&context, &mut session, events)
        });
        assert_eq!(session.owner(Color::White), SeatOwner::Ai);
        session.dismiss_dialog();
        click_text("Take White", |events| {
            ui_frame(&context, &mut session, events)
        });
        assert_eq!(session.owner(Color::White), SeatOwner::Human);
        assert!(!session.apply_ai_reply(flight.reply(Ok(player::Decision {
            action: player::DecisionAction::Play(gomoku_core::point(7, 8).unwrap()),
            search: None,
            guard: None,
        }))));
        click_text("Release White", |events| {
            ui_frame(&context, &mut session, events)
        });
        let request = session.next_ai_request().unwrap();
        assert!(session.apply_ai_reply(request.reply(Ok(player::Decision {
            action: player::DecisionAction::Play(gomoku_core::point(7, 8).unwrap()),
            search: None,
            guard: None,
        }))));
        click_text("Release Black", |events| {
            ui_frame(&context, &mut session, events)
        });
        assert!(
            session.next_ai_request().is_some(),
            "automation continues without Step"
        );
    }

    #[test]
    fn resignation_is_a_live_result_not_a_stone_even_while_reviewing() {
        let context = egui::Context::default();
        let mut session = Session::new(&gomoku_core::Settings::default());
        session.place(gomoku_core::point(7, 7).unwrap());
        session.knock.take();
        let request = session.next_ai_request().unwrap();
        session.apply_ai_reply(request.reply(Ok(player::Decision {
            action: player::DecisionAction::Resign(player::ResignReason::VerifiedOpponentVcf {
                candidates: 5,
            }),
            search: None,
            guard: None,
        })));
        session.notice = Some("old notice".into());
        assert!(session.title().contains("White resigned — Black wins"));
        assert_eq!(session.game().len(), 1);
        assert_eq!(session.game().stones().count(), 1);
        assert!(session.knock.is_none());
        assert!(winning_line(session.game()).is_none());
        session.seek(0);
        let mut output = ui_frame(&context, &mut session, vec![]);
        output.textures_delta.clear();
        assert!(output.shapes.iter().any(|shape| matches!(&shape.shape,
            egui::Shape::Text(text) if text.galley.text() == "Live result: White resigned — Black wins — reviewing 0 of 1")));
        assert!(session.knock.is_none());
    }

    #[test]
    fn names_edit_through_metadata_without_invalidating_ai() {
        let context = egui::Context::default();
        let mut session = Session::new(&gomoku_core::Settings::default());
        session.toggle_panel_section(PanelSection::Players);
        let generation = session.generation();
        edit_value(&context, &mut session, "Player name", "Wolfie");
        assert_eq!(session.game().meta().black.as_deref(), Some("Wolfie"));
        assert!(session.changed());
        assert_eq!(session.generation(), generation);
        click_text("Release Black", |events| {
            ui_frame(&context, &mut session, events)
        });
        assert_eq!(session.game().meta().black.as_deref(), Some("Wolfie"));
    }

    #[test]
    fn stationary_board_hover_follows_ownership_and_never_survives_resignation() {
        let context = egui::Context::default();
        let mut session = Session::new(&gomoku_core::Settings::default());
        ui_frame(&context, &mut session, vec![])
            .textures_delta
            .clear();
        session.pointer = session.camera.to_screen(session.viewport, [6.0, 6.0]);
        ui_frame(&context, &mut session, vec![])
            .textures_delta
            .clear();
        assert!(session.hover.is_some());
        session.place(gomoku_core::point(7, 7).unwrap());
        ui_frame(&context, &mut session, vec![])
            .textures_delta
            .clear();
        assert!(session.hover.is_none(), "now AI's turn");
        session.take_seat(Color::White);
        ui_frame(&context, &mut session, vec![])
            .textures_delta
            .clear();
        assert!(session.hover.is_some(), "no cursor motion was needed");
        session.release_seat(Color::White);
        let request = session.next_ai_request().unwrap();
        session.apply_ai_reply(request.reply(Ok(player::Decision {
            action: player::DecisionAction::Resign(player::ResignReason::VerifiedOpponentVcf {
                candidates: 5,
            }),
            search: None,
            guard: None,
        })));
        ui_frame(&context, &mut session, vec![])
            .textures_delta
            .clear();
        assert!(session.hover.is_none());
        assert!(session.knock.is_none());
    }

    #[test]
    fn numeric_widgets_clamp_bounds_and_snap_temperature_to_tenths() {
        let context = egui::Context::default();
        let mut session = Session::new(&gomoku_core::Settings::default());
        session.toggle_panel_section(PanelSection::Players);
        edit_value(&context, &mut session, "400", "10001");
        edit_value(&context, &mut session, "0.0", "0.74");
        assert_eq!(session.ai_options(Color::Black).simulations, 10000);
        assert_eq!(session.ai_options(Color::Black).temperature, 0.7);
        edit_value(&context, &mut session, "400", "0");
        edit_value(&context, &mut session, "0.0", "3");
        assert_eq!(session.ai_options(Color::White).simulations, 1);
        assert_eq!(session.ai_options(Color::White).temperature, 2.0);
    }

    #[test]
    fn panel_headers_open_exclusively_at_double_body_size() {
        let context = egui::Context::default();
        let mut session = Session::new(&gomoku_core::Settings::default());
        let mut output = ui_frame(&context, &mut session, vec![]);
        let body_size = egui::Style::default().text_styles[&egui::TextStyle::Body].size;
        let header = output
            .shapes
            .iter()
            .find_map(|shape| match &shape.shape {
                egui::Shape::Text(text) if text.galley.text() == "▸ Players" => Some(text),
                _ => None,
            })
            .expect("the closed Players header is drawn");
        assert_eq!(
            header.galley.job.sections[0].format.font_id.size,
            body_size * 2.0
        );
        output.textures_delta.clear();

        click_text("▸ Players", |events| {
            ui_frame(&context, &mut session, events)
        });
        assert_eq!(
            session.open_panel_section(),
            crate::session::PanelSection::Players
        );
        let mut output = ui_frame(&context, &mut session, vec![]);
        assert!(output.shapes.iter().any(|shape| matches!(&shape.shape,
            egui::Shape::Text(text) if text.galley.text() == "▾ Players")));
        assert!(output.shapes.iter().any(|shape| matches!(&shape.shape,
            egui::Shape::Text(text) if text.galley.text() == "▸ Moves")));
        output.textures_delta.clear();

        session.place(gomoku_core::point(7, 7).unwrap());
        let mut output = click_text("▸ Moves", |events| {
            ui_frame(&context, &mut session, events)
        });
        assert!(output.shapes.iter().any(|shape| matches!(&shape.shape,
            egui::Shape::Text(text) if text.galley.text() == "H8")));
        assert!(
            !output.shapes.iter().any(|shape| matches!(&shape.shape,
                egui::Shape::Text(text) if text.galley.text() == "Resignation ON")),
            "the superseded Players body must not render in the switching frame"
        );
        output.textures_delta.clear();
    }

    #[test]
    fn minimum_width_hidpi_controls_stay_inside_the_panel() {
        let context = egui::Context::default();
        context.set_pixels_per_point(2.0);
        let mut session = Session::new(&gomoku_core::Settings::default());
        session.toggle_panel_section(PanelSection::Players);
        session.panel_width = 180.0;
        session
            .set_ai_options(
                Color::Black,
                crate::ai::AiOptions {
                    simulations: 10000,
                    temperature: 2.0,
                    ..Default::default()
                },
            )
            .unwrap();
        ui_frame(&context, &mut session, vec![])
            .textures_delta
            .clear();
        let mut output = ui_frame(&context, &mut session, vec![]);
        output.textures_delta.clear();
        assert_eq!(context.pixels_per_point(), 2.0);
        let panel_left = session.viewport_points[0] + session.viewport_points[2];
        for label in ["Release Black", "Take White", "10000", "2.0"] {
            let text = output
                .shapes
                .iter()
                .find_map(|shape| match &shape.shape {
                    egui::Shape::Text(text) if text.galley.text() == label => Some(text),
                    _ => None,
                })
                .expect("control is visible");
            let rect = text.galley.rect.translate(text.pos.to_vec2());
            assert!(
                rect.left() >= panel_left && rect.right() <= 1200.0,
                "{label}: {rect:?}"
            );
        }
        click_text("Release Black", |events| {
            ui_frame(&context, &mut session, events)
        });
        assert_eq!(session.owner(Color::Black), SeatOwner::Ai);
        assert!(!session.on_board());
    }

    #[test]
    fn resume_is_absent_for_review_human_turns_and_finished_games() {
        let context = egui::Context::default();
        let mut session = Session::new(&gomoku_core::Settings::default());
        session.seek(0);
        let absent = |session: &mut Session| {
            let mut output = ui_frame(&context, session, vec![]);
            output.textures_delta.clear();
            assert!(!output.shapes.iter().any(|shape| matches!(&shape.shape,
                egui::Shape::Text(text) if text.galley.text() == "Resume AI")));
        };
        absent(&mut session); // paused but Human Black
        session.place(gomoku_core::point(7, 7).unwrap());
        session.seek(0);
        absent(&mut session); // reviewing
        session.seek(usize::MAX);
        session.resume_automation();
        let request = session.next_ai_request().unwrap();
        session.apply_ai_reply(request.reply(Ok(player::Decision {
            action: player::DecisionAction::Resign(player::ResignReason::VerifiedOpponentVcf {
                candidates: 5,
            }),
            search: None,
            guard: None,
        })));
        session.seek(usize::MAX);
        absent(&mut session); // live but finished
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
    fn continuing_an_unreadable_autosave_keeps_its_error_visible() {
        let context = egui::Context::default();
        let mut session = Session::new(&gomoku_core::Settings::default());
        session.place(gomoku_core::point(7, 7).unwrap());
        let before = session.game().clone();
        session.set_dialog(Dialog::Resume { source: None });
        let path =
            std::env::temp_dir().join(format!("gomoku-failed-resume-{}.json", std::process::id()));
        std::fs::write(&path, "not a game").unwrap();
        let frame = |session: &mut Session, events| {
            context.run_ui(
                egui::RawInput {
                    screen_rect: Some(egui::Rect::from_min_size(
                        egui::Pos2::ZERO,
                        egui::vec2(1200.0, 900.0),
                    )),
                    events,
                    ..Default::default()
                },
                |ui| {
                    dialogs(ui.ctx(), session, &mut Requests::default(), |session| {
                        session.resume_autosave_from(&path, None);
                    });
                },
            )
        };
        click_text("Continue it", |events| frame(&mut session, events));
        assert!(
            matches!(session.dialog(), Some(Dialog::Message { title, .. }) if title == "The autosaved game could not be read")
        );
        assert_eq!(session.game(), &before);
        assert!(session.automation_running());
        frame(&mut session, vec![]).textures_delta.clear();
        assert!(matches!(session.dialog(), Some(Dialog::Message { .. })));
        std::fs::remove_file(path).unwrap();
    }

    #[test]
    fn clicking_a_recent_file_does_not_bypass_unsaved_confirmation() {
        let context = egui::Context::default();
        let mut session = Session::new(&gomoku_core::Settings::default());
        session.place(gomoku_core::point(7, 7).unwrap());
        let before = session.game().clone();
        let path = PathBuf::from("recent-unsaved-game.json");
        session.recent = vec![path.clone()];
        click_text("recent-unsaved-game.json", |events| {
            context.run_ui(
                egui::RawInput {
                    screen_rect: Some(egui::Rect::from_min_size(
                        egui::Pos2::ZERO,
                        egui::vec2(1200.0, 900.0),
                    )),
                    events,
                    ..Default::default()
                },
                |ui| recents(ui, &mut session),
            )
        });
        assert_eq!(
            session.dialog(),
            Some(&Dialog::Unsaved {
                then: After::OpenRecent(path)
            })
        );
        assert_eq!(session.game(), &before);
        assert!(session.take_action().is_none());
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
            Dialog::PickPuzzle {
                puzzles: vec![gomoku_core::Puzzle {
                    name: "test #0".to_string(),
                    black: vec![point],
                    white: vec![],
                    to_move: gomoku_core::Color::Black,
                    solution: None,
                    depth: Some(3),
                }],
                index: 0,
                before: Box::new(crate::session::BeforePuzzle::from(&Session::new(
                    &gomoku_core::Settings::default(),
                ))),
            },
        ];
        for dialog in dialogs {
            let mut session = Session::new(&gomoku_core::Settings::default());
            session.set_dialog(dialog);
            // The interface must draw it without complaint, over the board.
            let (_, claim) = run(&mut session, egui::pos2(600.0, 450.0));
            assert!(
                claim.over_egui,
                "a dialog takes the pointer while it is open"
            );
        }
    }

    #[test]
    fn the_picker_blocks_background_menu_actions() {
        let mut session = Session::new(&gomoku_core::Settings::default());
        session.set_dialog(Dialog::PickPuzzle {
            puzzles: vec![gomoku_core::Puzzle {
                name: "test".to_string(),
                black: vec![],
                white: vec![],
                to_move: Color::Black,
                solution: None,
                depth: None,
            }],
            index: 0,
            before: Box::new(crate::session::BeforePuzzle::from(&session)),
        });
        let context = egui::Context::default();
        // Open File, then click its first action, well outside the picker.
        for (position, pressed) in [
            (egui::pos2(20.0, 14.0), None),
            (egui::pos2(20.0, 14.0), Some(true)),
            (egui::pos2(20.0, 14.0), Some(false)),
            (egui::pos2(40.0, 40.0), None),
            (egui::pos2(40.0, 40.0), Some(true)),
            (egui::pos2(40.0, 40.0), Some(false)),
        ] {
            let mut events = vec![egui::Event::PointerMoved(position)];
            if let Some(pressed) = pressed {
                events.push(egui::Event::PointerButton {
                    pos: position,
                    button: egui::PointerButton::Primary,
                    pressed,
                    modifiers: egui::Modifiers::NONE,
                });
            }
            let mut output = context.run_ui(
                egui::RawInput {
                    screen_rect: Some(egui::Rect::from_min_size(
                        egui::Pos2::ZERO,
                        egui::vec2(1600.0, 1000.0),
                    )),
                    events,
                    ..Default::default()
                },
                |ui| assert_eq!(draw(ui, &mut session), Requests::default()),
            );
            output.textures_delta.clear();
        }
        assert!(matches!(session.dialog(), Some(Dialog::PickPuzzle { .. })));
        let picker = session.dialog().cloned();
        // Disabling the background must not disable the floating picker itself.
        let mut output = context.run_ui(
            egui::RawInput {
                events: vec![egui::Event::Key {
                    key: egui::Key::Escape,
                    physical_key: None,
                    pressed: true,
                    repeat: false,
                    modifiers: egui::Modifiers::NONE,
                }],
                ..Default::default()
            },
            |ui| {
                let _ = draw(ui, &mut session);
            },
        );
        output.textures_delta.clear();
        assert!(session.dialog().is_none());

        // Its pointer controls must remain active too, not just its keys.
        session.set_dialog(picker.unwrap());
        let mut output = context.run_ui(egui::RawInput::default(), |ui| {
            let _ = draw(ui, &mut session);
        });
        let start = output.shapes.iter().find_map(|shape| match &shape.shape {
            egui::Shape::Text(text) if text.galley.text() == "Start solving" => {
                Some(text.pos + text.galley.size() * 0.5)
            }
            _ => None,
        });
        output.textures_delta.clear();
        let start = start.expect("the picker draws its confirmation button");
        for pressed in [true, false] {
            let mut output = context.run_ui(
                egui::RawInput {
                    events: vec![
                        egui::Event::PointerMoved(start),
                        egui::Event::PointerButton {
                            pos: start,
                            button: egui::PointerButton::Primary,
                            pressed,
                            modifiers: egui::Modifiers::NONE,
                        },
                    ],
                    ..Default::default()
                },
                |ui| {
                    let _ = draw(ui, &mut session);
                },
            );
            output.textures_delta.clear();
        }
        assert!(session.dialog().is_none());
        assert!(session.game().is_from_position());
    }

    /// A game that the given side resigns, through the same AI reply path the
    /// application uses, so the banner is tested against a real outcome.
    fn resigned_session(resigner: Color) -> Session {
        let mut session = Session::new(&gomoku_core::Settings::default());
        if resigner == Color::White {
            session.place(gomoku_core::point(7, 7).unwrap());
        } else {
            session.release_seat(Color::Black);
        }
        let request = session.next_ai_request().unwrap();
        session.apply_ai_reply(request.reply(Ok(player::Decision {
            action: player::DecisionAction::Resign(player::ResignReason::VerifiedOpponentVcf {
                candidates: 5,
            }),
            search: None,
            guard: None,
        })));
        session
    }

    /// The banner as drawn: the red rect and the text it carries, if any.
    fn banner(output: &egui::FullOutput) -> Option<(egui::Rect, egui::epaint::TextShape)> {
        let fill = output.shapes.iter().find_map(|shape| match &shape.shape {
            egui::Shape::Rect(rect) if rect.fill == BANNER => Some(rect.rect),
            _ => None,
        })?;
        let text = output.shapes.iter().find_map(|shape| match &shape.shape {
            egui::Shape::Text(text)
                if fill.contains(text.pos + text.galley.rect.center().to_vec2()) =>
            {
                Some(text.clone())
            }
            _ => None,
        })?;
        Some((fill, text))
    }

    #[test]
    fn resignation_banner_is_big_red_exact_and_persistent() {
        let context = egui::Context::default();
        for (resigner, line) in [
            (Color::White, "White resigned — Black wins"),
            (Color::Black, "Black resigned — White wins"),
        ] {
            let mut session = resigned_session(resigner);
            let mut output = ui_frame(&context, &mut session, vec![]);
            let (fill, text) = banner(&output).expect("a resigned game shows the banner");
            output.textures_delta.clear();
            assert_eq!(text.galley.text(), line);
            let format = &text.galley.job.sections[0].format;
            assert!(
                format.font_id.size >= 24.0,
                "the banner shouts: {}pt",
                format.font_id.size
            );
            assert_eq!(
                format.color,
                Color32::WHITE,
                "white on red reads at a glance"
            );
            // The banner sits between the menu bar and the board, and the board
            // knows it: the viewport starts where the banner ends (egui keeps a
            // one-point separator line between panels).
            let top = session.viewport_points[1];
            assert!(
                top >= fill.bottom() && top <= fill.bottom() + 2.0,
                "board viewport {:?} starts under the banner {fill:?}",
                session.viewport_points
            );
            // A pointer over the banner is the interface's, not the game's.
            session.pointer = [fill.center().x, fill.center().y];
            assert!(!session.on_board());
            // It is persistent: reviewing an earlier move keeps it, and drawing
            // it never disturbs the game or the search.
            session.seek(0);
            let mut output = ui_frame(&context, &mut session, vec![]);
            assert!(
                banner(&output).is_some(),
                "the banner stays while the resignation is reviewed"
            );
            output.textures_delta.clear();
            let generation = session.generation();
            for _ in 0..5 {
                ui_frame(&context, &mut session, vec![])
                    .textures_delta
                    .clear();
            }
            assert_eq!(session.generation(), generation);
        }
    }

    #[test]
    fn no_banner_for_ongoing_five_or_draw() {
        // The gate itself: only a resignation earns the banner.
        assert_eq!(resignation_banner(Outcome::Ongoing), None);
        assert_eq!(resignation_banner(Outcome::Draw), None);
        assert_eq!(
            resignation_banner(Outcome::Won {
                winner: Color::Black,
                method: WinMethod::Five
            }),
            None
        );

        // And through the interface, for the states a test can reach by play:
        // no banner, and no blank space where it would be.
        let context = egui::Context::default();
        let mut session = Session::new(&gomoku_core::Settings::default());
        let mut output = ui_frame(&context, &mut session, vec![]);
        output.textures_delta.clear();
        assert!(banner(&output).is_none(), "an ongoing game has no banner");
        assert_eq!(
            session.viewport_points[1], 28.0,
            "the board starts right under the menu bar"
        );

        session.take_seat(Color::White);
        for row in 3..8 {
            session.place(gomoku_core::point(row, 7).unwrap());
            if row < 7 {
                session.place(gomoku_core::point(row, 0).unwrap());
            }
        }
        assert!(matches!(
            session.game().outcome(),
            Outcome::Won {
                method: WinMethod::Five,
                ..
            }
        ));
        let mut output = ui_frame(&context, &mut session, vec![]);
        output.textures_delta.clear();
        assert!(banner(&output).is_none(), "a win by five has no banner");
        assert_eq!(session.viewport_points[1], 28.0);
    }

    #[test]
    fn the_banner_wraps_when_the_window_is_narrow() {
        let context = egui::Context::default();
        let mut session = resigned_session(Color::White);
        let mut output = ui_frame_sized(&context, &mut session, vec![], egui::vec2(360.0, 900.0));
        let (_, text) = banner(&output).expect("the banner shows at any width");
        output.textures_delta.clear();
        assert!(
            text.galley.rows.len() > 1,
            "the text wraps instead of clipping: {:?}",
            text.galley.rect
        );
    }

    #[test]
    fn parse_puzzle_index_accepts_valid_indices() {
        assert_eq!(parse_puzzle_index("0", 3), Some(0));
        assert_eq!(parse_puzzle_index("2", 3), Some(2));
    }

    fn explorer_session() -> Session {
        use gomoku_core::{IndexEntry, RecordKind};
        let mut session = Session::new(&gomoku_core::Settings::default());
        session.open_explorer_corpus(vec![
            IndexEntry {
                index: 0,
                kind: RecordKind::Game,
                name: "one.psq".into(),
                path: PathBuf::from("one.psq"),
            },
            IndexEntry {
                index: 1,
                kind: RecordKind::Game,
                name: "two.psq".into(),
                path: PathBuf::from("two.psq"),
            },
            IndexEntry {
                index: 2,
                kind: RecordKind::Game,
                name: "three.psq".into(),
                path: PathBuf::from("three.psq"),
            },
        ]);
        session
    }

    fn explorer_frame(
        context: &egui::Context,
        session: &mut Session,
        requests: &std::cell::Cell<Requests>,
        events: Vec<egui::Event>,
        time: Option<f64>,
    ) -> egui::FullOutput {
        context.run_ui(
            egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(
                    egui::Pos2::ZERO,
                    egui::vec2(1200.0, 900.0),
                )),
                events,
                time,
                ..Default::default()
            },
            |ui| {
                let next = draw(ui, session);
                requests.set(Requests {
                    explorer_select: next.explorer_select.or(requests.get().explorer_select),
                    ..next
                });
            },
        )
    }

    #[test]
    fn explorer_dialog_owns_arrow_and_enter_keys() {
        let context = egui::Context::default();
        let mut session = explorer_session();
        session.select_explorer_record(0);
        session.set_dialog(Dialog::Message {
            title: "Wait".into(),
            body: "Modal".into(),
        });
        let requests = std::cell::Cell::new(Requests::default());

        explorer_frame(
            &context,
            &mut session,
            &requests,
            vec![
                key(egui::Key::ArrowDown, egui::Modifiers::NONE),
                key(egui::Key::Enter, egui::Modifiers::NONE),
            ],
            None,
        )
        .textures_delta
        .clear();

        assert_eq!(session.explorer().unwrap().selected, Some(0));
        assert_eq!(requests.get().explorer_select, None);
        assert_eq!(session.open_panel_section(), PanelSection::Explorer);
    }

    #[test]
    fn explorer_opposite_arrows_clamp_in_order_from_the_first_row() {
        let context = egui::Context::default();
        let mut session = explorer_session();
        session.select_explorer_record(0);
        let requests = std::cell::Cell::new(Requests::default());

        explorer_frame(
            &context,
            &mut session,
            &requests,
            vec![
                key(egui::Key::ArrowUp, egui::Modifiers::NONE),
                key(egui::Key::ArrowDown, egui::Modifiers::NONE),
            ],
            None,
        )
        .textures_delta
        .clear();

        assert_eq!(session.explorer().unwrap().selected, Some(1));
        assert_eq!(requests.get().explorer_select, Some(1));
    }

    #[test]
    fn explorer_opposite_arrows_clamp_in_order_from_the_last_row() {
        let context = egui::Context::default();
        let mut session = explorer_session();
        session.select_explorer_record(2);
        let requests = std::cell::Cell::new(Requests::default());

        explorer_frame(
            &context,
            &mut session,
            &requests,
            vec![
                key(egui::Key::ArrowDown, egui::Modifiers::NONE),
                key(egui::Key::ArrowUp, egui::Modifiers::NONE),
            ],
            None,
        )
        .textures_delta
        .clear();

        assert_eq!(session.explorer().unwrap().selected, Some(1));
        assert_eq!(requests.get().explorer_select, Some(1));
    }

    #[test]
    fn explorer_arrow_down_loads_the_next_visible_record() {
        let context = egui::Context::default();
        let mut session = explorer_session();
        session.set_explorer_filter("t".into());
        session.select_explorer_record(1);
        let requests = std::cell::Cell::new(Requests::default());

        explorer_frame(
            &context,
            &mut session,
            &requests,
            vec![key(egui::Key::ArrowDown, egui::Modifiers::NONE)],
            None,
        )
        .textures_delta
        .clear();

        assert_eq!(session.explorer().unwrap().selected, Some(2));
        assert_eq!(requests.get().explorer_select, Some(2));
        assert_eq!(session.open_panel_section(), PanelSection::Explorer);
    }

    #[test]
    fn explorer_enter_hands_keyboard_to_moves() {
        let context = egui::Context::default();
        let mut session = explorer_session();
        let requests = std::cell::Cell::new(Requests::default());
        explorer_frame(&context, &mut session, &requests, vec![], None)
            .textures_delta
            .clear();
        context.memory_mut(|memory| {
            memory.request_focus(egui::Id::new("explorer-filter"));
        });
        assert!(context.egui_wants_keyboard_input());

        explorer_frame(
            &context,
            &mut session,
            &requests,
            vec![key(egui::Key::Enter, egui::Modifiers::NONE)],
            None,
        )
        .textures_delta
        .clear();

        assert_eq!(session.open_panel_section(), PanelSection::Moves);
        assert!(
            !context.egui_wants_keyboard_input(),
            "Left/Right must reach the application's move navigation"
        );
        assert_eq!(requests.get().explorer_select, None);
    }

    #[test]
    fn explorer_rows_select_on_single_click_without_loading() {
        let context = egui::Context::default();
        let mut session = explorer_session();
        let requests = std::cell::Cell::new(Requests::default());
        click_text("    1 two.psq", |events| {
            explorer_frame(&context, &mut session, &requests, events, None)
        });
        assert_eq!(session.explorer().unwrap().selected, Some(1));
        assert_eq!(requests.get().explorer_select, None);
        assert!(session.game().is_empty());
    }

    #[test]
    fn explorer_rows_emit_load_requests_on_double_click() {
        let context = egui::Context::default();
        let mut session = explorer_session();
        let requests = std::cell::Cell::new(Requests::default());
        double_click_text("    1 two.psq", |events, time| {
            explorer_frame(&context, &mut session, &requests, events, Some(time))
        });
        assert_eq!(requests.get().explorer_select, Some(1));
        assert_eq!(session.open_panel_section(), PanelSection::Moves);
        assert!(session.game().is_empty());
    }

    #[test]
    fn parse_puzzle_index_rejects_out_of_range_and_garbage() {
        assert_eq!(parse_puzzle_index("3", 3), None);
        assert_eq!(parse_puzzle_index("-1", 3), None);
        assert_eq!(parse_puzzle_index("abc", 3), None);
        assert_eq!(parse_puzzle_index("1.5", 3), None);
    }
}
