// Drawing helpers for HoloFan Studio: the fan preview, clock faces, logo and round buttons.

use chrono::Timelike;
use eframe::egui::{
    self, Align2, Color32, CornerRadius, FontId, Frame, Margin, Pos2, Rect, RichText, Sense,
    Stroke, Ui, pos2, vec2,
};

use crate::theme::{Palette, bold, mix};

pub fn input_frame(p: &Palette) -> Frame {
    Frame::new()
        .fill(if p.dark {
            Color32::from_rgb(18, 21, 26)
        } else {
            Color32::from_rgb(248, 249, 251)
        })
        .stroke(Stroke::new(1.0, p.border))
        .corner_radius(CornerRadius::same(9))
        .inner_margin(Margin::symmetric(10, 7))
}

pub fn modal_frame(p: &Palette) -> Frame {
    Frame::new()
        .fill(p.card)
        .stroke(Stroke::new(1.0, p.border))
        .corner_radius(CornerRadius::same(16))
        .inner_margin(Margin::same(24))
        .shadow(egui::Shadow {
            offset: [0, 16],
            blur: 40,
            spread: 0,
            color: Color32::from_black_alpha(90),
        })
}

pub fn setting_row(ui: &mut Ui, p: &Palette, label: &str, sub: &str, right: impl FnOnce(&mut Ui)) {
    ui.horizontal(|ui| {
        ui.vertical(|ui| {
            ui.label(
                RichText::new(label)
                    .font(FontId::new(14.0, bold()))
                    .color(p.text),
            );
            ui.label(RichText::new(sub).size(12.0).color(p.muted));
        });
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), right);
    });
    ui.add_space(8.0);
}

pub fn time_field(ui: &mut Ui, p: &Palette, v: &mut u32, max: u32, unit: &str) {
    input_frame(p)
        .inner_margin(Margin::symmetric(8, 4))
        .show(ui, |ui| {
            ui.add(
                egui::DragValue::new(v)
                    .range(0..=max)
                    .speed(0.2)
                    .custom_formatter(|n, _| format!("{:02}", n as u32)),
            );
        });
    ui.label(RichText::new(unit).color(p.muted));
    ui.add_space(6.0);
}

/// Round transport button with a caption under it.
pub fn round_button(
    ui: &mut Ui,
    p: &Palette,
    icon: &str,
    caption: &str,
    size: f32,
    active: bool,
    primary: bool,
) -> egui::Response {
    let (rect, resp) = ui.allocate_exact_size(vec2(size, size + 20.0), Sense::click());
    let c = pos2(rect.center().x, rect.top() + size / 2.0);
    let hover = ui.ctx().animate_bool(resp.id.with("h"), resp.hovered());
    let painter = ui.painter();
    let (fill, fg) = if primary {
        (mix(p.accent, Color32::WHITE, hover * 0.12), p.on_accent)
    } else if active {
        (p.accent_soft, p.accent)
    } else {
        (mix(p.track, p.card_hover, hover), p.text)
    };
    if primary {
        painter.circle_filled(
            c + vec2(0.0, 3.0),
            size / 2.0,
            p.accent.gamma_multiply(0.25),
        );
    }
    painter.circle_filled(c, size / 2.0, fill);
    if active {
        painter.circle_stroke(c, size / 2.0, Stroke::new(1.5, p.accent));
    }
    transport_icon(painter, c, if primary { 11.0 } else { 8.5 }, icon, fg);
    painter.text(
        pos2(rect.center().x, rect.bottom() - 6.0),
        Align2::CENTER_CENTER,
        caption,
        FontId::proportional(11.0),
        if active { p.accent } else { p.muted },
    );
    resp.on_hover_text(caption)
        .on_hover_cursor(egui::CursorIcon::PointingHand)
}

/// Vector transport icons: "prev", "next", "play", "pause", "loop1", "loop".
pub fn transport_icon(painter: &egui::Painter, c: Pos2, s: f32, name: &str, col: Color32) {
    let tri = |dir: f32, off: f32| {
        // triangle pointing right (dir = 1) or left (dir = -1), centred at c + off
        let cx = c.x + off;
        vec![
            pos2(cx - dir * s * 0.55, c.y - s * 0.75),
            pos2(cx + dir * s * 0.75, c.y),
            pos2(cx - dir * s * 0.55, c.y + s * 0.75),
        ]
    };
    let bar = |x: f32, w: f32| Rect::from_center_size(pos2(x, c.y), vec2(w, s * 1.5));
    match name {
        "play" => {
            painter.add(egui::Shape::convex_polygon(
                tri(1.0, s * 0.12),
                col,
                Stroke::NONE,
            ));
        }
        "pause" => {
            painter.rect_filled(bar(c.x - s * 0.38, s * 0.42), 2, col);
            painter.rect_filled(bar(c.x + s * 0.38, s * 0.42), 2, col);
        }
        "next" => {
            painter.add(egui::Shape::convex_polygon(
                tri(1.0, -s * 0.2),
                col,
                Stroke::NONE,
            ));
            painter.rect_filled(bar(c.x + s * 0.75, s * 0.24), 1, col);
        }
        "prev" => {
            painter.add(egui::Shape::convex_polygon(
                tri(-1.0, s * 0.2),
                col,
                Stroke::NONE,
            ));
            painter.rect_filled(bar(c.x - s * 0.75, s * 0.24), 1, col);
        }
        _ => {
            // loop arrows: an almost-closed circle with an arrow head
            let r = s * 0.85;
            let a0 = -0.5f32;
            let a1 = a0 + std::f32::consts::TAU * 0.82;
            let pts: Vec<Pos2> = (0..=28)
                .map(|i| {
                    let a = a0 + (a1 - a0) * i as f32 / 28.0;
                    c + vec2(a.cos(), a.sin()) * r
                })
                .collect();
            let end = *pts.last().unwrap();
            painter.add(egui::Shape::line(pts, Stroke::new(s * 0.2, col)));
            let tangent = vec2(-a1.sin(), a1.cos());
            let normal = vec2(a1.cos(), a1.sin());
            let head = vec![
                end + tangent * s * 0.45,
                end + normal * s * 0.35,
                end - normal * s * 0.35,
            ];
            painter.add(egui::Shape::convex_polygon(head, col, Stroke::NONE));
            if name == "loop1" {
                painter.text(
                    c,
                    Align2::CENTER_CENTER,
                    "1",
                    FontId::new(s * 0.95, bold()),
                    col,
                );
            }
        }
    }
}

pub fn small_icon_button(ui: &mut Ui, p: &Palette, icon: &str) -> egui::Response {
    ui.add(
        egui::Button::new(
            RichText::new(icon)
                .font(FontId::new(16.0, bold()))
                .color(p.text),
        )
        .fill(p.track)
        .stroke(Stroke::NONE)
        .corner_radius(8)
        .min_size(vec2(32.0, 30.0)),
    )
}

/// Draw a texture clipped to a circle (cover fit).
pub fn image_circle(
    painter: &egui::Painter,
    tex: &egui::TextureHandle,
    c: Pos2,
    r: f32,
    tint: f32,
) {
    let [w, h] = tex.size();
    let aspect = w as f32 / h.max(1) as f32;
    let (sx, sy) = if aspect > 1.0 {
        (1.0 / aspect, 1.0)
    } else {
        (1.0, aspect)
    };
    let mut mesh = egui::Mesh::with_texture(tex.id());
    let col = Color32::from_gray((255.0 * tint.clamp(0.0, 1.0)) as u8);
    mesh.vertices.push(egui::epaint::Vertex {
        pos: c,
        uv: pos2(0.5, 0.5),
        color: col,
    });
    let segs = 72;
    for i in 0..=segs {
        let a = std::f32::consts::TAU * i as f32 / segs as f32;
        let (nx, ny) = (a.cos(), a.sin());
        mesh.vertices.push(egui::epaint::Vertex {
            pos: c + vec2(nx, ny) * r,
            uv: pos2(0.5 + nx * 0.5 * sx, 0.5 + ny * 0.5 * sy),
            color: col,
        });
        if i > 0 {
            mesh.add_triangle(0, i as u32, i as u32 + 1);
        }
    }
    painter.add(egui::Shape::mesh(mesh));
}

/// The hologram fan preview: a dark disc, the current picture (or a glowing placeholder for
/// videos), spinning LED blades and a hub.
#[allow(clippy::too_many_arguments)]
pub fn fan(
    painter: &egui::Painter,
    rect: Rect,
    p: &Palette,
    thumb: Option<&egui::TextureHandle>,
    has_item: bool,
    blades: usize,
    spinning: bool,
    on: bool,
    t: f32,
    brightness: i32,
) {
    let c = rect.center();
    let r = rect.width() / 2.0 - 8.0;
    for k in 0..6 {
        let a = 14 - k * 2;
        painter.circle_filled(
            c,
            r + 8.0 - k as f32 * 1.5,
            p.accent
                .gamma_multiply(a as f32 / 255.0 * if on { 1.0 } else { 0.3 }),
        );
    }
    painter.circle_filled(c, r, Color32::from_rgb(5, 6, 10));
    let lum = 0.45 + 0.055 * brightness as f32;
    if on {
        match thumb {
            Some(tex) => image_circle(painter, tex, c, r - 6.0, lum),
            None => {
                let hue_a = p.accent;
                let hue_b = Color32::from_rgb(56, 189, 248);
                for i in 0..5 {
                    let rr = r * (0.25 + i as f32 * 0.14);
                    let phase = t * (0.8 + i as f32 * 0.25) * if i % 2 == 0 { 1.0 } else { -1.0 };
                    let segs = 48;
                    let len = 0.55 + 0.1 * i as f32;
                    let pts: Vec<Pos2> = (0..=segs)
                        .map(|k| {
                            let a = phase + std::f32::consts::TAU * len * k as f32 / segs as f32;
                            c + vec2(a.cos(), a.sin()) * rr
                        })
                        .collect();
                    let col = mix(hue_a, hue_b, i as f32 / 4.0).gamma_multiply(lum);
                    painter.add(egui::Shape::line(pts, Stroke::new(3.0, col)));
                }
                let label = if has_item { "▶ Playing" } else { "No media" };
                painter.text(
                    c + vec2(0.0, r * 0.12),
                    Align2::CENTER_CENTER,
                    label,
                    FontId::new(14.0, bold()),
                    Color32::from_gray(220).gamma_multiply(lum),
                );
            }
        }
    }
    let n = blades.max(1);
    let base = if spinning { t * 9.0 } else { 0.3 };
    for b in 0..n {
        let a = base + std::f32::consts::TAU * b as f32 / n as f32;
        let dir = vec2(a.cos(), a.sin());
        let alpha = if spinning { 0.16 } else { 0.5 };
        painter.line_segment(
            [c + dir * 14.0, c + dir * (r - 4.0)],
            Stroke::new(
                if spinning { 6.0 } else { 4.0 },
                Color32::from_white_alpha((alpha * 255.0) as u8),
            ),
        );
    }
    painter.circle_stroke(c, r, Stroke::new(2.0, Color32::from_rgb(52, 58, 70)));
    painter.circle_filled(c, 14.0, Color32::from_rgb(30, 34, 42));
    painter.circle_stroke(c, 14.0, Stroke::new(2.0, Color32::from_rgb(70, 76, 90)));
    painter.circle_filled(c, 4.0, p.accent);
}

pub fn fan_icon(painter: &egui::Painter, rect: Rect, p: &Palette, blades: usize, t: f32) {
    let c = rect.center();
    let r = rect.width() / 2.0;
    painter.circle_filled(c, r, Color32::from_rgb(8, 10, 14));
    let n = blades.max(1);
    for b in 0..n {
        let a = t * 0.6 + std::f32::consts::TAU * b as f32 / n as f32;
        painter.line_segment(
            [c, c + vec2(a.cos(), a.sin()) * (r - 3.0)],
            Stroke::new(3.0, p.accent),
        );
    }
    painter.circle_filled(c, 4.0, Color32::WHITE);
}

/// Clock face in one of the four dial styles (0 digital, 1 symbol, 2 zodiac, 3 constellation).
pub fn clock(
    painter: &egui::Painter,
    rect: Rect,
    dial: usize,
    needle_white: bool,
    now: &chrono::DateTime<chrono::Local>,
    large: bool,
) {
    let c = rect.center();
    let r = rect.width() / 2.0;
    let needle = if needle_white {
        Color32::WHITE
    } else {
        Color32::from_rgb(20, 20, 24)
    };
    // a black hand needs a light dial to be visible
    let dial_fill = if needle_white {
        Color32::from_rgb(10, 12, 18)
    } else {
        Color32::from_rgb(214, 220, 230)
    };
    let ink = if needle_white {
        Color32::from_rgb(200, 206, 220)
    } else {
        Color32::from_rgb(40, 44, 56)
    };
    painter.circle_filled(c, r, dial_fill);
    let (h, m, s) = (now.hour() % 12, now.minute(), now.second());
    let tau = std::f32::consts::TAU;
    let top = -std::f32::consts::FRAC_PI_2;
    match dial {
        0 => {
            let txt = format!("{:02}:{:02}", now.hour(), m);
            painter.text(
                c - vec2(0.0, r * 0.06),
                Align2::CENTER_CENTER,
                txt,
                FontId::new(r * 0.42, bold()),
                ink,
            );
            painter.text(
                c + vec2(0.0, r * 0.34),
                Align2::CENTER_CENTER,
                format!("{s:02}"),
                FontId::proportional((r * 0.16).max(6.0)),
                ink.gamma_multiply(0.7),
            );
            painter.circle_stroke(
                c,
                r * 0.9,
                Stroke::new((r * 0.03).max(1.0), ink.gamma_multiply(0.35)),
            );
            let a = top + tau * s as f32 / 60.0;
            painter.circle_filled(
                c + vec2(a.cos(), a.sin()) * r * 0.9,
                (r * 0.05).max(1.5),
                Color32::from_rgb(99, 102, 241),
            );
            return;
        }
        1 => {
            for i in 0..60 {
                let a = tau * i as f32 / 60.0;
                let d = vec2(a.cos(), a.sin());
                let (l, w) = if i % 5 == 0 {
                    (0.16, r * 0.04)
                } else {
                    (0.06, r * 0.015)
                };
                painter.line_segment(
                    [c + d * r * (0.92 - l), c + d * r * 0.92],
                    Stroke::new(w.max(1.0), ink),
                );
            }
        }
        2 => {
            // the twelve signs as short names on a ring (emoji glyphs don't render cleanly everywhere)
            let signs = [
                "ARI", "TAU", "GEM", "CAN", "LEO", "VIR", "LIB", "SCO", "SAG", "CAP", "AQU", "PIS",
            ];
            painter.circle_stroke(c, r * 0.9, Stroke::new(1.0, ink.gamma_multiply(0.4)));
            painter.circle_stroke(c, r * 0.62, Stroke::new(1.0, ink.gamma_multiply(0.4)));
            for (i, sg) in signs.iter().enumerate() {
                let a = top + tau * i as f32 / 12.0;
                let edge = top + tau * (i as f32 + 0.5) / 12.0;
                let e = vec2(edge.cos(), edge.sin());
                painter.line_segment(
                    [c + e * r * 0.62, c + e * r * 0.9],
                    Stroke::new(1.0, ink.gamma_multiply(0.4)),
                );
                painter.text(
                    c + vec2(a.cos(), a.sin()) * r * 0.76,
                    Align2::CENTER_CENTER,
                    *sg,
                    FontId::new((r * 0.11).max(5.0), bold()),
                    ink,
                );
            }
        }
        _ => {
            let pts: Vec<Pos2> = (0..12)
                .map(|i| {
                    let a = top + tau * i as f32 / 12.0;
                    let wob = if i % 2 == 0 { 0.80 } else { 0.66 };
                    c + vec2(a.cos(), a.sin()) * r * wob
                })
                .collect();
            for i in 0..12 {
                painter.line_segment(
                    [pts[i], pts[(i + 1) % 12]],
                    Stroke::new(1.0, ink.gamma_multiply(0.45)),
                );
            }
            for (i, pt) in pts.iter().enumerate() {
                painter.circle_filled(*pt, if i % 3 == 0 { r * 0.05 } else { r * 0.03 }, ink);
            }
        }
    }
    let hand = |frac: f32, len: f32, w: f32, col: Color32| {
        let a = top + tau * frac;
        painter.line_segment(
            [c, c + vec2(a.cos(), a.sin()) * r * len],
            Stroke::new(w.max(1.0), col),
        );
    };
    hand((h as f32 + m as f32 / 60.0) / 12.0, 0.5, r * 0.06, needle);
    hand((m as f32 + s as f32 / 60.0) / 60.0, 0.72, r * 0.04, needle);
    if large {
        hand(
            s as f32 / 60.0,
            0.8,
            r * 0.015,
            Color32::from_rgb(239, 68, 68),
        );
    }
    painter.circle_filled(c, (r * 0.06).max(2.0), needle);
}

pub fn logo(painter: &egui::Painter, rect: Rect, accent: Color32, t: f32) {
    let violet = Color32::from_rgb(168, 85, 247);
    let r = (rect.width() * 0.28) as u8;
    painter.rect_filled(rect, r, accent);
    let glow = Rect::from_min_max(rect.center(), rect.max);
    let gr = CornerRadius {
        nw: (rect.width() * 0.5) as u8,
        ne: 0,
        sw: 0,
        se: r,
    };
    painter.rect_filled(glow, gr, violet.gamma_multiply(0.55));
    let c = rect.center();
    let rr = rect.width() * 0.32;
    painter.circle_stroke(c, rr, Stroke::new(1.5, Color32::WHITE.gamma_multiply(0.85)));
    // one LED bar through the hub, slowly turning, with a soft trail
    for k in 0..3 {
        let a = t * 1.5 - k as f32 * 0.35;
        let d = vec2(a.cos(), a.sin()) * (rr - 2.0);
        painter.line_segment(
            [c - d, c + d],
            Stroke::new(2.2, Color32::WHITE.gamma_multiply(1.0 - k as f32 * 0.35)),
        );
    }
    painter.circle_filled(c, 3.0, Color32::WHITE);
}

pub fn short(s: &str, max: usize) -> String {
    let max = max.max(4);
    if s.chars().count() <= max {
        s.to_string()
    } else {
        let mut out: String = s.chars().take(max - 1).collect();
        out.push('…');
        out
    }
}
