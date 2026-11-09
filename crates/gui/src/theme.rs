//! The application's light, subdued laboratory theme.
//!
//! All shared colours, spacing, corner radii, and font sizes live here so a
//! future visual pass has one place to tune. Board materials and stone drawing
//! deliberately do not depend on this module.

use egui::{Color32, CornerRadius, FontId, Stroke, Style, TextStyle, Vec2, Visuals};

/// Warm white used by the menu bar and other chrome.
pub(crate) const SURFACE: Color32 = Color32::from_rgb(247, 245, 240);
/// Warm grey side-panel background, harmonious with the wooden board.
pub(crate) const PANEL: Color32 = Color32::from_rgb(238, 234, 226);
/// Primary text colour on light surfaces.
pub(crate) const PANEL_INK: Color32 = Color32::from_rgb(40, 45, 47);
/// Secondary text colour, dark enough to remain readable on the panel.
pub(crate) const MUTED: Color32 = Color32::from_rgb(101, 107, 105);
/// Quiet background for editable fields.
pub(crate) const PANEL_FIELD: Color32 = Color32::from_rgb(252, 251, 248);
/// Soft teal wash marking the current row or selection.
pub(crate) const PANEL_CURRENT: Color32 = Color32::from_rgb(201, 216, 214);
/// Muted teal used for links, focus, and active controls.
pub(crate) const ACCENT: Color32 = Color32::from_rgb(63, 111, 117);
/// Resignation banner red; intentionally stronger than the interactive accent.
pub(crate) const BANNER: Color32 = Color32::from_rgb(178, 54, 54);

/// Background for floating windows and dialogs.
const WINDOW: Color32 = Color32::from_rgb(245, 243, 238);
/// Neutral fill for inactive widgets.
const WIDGET: Color32 = Color32::from_rgb(231, 226, 218);
/// Subtle teal hover fill.
const HOVER: Color32 = Color32::from_rgb(220, 232, 230);
/// Restrained separators and control outlines.
const BORDER: Color32 = Color32::from_rgb(212, 206, 195);
/// Very light background for alternating or recessed regions.
const FAINT: Color32 = Color32::from_rgb(229, 224, 215);

/// Body text size, unchanged from egui's previous default.
pub(crate) const BODY_FONT_SIZE: f32 = 13.0;
/// Button text size before the menu-specific doubling.
const BUTTON_FONT_SIZE: f32 = 13.0;
/// Main menu and submenu text size: exactly twice the former button size.
pub(crate) const MENU_FONT_SIZE: f32 = BUTTON_FONT_SIZE * 2.0;
/// Accordion section headers remain at the size introduced in Task 1.8.
pub(crate) const SECTION_HEADER_FONT_SIZE: f32 = BODY_FONT_SIZE * 2.0;
/// The resignation banner remains the loudest text in the interface.
pub(crate) const BANNER_FONT_SIZE: f32 = 28.0;

/// Menu bar height, enlarged to contain the doubled menu labels comfortably.
pub(crate) const MENU_BAR_HEIGHT: f32 = 44.0;
/// Side-panel content padding.
pub(crate) const PANEL_MARGIN: i8 = 10;
/// Shared corner radius for ordinary controls.
const WIDGET_CORNER_RADIUS: u8 = 5;
/// Shared corner radius for menus.
const MENU_CORNER_RADIUS: u8 = 6;
/// Slightly softer rounding for floating windows.
const WINDOW_CORNER_RADIUS: u8 = 8;

/// Install the designed light theme on an egui context.
pub(crate) fn install(context: &egui::Context) {
    context.global_style_mut(|style| *style = lab_style());
}

/// Build a complete style rather than layering more local colour overrides.
fn lab_style() -> Style {
    let mut style = Style {
        text_styles: [
            (TextStyle::Small, FontId::proportional(9.0)),
            (TextStyle::Body, FontId::proportional(BODY_FONT_SIZE)),
            (TextStyle::Monospace, FontId::monospace(13.0)),
            (TextStyle::Button, FontId::proportional(BUTTON_FONT_SIZE)),
            (TextStyle::Heading, FontId::proportional(18.0)),
        ]
        .into(),
        ..Default::default()
    };
    style.spacing.item_spacing = Vec2::new(8.0, 6.0);
    style.spacing.button_padding = Vec2::new(10.0, 5.0);
    style.spacing.menu_margin = egui::Margin::same(8);
    style.spacing.window_margin = egui::Margin::same(12);
    style.visuals = lab_visuals();
    style
}

/// The light palette and restrained widget geometry.
fn lab_visuals() -> Visuals {
    let mut visuals = Visuals::light();
    visuals.panel_fill = PANEL;
    visuals.window_fill = WINDOW;
    visuals.faint_bg_color = FAINT;
    visuals.extreme_bg_color = PANEL_FIELD;
    visuals.text_edit_bg_color = Some(PANEL_FIELD);
    visuals.override_text_color = None;
    visuals.hyperlink_color = ACCENT;
    visuals.code_bg_color = FAINT;
    visuals.warn_fg_color = Color32::from_rgb(145, 96, 24);
    visuals.error_fg_color = Color32::from_rgb(166, 68, 68);
    visuals.window_corner_radius = CornerRadius::same(WINDOW_CORNER_RADIUS);
    visuals.menu_corner_radius = CornerRadius::same(MENU_CORNER_RADIUS);
    visuals.window_stroke = Stroke::new(1.0, BORDER);
    visuals.window_shadow = egui::Shadow::NONE;
    visuals.popup_shadow = egui::Shadow::NONE;
    visuals.selection.bg_fill = ACCENT;
    visuals.selection.stroke = Stroke::new(1.0, Color32::WHITE);

    let widgets = &mut visuals.widgets;
    for state in [
        &mut widgets.noninteractive,
        &mut widgets.inactive,
        &mut widgets.hovered,
        &mut widgets.active,
        &mut widgets.open,
    ] {
        state.corner_radius = CornerRadius::same(WIDGET_CORNER_RADIUS);
        state.fg_stroke.color = PANEL_INK;
    }
    widgets.noninteractive.bg_fill = PANEL;
    widgets.noninteractive.weak_bg_fill = PANEL;
    widgets.noninteractive.bg_stroke = Stroke::NONE;

    widgets.inactive.bg_fill = WIDGET;
    widgets.inactive.weak_bg_fill = WIDGET;
    widgets.inactive.bg_stroke = Stroke::new(1.0, BORDER);

    widgets.hovered.bg_fill = HOVER;
    widgets.hovered.weak_bg_fill = HOVER;
    widgets.hovered.bg_stroke = Stroke::new(1.0, ACCENT);

    widgets.active.bg_fill = ACCENT;
    widgets.active.weak_bg_fill = ACCENT;
    widgets.active.fg_stroke.color = Color32::WHITE;
    widgets.active.bg_stroke = Stroke::new(1.0, ACCENT);

    widgets.open.bg_fill = HOVER;
    widgets.open.weak_bg_fill = HOVER;
    widgets.open.bg_stroke = Stroke::new(1.0, ACCENT);
    visuals
}

#[cfg(test)]
mod tests {
    use super::*;

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

    #[test]
    fn installs_the_light_lab_theme() {
        let context = egui::Context::default();
        install(&context);
        let style = context.global_style();
        assert!(!style.visuals.dark_mode);
        assert_eq!(style.visuals.panel_fill, PANEL);
        assert_eq!(style.visuals.text_edit_bg_color, Some(PANEL_FIELD));
        assert_eq!(style.visuals.window_corner_radius, CornerRadius::same(8));
        assert_eq!(
            style.text_styles[&TextStyle::Body].size,
            BODY_FONT_SIZE,
            "the theme pass leaves body text at its former size"
        );
        assert_eq!(
            style.text_styles[&TextStyle::Button].size * 2.0,
            MENU_FONT_SIZE
        );
    }

    #[test]
    fn primary_text_and_accent_pairs_have_readable_contrast() {
        for (ink, paper) in [
            (PANEL_INK, PANEL),
            (PANEL_INK, PANEL_FIELD),
            (MUTED, PANEL),
            (MUTED, SURFACE),
            (Color32::WHITE, ACCENT),
            (Color32::WHITE, BANNER),
        ] {
            assert!(
                contrast_ratio(ink, paper) >= 4.5,
                "{ink:?} on {paper:?} has contrast {:.2}:1",
                contrast_ratio(ink, paper)
            );
        }
    }
}
