//! The Lab Explorer panel: a list of records and a way to load one.
//!
//! It is one section of the side panel's accordion, shown while a corpus is
//! open. It only reads and edits the session's Explorer state; loading a
//! record is the application's job, asked for through [`Requests`].

use egui::RichText;

use crate::session::Session;
use crate::theme::{PANEL_CURRENT, PANEL_INK};
use crate::ui::Requests;

/// Draw the Explorer section and return any application requests.
///
/// Nothing is drawn and nothing is asked when no corpus is open.
pub fn draw(ui: &mut egui::Ui, session: &mut Session) -> Requests {
    let mut requests = Requests::default();
    if session.explorer().is_none() {
        return requests;
    }

    ui.horizontal(|ui| {
        ui.label(RichText::new("Filter:").color(PANEL_INK));
        let mut filter = session
            .explorer()
            .map(|explorer| explorer.filter.clone())
            .unwrap_or_default();
        if ui
            .add(egui::TextEdit::singleline(&mut filter).id(egui::Id::new("explorer-filter")))
            .changed()
        {
            session.set_explorer_filter(filter);
        }
    });

    ui.horizontal(|ui| {
        ui.label(RichText::new("Jump to #").color(PANEL_INK));
        let mut jump = session
            .explorer()
            .map(|explorer| explorer.jump.clone())
            .unwrap_or_default();
        if ui
            .add(egui::TextEdit::singleline(&mut jump).id(egui::Id::new("explorer-jump")))
            .changed()
        {
            session.set_explorer_jump(jump);
        }
        if ui.button("Go").clicked() {
            if let Ok(index) = session.explorer_jump_index() {
                requests.explorer_select = Some(index);
            }
        }
    });

    ui.separator();

    // Apply every press in order, clamping at each end, but emit only the final
    // selection. A burst of key repeats therefore does not queue skipped loads.
    let mut keyboard_selection = None;
    if session.dialog().is_none() {
        ui.input(|input| {
            for event in &input.events {
                let step = match event {
                    egui::Event::Key {
                        key: egui::Key::ArrowUp,
                        pressed: true,
                        ..
                    } => -1,
                    egui::Event::Key {
                        key: egui::Key::ArrowDown,
                        pressed: true,
                        ..
                    } => 1,
                    _ => continue,
                };
                if let Some(index) = session.move_explorer_selection(step) {
                    keyboard_selection = Some(index);
                }
            }
        });
        if let Some(index) = keyboard_selection {
            requests.explorer_select = Some(index);
        }

        if ui.input(|input| input.key_pressed(egui::Key::Enter)) {
            session.hand_over_explorer_to_moves();
            if session.open_panel_section() == crate::session::PanelSection::Moves {
                surrender_explorer_focus(ui);
            }
        }
    }
    let scroll_to_selection = keyboard_selection.is_some();

    // The edits above are settled, so the state can be borrowed for the list.
    let Some(explorer) = session.explorer() else {
        return requests;
    };
    let filter_lower = explorer.filter.to_lowercase();
    let selected = explorer.selected;
    let records: Vec<&gomoku_core::IndexEntry> = explorer
        .records
        .iter()
        .filter(|record| {
            filter_lower.is_empty() || record.name.to_lowercase().contains(&filter_lower)
        })
        .collect();

    let mut clicked_record = None;
    let mut hand_over = false;
    egui::ScrollArea::vertical()
        .auto_shrink([false, false])
        .max_height((ui.available_height() - 60.0).max(80.0))
        .show(ui, |ui| {
            for record in records {
                let here = selected == Some(record.index);
                let text = format!("{:>5} {}", record.index, record.name);
                let text = RichText::new(text).color(PANEL_INK);
                let text = if here { text.strong() } else { text };
                let frame = if here {
                    egui::Frame::NONE.fill(PANEL_CURRENT).inner_margin(2.0)
                } else {
                    egui::Frame::NONE
                };
                // Keep the same widget identity when selection changes, so the
                // second half of a double-click still belongs to this row.
                let response = frame
                    .show(ui, |ui| {
                        // A record is a click target, never a text-selection
                        // target, including on the second click of a double.
                        ui.add(egui::Label::new(text).selectable(false))
                    })
                    .inner
                    .interact(egui::Sense::click());
                if response.clicked() {
                    clicked_record = Some(record.index);
                }
                if here && scroll_to_selection {
                    ui.scroll_to_rect(response.rect, Some(egui::Align::Center));
                }
                if response.double_clicked() {
                    requests.explorer_select = Some(record.index);
                    hand_over = true;
                }
            }
        });
    if let Some(index) = clicked_record {
        session.select_explorer_record(index);
    }
    if hand_over {
        session.hand_over_explorer_to_moves();
        surrender_explorer_focus(ui);
    }

    requests
}

/// Release Explorer text fields so Left/Right reach application move navigation.
fn surrender_explorer_focus(ui: &mut egui::Ui) {
    ui.memory_mut(|memory| {
        memory.surrender_focus(egui::Id::new("explorer-filter"));
        memory.surrender_focus(egui::Id::new("explorer-jump"));
    });
}
