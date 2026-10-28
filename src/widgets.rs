// Small custom-painted widgets used across the UI.

use crate::theme::{self, Palette};
use eframe::egui::{
    self, Align2, Color32, CornerRadius, FontId, Frame, Margin, Painter, Rect, Response, RichText,
    Sense, Stroke, StrokeKind, Ui, WidgetInfo, pos2,
    text::{LayoutJob, TextWrapping},
    vec2,
};
use std::sync::Arc;

#[derive(Clone, Copy, PartialEq)]
pub enum Kind {
    Primary,
    Danger,
    Secondary,
    DangerOutline,
}

pub fn button(ui: &mut Ui, text: &str, kind: Kind) -> Response {
    let p = theme::palette(ui);
    let font = if matches!(kind, Kind::Primary | Kind::Danger) {
        theme::bold(14.0)
    } else {
        FontId::proportional(14.0)
    };
    let galley = ui
        .painter()
        .layout_no_wrap(text.to_owned(), font, Color32::PLACEHOLDER);
    let size = vec2(galley.size().x + 32.0, 34.0);
    let (rect, resp) = ui.allocate_exact_size(size, Sense::click());
    resp.widget_info(|| WidgetInfo::labeled(egui::Role::Button, ui.is_enabled(), text));
    if ui.is_rect_visible(rect) {
        let hot = resp.hovered() || resp.is_pointer_button_down_on();
        let (fill, stroke, fg) = match kind {
            Kind::Primary => (
                if hot { p.accent_hover } else { p.accent },
                Stroke::NONE,
                Color32::WHITE,
            ),
            Kind::Danger => (
                if hot { p.danger_hover } else { p.danger },
                Stroke::NONE,
                Color32::WHITE,
            ),
            Kind::Secondary => (
                if hot { p.surface_hover } else { p.surface },
                Stroke::new(1.0, p.border),
                p.text,
            ),
            Kind::DangerOutline => (
                if hot {
                    p.danger.gamma_multiply(0.12)
                } else {
                    p.surface
                },
                Stroke::new(1.0, p.border),
                p.danger,
            ),
        };
        let painter = ui.painter();
        painter.rect(
            rect,
            CornerRadius::same(7),
            fill,
            stroke,
            StrokeKind::Inside,
        );
        if resp.has_focus() {
            painter.rect_stroke(
                rect,
                CornerRadius::same(9),
                Stroke::new(2.0, p.accent),
                StrokeKind::Outside,
            );
        }
        painter.galley(rect.center() - galley.size() / 2.0, galley, fg);
    }
    resp
}

/// Borderless square button with a symbol and a tooltip.
pub fn icon_button(ui: &mut Ui, icon: &str, tooltip: &str) -> Response {
    ui.add(
        egui::Button::new(RichText::new(icon).size(14.0))
            .frame_when_inactive(false)
            .min_size(vec2(28.0, 28.0)),
    )
    .on_hover_text(tooltip)
}

/// Line drawing of a monitor, used as the connection icon.
pub fn paint_monitor(painter: &Painter, rect: Rect, color: Color32) {
    let w = rect.width();
    let stroke = Stroke::new((w * 0.09).max(1.5), color);
    let screen = Rect::from_min_max(
        pos2(rect.left(), rect.top() + w * 0.1),
        pos2(rect.right(), rect.top() + w * 0.72),
    );
    painter.rect_stroke(
        screen,
        CornerRadius::same((w * 0.1) as u8),
        stroke,
        StrokeKind::Inside,
    );
    let cx = rect.center().x;
    painter.line_segment(
        [pos2(cx, screen.bottom()), pos2(cx, rect.top() + w * 0.88)],
        stroke,
    );
    painter.line_segment(
        [
            pos2(cx - w * 0.22, rect.top() + w * 0.9),
            pos2(cx + w * 0.22, rect.top() + w * 0.9),
        ],
        stroke,
    );
}

/// Rounded tile with a monitor icon and an optional "connected" dot.
pub fn connection_badge(ui: &Ui, rect: Rect, p: &Palette, fill: Color32, active: bool) {
    let painter = ui.painter();
    painter.rect_filled(rect, CornerRadius::same((rect.width() * 0.25) as u8), fill);
    paint_monitor(painter, rect.shrink(rect.width() * 0.24), p.accent);
    if active {
        let r = (rect.width() * 0.14).max(4.5);
        let c = rect.right_bottom() - vec2(r * 0.6, r * 0.6);
        painter.circle(c, r, p.success, Stroke::new(2.0, p.surface));
    }
}

pub fn elided(
    ui: &Ui,
    text: &str,
    font: FontId,
    color: Color32,
    max_width: f32,
) -> Arc<egui::Galley> {
    let mut job = LayoutJob::simple_singleline(text.to_owned(), font, color);
    job.wrap = TextWrapping {
        max_width,
        max_rows: 1,
        break_anywhere: true,
        overflow_character: Some('…'),
    };
    ui.painter().layout_job(job)
}

/// Rounded label such as "● Connected".
pub fn pill(ui: &mut Ui, text: &str, color: Color32) {
    let galley = ui
        .painter()
        .layout_no_wrap(text.to_owned(), theme::bold(12.0), color);
    let size = vec2(galley.size().x + 30.0, 22.0);
    let (rect, _) = ui.allocate_exact_size(size, Sense::hover());
    let painter = ui.painter();
    painter.rect_filled(rect, CornerRadius::same(11), color.gamma_multiply(0.14));
    painter.circle_filled(pos2(rect.left() + 11.0, rect.center().y), 3.5, color);
    painter.galley(
        pos2(rect.left() + 19.0, rect.center().y - galley.size().y / 2.0),
        galley,
        color,
    );
}

pub fn card<R>(ui: &mut Ui, p: &Palette, add: impl FnOnce(&mut Ui) -> R) -> R {
    Frame::new()
        .fill(p.surface)
        .stroke(Stroke::new(1.0, p.border))
        .corner_radius(CornerRadius::same(12))
        .inner_margin(Margin::same(20))
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            add(ui)
        })
        .inner
}

/// Small uppercase heading for list sections.
pub fn section_label(ui: &mut Ui, p: &Palette, text: &str) {
    ui.label(
        RichText::new(text.to_uppercase())
            .font(theme::bold(11.0))
            .color(p.text_weak),
    );
}

pub fn divider(ui: &mut Ui, p: &Palette) {
    let (rect, _) = ui.allocate_exact_size(vec2(ui.available_width(), 1.0), Sense::hover());
    ui.painter().rect_filled(rect, 0, p.border);
}

/// Paints `text` centered in `rect` (used for symbol glyphs).
pub fn centered_text(painter: &Painter, rect: Rect, text: &str, font: FontId, color: Color32) {
    painter.text(rect.center(), Align2::CENTER_CENTER, text, font, color);
}
