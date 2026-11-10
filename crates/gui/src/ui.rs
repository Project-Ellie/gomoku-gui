//! The interface: the menus, the side panel, the board overlays, and the dialogs.
//!
//! It is drawn with egui, on top of the board, in one pass after the resolve.
//! Text and markers are placed by projecting board points through the camera, so
//! they stay attached to the board at any zoom.

use egui::{Align2, Color32, FontId, Frame, RichText, Stroke, Vec2};
use gomoku_core::{Color, Outcome, WinMethod, label};

use crate::ai::SeatOwner;
use crate::render::WOOD;
use crate::session::{Dialog, PanelSection, Session, name_of, winning_line};
use crate::theme::{
    BANNER, BANNER_FONT_SIZE, MENU_BAR_HEIGHT, MENU_FONT_SIZE, MOVE_LIST_FONT_SIZE,
    MOVE_LIST_STONE_DOT_SIZE, MUTED, PANEL, PANEL_CURRENT, PANEL_HEADER, PANEL_INK, PANEL_MARGIN,
    RECENT_FONT_SIZE, SECTION_HEADER_FONT_SIZE, STONE_DOT_SIZE, SURFACE,
};

/// What the interface asks the application to do once the frame is drawn. The
/// application owns the file dialogs and the window, so it performs these.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct Requests {
    /// Start a new game immediately.
    pub new_game: bool,
    /// Ask for a file and open it.
    pub open: bool,
    /// Ask for a puzzle file and load it.
    pub open_puzzle: bool,
    /// Ask for a corpus folder and open it in the Lab Explorer.
    pub open_explorer: bool,
    /// Reopen the last corpus folder without asking for one.
    pub reopen_explorer: bool,
    /// Save to the open file, or ask for one.
    pub save: bool,
    /// Ask for a file and save to it.
    pub save_as: bool,
    /// Close the application.
    pub quit: bool,
    /// Load the record with this index from the Explorer into the board.
    pub explorer_select: Option<usize>,
}

/// The puzzle-start cursor ring.
const PUZZLE_CURSOR: Color32 = Color32::from_rgb(50, 232, 90);
/// The colour of a black stone in the move list.
const SLATE_MARK: Color32 = Color32::from_rgb(22, 22, 26);
/// The colour of a white stone in the move list.
const SHELL_MARK: Color32 = Color32::from_rgb(246, 246, 250);
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

/// Text for a main-menu label or submenu entry at the shared doubled size.
fn menu_text(text: impl Into<String>) -> RichText {
    RichText::new(text).font(FontId::proportional(MENU_FONT_SIZE))
}

/// Draw one accordion header and report whether its section is open.
fn panel_section_header(
    ui: &mut egui::Ui,
    session: &mut Session,
    section: PanelSection,
    title: &str,
    font_size: f32,
) -> bool {
    // A full-width strip, so the header reads as a control. It ignores the
    // panel's padding and runs from border to border.
    let (rect, _) = ui.allocate_exact_size(
        Vec2::new(ui.available_width(), font_size * 1.9),
        egui::Sense::hover(),
    );
    let strip = rect.expand2(Vec2::new(PANEL_MARGIN as f32, 0.0));
    ui.painter()
        .rect_filled(strip, egui::CornerRadius::ZERO, PANEL_HEADER);
    let response = ui
        .put(
            rect.shrink2(Vec2::new(2.0, 0.0)),
            egui::Label::new(
                RichText::new(title)
                    .font(crate::theme::bold(font_size))
                    .color(Color32::WHITE),
            )
            .sense(egui::Sense::click()),
        )
        .on_hover_cursor(egui::CursorIcon::PointingHand);
    if response.clicked() {
        session.toggle_panel_section(section);
    }
    session.open_panel_section() == section
}

/// Draw a full-width boolean option and report whether it was clicked.
/// Every boolean option looks like this: twice the body size, 40 points tall.
fn on_off_button(ui: &mut egui::Ui, label: &str, state: bool, hover: &str) -> bool {
    let state = if state { "ON" } else { "OFF" };
    ui.add_sized(
        [ui.available_width(), 40.0],
        egui::Button::new(
            RichText::new(format!("{label} {state}")).size(crate::theme::BODY_FONT_SIZE * 2.0),
        ),
    )
    .on_hover_cursor(egui::CursorIcon::PointingHand)
    .on_hover_text(hover)
    .clicked()
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

    egui::Panel::top("bar")
        .exact_size(MENU_BAR_HEIGHT)
        .frame(
            Frame::NONE
                .fill(SURFACE)
                .inner_margin(egui::Margin::symmetric(12, 0)),
        )
        .show(ui, |ui| {
            // Let the menu row fill the bar's height, so its labels center
            // vertically instead of hugging the top edge.
            ui.spacing_mut().interact_size.y = MENU_BAR_HEIGHT;
            egui::MenuBar::new().ui(ui, |ui| {
                ui.menu_button(menu_text("File"), |ui| {
                    if ui.button(menu_text("New game")).clicked() {
                        requests.new_game = true;
                        ui.close();
                    }
                    if ui.button(menu_text("Open...")).clicked() {
                        requests.open = true;
                        ui.close();
                    }
                    if ui.button(menu_text("Load puzzle...")).clicked() {
                        requests.open_puzzle = true;
                        ui.close();
                    }
                    ui.separator();
                    if ui.button(menu_text("Save")).clicked() {
                        requests.save = true;
                        ui.close();
                    }
                    if ui.button(menu_text("Save as...")).clicked() {
                        requests.save_as = true;
                        ui.close();
                    }
                    ui.separator();
                    if ui.button(menu_text("Quit")).clicked() {
                        requests.quit = true;
                        ui.close();
                    }
                });
                ui.menu_button(menu_text("Edit"), |ui| {
                    if ui.button(menu_text("Undo")).clicked() {
                        session.undo();
                        ui.close();
                    }
                });
                ui.menu_button(menu_text("View"), |ui| {
                    if ui.button(menu_text("Fit the board")).clicked() {
                        session.fit();
                        ui.close();
                    }
                    if ui.button(menu_text("Turn the board around")).clicked() {
                        session.flip();
                        ui.close();
                    }
                    ui.separator();
                    ui.checkbox(&mut session.toggles.coordinates, menu_text("Coordinates"));
                    ui.checkbox(&mut session.toggles.move_numbers, menu_text("Move numbers"));
                    ui.checkbox(&mut session.toggles.last_move, menu_text("Last move"));
                    ui.checkbox(&mut session.toggles.win_line, menu_text("Winning line"));
                    ui.separator();
                    ui.checkbox(&mut session.sound, menu_text("Sound"));
                    ui.add(
                        egui::Slider::new(&mut session.volume, 0.0..=1.0)
                            .text(menu_text("Volume"))
                            .show_value(false),
                    );
                    ui.add(
                        egui::Slider::new(&mut session.gain, 1.0..=6.0)
                            .text(menu_text("Wood lightness"))
                            .show_value(false),
                    );
                    if ui.button(menu_text("Reset the wood lightness")).clicked() {
                        session.gain = WOOD.gain;
                        ui.close();
                    }
                });
                ui.menu_button(menu_text("Go"), |ui| {
                    if ui.button(menu_text("To the start")).clicked() {
                        session.seek(0);
                        ui.close();
                    }
                    if ui.button(menu_text("Back one move")).clicked() {
                        session.rewind();
                        ui.close();
                    }
                    if ui.button(menu_text("Forward one move")).clicked() {
                        session.forward();
                        ui.close();
                    }
                    if ui.button(menu_text("To the latest move")).clicked() {
                        session.seek(usize::MAX);
                        ui.close();
                    }
                });
                ui.menu_button(menu_text("Lab"), |ui| {
                    if ui.button(menu_text("Explorer...")).clicked() {
                        requests.open_explorer = true;
                        ui.close();
                    }
                    let reopen = session
                        .last_corpus_directory
                        .as_ref()
                        .and_then(|path| path.file_name())
                        .map(|name| format!("Reopen {}", name.to_string_lossy()));
                    if ui
                        .add_enabled(
                            reopen.is_some(),
                            egui::Button::new(menu_text(
                                reopen.unwrap_or_else(|| "Reopen last corpus".into()),
                            )),
                        )
                        .on_hover_text(
                            "Open the last corpus folder again, without the folder picker.",
                        )
                        .clicked()
                    {
                        requests.reopen_explorer = true;
                        ui.close();
                    }
                });
                // The status line anchors to the right end of the bar, above
                // the side panel, at the same size as the menu entries.
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    ui.label(
                        RichText::new(status_line(session))
                            .font(FontId::proportional(MENU_FONT_SIZE))
                            .color(MUTED),
                    );
                });
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
                                .font(FontId::proportional(BANNER_FONT_SIZE))
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
        .frame(Frame::NONE.fill(PANEL).inner_margin(PANEL_MARGIN))
        .show(ui, |ui| {
            // Exactly one section is open; all bodies inherit the same light
            // theme, so no section needs its own dark-theme colour corrections.
            let header_size = SECTION_HEADER_FONT_SIZE;
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
            ui.separator();
            if panel_section_header(
                ui,
                session,
                PanelSection::Policy,
                "Display options",
                header_size,
            ) {
                body_paints.push(paint_panel_body(ui, PanelSection::Policy, |ui| {
                    if on_off_button(
                        ui,
                        "Policy overlay",
                        session.policy_overlay,
                        "Show the baseline prior for the side to move on empty crossings.",
                    ) {
                        session.set_policy_overlay(!session.policy_overlay);
                    }
                    if on_off_button(
                        ui,
                        "Threat sequences",
                        session.threat_sequences,
                        "Show certified forced-win threat sequences as numbered circles.",
                    ) {
                        session.threat_sequences = !session.threat_sequences;
                    }
                    if on_off_button(
                        ui,
                        "Branch alternatives",
                        session.branch_alternatives,
                        "Show branch alternatives as translucent stones and select them on the board.",
                    ) {
                        session.branch_alternatives = !session.branch_alternatives;
                    }
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
    session.refresh_threat_sequences();

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
            } else if area_changed {
                // The board's size belongs to the window: the board grows and
                // shrinks with the area it is drawn into, always fitted, with
                // a margin of the marble table around it.
                session.camera.refit(session.viewport);
            }
            // Turn/ownership changes also refresh a stationary pointer.
            session.update_hover();
            let painter = ui.painter().clone();
            overlays(&painter, session, scale);
            // Nothing here senses the pointer: a click on the board is the game's,
            // and the application tests for it with `Session::on_board`.
        });

    dialogs(ui.ctx(), session);
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
                && !session.game().is_rewound()
            {
                return format!("{side} · Thinking…");
            }
            let notice = session
                .notice
                .as_ref()
                .map(|text| format!(" — {text}"))
                .unwrap_or_default();
            format!(
                "{side} to play — move {}{review}{notice}",
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
    // The Players body reads at twice the body size; only the section title
    // above stays as is. The scope keeps the enlargement from leaking into
    // the panel sections painted after this one.
    ui.scope(|ui| {
        for font in ui.style_mut().text_styles.values_mut() {
            font.size *= 2.0;
        }
        ui.spacing_mut().interact_size.y = 40.0;
        for (side, label) in [(Color::Black, "Black"), (Color::White, "White")] {
            ui.push_id(label, |ui| {
                Frame::NONE.show(ui, |ui| {
                    let human = session.owner(side) == SeatOwner::Human;
                    ui.horizontal(|ui| {
                        stone_dot(ui, side, STONE_DOT_SIZE);
                        ui.label(RichText::new(label).strong().color(PANEL_INK));
                        ui.label(if human {
                            "Human"
                        } else if session.ai_thinking(side) {
                            "AI · thinking…"
                        } else {
                            "AI · baseline"
                        });
                    });
                    let mut name = match side {
                        Color::Black => session.game().meta().black.clone(),
                        Color::White => session.game().meta().white.clone(),
                    }.unwrap_or_default();
                    if ui.add(egui::TextEdit::singleline(&mut name)
                        .id_salt("name")
                        .desired_width(ui.available_width())
                        .hint_text("Player name"))
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
                    if ui.add_sized([ui.available_width(), 40.0], egui::Button::new(format!("{action} {label}")))
                        .on_hover_cursor(egui::CursorIcon::PointingHand)
                        .on_hover_text(help).clicked()
                    {
                        if human { session.release_seat(side); } else { session.take_seat(side); }
                    }
                    let mut options = session.ai_options(side);
                    let before = options;
                    let field_width = (ui.available_width() - 12.0) / 2.0;
                    egui::Grid::new("options").spacing([12.0, 3.0]).show(ui, |ui| {
                        ui.label("Sims").on_hover_text("Retained for this color, even while human. 1–50,000 simulations per decision.");
                        ui.label("Temp");
                        ui.end_row();
                        ui.add_sized([field_width, 40.0], egui::DragValue::new(&mut options.simulations).range(1..=50_000).speed(1));
                        ui.add_sized([field_width, 40.0], egui::DragValue::new(&mut options.temperature).range(0.0..=2.0)
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
        let initiative_side = session.game().to_move();
        let initiative_label = if initiative_side == Color::Black {
            "AI plays Black"
        } else {
            "AI plays White"
        };
        let can_move = session.game().outcome() == Outcome::Ongoing && session.game().can_play();
        if ui
            .add_enabled(
                can_move,
                egui::Button::new(initiative_label)
                    .min_size(egui::vec2(ui.available_width(), 40.0)),
            )
            .on_hover_cursor(egui::CursorIcon::PointingHand)
            .on_hover_text(
                "Hands the side to move to the AI and starts thinking immediately: \
                 abandons any stuck flight and works even mid-review.",
            )
            .clicked()
        {
            session.hand_initiative_to_ai();
        }
        let resignation = session.resignation_enabled();
        if on_off_button(
            ui,
            "Resignation",
            resignation,
            "Resignation is a heuristic concession by the AI, not a proof of a forced win. \
             This switch applies to both AI seats.",
        ) {
            session.set_resignation(!resignation);
        }
    });
}

/// The list of moves, one row each, with a click to look at that position.
/// The section header above it carries the title.
fn move_list(ui: &mut egui::Ui, session: &mut Session) {
    if session.game().outcome() != Outcome::Ongoing {
        ui.label(RichText::new(format!("Live result: {}", status_line(session))).color(PANEL_INK));
    }
    let cursor = session.game().cursor();
    let branch_options = session.is_branch_point().then(|| session.branch_options());
    let selected_branch = session.selected_branch();
    let branch_colour = session.game().to_move();
    let mut seek = None;
    let mut select_branch = None;
    // The whole game, so that the moves ahead of the cursor stay visible. They
    // are dimmed, because they are not on the board yet.
    let record: Vec<(gomoku_core::Move, Color)> = session.game().record().collect();
    let siblings = session.game().moves_with_siblings();
    egui::ScrollArea::vertical()
        .auto_shrink([false, false])
        .max_height((ui.available_height() - 150.0).max(80.0))
        .show(ui, |ui| {
            for (index, (point, colour)) in record.into_iter().enumerate() {
                let number = index + 1;
                if index == cursor {
                    if let Some(options) = &branch_options {
                        // Replace the selected line's next row with all child choices,
                        // rather than drawing its selected child a second time.
                        for (branch, point) in options.iter().enumerate() {
                            let ink = RichText::new(format!("{number:>4}"))
                                .size(MOVE_LIST_FONT_SIZE)
                                .color(MUTED);
                            let place = RichText::new(format!("[{branch}] {}", label(*point)))
                                .size(MOVE_LIST_FONT_SIZE)
                                .color(PANEL_INK);
                            // The whole row is the click target, including its text.
                            let row = if selected_branch == Some(branch) {
                                Frame::NONE
                                    .fill(PANEL_CURRENT)
                                    .inner_margin(2.0)
                                    .show(ui, |ui| {
                                        ui.horizontal(|ui| {
                                            ui.add(
                                                egui::Label::new(ink.strong()).selectable(false),
                                            );
                                            stone_dot(ui, branch_colour, MOVE_LIST_STONE_DOT_SIZE);
                                            ui.add(
                                                egui::Label::new(place.strong()).selectable(false),
                                            );
                                        });
                                    })
                                    .response
                            } else {
                                ui.horizontal(|ui| {
                                    ui.add(egui::Label::new(ink).selectable(false));
                                    stone_dot(ui, branch_colour, MOVE_LIST_STONE_DOT_SIZE);
                                    ui.add(egui::Label::new(place).selectable(false));
                                })
                                .response
                            };
                            if row.interact(egui::Sense::click()).clicked() {
                                select_branch = Some(branch);
                            }
                        }
                        continue;
                    }
                }
                let here = number == cursor;
                let ink = if number > cursor {
                    RichText::new(format!("{number:>4}"))
                        .size(MOVE_LIST_FONT_SIZE)
                        .color(MUTED)
                } else {
                    RichText::new(format!("{number:>4}"))
                        .size(MOVE_LIST_FONT_SIZE)
                        .color(PANEL_INK)
                };
                // A move that was one of several options carries ",…".
                let marker = if siblings.get(index).copied().unwrap_or(false) {
                    ",…"
                } else {
                    ""
                };
                let place = RichText::new(format!("{}{marker}", label(point)))
                    .size(MOVE_LIST_FONT_SIZE)
                    .color(PANEL_INK);
                if here {
                    let row = Frame::NONE
                        .fill(PANEL_CURRENT)
                        .inner_margin(2.0)
                        .show(ui, |ui| {
                            ui.horizontal(|ui| {
                                ui.label(ink.strong());
                                stone_dot(ui, colour, MOVE_LIST_STONE_DOT_SIZE);
                                ui.label(place.strong());
                            });
                        });
                    if row.response.interact(egui::Sense::click()).clicked() {
                        seek = Some(number);
                    }
                } else {
                    let row = ui.horizontal(|ui| {
                        ui.label(ink);
                        stone_dot(ui, colour, MOVE_LIST_STONE_DOT_SIZE);
                        ui.label(place);
                    });
                    if row.response.interact(egui::Sense::click()).clicked() {
                        seek = Some(number);
                    }
                }
            }
        });
    if let Some(branch) = select_branch {
        session.select_branch(branch);
    }
    if let Some(number) = seek {
        session.seek(number);
    }
}

/// A filled circle in the colour of a stone.
///
/// It is drawn rather than typed: the interface font has no filled circle glyph,
/// so typing one shows an empty box. A drawn circle can also carry the thin
/// outline that each stone needs against the row behind it.
fn stone_dot(ui: &mut egui::Ui, colour: Color, size: f32) {
    let scale = size / STONE_DOT_SIZE;
    let (rect, _) = ui.allocate_exact_size(egui::vec2(size, size), egui::Sense::hover());
    let radius = size * 0.42;
    let centre = rect.center();
    let painter = ui.painter();
    let (fill, outline) = match colour {
        Color::Black => (SLATE_MARK, Color32::from_rgb(150, 150, 158)),
        Color::White => (SHELL_MARK, Color32::from_rgb(64, 64, 70)),
    };
    painter.circle_filled(centre, radius, fill);
    painter.circle_stroke(centre, radius, Stroke::new(0.8 * scale, outline));
}

/// The recently used files.
fn recents(ui: &mut egui::Ui, session: &mut Session) {
    if session.recent.is_empty() {
        return;
    }
    // The divider belongs to this section, so that it does not hang in the air
    // when there are no recent files.
    ui.separator();
    ui.label(
        RichText::new("Recent")
            .font(FontId::proportional(SECTION_HEADER_FONT_SIZE))
            .color(PANEL_INK),
    );
    let mut open = None;
    for path in session.recent.clone().into_iter().take(6) {
        if ui
            .link(RichText::new(name_of(&path)).size(RECENT_FONT_SIZE))
            .clicked()
        {
            open = Some(path);
        }
    }
    if let Some(path) = open {
        session.open(&path);
    }
}

/// Branch alternatives are navigation hints, not stones: a present but
/// translucent fill keeps them clearly below the real stones, and a crisp
/// outline keeps them legible at any zoom.
const BRANCH_ALTERNATIVE_FILL: u8 = 72;
const BRANCH_ALTERNATIVE_LINE: u8 = 200;

/// The weakest two of the twenty shades say nothing, so they are not drawn at
/// all: a candidate at or below this share of the strongest gets no ring.
const POLICY_RING_FLOOR: f32 = 2.0 / 20.0;

/// Neutral grey behind the black and white threat-sequence numbers.
const THREAT_CIRCLE: Color32 = Color32::from_rgb(128, 128, 128);

/// One of twenty shades of green for the policy overlay. The value is
/// relative to the strongest candidate: 1.0 is a deep, saturated green and
/// weaker candidates fade towards a pale whisper.
fn policy_colour(value: f32) -> Color32 {
    let v = value.clamp(0.0, 1.0);
    egui::ecolor::Hsva::new(0.38, 0.30 + 0.65 * v, 0.95 - 0.45 * v, 0.30 + 0.55 * v).into()
}

/// The ghost stone for an alternative continuation: `alpha` for the fill,
/// `BRANCH_ALTERNATIVE_LINE` for the outline.
fn branch_alternative_colour(side: Color, alpha: u8) -> Color32 {
    let stone = match side {
        Color::Black => SLATE_MARK,
        Color::White => SHELL_MARK,
    };
    Color32::from_rgba_unmultiplied(stone.r(), stone.g(), stone.b(), alpha)
}

/// The markers and the winning line, drawn on top of the board.
fn overlays(painter: &egui::Painter, session: &Session, pixels_per_point: f32) {
    let ppc = session.camera.pixels_per_cell;
    let font = FontId::monospace((ppc * 0.40 * 0.6 / pixels_per_point).clamp(5.4, 16.8));
    let to_screen = |board: [f32; 2]| -> egui::Pos2 {
        let pixel = session.camera.to_screen(session.viewport, board);
        egui::Pos2::new(pixel[0] / pixels_per_point, pixel[1] / pixels_per_point)
    };

    // Alternative continuations look like translucent stones with a crisp
    // outline; the one the move list currently follows wears a solid pupil.
    if session.branch_alternatives && session.is_branch_point() {
        let selected = session.selected_branch();
        let side = session.game().to_move();
        let fill = branch_alternative_colour(side, BRANCH_ALTERNATIVE_FILL);
        let line = branch_alternative_colour(side, BRANCH_ALTERNATIVE_LINE);
        for (index, point) in session.branch_options().into_iter().enumerate() {
            let centre = to_screen([point.col() as f32, point.row() as f32]);
            let radius = ppc * 0.44 / pixels_per_point;
            painter.circle_filled(centre, radius, fill);
            painter.circle_stroke(
                centre,
                radius,
                Stroke::new((ppc * 0.03 / pixels_per_point).max(1.5), line),
            );
            if selected == Some(index) {
                painter.circle_filled(centre, radius * 0.28, branch_alternative_colour(side, 255));
            }
        }
    }

    // The policy overlay: a translucent green circle on each empty candidate
    // crossing.
    if session.policy_overlay {
        if let Some(rings) = &session.policy_rings {
            for (point, value) in rings {
                if session.game().stone_at(*point).is_some() {
                    continue;
                }
                if *value <= POLICY_RING_FLOOR {
                    continue;
                }
                let centre = to_screen([point.col() as f32, point.row() as f32]);
                painter.circle_filled(
                    centre,
                    ppc * 0.3525 / pixels_per_point,
                    policy_colour(*value),
                );
            }
        }
    }

    // Threat numbers sit above the policy rings when both overlays are on.
    if let Some(proof) = session.threat_proof() {
        let radius = ppc * 0.47 * 0.5 / pixels_per_point;
        for (index, point) in proof.line.iter().enumerate() {
            let centre = to_screen([point.col() as f32, point.row() as f32]);
            painter.circle_filled(centre, radius, THREAT_CIRCLE);
            let side = if index % 2 == 0 {
                proof.winner
            } else {
                proof.winner.other()
            };
            let ink = match side {
                Color::Black => Color32::BLACK,
                Color::White => Color32::WHITE,
            };
            painter.text(
                centre,
                Align2::CENTER_CENTER,
                (index + 1).to_string(),
                FontId::monospace(radius * 1.2),
                ink,
            );
        }
    }

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
        let siblings = session.game().moves_with_siblings();
        // Imported puzzle stones have numbers but no move-tree nodes.
        let initial_stones = session
            .game()
            .stones()
            .count()
            .saturating_sub(session.game().cursor());
        let sibling_font = FontId::monospace(font.size * 0.75);
        for (index, (point, colour)) in session.game().stones().enumerate() {
            let centre = to_screen([point.col() as f32, point.row() as f32]);
            let ink = match colour {
                Color::Black => Color32::from_rgb(235, 235, 240),
                Color::White => Color32::from_rgb(30, 30, 36),
            };
            let has_siblings = index
                .checked_sub(initial_stones)
                .and_then(|move_index| siblings.get(move_index))
                .copied()
                .unwrap_or(false);
            let (label, label_font) = if has_siblings {
                (format!("{},…", index + 1), sibling_font.clone())
            } else {
                ((index + 1).to_string(), font.clone())
            };
            painter.text(
                centre + Vec2::new(0.0, 0.0),
                Align2::CENTER_CENTER,
                label,
                label_font,
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
fn dialogs(ctx: &egui::Context, session: &mut Session) {
    let Some(dialog) = session.dialog().cloned() else {
        return;
    };
    let mut close = false;
    match &dialog {
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
        crate::theme::install(&context);
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
            .unwrap_or_else(|| panic!("the requested action is drawn: {text}"));
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
        session.release_seat(Color::White);
        session.place(gomoku_core::point(7, 7).unwrap());
        let request = session.next_ai_request().unwrap();
        session.apply_ai_reply(request.reply(Ok(player::Decision {
            action: player::DecisionAction::Resign(player::ResignReason::VerifiedOpponentVcf {
                candidates: 5,
            }),
            search: None,
            guard: None,
            resignation: None,
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
            resignation: None,
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
        crate::theme::install(context);
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
    fn edit_menu_undo_deletes_the_live_tip() {
        let context = egui::Context::default();
        let mut session = Session::new(&gomoku_core::Settings::default());
        session.place(gomoku_core::point(7, 7).unwrap());

        click_text("Edit", |events| ui_frame(&context, &mut session, events));
        click_text("Undo", |events| ui_frame(&context, &mut session, events));

        assert!(session.game().is_empty());
        let mut deleted = session.game().clone();
        assert!(!deleted.forward());
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
            ("Release White", Color::White, SeatOwner::Ai),
            ("Take White", Color::White, SeatOwner::Human),
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
        edit_value(&context, &mut session, "4000", "900");
        edit_value(&context, &mut session, "0.0", "0.7");
        assert_eq!(session.ai_options(Color::Black).simulations, 900);
        assert_eq!(session.ai_options(Color::Black).temperature, 0.7);
        edit_value(&context, &mut session, "4000", "700");
        edit_value(&context, &mut session, "0.0", "1.2");
        assert_eq!(session.ai_options(Color::White).simulations, 700);
        assert_eq!(session.ai_options(Color::White).temperature, 1.2);
        assert!(!session.changed(), "AI settings are not game metadata");
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
            resignation: None,
        }))));
        // The AI's move opened the Moves section; return to the Players seat.
        session.toggle_panel_section(PanelSection::Players);
        click_text("Release Black", |events| {
            ui_frame(&context, &mut session, events)
        });
        let request = session.next_ai_request().unwrap();
        assert_eq!(request.options.simulations, 900);
        assert_eq!(request.options.temperature, 0.7);
    }

    #[test]
    fn the_initiative_button_hands_the_side_to_move_to_the_ai() {
        let context = egui::Context::default();
        let mut session = Session::new(&gomoku_core::Settings::default());
        session.toggle_panel_section(PanelSection::Players);
        assert_eq!(session.owner(Color::Black), SeatOwner::Human);
        let mut output = ui_frame(&context, &mut session, vec![]);
        output.textures_delta.clear();

        click_text("AI plays Black", |events| {
            ui_frame(&context, &mut session, events)
        });
        assert_eq!(session.owner(Color::Black), SeatOwner::Ai);
        let request = session
            .next_ai_request()
            .expect("the kick reserves a flight immediately");
        assert_eq!(request.side, Color::Black);

        let mut output = ui_frame(&context, &mut session, vec![]);
        assert!(
            output.shapes.iter().any(|shape| matches!(&shape.shape,
                egui::Shape::Text(text) if text.galley.text() == "AI · thinking…")),
            "the seat row reads thinking right after the kick"
        );
        output.textures_delta.clear();
    }

    #[test]
    fn the_initiative_button_is_disabled_when_the_game_is_decided() {
        let context = egui::Context::default();
        let mut session = Session::new(&gomoku_core::Settings::default());
        session.take_seat(Color::White);
        // Black plays out a vertical five on column 7.
        for (row, col) in [
            (7, 7),
            (0, 0),
            (8, 7),
            (0, 1),
            (9, 7),
            (0, 2),
            (10, 7),
            (0, 3),
            (11, 7),
        ] {
            session.place(gomoku_core::point(row, col).unwrap());
        }
        assert_ne!(session.game().outcome(), Outcome::Ongoing);
        session.toggle_panel_section(PanelSection::Players);
        let generation = session.generation();
        click_text("AI plays White", |events| {
            ui_frame(&context, &mut session, events)
        });
        assert_eq!(
            session.generation(),
            generation,
            "a decided game ignores the kick"
        );
        assert!(session.next_ai_request().is_none());
    }

    #[test]
    fn release_black_with_black_to_move_starts_thinking() {
        let context = egui::Context::default();
        let mut session = Session::new(&gomoku_core::Settings::default());
        session.toggle_panel_section(PanelSection::Players);
        assert_eq!(
            session.game().board_snapshot().to_move(),
            Color::Black,
            "fresh game: Black moves first"
        );
        assert_eq!(session.owner(Color::Black), SeatOwner::Human);
        let mut output = ui_frame(&context, &mut session, vec![]);
        output.textures_delta.clear();

        click_text("Release Black", |events| {
            ui_frame(&context, &mut session, events)
        });
        assert_eq!(session.owner(Color::Black), SeatOwner::Ai);
        // prepare_wait reconciles after the click frame settles.
        let request = session
            .next_ai_request()
            .expect("releasing the side to move reserves a flight");
        assert_eq!(request.side, Color::Black);

        let mut output = ui_frame(&context, &mut session, vec![]);
        assert!(
            output.shapes.iter().any(|shape| matches!(&shape.shape,
                egui::Shape::Text(text) if text.galley.text() == "AI · thinking…")),
            "the seat row reads thinking right after the release"
        );
        output.textures_delta.clear();
    }

    #[test]
    fn the_seat_row_reports_thinking_while_a_flight_computes() {
        let context = egui::Context::default();
        let mut session = Session::new(&gomoku_core::Settings::default());
        session.release_seat(Color::White);
        session.place(gomoku_core::point(7, 7).unwrap());
        // A played move opens the Moves section; bring the seats back into view.
        session.toggle_panel_section(PanelSection::Players);
        let mut output = ui_frame(&context, &mut session, vec![]);
        let seat_line = |output: &egui::FullOutput| {
            output.shapes.iter().any(|shape| {
                matches!(&shape.shape,
                    egui::Shape::Text(text) if text.galley.text() == "AI · thinking…")
            })
        };
        assert!(
            !seat_line(&output),
            "no flight is outstanding before the first request"
        );
        output.textures_delta.clear();

        let _flight = session.next_ai_request().unwrap();
        let mut output = ui_frame(&context, &mut session, vec![]);
        assert!(seat_line(&output), "the seat row reports thinking");
        output.textures_delta.clear();
    }

    #[test]
    fn release_reactivates_reviewed_ai_and_take_is_active_while_thinking_but_not_modal() {
        let context = egui::Context::default();
        let mut session = Session::new(&gomoku_core::Settings::default());
        session.toggle_panel_section(PanelSection::Players);
        session.place(gomoku_core::point(7, 7).unwrap());
        session.seek(0);
        session.seek(usize::MAX);
        assert_eq!(session.owner(Color::White), SeatOwner::Human);
        assert!(!status_line(&session).contains("AI paused"));
        session.toggle_panel_section(PanelSection::Players);
        let mut output = ui_frame(&context, &mut session, vec![]);
        output.textures_delta.clear();
        assert!(!output.shapes.iter().any(|shape| matches!(&shape.shape,
            egui::Shape::Text(text) if text.galley.text() == "Resume AI"
                || text.galley.text() == "AI paused")));
        click_text("Release White", |events| {
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
            resignation: None,
        }))));
        click_text("Release White", |events| {
            ui_frame(&context, &mut session, events)
        });
        let request = session.next_ai_request().unwrap();
        assert!(session.apply_ai_reply(request.reply(Ok(player::Decision {
            action: player::DecisionAction::Play(gomoku_core::point(7, 8).unwrap()),
            search: None,
            guard: None,
            resignation: None,
        }))));
        // The AI's move opened the Moves section; return to the Players seat.
        session.toggle_panel_section(PanelSection::Players);
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
        session.release_seat(Color::White);
        session.place(gomoku_core::point(7, 7).unwrap());
        session.knock.take();
        let request = session.next_ai_request().unwrap();
        session.apply_ai_reply(request.reply(Ok(player::Decision {
            action: player::DecisionAction::Resign(player::ResignReason::VerifiedOpponentVcf {
                candidates: 5,
            }),
            search: None,
            guard: None,
            resignation: None,
        })));
        session.notice = Some("old notice".into());
        assert!(session.title().contains("White resigned — Black wins"));
        assert_eq!(session.game().len(), 1);
        assert_eq!(session.game().stones().count(), 1);
        assert!(session.knock.is_none());
        assert!(winning_line(session.game()).is_none());
        session.seek(0);
        session.toggle_panel_section(PanelSection::Moves);
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
        session.release_seat(Color::White);
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
            resignation: None,
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
        edit_value(&context, &mut session, "4000", "50001");
        edit_value(&context, &mut session, "0.0", "0.74");
        assert_eq!(session.ai_options(Color::Black).simulations, 50000);
        assert_eq!(session.ai_options(Color::Black).temperature, 0.7);
        edit_value(&context, &mut session, "4000", "0");
        edit_value(&context, &mut session, "0.0", "3");
        assert_eq!(session.ai_options(Color::White).simulations, 1);
        assert_eq!(session.ai_options(Color::White).temperature, 2.0);
    }

    #[test]
    fn menu_bar_and_submenu_entries_use_double_sized_fonts() {
        let context = egui::Context::default();
        let mut session = Session::new(&gomoku_core::Settings::default());
        let mut output = ui_frame(&context, &mut session, vec![]);
        let file = output
            .shapes
            .iter()
            .find_map(|shape| match &shape.shape {
                egui::Shape::Text(text) if text.galley.text() == "File" => Some(text),
                _ => None,
            })
            .expect("the File menu is drawn");
        let file_size = file.galley.job.sections[0].format.font_id.size;
        output.textures_delta.clear();
        assert_eq!(file_size, MENU_FONT_SIZE);
        let center_y = file.pos.y + file.galley.rect.height() / 2.0;
        assert!(
            (center_y - MENU_BAR_HEIGHT / 2.0).abs() < 2.0,
            "the menu label is vertically centered in the bar: {center_y}"
        );

        click_text("File", |events| ui_frame(&context, &mut session, events))
            .textures_delta
            .clear();
        let mut output = ui_frame(&context, &mut session, vec![]);
        let new_game = output
            .shapes
            .iter()
            .find_map(|shape| match &shape.shape {
                egui::Shape::Text(text) if text.galley.text() == "New game" => Some(text),
                _ => None,
            })
            .expect("the open File menu draws its entries");
        let new_game_size = new_game.galley.job.sections[0].format.font_id.size;
        output.textures_delta.clear();
        assert_eq!(new_game_size, MENU_FONT_SIZE);
    }

    #[test]
    fn the_board_refits_when_the_area_changes_even_after_a_hand_zoom() {
        let context = egui::Context::default();
        let mut session = Session::new(&gomoku_core::Settings::default());
        ui_frame_sized(&context, &mut session, vec![], egui::vec2(1200.0, 900.0))
            .textures_delta
            .clear();
        session.camera.zoom_about(
            session.viewport,
            [session.viewport.width * 0.5, session.viewport.height * 0.5],
            2.0,
        );
        ui_frame_sized(&context, &mut session, vec![], egui::vec2(700.0, 600.0))
            .textures_delta
            .clear();
        assert!(
            (session.camera.pixels_per_cell - crate::camera::Camera::fit_scale(session.viewport))
                .abs()
                < 1e-3,
            "the board follows the window again: {}",
            session.camera.pixels_per_cell
        );
    }

    #[test]
    fn the_status_line_anchors_right_at_menu_size() {
        let context = egui::Context::default();
        let mut session = Session::new(&gomoku_core::Settings::default());
        let mut output = ui_frame(&context, &mut session, vec![]);
        let status = output
            .shapes
            .iter()
            .find_map(|shape| match &shape.shape {
                egui::Shape::Text(text) if text.galley.text() == "Black to play — move 1" => {
                    Some(text)
                }
                _ => None,
            })
            .expect("the status line is drawn in the bar");
        assert_eq!(
            status.galley.job.sections[0].format.font_id.size,
            MENU_FONT_SIZE
        );
        assert!(
            status.pos.x > 600.0,
            "the status anchors to the right half of the 1200-wide window: {:?}",
            status.pos
        );
        output.textures_delta.clear();
    }

    #[test]
    fn pressed_menu_entries_have_readable_contrast() {
        fn contrast_ratio(ink: Color32, paper: Color32) -> f32 {
            fn luminance(color: Color32) -> f32 {
                fn linear(channel: u8) -> f32 {
                    let srgb = f32::from(channel) / 255.0;
                    if srgb <= 0.04045 {
                        srgb / 12.92
                    } else {
                        ((srgb + 0.055) / 1.055).powf(2.4)
                    }
                }
                0.2126 * linear(color.r()) + 0.7152 * linear(color.g()) + 0.0722 * linear(color.b())
            }
            let light = luminance(ink).max(luminance(paper));
            let dark = luminance(ink).min(luminance(paper));
            (light + 0.05) / (dark + 0.05)
        }

        let context = egui::Context::default();
        let mut session = Session::new(&gomoku_core::Settings::default());
        let mut frame = |events, time| {
            crate::theme::install(&context);
            context.run_ui(
                egui::RawInput {
                    screen_rect: Some(egui::Rect::from_min_size(
                        egui::Pos2::ZERO,
                        egui::vec2(1200.0, 900.0),
                    )),
                    events,
                    time: Some(time),
                    ..Default::default()
                },
                |ui| {
                    draw(ui, &mut session);
                },
            )
        };
        click_text_timed("File", 1, &mut frame)
            .textures_delta
            .clear();
        let mut output = frame(vec![], 1.0);
        let position = output
            .shapes
            .iter()
            .find_map(|shape| match &shape.shape {
                egui::Shape::Text(text) if text.galley.text() == "New game" => {
                    Some(text.pos + text.galley.rect.center().to_vec2())
                }
                _ => None,
            })
            .expect("the open File menu draws its entries");
        output.textures_delta.clear();

        let mut output = frame(
            vec![
                egui::Event::PointerMoved(position),
                egui::Event::PointerButton {
                    pos: position,
                    button: egui::PointerButton::Primary,
                    pressed: true,
                    modifiers: egui::Modifiers::NONE,
                },
            ],
            1.01,
        );
        let text = output
            .shapes
            .iter()
            .find_map(|shape| match &shape.shape {
                egui::Shape::Text(text) if text.galley.text() == "New game" => Some(text),
                _ => None,
            })
            .expect("the pressed menu entry draws its text");
        let centre = text.pos + text.galley.rect.center().to_vec2();
        let fill = output
            .shapes
            .iter()
            .filter_map(|shape| match &shape.shape {
                egui::Shape::Rect(rect)
                    if rect.fill != Color32::TRANSPARENT && rect.rect.contains(centre) =>
                {
                    Some(rect)
                }
                _ => None,
            })
            .min_by(|a, b| a.rect.area().total_cmp(&b.rect.area()))
            .expect("the pressed menu entry sits on its button fill");
        let format_color = text.galley.job.sections[0].format.color;
        let ink = if format_color == Color32::PLACEHOLDER {
            text.fallback_color
        } else {
            format_color
        };
        assert_eq!(fill.fill, crate::theme::ACCENT, "the menu entry is pressed");
        assert!(
            contrast_ratio(ink, fill.fill) >= 4.5,
            "pressed menu entry is {ink:?} on {:?}; contrast {:.2}:1",
            fill.fill,
            contrast_ratio(ink, fill.fill),
        );
        output.textures_delta.clear();
    }

    fn branched_session() -> Session {
        let mut session = Session::new(&gomoku_core::Settings::default());
        session.take_seat(Color::Black);
        session.take_seat(Color::White);
        session.place(gomoku_core::point(7, 7).unwrap());
        session.place(gomoku_core::point(8, 7).unwrap());
        session.seek(1);
        session.place(gomoku_core::point(7, 8).unwrap());
        session.seek(1);
        session
    }

    #[test]
    fn move_numbers_mark_only_stones_with_branch_alternatives() {
        let context = egui::Context::default();
        let mut branched = branched_session();
        branched.seek(usize::MAX);
        branched.toggles.move_numbers = true;
        let mut output = ui_frame(&context, &mut branched, vec![]);
        let marker = output.shapes.iter().find_map(|shape| match &shape.shape {
            egui::Shape::Text(text) if text.galley.text() == "2,…" => Some(text),
            _ => None,
        });
        output.textures_delta.clear();
        assert!(marker.is_some(), "the siblinged second move is marked");

        let mut linear = Session::new(&gomoku_core::Settings::default());
        linear.take_seat(Color::Black);
        linear.take_seat(Color::White);
        linear.place(gomoku_core::point(7, 7).unwrap());
        linear.place(gomoku_core::point(8, 7).unwrap());
        linear.toggles.move_numbers = true;
        let mut output = ui_frame(&context, &mut linear, vec![]);
        let has_marker = output.shapes.iter().any(|shape| {
            matches!(&shape.shape, egui::Shape::Text(text) if text.galley.text().contains(",…"))
        });
        output.textures_delta.clear();
        assert!(!has_marker, "a linear game has no branch markers");
    }

    #[test]
    fn move_list_shows_branch_options_only_at_branch_points() {
        let context = egui::Context::default();
        let mut session = branched_session();
        assert!(session.is_branch_point());
        assert_eq!(session.branch_options().len(), 2);
        let mut output = ui_frame(&context, &mut session, vec![]);
        let texts: Vec<_> = output
            .shapes
            .iter()
            .filter_map(|shape| match &shape.shape {
                egui::Shape::Text(text) => Some(text.galley.text()),
                _ => None,
            })
            .collect();
        let counts = [
            texts.iter().filter(|text| **text == "[0] H7").count(),
            texts.iter().filter(|text| **text == "[1] I8").count(),
        ];
        output.textures_delta.clear();
        assert_eq!(counts, [1, 1], "each branch option is drawn exactly once");

        let mut linear = Session::new(&gomoku_core::Settings::default());
        linear.take_seat(Color::Black);
        linear.take_seat(Color::White);
        linear.place(gomoku_core::point(7, 7).unwrap());
        linear.place(gomoku_core::point(8, 7).unwrap());
        linear.seek(1);
        let mut output = ui_frame(&context, &mut linear, vec![]);
        let has_branch_affordance = output.shapes.iter().any(|shape| {
            matches!(&shape.shape,
            egui::Shape::Text(text) if text.galley.text().starts_with('['))
        });
        output.textures_delta.clear();
        assert!(
            !has_branch_affordance,
            "a single continuation is an ordinary move row, not a branch option"
        );
    }

    #[test]
    fn move_list_marks_moves_that_have_branch_alternatives() {
        let context = egui::Context::default();
        let mut session = branched_session();
        // Live view: the selected second move is an ordinary row again.
        session.seek(usize::MAX);
        let mut output = ui_frame(&context, &mut session, vec![]);
        let texts: Vec<_> = output
            .shapes
            .iter()
            .filter_map(|shape| match &shape.shape {
                egui::Shape::Text(text) => Some(text.galley.text()),
                _ => None,
            })
            .collect();
        output.textures_delta.clear();
        assert!(
            texts.contains(&"I8,\u{2026}"),
            "the siblinged second move carries the marker, got {texts:?}"
        );
        assert!(
            !texts.iter().any(|text| text.contains("H8,")),
            "the only-child first move stays plain"
        );

        let mut linear = Session::new(&gomoku_core::Settings::default());
        linear.take_seat(Color::Black);
        linear.take_seat(Color::White);
        linear.place(gomoku_core::point(7, 7).unwrap());
        linear.place(gomoku_core::point(8, 7).unwrap());
        let mut output = ui_frame(&context, &mut linear, vec![]);
        let marked = output.shapes.iter().any(|shape| {
            matches!(&shape.shape,
                egui::Shape::Text(text) if text.galley.text().contains(",\u{2026}"))
        });
        output.textures_delta.clear();
        assert!(!marked, "a linear history has no markers");
    }

    #[test]
    fn selected_branch_option_uses_the_current_row_highlight() {
        let context = egui::Context::default();
        let mut session = branched_session();
        let mut output = ui_frame(&context, &mut session, vec![]);
        let highlighted = |label| {
            let centre = output
                .shapes
                .iter()
                .find_map(|shape| match &shape.shape {
                    egui::Shape::Text(text) if text.galley.text() == label => {
                        Some(text.pos + text.galley.rect.center().to_vec2())
                    }
                    _ => None,
                })
                .expect("the branch option is drawn");
            output.shapes.iter().any(|shape| {
                matches!(&shape.shape,
                    egui::Shape::Rect(rect)
                        if rect.fill == PANEL_CURRENT && rect.rect.contains(centre))
            })
        };
        let selected = highlighted("[1] I8");
        let unselected = highlighted("[0] H7");
        output.textures_delta.clear();
        assert!(selected, "the remembered child is visibly selected");
        assert!(!unselected, "the other branch is not marked as selected");
    }

    #[test]
    fn clicking_a_branch_option_enters_it_and_updates_the_move_list() {
        let context = egui::Context::default();
        let mut session = branched_session();
        click_text("[0] H7", |events| ui_frame(&context, &mut session, events));

        assert_eq!(session.game().cursor(), 2, "selection enters the child");
        assert_eq!(
            session.game().last_move(),
            gomoku_core::point(8, 7),
            "the clicked sibling becomes the visible move"
        );
        let mut output = ui_frame(&context, &mut session, vec![]);
        let list_follows = output.shapes.iter().any(|shape| {
            matches!(&shape.shape,
            egui::Shape::Text(text) if text.galley.text() == "H7,\u{2026}")
        }) && !output.shapes.iter().any(|shape| {
            matches!(&shape.shape,
                egui::Shape::Text(text)
                    if text.galley.text() == "I8" || text.galley.text().starts_with('['))
        });
        output.textures_delta.clear();
        assert!(
            list_follows,
            "the ordinary move rows show the chosen branch"
        );

        session.rewind();
        assert_eq!(
            session.selected_branch(),
            Some(0),
            "the choice is remembered at the branch point"
        );
    }

    #[test]
    fn move_rows_use_one_and_a_half_times_body_font() {
        let context = egui::Context::default();
        let mut session = Session::new(&gomoku_core::Settings::default());
        session.take_seat(Color::White);
        session.place(gomoku_core::point(7, 7).unwrap());
        session.place(gomoku_core::point(7, 8).unwrap());
        session.seek(1);

        let mut output = ui_frame(&context, &mut session, vec![]);
        for label in ["   1", "H8", "   2", "I8"] {
            let text = output
                .shapes
                .iter()
                .find_map(|shape| match &shape.shape {
                    egui::Shape::Text(text) if text.galley.text() == label => Some(text),
                    _ => None,
                })
                .unwrap_or_else(|| panic!("move-row text is drawn: {label}"));
            assert_eq!(
                text.galley.job.sections[0].format.font_id.size,
                crate::theme::BODY_FONT_SIZE * 1.5,
                "{label} is enlarged in both current and future move rows"
            );
        }
        output.textures_delta.clear();
    }

    #[test]
    fn display_options_section_offers_policy_overlay_on_off_button() {
        let context = egui::Context::default();
        let mut session = Session::new(&gomoku_core::Settings::default());
        session.toggle_panel_section(PanelSection::Policy);
        let mut output = ui_frame(&context, &mut session, vec![]);
        let button = output
            .shapes
            .iter()
            .find_map(|shape| match &shape.shape {
                egui::Shape::Text(text) if text.galley.text() == "Policy overlay OFF" => Some(text),
                _ => None,
            })
            .expect("the open Display options section contains its ON/OFF button");
        assert_eq!(
            button.galley.job.sections[0].format.font_id.size,
            crate::theme::BODY_FONT_SIZE * 2.0,
            "the display option matches the Resignation switch: twice the body size"
        );
        output.textures_delta.clear();

        click_text("Policy overlay OFF", |events| {
            ui_frame(&context, &mut session, events)
        });
        assert!(session.policy_overlay);
        assert_eq!(
            session.policy_rings, None,
            "the flat opening prior carries no guidance"
        );
        let mut output = ui_frame(&context, &mut session, vec![]);
        assert!(output.shapes.iter().any(|shape| matches!(&shape.shape,
            egui::Shape::Text(text) if text.galley.text() == "Policy overlay ON")));
        output.textures_delta.clear();

        click_text("Policy overlay ON", |events| {
            ui_frame(&context, &mut session, events)
        });
        assert!(!session.policy_overlay);
        assert_eq!(session.policy_rings, None);
    }

    #[test]
    fn display_options_section_offers_the_branch_alternatives_toggle() {
        let context = egui::Context::default();
        let mut session = Session::new(&gomoku_core::Settings::default());
        session.toggle_panel_section(PanelSection::Policy);

        let mut output = ui_frame(&context, &mut session, vec![]);
        assert!(output.shapes.iter().any(|shape| matches!(&shape.shape,
            egui::Shape::Text(text) if text.galley.text() == "Display options")));
        assert!(output.shapes.iter().any(|shape| matches!(&shape.shape,
            egui::Shape::Text(text) if text.galley.text() == "Branch alternatives OFF")));
        assert!(!output.shapes.iter().any(|shape| matches!(&shape.shape,
            egui::Shape::Text(text) if text.galley.text() == "Policy")));
        output.textures_delta.clear();

        click_text("Branch alternatives OFF", |events| {
            ui_frame(&context, &mut session, events)
        });

        assert!(session.branch_alternatives);
    }

    #[test]
    fn branch_alternative_toggle_gates_translucent_stone_painting() {
        let context = egui::Context::default();
        let mut session = branched_session();
        let alternative = session.branch_options()[0];
        let fill = Color32::from_rgba_unmultiplied(246, 246, 250, 72);
        assert_eq!(branch_alternative_colour(Color::White, 72), fill);
        assert_eq!(
            branch_alternative_colour(Color::Black, 72),
            Color32::from_rgba_unmultiplied(22, 22, 26, 72)
        );
        let circles = |output: &egui::FullOutput| {
            output
                .shapes
                .iter()
                .filter_map(|shape| match &shape.shape {
                    egui::Shape::Circle(circle) if circle.fill == fill => Some(circle),
                    _ => None,
                })
                .count()
        };

        let mut output = ui_frame(&context, &mut session, vec![]);
        assert_eq!(circles(&output), 0, "the toggle starts off");
        output.textures_delta.clear();

        session.branch_alternatives = true;
        let mut output = ui_frame(&context, &mut session, vec![]);
        assert_eq!(circles(&output), 2, "every continuation paints");
        let selected = session
            .selected_branch()
            .expect("a branch point has a current child");
        let pupil = Color32::from_rgba_unmultiplied(246, 246, 250, 255);
        let pupil_at = |point: gomoku_core::Move, output: &egui::FullOutput| {
            let pixel = session
                .camera
                .to_screen(session.viewport, [point.col() as f32, point.row() as f32]);
            let centre = egui::Pos2::new(
                pixel[0] / context.pixels_per_point(),
                pixel[1] / context.pixels_per_point(),
            );
            output.shapes.iter().any(|shape| {
                matches!(&shape.shape,
                    egui::Shape::Circle(circle)
                        if circle.fill == pupil
                            && circle.center == centre
                            && (circle.radius
                                - session.camera.pixels_per_cell * 0.44 * 0.28
                                    / context.pixels_per_point())
                                .abs() < 0.01)
            })
        };
        assert!(
            pupil_at(session.branch_options()[selected], &output),
            "the continuation the move list follows wears a pupil"
        );
        assert!(
            !pupil_at(session.branch_options()[1 - selected], &output),
            "the unselected alternative wears none"
        );
        let circle = output
            .shapes
            .iter()
            .find_map(|shape| match &shape.shape {
                egui::Shape::Circle(circle) if circle.fill == fill => Some(circle),
                _ => None,
            })
            .expect("the alternative stone is painted");
        let pixel = session.camera.to_screen(
            session.viewport,
            [alternative.col() as f32, alternative.row() as f32],
        );
        let expected_centre = egui::Pos2::new(
            pixel[0] / context.pixels_per_point(),
            pixel[1] / context.pixels_per_point(),
        );
        assert_eq!(circle.center, expected_centre);
        assert_eq!(
            circle.radius,
            session.camera.pixels_per_cell * 0.44 / context.pixels_per_point()
        );
        output.textures_delta.clear();
    }

    fn cache_threat_proof(session: &mut Session, proof: engine::Proof) {
        session.threat_cache = Some(crate::session::ThreatSequenceCache {
            position: session.game().board_snapshot(),
            proof: Some(proof),
        });
    }

    #[test]
    fn display_options_section_offers_the_threat_sequences_toggle() {
        let context = egui::Context::default();
        let mut session = Session::new(&gomoku_core::Settings::default());
        session.toggle_panel_section(PanelSection::Policy);

        let mut output = ui_frame(&context, &mut session, vec![]);
        assert!(output.shapes.iter().any(|shape| matches!(&shape.shape,
            egui::Shape::Text(text) if text.galley.text() == "Threat sequences OFF")));
        output.textures_delta.clear();

        click_text("Threat sequences OFF", |events| {
            ui_frame(&context, &mut session, events)
        });

        assert!(session.threat_sequences);
    }

    #[test]
    fn threat_sequence_toggle_gates_circle_painting() {
        fn circle_count(output: &egui::FullOutput) -> usize {
            output
                .shapes
                .iter()
                .filter(|shape| {
                    matches!(&shape.shape,
                        egui::Shape::Circle(circle) if circle.fill == THREAT_CIRCLE)
                })
                .count()
        }

        let context = egui::Context::default();
        let mut session = Session::new(&gomoku_core::Settings::default());
        let proof = engine::Proof {
            winner: Color::Black,
            line: vec![
                gomoku_core::point(6, 6).unwrap(),
                gomoku_core::point(7, 7).unwrap(),
                gomoku_core::point(8, 8).unwrap(),
            ],
        };
        cache_threat_proof(&mut session, proof.clone());
        let mut output = ui_frame(&context, &mut session, vec![]);
        let count = circle_count(&output);
        output.textures_delta.clear();
        assert_eq!(count, 0, "the toggle starts off");

        session.threat_sequences = true;
        cache_threat_proof(&mut session, proof);
        let mut output = ui_frame(&context, &mut session, vec![]);
        let count = circle_count(&output);
        let radius = output.shapes.iter().find_map(|shape| match &shape.shape {
            egui::Shape::Circle(circle) if circle.fill == THREAT_CIRCLE => Some(circle.radius),
            _ => None,
        });
        output.textures_delta.clear();
        assert_eq!(count, 3, "one grey circle paints for every proof move");
        let expected = session.camera.pixels_per_cell * 0.47 * 0.5 / context.pixels_per_point();
        assert!(
            (radius.unwrap() - expected).abs() < f32::EPSILON,
            "each circle is half a stone"
        );
    }

    #[test]
    fn threat_sequence_numbers_follow_line_order_and_move_colors() {
        let context = egui::Context::default();
        let mut session = Session::new(&gomoku_core::Settings::default());
        session.threat_sequences = true;
        cache_threat_proof(
            &mut session,
            engine::Proof {
                winner: Color::Black,
                line: vec![
                    gomoku_core::point(6, 6).unwrap(),
                    gomoku_core::point(7, 7).unwrap(),
                    gomoku_core::point(8, 8).unwrap(),
                    gomoku_core::point(9, 9).unwrap(),
                ],
            },
        );

        let mut output = ui_frame(&context, &mut session, vec![]);
        for (number, ink) in [
            ("1", Color32::BLACK),
            ("2", Color32::WHITE),
            ("3", Color32::BLACK),
            ("4", Color32::WHITE),
        ] {
            let text = output
                .shapes
                .iter()
                .find_map(|shape| match &shape.shape {
                    egui::Shape::Text(text) if text.galley.text() == number => Some(text),
                    _ => None,
                })
                .unwrap_or_else(|| panic!("proof move {number} is painted"));
            assert_eq!(text.galley.job.sections[0].format.color, ink);
            assert!(
                output.shapes.iter().any(|shape| {
                    matches!(&shape.shape,
                        egui::Shape::Circle(circle)
                            if circle.fill == THREAT_CIRCLE
                                && circle.center
                                    == text.pos + text.galley.rect.center().to_vec2())
                }),
                "proof move {number} is centered in its grey circle"
            );
            assert!(
                text.galley.job.sections[0].format.font_id.size
                    <= session.camera.pixels_per_cell * 0.47 * 0.5 / context.pixels_per_point()
                        * 1.2,
                "the number is scaled to its tiny circle"
            );
        }
        output.textures_delta.clear();
    }

    #[test]
    fn threat_circles_paint_after_policy_rings() {
        let context = egui::Context::default();
        let mut session = Session::new(&gomoku_core::Settings::default());
        let point = gomoku_core::point(8, 8).unwrap();
        session.policy_overlay = true;
        session.policy_rings = Some(vec![(point, 1.0)]);
        session.threat_sequences = true;
        cache_threat_proof(
            &mut session,
            engine::Proof {
                winner: Color::Black,
                line: vec![point],
            },
        );

        let mut output = ui_frame(&context, &mut session, vec![]);
        let position = |predicate: &dyn Fn(&egui::Shape) -> bool| {
            output
                .shapes
                .iter()
                .position(|shape| predicate(&shape.shape))
        };
        let ring = position(&|shape| {
            matches!(shape,
                egui::Shape::Circle(circle) if circle.fill == policy_colour(1.0))
        })
        .expect("the policy ring paints");
        let threat = position(
            &|shape| matches!(shape, egui::Shape::Circle(circle) if circle.fill == THREAT_CIRCLE),
        )
        .expect("the threat circle paints");
        let number = position(
            &|shape| matches!(shape, egui::Shape::Text(text) if text.galley.text() == "1"),
        )
        .expect("the threat number paints");
        output.textures_delta.clear();
        assert!(ring < threat && threat < number);
    }

    #[test]
    fn policy_overlay_toggle_gates_ring_painting() {
        fn ring_count(output: &egui::FullOutput) -> usize {
            output
                .shapes
                .iter()
                .filter(|shape| {
                    matches!(&shape.shape,
                        egui::Shape::Circle(circle) if circle.fill == policy_colour(1.0))
                })
                .count()
        }

        let context = egui::Context::default();
        let mut session = Session::new(&gomoku_core::Settings::default());
        session.policy_rings = Some(vec![(gomoku_core::point(8, 8).unwrap(), 1.0)]);
        let mut output = ui_frame(&context, &mut session, vec![]);
        let count = ring_count(&output);
        output.textures_delta.clear();
        assert_eq!(count, 0, "the toggle starts off");

        session.policy_overlay = true;
        let mut output = ui_frame(&context, &mut session, vec![]);
        let count = ring_count(&output);
        let radius = output.shapes.iter().find_map(|shape| match &shape.shape {
            egui::Shape::Circle(circle) if circle.fill == policy_colour(1.0) => Some(circle.radius),
            _ => None,
        });
        output.textures_delta.clear();
        assert_eq!(count, 1, "the toggle reveals the rings");
        let expected = session.camera.pixels_per_cell * 0.3525 / context.pixels_per_point();
        assert!(
            (radius.unwrap() - expected).abs() < f32::EPSILON,
            "the ring is 75% of its former size"
        );
    }

    #[test]
    fn policy_rings_suppress_the_weakest_two_shades() {
        let context = egui::Context::default();
        let mut session = Session::new(&gomoku_core::Settings::default());
        session.policy_overlay = true;
        // Shades one and two of twenty (0.05, 0.10) are too faint to matter;
        // shade three (0.15) and the strongest (1.0) must still paint.
        session.policy_rings = Some(vec![
            (gomoku_core::point(6, 6).unwrap(), 0.05),
            (gomoku_core::point(7, 7).unwrap(), 0.10),
            (gomoku_core::point(8, 8).unwrap(), 0.15),
            (gomoku_core::point(9, 9).unwrap(), 1.0),
        ]);

        let mut output = ui_frame(&context, &mut session, vec![]);
        let painted = |fill| {
            output
                .shapes
                .iter()
                .filter(|shape| {
                    matches!(&shape.shape,
                        egui::Shape::Circle(circle) if circle.fill == fill)
                })
                .count()
        };
        assert_eq!(painted(policy_colour(0.05)), 0, "shade one is suppressed");
        assert_eq!(painted(policy_colour(0.10)), 0, "shade two is suppressed");
        assert_eq!(painted(policy_colour(0.15)), 1, "shade three paints");
        assert_eq!(painted(policy_colour(1.0)), 1, "the strongest paints");
        output.textures_delta.clear();
    }

    #[test]
    fn policy_rings_never_paint_over_stones() {
        let context = egui::Context::default();
        let mut session = Session::new(&gomoku_core::Settings::default());
        let occupied = gomoku_core::point(7, 7).unwrap();
        session.place(occupied);
        session.policy_overlay = true;
        session.policy_rings = Some(vec![
            (occupied, 1.0),
            (gomoku_core::point(8, 8).unwrap(), 1.0),
        ]);

        let mut output = ui_frame(&context, &mut session, vec![]);
        let count = output
            .shapes
            .iter()
            .filter(|shape| {
                matches!(&shape.shape,
                    egui::Shape::Circle(circle) if circle.fill == policy_colour(1.0))
            })
            .count();
        output.textures_delta.clear();
        assert_eq!(count, 1, "only the empty crossing has a policy ring");
    }

    #[test]
    fn resignation_button_toggles_both_ai_seats() {
        let context = egui::Context::default();
        let mut session = Session::new(&gomoku_core::Settings::default());
        let mut output = ui_frame(&context, &mut session, vec![]);
        assert!(output.shapes.iter().any(|shape| matches!(&shape.shape,
            egui::Shape::Text(text) if text.galley.text() == "Resignation ON")));
        output.textures_delta.clear();

        let mut output = click_text("Resignation ON", |events| {
            ui_frame(&context, &mut session, events)
        });

        assert!(!session.resignation_enabled());
        assert!(!session.ai_options(Color::Black).resign);
        assert!(!session.ai_options(Color::White).resign);
        output.textures_delta.clear();

        let mut output = ui_frame(&context, &mut session, vec![]);
        assert!(output.shapes.iter().any(|shape| matches!(&shape.shape,
            egui::Shape::Text(text) if text.galley.text() == "Resignation OFF")));
        output.textures_delta.clear();
    }

    #[test]
    fn panel_headers_open_exclusively_at_header_size() {
        let context = egui::Context::default();
        let mut session = Session::new(&gomoku_core::Settings::default());
        assert_eq!(
            session.open_panel_section(),
            crate::session::PanelSection::Players,
            "the Players section is open from the start"
        );
        let mut output = ui_frame(&context, &mut session, vec![]);
        let header = output
            .shapes
            .iter()
            .find_map(|shape| match &shape.shape {
                egui::Shape::Text(text) if text.galley.text() == "Players" => Some(text),
                _ => None,
            })
            .expect("the open Players header is drawn");
        assert_eq!(
            header.galley.job.sections[0].format.font_id.size,
            SECTION_HEADER_FONT_SIZE
        );
        assert_eq!(
            header.galley.job.sections[0].format.color,
            Color32::WHITE,
            "the header text is white on its grey strip"
        );
        output.textures_delta.clear();

        session.place(gomoku_core::point(7, 7).unwrap());
        assert_eq!(
            session.open_panel_section(),
            crate::session::PanelSection::Moves,
            "a played move opens the Moves section"
        );
        let mut output = click_text("Players", |events| ui_frame(&context, &mut session, events));
        assert_eq!(
            session.open_panel_section(),
            crate::session::PanelSection::Players
        );
        assert!(output.shapes.iter().any(|shape| matches!(&shape.shape,
            egui::Shape::Text(text) if text.galley.text() == "Resignation ON")));
        assert!(
            !output.shapes.iter().any(|shape| matches!(&shape.shape,
                egui::Shape::Text(text) if text.galley.text() == "H8")),
            "the superseded Moves body must not render in the switching frame"
        );
        output.textures_delta.clear();
    }

    #[test]
    fn panel_headers_are_full_width_grey_strips() {
        let context = egui::Context::default();
        let mut session = Session::new(&gomoku_core::Settings::default());
        let mut output = ui_frame(&context, &mut session, vec![]);
        let strips: Vec<_> = output
            .shapes
            .iter()
            .filter_map(|shape| match &shape.shape {
                egui::Shape::Rect(rect) if rect.fill == crate::theme::PANEL_HEADER => {
                    Some(rect.rect)
                }
                _ => None,
            })
            .collect();
        output.textures_delta.clear();
        assert_eq!(
            strips.len(),
            3,
            "Players, Moves and Display options each get a grey strip"
        );
        for strip in strips {
            assert!(
                (strip.max.x - 1200.0).abs() < 1.0,
                "the strip reaches the window's right border, got {strip:?}"
            );
        }
    }

    #[test]
    fn minimum_width_hidpi_controls_stay_inside_the_panel() {
        let context = egui::Context::default();
        context.set_pixels_per_point(2.0);
        let mut session = Session::new(&gomoku_core::Settings::default());
        session.toggle_panel_section(PanelSection::Players);
        session.panel_width = 180.0;
        session.release_seat(Color::White);
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
    fn paused_controls_are_absent_for_review_human_turns_and_finished_games() {
        let context = egui::Context::default();
        let mut session = Session::new(&gomoku_core::Settings::default());
        session.seek(0);
        let absent = |session: &mut Session| {
            let mut output = ui_frame(&context, session, vec![]);
            output.textures_delta.clear();
            assert!(!output.shapes.iter().any(|shape| matches!(&shape.shape,
                egui::Shape::Text(text) if text.galley.text() == "Resume AI"
                    || text.galley.text() == "AI paused")));
        };
        absent(&mut session); // Human Black
        session.place(gomoku_core::point(7, 7).unwrap());
        session.seek(0);
        absent(&mut session); // reviewing
        session.seek(usize::MAX);
        session.release_seat(Color::White);
        let request = session.next_ai_request().unwrap();
        session.apply_ai_reply(request.reply(Ok(player::Decision {
            action: player::DecisionAction::Resign(player::ResignReason::VerifiedOpponentVcf {
                candidates: 5,
            }),
            search: None,
            guard: None,
            resignation: None,
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
    fn clicking_a_recent_file_opens_it_without_an_unsaved_prompt() {
        let context = egui::Context::default();
        crate::theme::install(&context);
        let mut session = Session::new(&gomoku_core::Settings::default());
        session.place(gomoku_core::point(7, 7).unwrap());
        let path = std::env::temp_dir().join(format!(
            "gomoku-recent-no-prompt-{}.json",
            std::process::id()
        ));
        let mut expected = gomoku_core::Game::new();
        expected.play(gomoku_core::point(3, 3).unwrap()).unwrap();
        gomoku_core::save(&path, &expected).unwrap();
        session.recent = vec![path.clone()];
        let label = name_of(&path);
        click_text(&label, |events| {
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
        assert!(session.dialog().is_none());
        assert_eq!(session.game(), &expected);
        assert!(!session.changed());
        std::fs::remove_file(path).unwrap();
    }

    #[test]
    fn every_dialog_draws() {
        let point = gomoku_core::point(7, 7).expect("inside the board");
        let dialogs = [
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
        crate::theme::install(&context);
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
            session.release_seat(Color::White);
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
            resignation: None,
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
                format.font_id.size >= crate::theme::BODY_FONT_SIZE * 2.0,
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
            session.viewport_points[1], MENU_BAR_HEIGHT,
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
        assert_eq!(session.viewport_points[1], MENU_BAR_HEIGHT);
    }

    #[test]
    fn the_banner_wraps_when_the_window_is_narrow() {
        let context = egui::Context::default();
        let mut session = resigned_session(Color::White);
        let mut output = ui_frame_sized(&context, &mut session, vec![], egui::vec2(240.0, 900.0));
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
        crate::theme::install(context);
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
    fn reopening_the_last_corpus_is_a_lab_menu_request() {
        let context = egui::Context::default();
        let mut session = Session::new(&gomoku_core::Settings::default());
        let requests = std::cell::Cell::new(Requests::default());
        session.remember_corpus_directory(std::path::Path::new("/games/Freestyle15_1"));

        click_text("Lab", |events| {
            explorer_frame(&context, &mut session, &requests, events, None)
        });
        click_text("Reopen Freestyle15_1", |events| {
            explorer_frame(&context, &mut session, &requests, events, None)
        });
        assert!(
            requests.get().reopen_explorer,
            "the menu item requests a pickerless reload"
        );
    }

    #[test]
    fn reopen_last_corpus_is_disabled_without_a_corpus() {
        let context = egui::Context::default();
        let mut session = Session::new(&gomoku_core::Settings::default());
        let requests = std::cell::Cell::new(Requests::default());

        click_text("Lab", |events| {
            explorer_frame(&context, &mut session, &requests, events, None)
        });
        click_text("Reopen last corpus", |events| {
            explorer_frame(&context, &mut session, &requests, events, None)
        });
        assert!(
            !requests.get().reopen_explorer,
            "a disabled item never requests"
        );
    }

    #[test]
    fn explorer_text_edit_has_readable_contrast_while_focused() {
        fn contrast_ratio(ink: Color32, paper: Color32) -> f32 {
            fn luminance(color: Color32) -> f32 {
                fn linear(channel: u8) -> f32 {
                    let srgb = f32::from(channel) / 255.0;
                    if srgb <= 0.04045 {
                        srgb / 12.92
                    } else {
                        ((srgb + 0.055) / 1.055).powf(2.4)
                    }
                }
                0.2126 * linear(color.r()) + 0.7152 * linear(color.g()) + 0.0722 * linear(color.b())
            }
            let light = luminance(ink).max(luminance(paper));
            let dark = luminance(ink).min(luminance(paper));
            (light + 0.05) / (dark + 0.05)
        }

        let context = egui::Context::default();
        let mut session = explorer_session();
        session.set_explorer_filter("needle".into());
        let requests = std::cell::Cell::new(Requests::default());
        explorer_frame(&context, &mut session, &requests, vec![], None)
            .textures_delta
            .clear();
        context.memory_mut(|memory| {
            memory.request_focus(egui::Id::new("explorer-filter"));
        });
        let mut output = explorer_frame(&context, &mut session, &requests, vec![], None);
        output.textures_delta.clear();

        let text = output
            .shapes
            .iter()
            .find_map(|shape| match &shape.shape {
                egui::Shape::Text(text) if text.galley.text() == "needle" => Some(text),
                _ => None,
            })
            .expect("the focused filter draws its text");
        let centre = text.pos + text.galley.rect.center().to_vec2();
        let fill = output
            .shapes
            .iter()
            .filter_map(|shape| match &shape.shape {
                egui::Shape::Rect(rect)
                    if rect.fill != Color32::TRANSPARENT && rect.rect.contains(centre) =>
                {
                    Some(rect)
                }
                _ => None,
            })
            .min_by(|a, b| a.rect.area().total_cmp(&b.rect.area()))
            .expect("the filter text sits on its edit fill");
        let ink = text.galley.job.sections[0].format.color;
        assert!(
            contrast_ratio(ink, fill.fill) >= 4.5,
            "focused edit is {ink:?} on {:?}; contrast {:.2}:1",
            fill.fill,
            contrast_ratio(ink, fill.fill),
        );
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
    fn explorer_clamped_up_arrows_keep_first_row_without_loading() {
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
                key(egui::Key::ArrowUp, egui::Modifiers::NONE),
            ],
            None,
        )
        .textures_delta
        .clear();

        assert_eq!(session.explorer().unwrap().selected, Some(0));
        assert_eq!(requests.get().explorer_select, None);
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
