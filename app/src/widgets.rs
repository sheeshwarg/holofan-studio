// Small reusable pieces of the modern UI: cards, bars, sparklines, nav items, buttons.

use std::collections::VecDeque;

use eframe::egui::{
    self, Align2, Color32, CornerRadius, FontId, Frame, Margin, Pos2, Rect, Response, RichText,
    Sense, Shadow, Shape, Stroke, Ui, Vec2, pos2, vec2,
};

use crate::theme::{Palette, bold, mix};

pub fn card_frame(p: &Palette) -> Frame {
    Frame::new()
        .fill(p.card)
        .stroke(Stroke::new(1.0, p.border))
        .corner_radius(CornerRadius::same(14))
        .inner_margin(Margin::same(18))
        .shadow(Shadow {
            offset: [0, 2],
            blur: 10,
            spread: 0,
            color: Color32::from_black_alpha(if p.dark { 40 } else { 10 }),
        })
}

pub fn card<R>(ui: &mut Ui, p: &Palette, add: impl FnOnce(&mut Ui) -> R) -> R {
    card_frame(p).show(ui, add).inner
}

pub fn title(text: &str, size: f32, color: Color32) -> RichText {
    RichText::new(text)
        .font(FontId::new(size, bold()))
        .color(color)
}

pub fn page_header(ui: &mut Ui, p: &Palette, heading: &str, sub: &str) {
    ui.label(title(heading, 26.0, p.text));
    ui.add_space(2.0);
    ui.label(RichText::new(sub).size(14.0).color(p.muted));
    ui.add_space(18.0);
}

pub fn section_label(ui: &mut Ui, p: &Palette, text: &str) {
    ui.label(
        RichText::new(text.to_uppercase())
            .size(11.5)
            .color(p.muted)
            .strong(),
    );
}

/// Rounded progress bar with a track.
pub fn bar(ui: &mut Ui, p: &Palette, frac: f32, color: Color32, height: f32) -> Response {
    let w = ui.available_width();
    let (rect, resp) = ui.allocate_exact_size(vec2(w, height), Sense::hover());
    let r = (height / 2.0) as u8;
    ui.painter().rect_filled(rect, r, p.track);
    let frac = frac.clamp(0.0, 1.0);
    if frac > 0.0 {
        let mut fill = rect;
        fill.set_width((rect.width() * frac).max(height));
        ui.painter().rect_filled(fill, r, color);
    }
    resp
}

pub fn usage_color(p: &Palette, frac: f32) -> Color32 {
    if frac >= 0.9 {
        p.danger
    } else if frac >= 0.75 {
        p.warning
    } else {
        p.accent
    }
}

/// An area chart of the last values. `max` = None scales to the data.
pub fn sparkline(
    ui: &mut Ui,
    data: &VecDeque<f32>,
    size: Vec2,
    color: Color32,
    max: Option<f32>,
    capacity: usize,
) {
    let (rect, _) = ui.allocate_exact_size(size, Sense::hover());
    paint_sparkline(ui.painter(), rect, data, color, max, capacity);
}

pub fn paint_sparkline(
    painter: &egui::Painter,
    rect: Rect,
    data: &VecDeque<f32>,
    color: Color32,
    max: Option<f32>,
    capacity: usize,
) {
    // faint guide lines so a young history still reads as a chart
    let guide = color.gamma_multiply(0.12);
    painter.hline(
        rect.x_range(),
        rect.bottom() - 0.5,
        Stroke::new(1.0, color.gamma_multiply(0.35)),
    );
    painter.hline(rect.x_range(), rect.center().y, Stroke::new(1.0, guide));
    painter.hline(rect.x_range(), rect.top() + 0.5, Stroke::new(1.0, guide));
    if data.len() < 2 {
        return;
    }
    let peak = max.unwrap_or_else(|| data.iter().cloned().fold(1.0f32, f32::max) * 1.15);
    let n = capacity.max(data.len());
    let step = rect.width() / (n - 1) as f32;
    let start = n - data.len();
    let pts: Vec<Pos2> = data
        .iter()
        .enumerate()
        .map(|(i, v)| {
            let x = rect.left() + (start + i) as f32 * step;
            let y = rect.bottom() - (v / peak).clamp(0.0, 1.0) * rect.height();
            pos2(x, y)
        })
        .collect();
    // filled area as a vertical-gradient mesh
    let mut mesh = egui::Mesh::default();
    let top_col = color.gamma_multiply(0.35);
    let bot_col = color.gamma_multiply(0.0);
    for (i, pt) in pts.iter().enumerate() {
        mesh.colored_vertex(*pt, top_col);
        mesh.colored_vertex(pos2(pt.x, rect.bottom()), bot_col);
        if i > 0 {
            let b = (i as u32) * 2;
            mesh.add_triangle(b - 2, b - 1, b);
            mesh.add_triangle(b - 1, b, b + 1);
        }
    }
    painter.add(Shape::mesh(mesh));
    painter.add(Shape::line(pts.clone(), Stroke::new(2.0, color)));
    if let Some(last) = pts.last() {
        painter.circle_filled(*last, 3.5, color);
    }
}

/// Sidebar navigation entry with a pill highlight when selected.
pub fn nav_item(ui: &mut Ui, p: &Palette, icon: &str, label: &str, selected: bool) -> Response {
    let w = ui.available_width();
    let (rect, resp) = ui.allocate_exact_size(vec2(w, 40.0), Sense::click());
    let hover = ui.ctx().animate_bool(resp.id.with("hover"), resp.hovered());
    let painter = ui.painter();
    if selected {
        painter.rect_filled(rect, 10, p.accent_soft);
        let bar = Rect::from_min_size(pos2(rect.left(), rect.center().y - 9.0), vec2(3.0, 18.0));
        painter.rect_filled(bar, 2, p.accent);
    } else if hover > 0.0 {
        let c = if p.dark {
            Color32::from_white_alpha((14.0 * hover) as u8)
        } else {
            Color32::from_black_alpha((10.0 * hover) as u8)
        };
        painter.rect_filled(rect, 10, c);
    }
    let fg = if selected {
        p.text
    } else {
        mix(p.muted, p.text, hover * 0.6)
    };
    let icon_col = if selected { p.accent } else { fg };
    painter.text(
        pos2(rect.left() + 16.0, rect.center().y),
        Align2::LEFT_CENTER,
        icon,
        FontId::proportional(16.0),
        icon_col,
    );
    painter.text(
        pos2(rect.left() + 44.0, rect.center().y),
        Align2::LEFT_CENTER,
        label,
        FontId::new(
            14.0,
            if selected {
                bold()
            } else {
                egui::FontFamily::Proportional
            },
        ),
        fg,
    );
    resp.on_hover_cursor(egui::CursorIcon::PointingHand)
}

/// Filled accent button.
pub fn primary_button(ui: &mut Ui, p: &Palette, text: &str, enabled: bool) -> Response {
    let btn = egui::Button::new(
        RichText::new(text)
            .color(p.on_accent)
            .font(FontId::new(14.0, bold())),
    )
    .fill(if enabled {
        p.accent
    } else {
        p.accent.gamma_multiply(0.45)
    })
    .stroke(Stroke::NONE)
    .corner_radius(10)
    .min_size(vec2(0.0, 36.0));
    ui.add_enabled(enabled, btn)
}

/// Quiet outline button.
pub fn secondary_button(ui: &mut Ui, p: &Palette, text: &str) -> Response {
    let btn = egui::Button::new(RichText::new(text).color(p.text))
        .fill(p.card)
        .stroke(Stroke::new(1.0, p.border))
        .corner_radius(10)
        .min_size(vec2(0.0, 36.0));
    ui.add(btn)
}

pub fn danger_button(ui: &mut Ui, p: &Palette, text: &str) -> Response {
    let btn = egui::Button::new(
        RichText::new(text)
            .color(Color32::WHITE)
            .font(FontId::new(14.0, bold())),
    )
    .fill(p.danger)
    .stroke(Stroke::NONE)
    .corner_radius(10)
    .min_size(vec2(0.0, 36.0));
    ui.add(btn)
}

/// A small coloured tag.
pub fn chip(ui: &mut Ui, text: &str, fg: Color32, bg: Color32) {
    Frame::new()
        .fill(bg)
        .corner_radius(CornerRadius::same(255))
        .inner_margin(Margin::symmetric(8, 2))
        .show(ui, |ui| {
            ui.label(RichText::new(text).size(11.5).color(fg));
        });
}

/// Circular gauge with the value in the middle.
pub fn ring(ui: &mut Ui, p: &Palette, frac: f32, size: f32, color: Color32, label: &str) {
    let (rect, _) = ui.allocate_exact_size(vec2(size, size), Sense::hover());
    let c = rect.center();
    let r = size / 2.0 - 5.0;
    let painter = ui.painter();
    painter.circle_stroke(c, r, Stroke::new(8.0, p.track));
    let frac = frac.clamp(0.0, 1.0);
    if frac > 0.001 {
        let segs = (64.0 * frac).ceil().max(2.0) as usize;
        let a0 = -std::f32::consts::FRAC_PI_2;
        let pts: Vec<Pos2> = (0..=segs)
            .map(|i| {
                let a = a0 + std::f32::consts::TAU * frac * i as f32 / segs as f32;
                c + vec2(a.cos(), a.sin()) * r
            })
            .collect();
        painter.add(Shape::line(pts, Stroke::new(8.0, color)));
    }
    painter.text(
        c,
        Align2::CENTER_CENTER,
        label,
        FontId::new(size * 0.2, bold()),
        p.text,
    );
}

/// iOS-style toggle switch.
pub fn toggle(ui: &mut Ui, p: &Palette, on: &mut bool) -> Response {
    let size = vec2(40.0, 22.0);
    let (rect, mut resp) = ui.allocate_exact_size(size, Sense::click());
    if resp.clicked() {
        *on = !*on;
        resp.mark_changed();
    }
    let t = ui.ctx().animate_bool(resp.id, *on);
    let bg = mix(p.track, p.accent, t);
    let painter = ui.painter();
    painter.rect_filled(rect, 11, bg);
    let x = egui::lerp((rect.left() + 11.0)..=(rect.right() - 11.0), t);
    painter.circle_filled(pos2(x, rect.center().y), 8.0, Color32::WHITE);
    resp.on_hover_cursor(egui::CursorIcon::PointingHand)
}

/// Segmented control: returns the newly chosen index when it changes.
pub fn segmented(ui: &mut Ui, p: &Palette, options: &[&str], selected: usize) -> Option<usize> {
    // Painted by hand so it keeps its size and order inside any layout.
    let mut out = None;
    let seg_w = 72.0;
    let pad = 3.0;
    let size = vec2(seg_w * options.len() as f32 + pad * 2.0, 34.0);
    let (rect, _) = ui.allocate_exact_size(size, Sense::hover());
    ui.painter().rect_filled(rect, 10, p.track);
    for (i, o) in options.iter().enumerate() {
        let r = Rect::from_min_size(
            rect.min + vec2(pad + seg_w * i as f32, pad),
            vec2(seg_w, size.y - pad * 2.0),
        );
        let resp = ui.interact(r, ui.scope_id().with(("seg", *o, i)), Sense::click());
        let sel = i == selected;
        let painter = ui.painter();
        if sel {
            painter.rect_filled(r, 8, p.card);
            painter.rect_stroke(r, 8, Stroke::new(1.0, p.border), egui::StrokeKind::Inside);
        } else if resp.hovered() {
            painter.rect_filled(r, 8, p.card.gamma_multiply(0.5));
        }
        painter.text(
            r.center(),
            Align2::CENTER_CENTER,
            *o,
            FontId::new(
                13.0,
                if sel {
                    bold()
                } else {
                    egui::FontFamily::Proportional
                },
            ),
            if sel || resp.hovered() {
                p.text
            } else {
                p.muted
            },
        );
        if resp
            .on_hover_cursor(egui::CursorIcon::PointingHand)
            .clicked()
            && !sel
        {
            out = Some(i);
        }
    }
    out
}

/// Thin separator line in the card colour scheme.
pub fn divider(ui: &mut Ui, p: &Palette) {
    let w = ui.available_width();
    let (rect, _) = ui.allocate_exact_size(vec2(w, 1.0), Sense::hover());
    ui.painter().rect_filled(rect, 0, p.border);
}

/// Key / value row used in info cards.
pub fn kv(ui: &mut Ui, p: &Palette, key: &str, value: &str) {
    ui.horizontal(|ui| {
        ui.allocate_ui_with_layout(
            vec2(130.0, 20.0),
            egui::Layout::left_to_right(egui::Align::Center),
            |ui| {
                ui.set_width(130.0);
                ui.add(egui::Label::new(RichText::new(key).color(p.muted)).truncate());
            },
        );
        ui.add(egui::Label::new(RichText::new(value).color(p.text)).truncate());
    });
}
