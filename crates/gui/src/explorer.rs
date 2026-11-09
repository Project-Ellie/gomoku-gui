//! The Lab Explorer panel: a list of records and a way to load one.
//!
//! It is one section of the side panel's accordion, shown while a corpus is
//! open. It only reads and edits the session's Explorer state; loading a
//! record is the application's job, asked for through [`Requests`].

use egui::{Color32, RichText};

use crate::session::Session;
use crate::ui::Requests;

/// The ink for the middle grey panel.
const PANEL_INK: Color32 = Color32::from_rgb(26, 26, 30);
/// The background of the row that the board is showing, lighter than the
/// panel so that both stone colours stay visible on it.
const PANEL_CURRENT: Color32 = Color32::from_rgb(178, 178, 184);
/// The background of a text field on the middle grey panel. The dark theme
/// gives a text field a near-black background, which would not read here.
const PANEL_FIELD: Color32 = Color32::from_rgb(200, 200, 206);

/// Draw the Explorer section and return any application requests.
///
/// Nothing is drawn and nothing is asked when no corpus is open.
pub fn draw(ui: &mut egui::Ui, session: &mut Session) -> Requests {
    let mut requests = Requests::default();
    if session.explorer().is_none() {
        return requests;
    }

    // The filter and jump fields sit on the middle grey panel, so they need
    // the same light field background the seat section gives its own fields.
    // The section draws into a child region, which does not share the seat
    // section's styling.
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

    ui.horizontal(|ui| {
        ui.label(RichText::new("Filter:").color(PANEL_INK));
        let mut filter = session
            .explorer()
            .map(|explorer| explorer.filter.clone())
            .unwrap_or_default();
        if ui.text_edit_singleline(&mut filter).changed() {
            session.set_explorer_filter(filter);
        }
    });

    ui.horizontal(|ui| {
        ui.label(RichText::new("Jump to #").color(PANEL_INK));
        let mut jump = session
            .explorer()
            .map(|explorer| explorer.jump.clone())
            .unwrap_or_default();
        if ui.text_edit_singleline(&mut jump).changed() {
            session.set_explorer_jump(jump);
        }
        if ui.button("Go").clicked() {
            if let Ok(index) = session.explorer_jump_index() {
                requests.explorer_select = Some(index);
            }
        }
    });

    ui.separator();

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

    egui::ScrollArea::vertical()
        .auto_shrink([false, false])
        .max_height((ui.available_height() - 60.0).max(80.0))
        .show(ui, |ui| {
            for record in records {
                let here = selected == Some(record.index);
                let text = format!("{:>5} {}", record.index, record.name);
                let response = if here {
                    egui::Frame::NONE
                        .fill(PANEL_CURRENT)
                        .inner_margin(2.0)
                        .show(ui, |ui| {
                            ui.label(RichText::new(text).strong().color(PANEL_INK))
                        })
                        .response
                } else {
                    ui.label(RichText::new(text).color(PANEL_INK))
                };
                if response.interact(egui::Sense::click()).clicked() {
                    requests.explorer_select = Some(record.index);
                }
            }
        });

    requests
}
