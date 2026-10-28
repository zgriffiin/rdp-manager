// Colors, fonts, and widget styling for the light and dark themes.

use eframe::egui::{
    self, Color32, CornerRadius, FontData, FontDefinitions, FontFamily, FontId, Margin, Shadow,
    Stroke, TextStyle, Theme, Visuals,
};
use std::sync::Arc;

pub const BOLD: &str = "bold";

#[derive(Clone, Copy)]
pub struct Palette {
    pub accent: Color32,
    pub accent_hover: Color32,
    pub accent_soft: Color32,
    pub bg: Color32,
    pub surface: Color32,
    pub surface_hover: Color32,
    pub border: Color32,
    pub text: Color32,
    pub text_weak: Color32,
    pub success: Color32,
    pub danger: Color32,
    pub danger_hover: Color32,
}

const LIGHT: Palette = Palette {
    accent: Color32::from_rgb(37, 99, 235),
    accent_hover: Color32::from_rgb(29, 78, 216),
    accent_soft: Color32::from_rgb(228, 236, 253),
    bg: Color32::from_rgb(244, 245, 247),
    surface: Color32::WHITE,
    surface_hover: Color32::from_rgb(238, 240, 243),
    border: Color32::from_rgb(222, 225, 230),
    text: Color32::from_rgb(31, 35, 40),
    text_weak: Color32::from_rgb(107, 114, 128),
    success: Color32::from_rgb(22, 163, 74),
    danger: Color32::from_rgb(220, 38, 38),
    danger_hover: Color32::from_rgb(185, 28, 28),
};

const DARK: Palette = Palette {
    accent: Color32::from_rgb(59, 130, 246),
    accent_hover: Color32::from_rgb(37, 99, 235),
    accent_soft: Color32::from_rgb(30, 45, 75),
    bg: Color32::from_rgb(24, 26, 30),
    surface: Color32::from_rgb(34, 37, 42),
    surface_hover: Color32::from_rgb(44, 48, 54),
    border: Color32::from_rgb(52, 56, 63),
    text: Color32::from_rgb(230, 232, 235),
    text_weak: Color32::from_rgb(154, 161, 171),
    success: Color32::from_rgb(34, 197, 94),
    danger: Color32::from_rgb(239, 68, 68),
    danger_hover: Color32::from_rgb(220, 38, 38),
};

pub fn palette(ui: &egui::Ui) -> Palette {
    if ui.visuals().dark_mode { DARK } else { LIGHT }
}

fn visuals(p: &Palette, dark: bool) -> Visuals {
    let mut v = if dark {
        Visuals::dark()
    } else {
        Visuals::light()
    };
    v.panel_fill = p.bg;
    v.window_fill = p.surface;
    v.extreme_bg_color = p.surface;
    v.text_edit_bg_color = Some(p.surface);
    v.faint_bg_color = p.bg;
    v.window_stroke = Stroke::new(1.0, p.border);
    v.window_corner_radius = CornerRadius::same(12);
    v.menu_corner_radius = CornerRadius::same(8);
    v.window_shadow = Shadow {
        offset: [0, 12],
        blur: 36,
        spread: 0,
        color: Color32::from_black_alpha(if dark { 140 } else { 50 }),
    };
    v.popup_shadow = Shadow {
        offset: [0, 4],
        blur: 14,
        spread: 0,
        color: Color32::from_black_alpha(if dark { 110 } else { 35 }),
    };
    v.selection.bg_fill = p.accent.gamma_multiply(0.35);
    v.selection.stroke = Stroke::new(1.0, p.accent);
    v.hyperlink_color = p.accent;

    let w = &mut v.widgets;
    w.noninteractive.bg_fill = p.surface;
    w.noninteractive.weak_bg_fill = p.surface;
    w.noninteractive.bg_stroke = Stroke::new(1.0, p.border);
    w.noninteractive.fg_stroke = Stroke::new(1.0, p.text);
    for (state, fill, stroke) in [
        (&mut w.inactive, p.surface, p.border),
        (&mut w.hovered, p.surface_hover, p.text_weak),
        (&mut w.active, p.surface_hover, p.accent),
        (&mut w.open, p.surface_hover, p.border),
    ] {
        state.bg_fill = fill;
        state.weak_bg_fill = fill;
        state.bg_stroke = Stroke::new(1.0, stroke);
        state.fg_stroke = Stroke::new(1.0, p.text);
        state.expansion = 0.0;
    }
    for state in [
        &mut w.noninteractive,
        &mut w.inactive,
        &mut w.hovered,
        &mut w.active,
        &mut w.open,
    ] {
        state.corner_radius = CornerRadius::same(6);
    }
    v
}

pub fn setup(ctx: &egui::Context) {
    install_fonts(ctx);
    ctx.set_visuals_of(Theme::Light, visuals(&LIGHT, false));
    ctx.set_visuals_of(Theme::Dark, visuals(&DARK, true));
    ctx.all_styles_mut(|style| {
        style.text_styles = [
            (
                TextStyle::Heading,
                FontId::new(20.0, FontFamily::Name(BOLD.into())),
            ),
            (TextStyle::Body, FontId::proportional(14.0)),
            (TextStyle::Button, FontId::proportional(14.0)),
            (TextStyle::Small, FontId::proportional(12.0)),
            (TextStyle::Monospace, FontId::monospace(13.0)),
        ]
        .into();
        style.spacing.item_spacing = egui::vec2(8.0, 8.0);
        style.spacing.button_padding = egui::vec2(12.0, 6.0);
        style.spacing.interact_size.y = 30.0;
        style.spacing.window_margin = Margin::same(16);
        style.spacing.menu_margin = Margin::same(6);
        style.spacing.scroll = egui::style::ScrollStyle::thin();
        style.interaction.selectable_labels = false;
    });
}

/// Uses Segoe UI (the Windows UI font) when present, with egui's fonts as fallback for symbols.
fn install_fonts(ctx: &egui::Context) {
    let mut fonts = FontDefinitions::default();
    let fallback = fonts.families[&FontFamily::Proportional].clone();
    let load = |file: &str| std::fs::read(format!("C:/Windows/Fonts/{file}")).ok();

    let mut regular = fallback.clone();
    if let Some(bytes) = load("segoeui.ttf") {
        fonts
            .font_data
            .insert("segoe".into(), Arc::new(FontData::from_owned(bytes)));
        regular.insert(0, "segoe".into());
    }
    let mut bold = regular.clone();
    if let Some(bytes) = load("seguisb.ttf") {
        fonts.font_data.insert(
            "segoe-semibold".into(),
            Arc::new(FontData::from_owned(bytes)),
        );
        bold.insert(0, "segoe-semibold".into());
    }
    fonts.families.insert(FontFamily::Proportional, regular);
    fonts.families.insert(FontFamily::Name(BOLD.into()), bold);
    ctx.set_fonts(fonts);
}

pub fn bold(size: f32) -> FontId {
    FontId::new(size, FontFamily::Name(BOLD.into()))
}
