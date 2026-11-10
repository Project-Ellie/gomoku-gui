//! The application's light, subdued laboratory theme.
//!
//! All shared colours, spacing, corner radii, and font sizes live here so a
//! future visual pass has one place to tune. Board materials and stone drawing
//! deliberately do not depend on this module.

use std::path::Path;

use anyhow::{Context as _, Result};
use egui::{Color32, CornerRadius, FontId, Stroke, Style, TextStyle, Vec2, Visuals};

/// Seravek is the chosen UI font on the owner's macOS development machine.
/// Face zero of the collection is the Regular weight. Other systems retain
/// egui's built-in fonts when this system path is unavailable.
const DEFAULT_UI_FONT_PATH: &str = "/System/Library/Fonts/Supplemental/Seravek.ttc";

/// The key used for the selected application font in egui's font table.
const OVERRIDE_FONT: &str = "app-font-override";

/// The bold cut of the interface font, for controls that must read as controls
/// (the section headers). Always bound: Seravek Bold when the default
/// collection is installed, the override or default face otherwise.
pub(crate) const BOLD_FONT_FAMILY: &str = "app-font-bold";

/// Seravek Bold is face nine of the default TrueType collection.
const SERAVEK_BOLD_FACE: u32 = 9;

/// Bold text of the interface font.
pub(crate) fn bold(size: f32) -> FontId {
    FontId::new(size, egui::FontFamily::Name(BOLD_FONT_FAMILY.into()))
}

/// Put an override before the built-in proportional fonts, retaining glyph fallbacks.
fn font_definitions(font: egui::FontData) -> egui::FontDefinitions {
    let mut definitions = egui::FontDefinitions::default();
    definitions
        .font_data
        .insert(OVERRIDE_FONT.to_owned(), std::sync::Arc::new(font));
    if let Some(proportional) = definitions
        .families
        .get_mut(&egui::FontFamily::Proportional)
    {
        proportional.insert(0, OVERRIDE_FONT.to_owned());
    }
    // An override carries no bold cut of its own: bold falls back to it.
    definitions.families.insert(
        egui::FontFamily::Name(BOLD_FONT_FAMILY.into()),
        vec![OVERRIDE_FONT.to_owned()],
    );
    definitions
}

/// Install a TTF/OTF/TTC font as the primary face for every text style.
///
/// Font collections use face index zero, the first font in the collection.
/// Built-in egui fonts remain as fallbacks for glyphs the override does not contain.
///
/// # Errors
///
/// Returns an error when the font file cannot be read.
pub(crate) fn install_font(context: &egui::Context, path: &Path) -> Result<()> {
    let bytes =
        std::fs::read(path).with_context(|| format!("cannot read font {}", path.display()))?;
    // `from_owned` selects face zero, including for a TrueType collection.
    install_font_data(context, egui::FontData::from_owned(bytes));
    Ok(())
}

/// Install loaded font data and route every text style through it.
fn install_font_data(context: &egui::Context, font: egui::FontData) {
    context.set_fonts(font_definitions(font));
    route_text_styles(context);
}

/// Route every text style through the proportional family.
fn route_text_styles(context: &egui::Context) {
    context.global_style_mut(|style| {
        for font in style.text_styles.values_mut() {
            font.family = egui::FontFamily::Proportional;
        }
    });
}

/// Warm white used by the menu bar and other chrome.
pub(crate) const SURFACE: Color32 = Color32::from_rgb(247, 245, 240);
/// Warm grey side-panel background, harmonious with the wooden board.
pub(crate) const PANEL: Color32 = Color32::from_rgb(238, 234, 226);
/// Primary text colour on light surfaces.
pub(crate) const PANEL_INK: Color32 = Color32::from_rgb(40, 45, 47);
/// Secondary text colour, dark enough to remain readable on the panel.
pub(crate) const MUTED: Color32 = Color32::from_rgb(101, 107, 105);
/// Section-header strip grey: dark enough to carry bold white text.
pub(crate) const PANEL_HEADER: Color32 = MUTED;
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

/// Body text size: three quarters of the original 13pt baseline.
pub(crate) const BODY_FONT_SIZE: f32 = 9.75;
/// Button text size before the menu-specific doubling.
const BUTTON_FONT_SIZE: f32 = 9.75;
/// Main menu and submenu text size: exactly twice the former button size.
pub(crate) const MENU_FONT_SIZE: f32 = BUTTON_FONT_SIZE * 2.0;
/// Accordion section headers: 2.4 times the body size.
pub(crate) const SECTION_HEADER_FONT_SIZE: f32 = BODY_FONT_SIZE * 2.4;
/// Recent-file entries: halfway between body text and the section headers.
pub(crate) const RECENT_FONT_SIZE: f32 = BODY_FONT_SIZE * 1.5;
/// Shared enlargement for move-row text and its drawn stone marker.
const MOVE_LIST_SCALE: f32 = 1.5;
/// Move numbers and coordinates: 150% of their inherited body size.
pub(crate) const MOVE_LIST_FONT_SIZE: f32 = BODY_FONT_SIZE * MOVE_LIST_SCALE;
/// The resignation banner remains the loudest text in the interface.
pub(crate) const BANNER_FONT_SIZE: f32 = 21.0;

/// Menu bar height, sized to contain the menu labels comfortably.
pub(crate) const MENU_BAR_HEIGHT: f32 = 33.0;
/// Side-panel content padding.
pub(crate) const PANEL_MARGIN: i8 = 10;
/// Drawn stone marker used beside player names.
pub(crate) const STONE_DOT_SIZE: f32 = 13.0;
/// Move-row stone markers grow with their text to keep the row balanced.
pub(crate) const MOVE_LIST_STONE_DOT_SIZE: f32 = STONE_DOT_SIZE * MOVE_LIST_SCALE;
/// Shared corner radius for ordinary controls.
const WIDGET_CORNER_RADIUS: u8 = 5;
/// Shared corner radius for menus.
const MENU_CORNER_RADIUS: u8 = 6;
/// Slightly softer rounding for floating windows.
const WINDOW_CORNER_RADIUS: u8 = 8;

/// Install the designed light theme on an egui context.
pub(crate) fn install(context: &egui::Context) {
    install_with_default_font(context, Path::new(DEFAULT_UI_FONT_PATH));
}

/// Install the theme, retaining egui's fonts if the preferred font cannot be read.
fn install_with_default_font(context: &egui::Context, default_font: &Path) {
    context.global_style_mut(|style| *style = lab_style());
    match std::fs::read(default_font) {
        Ok(bytes) => {
            // Face zero of the collection is the regular cut.
            let mut definitions = font_definitions(egui::FontData::from_owned(bytes.clone()));
            let bold = egui::FontData {
                index: SERAVEK_BOLD_FACE,
                ..egui::FontData::from_owned(bytes)
            };
            definitions
                .font_data
                .insert(BOLD_FONT_FAMILY.to_owned(), std::sync::Arc::new(bold));
            if let Some(family) = definitions
                .families
                .get_mut(&egui::FontFamily::Name(BOLD_FONT_FAMILY.into()))
            {
                family.insert(0, BOLD_FONT_FAMILY.to_owned());
            }
            context.set_fonts(definitions);
            route_text_styles(context);
        }
        Err(error) => {
            log::warn!("the default UI font is unavailable; using egui's fonts: {error}");
            // egui's built-ins have no bold cut: bind the family to the
            // proportional default so bold text always has a face.
            let mut definitions = egui::FontDefinitions::default();
            if let Some(proportional) = definitions
                .families
                .get(&egui::FontFamily::Proportional)
                .cloned()
            {
                definitions.families.insert(
                    egui::FontFamily::Name(BOLD_FONT_FAMILY.into()),
                    proportional,
                );
            }
            context.set_fonts(definitions);
        }
    }
}

/// Build a complete style rather than layering more local colour overrides.
fn lab_style() -> Style {
    let mut style = Style {
        text_styles: [
            (TextStyle::Small, FontId::proportional(6.75)),
            (TextStyle::Body, FontId::proportional(BODY_FONT_SIZE)),
            (TextStyle::Monospace, FontId::monospace(9.75)),
            (TextStyle::Button, FontId::proportional(BUTTON_FONT_SIZE)),
            (TextStyle::Heading, FontId::proportional(13.5)),
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
    fn seravek_is_the_default_primary_font_when_available() {
        let path = Path::new(DEFAULT_UI_FONT_PATH);
        if !path.is_file() {
            eprintln!("Seravek is unavailable; skipping the default-font assertion");
            return;
        }

        let context = egui::Context::default();
        install(&context);
        let mut output = context.run_ui(egui::RawInput::default(), |_| {});
        let proportional = context.fonts(|fonts| {
            fonts.definitions().families[&egui::FontFamily::Proportional]
                .first()
                .cloned()
        });
        output.textures_delta.clear();
        assert_eq!(proportional.as_deref(), Some(OVERRIDE_FONT));
    }

    #[test]
    fn missing_default_font_keeps_egui_fallbacks() {
        let context = egui::Context::default();
        install_with_default_font(
            &context,
            Path::new("../../target/font-previews/no-such-default-font.ttf"),
        );
        let mut output = context.run_ui(egui::RawInput::default(), |_| {});
        let proportional = context.fonts(|fonts| {
            fonts.definitions().families[&egui::FontFamily::Proportional]
                .first()
                .cloned()
        });
        output.textures_delta.clear();
        assert_eq!(proportional.as_deref(), Some("Ubuntu-Light"));
    }

    #[test]
    fn missing_custom_font_reports_its_path() {
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../target/font-previews/no-such-font.ttf");
        let error = install_font(&egui::Context::default(), &path)
            .expect_err("a missing font cannot be installed");
        assert!(error.to_string().contains("no-such-font.ttf"));
    }

    #[test]
    fn custom_font_overrides_the_default_for_every_text_style() {
        let context = egui::Context::default();
        install(&context);
        let font = egui::FontDefinitions::default().font_data["Ubuntu-Light"].clone();
        let custom_font = (*font).clone();
        install_font_data(&context, custom_font.clone());
        let mut output = context.run_ui(egui::RawInput::default(), |_| {});
        let (primary, custom_data_is_installed) = context.fonts(|fonts| {
            let definitions = fonts.definitions();
            (
                definitions.families[&egui::FontFamily::Proportional]
                    .first()
                    .cloned(),
                definitions.font_data[OVERRIDE_FONT].font.as_ref() == custom_font.font.as_ref(),
            )
        });
        output.textures_delta.clear();

        assert_eq!(primary.as_deref(), Some(OVERRIDE_FONT));
        assert!(custom_data_is_installed);
        let style = context.global_style();
        assert!(
            style
                .text_styles
                .values()
                .all(|font| font.family == egui::FontFamily::Proportional)
        );
    }

    #[test]
    fn custom_font_is_the_primary_proportional_font() {
        let definitions = font_definitions(egui::FontData::from_static(&[]));
        let proportional = definitions
            .families
            .get(&egui::FontFamily::Proportional)
            .expect("egui defines a proportional family");
        assert_eq!(
            proportional.first().map(String::as_str),
            Some(OVERRIDE_FONT)
        );
        assert!(
            proportional.iter().any(|name| name == "Ubuntu-Light"),
            "built-in fonts remain available as fallbacks"
        );
        assert_eq!(definitions.font_data[OVERRIDE_FONT].index, 0);
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
