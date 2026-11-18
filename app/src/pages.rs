// Player, Clock and Device pages (Library, Playlists and Fans are in pages_lib.rs).

use std::time::Duration;

use eframe::egui::{
    self, Align, Align2, Color32, CornerRadius, FontId, Frame, Layout, Margin, Rect, RichText,
    ScrollArea, Sense, Stroke, TextEdit, Ui, pos2, vec2,
};

use crate::app::{App, Confirm, DIALS, LoopMode, status_look};
use crate::device::{Cmd, LogKind};
use crate::models::MODELS;
use crate::paint::{
    self, input_frame, round_button, setting_row, short, small_icon_button, time_field,
};
use crate::theme::{Palette, bold, mix};
use crate::widgets::*;

impl App {
    // ------------------------------------------------------------------ player

    pub(crate) fn page_player(&mut self, ui: &mut Ui, p: &Palette) {
        let ctx = ui.ctx().clone();
        page_header(ui, p, "Player", "Control what your hologram fan shows");
        let time = ctx.input(|i| i.time) as f32;
        let total_w = ui.available_width();
        let left_w = (total_w * 0.52).floor();
        let right_w = total_w - left_w - 16.0;

        ui.horizontal_top(|ui| {
            ui.spacing_mut().item_spacing.x = 16.0;
            // ---- left: preview + transport
            ui.vertical(|ui| {
                ui.set_width(left_w);
                card(ui, p, |ui| {
                    ui.set_width(left_w - 36.0);
                    let st = self.dev.fan.clone();
                    ui.horizontal(|ui| {
                        ui.label(title("Now showing", 16.0, p.text));
                        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                            let mode = if self.loop_mode == LoopMode::Single {
                                "Single loop"
                            } else {
                                "List loop"
                            };
                            chip(ui, mode, p.accent, p.accent_soft);
                            if !self.powered {
                                chip(ui, "Off", p.muted, p.track);
                            }
                        });
                    });
                    ui.add_space(6.0);
                    // what the fan plays now, and its preview when the library has that file
                    let now_name = st
                        .as_ref()
                        .and_then(|s| s.files.get(s.current as usize).cloned());
                    self.lib.request_thumbs(&ctx);
                    let thumb = now_name
                        .as_ref()
                        .and_then(|n| self.lib_match(n))
                        .and_then(|f| f.thumb.clone());
                    let blades = self.model.map(|m| MODELS[m].blades()).unwrap_or(2);
                    let size = (left_w - 36.0).min(250.0);
                    ui.vertical_centered(|ui| {
                        let (rect, _) = ui.allocate_exact_size(vec2(size, size), Sense::hover());
                        paint::fan(
                            ui.painter(),
                            rect,
                            p,
                            thumb.as_ref(),
                            now_name.is_some(),
                            blades,
                            self.playing && self.powered,
                            self.powered,
                            time,
                            self.brightness,
                        );
                    });
                    ui.add_space(8.0);
                    ui.vertical_centered(|ui| match (&now_name, &st) {
                        (Some(name), Some(st)) => {
                            ui.label(
                                RichText::new(short(name, 44))
                                    .font(FontId::new(15.0, bold()))
                                    .color(p.text),
                            );
                            ui.label(
                                RichText::new(format!(
                                    "File {} of {} on the fan",
                                    st.current as usize + 1,
                                    st.files.len()
                                ))
                                .size(12.0)
                                .color(p.muted),
                            );
                        }
                        (None, Some(_)) => {
                            ui.label(
                                RichText::new("The fan has no files yet")
                                    .font(FontId::new(15.0, bold()))
                                    .color(p.text),
                            );
                            ui.label(
                                RichText::new("Send some from the Library or Playlists page")
                                    .size(12.0)
                                    .color(p.muted),
                            );
                        }
                        _ => {
                            ui.label(
                                RichText::new("Not connected")
                                    .font(FontId::new(15.0, bold()))
                                    .color(p.text),
                            );
                            ui.label(
                                RichText::new("Join the fan's Wi-Fi; the app connects by itself")
                                    .size(12.0)
                                    .color(p.muted),
                            );
                        }
                    });
                    ui.add_space(12.0);
                    // transport: Last one · Single loop · Play/Pause · List loop · Next one
                    let bw = 50.0;
                    let gap = 14.0;
                    let row_w = bw * 4.0 + 62.0 + gap * 4.0;
                    ui.horizontal(|ui| {
                        ui.add_space(((ui.available_width() - row_w) / 2.0).max(0.0));
                        ui.spacing_mut().item_spacing.x = gap;
                        if round_button(ui, p, "prev", "Previous", bw, false, false).clicked() {
                            self.send(&ctx, Cmd::Prev);
                        }
                        if round_button(
                            ui,
                            p,
                            "loop1",
                            "Single loop",
                            bw,
                            self.loop_mode == LoopMode::Single,
                            false,
                        )
                        .clicked()
                        {
                            self.loop_mode = LoopMode::Single;
                            self.send(&ctx, Cmd::SingleLoop);
                        }
                        let (pp, cap) = if self.playing {
                            ("pause", "Pause")
                        } else {
                            ("play", "Play")
                        };
                        if round_button(ui, p, pp, cap, 62.0, false, true).clicked() {
                            self.playing = !self.playing;
                            self.send(&ctx, Cmd::PlayPause);
                        }
                        if round_button(
                            ui,
                            p,
                            "loop",
                            "List loop",
                            bw,
                            self.loop_mode == LoopMode::List,
                            false,
                        )
                        .clicked()
                        {
                            self.loop_mode = LoopMode::List;
                            self.send(&ctx, Cmd::ListLoop);
                        }
                        if round_button(ui, p, "next", "Next", bw, false, false).clicked() {
                            self.send(&ctx, Cmd::Next);
                        }
                    });
                    ui.add_space(10.0);
                    ui.vertical_centered(|ui| {
                        if secondary_button(ui, p, "  ■  Blank (black)  ")
                            .on_hover_text("Show nothing: plays the fan's BLANK file, and sends it first if the fan doesn't have it")
                            .clicked()
                        {
                            self.show_blank(&ctx);
                        }
                    });
                });
            });

            // ---- right: status, display, picture time
            ui.vertical(|ui| {
                ui.set_width(right_w);
                card(ui, p, |ui| {
                    ui.set_width(right_w - 36.0);
                    ui.horizontal(|ui| {
                        ui.label(title("Device status", 16.0, p.text));
                        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                            let (dot, text) = status_look(p, &self.dev.status);
                            chip(ui, text, dot, mix(p.card, dot, 0.16));
                        });
                    });
                    ui.add_space(6.0);
                    kv(ui, p, "Fan model", &self.model_name());
                    if let Some(m) = self.model {
                        kv(
                            ui,
                            p,
                            "Diameter",
                            &format!("{:.0} cm · {} blades", MODELS[m].cm(), MODELS[m].blades()),
                        );
                    }
                    kv(ui, p, "Address", &self.dev.address.clone());
                    if let Some(st) = self.dev.fan.clone() {
                        let state = if st.sleeping {
                            "Sleeping (off)".to_string()
                        } else {
                            format!(
                                "{} · {}",
                                if st.paused { "Paused" } else { "Playing" },
                                if st.list_loop {
                                    "list loop"
                                } else {
                                    "single loop"
                                }
                            )
                        };
                        kv(ui, p, "Fan reports", &state);
                        if st.audio_version {
                            kv(ui, p, "Sound", "This fan plays sound");
                        }
                    }
                    ui.add_space(4.0);
                    ui.horizontal(|ui| {
                        ui.label(
                            RichText::new("Power")
                                .font(FontId::new(14.0, bold()))
                                .color(p.text),
                        );
                        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                            let mut on = self.powered;
                            if toggle(ui, p, &mut on).changed() {
                                self.powered = on;
                                self.send(&ctx, Cmd::Power);
                            }
                            ui.label(
                                RichText::new(if self.powered { "On" } else { "Off" })
                                    .color(p.muted),
                            );
                        });
                    });
                });
                ui.add_space(14.0);
                card(ui, p, |ui| {
                    ui.set_width(right_w - 36.0);
                    ui.label(title("Display", 16.0, p.text));
                    ui.add_space(8.0);
                    // the fan has three levels (status byte 4: 0 high, 1 medium, 2 low)
                    let level = self
                        .dev
                        .fan
                        .as_ref()
                        .map(|st| st.brightness.min(2) as usize);
                    ui.horizontal(|ui| {
                        ui.label(RichText::new("Brightness").color(p.text));
                        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                            let now = self
                                .dev
                                .fan
                                .as_ref()
                                .map(|st| format!("Fan reports: {}", st.brightness_label()))
                                .unwrap_or_else(|| "Not reported yet".into());
                            ui.label(RichText::new(now).size(11.5).color(p.muted));
                        });
                    });
                    ui.add_space(6.0);
                    ui.horizontal(|ui| {
                        if small_icon_button(ui, p, "−")
                            .on_hover_text("Darker")
                            .clicked()
                        {
                            self.send(&ctx, Cmd::BrightnessDown);
                        }
                        // shown low to high; a click sends the steps needed to get there
                        let shown = level.map(|l| 2 - l).unwrap_or(usize::MAX);
                        if let Some(want) = segmented(ui, p, &["Low", "Medium", "High"], shown) {
                            let cur = level.map(|l| 2 - l as i32).unwrap_or(1);
                            let diff = want as i32 - cur;
                            for _ in 0..diff.abs() {
                                self.send(
                                    &ctx,
                                    if diff > 0 {
                                        Cmd::BrightnessUp
                                    } else {
                                        Cmd::BrightnessDown
                                    },
                                );
                            }
                        }
                        if small_icon_button(ui, p, "+")
                            .on_hover_text("Brighter")
                            .clicked()
                        {
                            self.send(&ctx, Cmd::BrightnessUp);
                        }
                    });
                    ui.add_space(12.0);
                    ui.horizontal(|ui| {
                        ui.vertical(|ui| {
                            ui.label(RichText::new("Image alignment").color(p.text));
                            ui.label(
                                RichText::new("Turn the picture if it looks rotated")
                                    .size(11.5)
                                    .color(p.muted),
                            );
                        });
                        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                            if secondary_button(ui, p, " CW ⟳ ")
                                .on_hover_text("Clockwise adjust")
                                .clicked()
                            {
                                self.send(&ctx, Cmd::RotateCw);
                            }
                            if secondary_button(ui, p, " ⟲ CCW ")
                                .on_hover_text("Counter-clockwise adjust")
                                .clicked()
                            {
                                self.send(&ctx, Cmd::RotateCcw);
                            }
                        });
                    });
                });
                ui.add_space(14.0);
                card(ui, p, |ui| {
                    ui.set_width(right_w - 36.0);
                    ui.label(title("Picture play time", 16.0, p.text));
                    ui.label(
                        RichText::new("How long each picture stays on (5–30 s)")
                            .size(11.5)
                            .color(p.muted),
                    );
                    ui.add_space(6.0);
                    ui.horizontal(|ui| {
                        let mut v = self.picture_secs;
                        ui.spacing_mut().slider_width = (right_w - 36.0 - 170.0).max(80.0);
                        if ui
                            .add(egui::Slider::new(&mut v, 5..=30).suffix(" s"))
                            .changed()
                        {
                            self.picture_secs = v;
                        }
                        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                            if primary_button(ui, p, " Apply ", true).clicked() {
                                let s = self.picture_secs;
                                self.send(&ctx, Cmd::PictureTime(s));
                            }
                        });
                    });
                });
            });
        });

        ui.add_space(16.0);
        self.activity_log(ui, p, 150.0);
    }

    fn activity_log(&mut self, ui: &mut Ui, p: &Palette, height: f32) {
        card(ui, p, |ui| {
            ui.set_min_width(ui.available_width());
            ui.horizontal(|ui| {
                ui.label(title("Activity", 16.0, p.text));
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    if ui
                        .add(
                            egui::Button::new(RichText::new("Clear").size(12.0).color(p.muted))
                                .frame(false),
                        )
                        .clicked()
                    {
                        self.dev.log.clear();
                    }
                });
            });
            ui.add_space(6.0);
            Frame::new()
                .fill(if p.dark {
                    Color32::from_rgb(16, 19, 24)
                } else {
                    Color32::from_rgb(248, 249, 251)
                })
                .corner_radius(CornerRadius::same(10))
                .inner_margin(Margin::symmetric(12, 8))
                .show(ui, |ui| {
                    ui.set_min_width(ui.available_width());
                    ScrollArea::vertical()
                        .max_height(height)
                        .auto_shrink([false, false])
                        .stick_to_bottom(true)
                        .show(ui, |ui| {
                            for l in &self.dev.log {
                                let (col, tag) = match l.kind {
                                    LogKind::Info => (p.muted, "•"),
                                    LogKind::Ok => (p.success, "✔"),
                                    LogKind::Warn => (p.warning, "!"),
                                    LogKind::Sent => (p.accent, "→"),
                                    LogKind::Fan => (p.text, "←"),
                                };
                                ui.horizontal(|ui| {
                                    ui.label(
                                        RichText::new(&l.time)
                                            .monospace()
                                            .size(11.5)
                                            .color(p.faint),
                                    );
                                    ui.label(RichText::new(tag).color(col));
                                    ui.label(RichText::new(&l.text).size(12.5).color(
                                        if l.kind == LogKind::Info {
                                            p.muted
                                        } else {
                                            p.text
                                        },
                                    ));
                                });
                            }
                        });
                });
        });
    }

    // ------------------------------------------------------------------ clock

    pub(crate) fn page_clock(&mut self, ui: &mut Ui, p: &Palette) {
        let ctx = ui.ctx().clone();
        page_header(
            ui,
            p,
            "Clock",
            "Let the fan show a clock and set the power-on display time",
        );
        let total_w = ui.available_width();
        let left_w = (total_w * 0.56).floor();
        let right_w = total_w - left_w - 16.0;
        let now = chrono::Local::now();
        ui.horizontal_top(|ui| {
            ui.spacing_mut().item_spacing.x = 16.0;
            ui.vertical(|ui| {
                ui.set_width(left_w);
                card(ui, p, |ui| {
                    ui.set_width(left_w - 36.0);
                    ui.label(title("Clock settings", 16.0, p.text));
                    ui.add_space(10.0);
                    setting_row(ui, p, "Show clock", "Display the time on the fan", |ui| {
                        toggle(ui, p, &mut self.clock_on);
                    });
                    divider(ui, p);
                    ui.add_space(8.0);
                    setting_row(ui, p, "Hand colour", "Colour of the clock hands", |ui| {
                        let idx = if self.needle_white { 0 } else { 1 };
                        if let Some(i) = segmented(ui, p, &["White", "Black"], idx) {
                            self.needle_white = i == 0;
                        }
                    });
                    divider(ui, p);
                    ui.add_space(8.0);
                    ui.label(
                        RichText::new("Dial style")
                            .font(FontId::new(14.0, bold()))
                            .color(p.text),
                    );
                    ui.add_space(6.0);
                    let gap = 10.0;
                    let tw = ((ui.available_width() - gap * 3.0) / 4.0).floor();
                    ui.horizontal(|ui| {
                        ui.spacing_mut().item_spacing.x = gap;
                        for (i, d) in DIALS.iter().enumerate() {
                            let (r, resp) =
                                ui.allocate_exact_size(vec2(tw, tw + 22.0), Sense::click());
                            let sel = i == self.dial;
                            let painter = ui.painter();
                            painter.rect_filled(
                                r,
                                12,
                                if sel {
                                    p.accent_soft
                                } else if resp.hovered() {
                                    p.card_hover
                                } else {
                                    p.card
                                },
                            );
                            painter.rect_stroke(
                                r,
                                12,
                                Stroke::new(
                                    if sel { 2.0 } else { 1.0 },
                                    if sel { p.accent } else { p.border },
                                ),
                                egui::StrokeKind::Inside,
                            );
                            let face = Rect::from_center_size(
                                pos2(r.center().x, r.top() + tw / 2.0 + 2.0),
                                vec2(tw - 24.0, tw - 24.0),
                            );
                            paint::clock(painter, face, i, self.needle_white, &now, false);
                            painter.text(
                                pos2(r.center().x, r.bottom() - 14.0),
                                Align2::CENTER_CENTER,
                                *d,
                                FontId::new(
                                    12.0,
                                    if sel {
                                        bold()
                                    } else {
                                        egui::FontFamily::Proportional
                                    },
                                ),
                                if sel { p.text } else { p.muted },
                            );
                            if resp
                                .on_hover_cursor(egui::CursorIcon::PointingHand)
                                .clicked()
                            {
                                self.dial = i;
                            }
                        }
                    });
                    ui.add_space(14.0);
                    ui.horizontal(|ui| {
                        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                            if primary_button(ui, p, "  Apply clock  ", true).clicked() {
                                let cmd = Cmd::Clock {
                                    on: self.clock_on,
                                    needle_white: self.needle_white,
                                    dial: DIALS[self.dial],
                                };
                                self.send_clock(&ctx, cmd);
                            }
                        });
                    });
                });
                ui.add_space(14.0);
                card(ui, p, |ui| {
                    ui.set_width(left_w - 36.0);
                    ui.label(title("Power-on display time", 16.0, p.text));
                    ui.label(
                        RichText::new(
                            "Time shown on the fan's start-up screen (hours, minutes, seconds)",
                        )
                        .size(12.0)
                        .color(p.muted),
                    );
                    ui.add_space(10.0);
                    ui.horizontal(|ui| {
                        time_field(ui, p, &mut self.boot_h, 23, "h");
                        time_field(ui, p, &mut self.boot_m, 59, "min");
                        time_field(ui, p, &mut self.boot_s, 59, "s");
                        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                            if primary_button(ui, p, "  Apply  ", true).clicked() {
                                let cmd = Cmd::PowerOnTime {
                                    h: self.boot_h,
                                    m: self.boot_m,
                                    s: self.boot_s,
                                };
                                self.send_clock(&ctx, cmd);
                            }
                        });
                    });
                });
            });
            ui.vertical(|ui| {
                ui.set_width(right_w);
                card(ui, p, |ui| {
                    ui.set_width(right_w - 36.0);
                    ui.label(title("Preview", 16.0, p.text));
                    ui.label(
                        RichText::new(format!(
                            "{} dial · {} hands",
                            DIALS[self.dial],
                            if self.needle_white { "white" } else { "black" }
                        ))
                        .size(12.0)
                        .color(p.muted),
                    );
                    ui.add_space(10.0);
                    let size = (right_w - 36.0).min(320.0);
                    ui.vertical_centered(|ui| {
                        let (rect, _) = ui.allocate_exact_size(vec2(size, size), Sense::hover());
                        let painter = ui.painter();
                        painter.circle_filled(
                            rect.center(),
                            size / 2.0,
                            Color32::from_rgb(6, 8, 12),
                        );
                        painter.circle_stroke(
                            rect.center(),
                            size / 2.0,
                            Stroke::new(2.0, p.border),
                        );
                        paint::clock(
                            painter,
                            rect.shrink(16.0),
                            self.dial,
                            self.needle_white,
                            &now,
                            true,
                        );
                        if !self.clock_on {
                            painter.circle_filled(
                                rect.center(),
                                size / 2.0,
                                Color32::from_black_alpha(150),
                            );
                            let pill = Rect::from_center_size(rect.center(), vec2(120.0, 32.0));
                            painter.rect_filled(pill, 16, Color32::from_rgb(30, 34, 42));
                            painter.text(
                                rect.center(),
                                Align2::CENTER_CENTER,
                                "Clock is off",
                                FontId::new(13.0, bold()),
                                Color32::from_gray(210),
                            );
                        }
                    });
                    ui.add_space(6.0);
                });
            });
        });
        ctx.request_repaint_after(Duration::from_millis(500));
    }

    // ------------------------------------------------------------------ device

    pub(crate) fn page_device(&mut self, ui: &mut Ui, p: &Palette) {
        let ctx = ui.ctx().clone();
        page_header(
            ui,
            p,
            "Device",
            "Fan model, connection, Wi-Fi and maintenance",
        );
        let total_w = ui.available_width();
        let col_w = ((total_w - 16.0) / 2.0).floor();
        let t = ctx.input(|i| i.time) as f32;
        ui.horizontal_top(|ui| {
            ui.spacing_mut().item_spacing.x = 16.0;
            ui.vertical(|ui| {
                ui.set_width(col_w);
                card(ui, p, |ui| {
                    ui.set_width(col_w - 36.0);
                    ui.label(title("Fan model", 16.0, p.text));
                    ui.add_space(8.0);
                    ui.horizontal(|ui| {
                        let (r, _) = ui.allocate_exact_size(vec2(56.0, 56.0), Sense::hover());
                        paint::fan_icon(ui.painter(), r, p, self.model.map(|m| MODELS[m].blades()).unwrap_or(2), t);
                        ui.vertical(|ui| {
                            ui.label(RichText::new(self.model_name()).font(FontId::new(18.0, bold())).color(p.text));
                            if let Some(m) = self.model {
                                let vs = MODELS[m].video_size();
                                ui.label(RichText::new(format!("{:.1} cm · videos prepared at {vs}×{vs}", MODELS[m].cm())).size(12.0).color(p.muted));
                            }
                        });
                    });
                    ui.add_space(10.0);
                    if secondary_button(ui, p, "  Change model  ").clicked() {
                        self.picking_model = true;
                    }
                });
                ui.add_space(14.0);
                card(ui, p, |ui| {
                    ui.set_width(col_w - 36.0);
                    ui.horizontal(|ui| {
                        ui.label(title("Connection", 16.0, p.text));
                        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                            let (dot, text) = status_look(p, &self.dev.status);
                            chip(ui, text, dot, mix(p.card, dot, 0.16));
                        });
                    });
                    ui.label(RichText::new("Join the fan's Wi-Fi first. The fan is always at 192.168.4.1, port 20320.").size(12.0).color(p.muted));
                    ui.add_space(10.0);
                    ui.label(RichText::new("Fan address (IP:port)").size(12.5).color(p.text));
                    input_frame(p).show(ui, |ui| {
                        ui.add(TextEdit::singleline(&mut self.addr_edit).frame(Frame::NONE).desired_width(f32::INFINITY).hint_text("192.168.4.1:20320"));
                    });
                    ui.add_space(10.0);
                    ui.horizontal(|ui| {
                        if primary_button(ui, p, if self.dev.is_active() { "  Reconnect  " } else { "  Connect  " }, true).clicked() {
                            self.dev.address = self.addr_edit.trim().to_string();
                            self.dev.connect(&ctx);
                            self.addr_edit = self.dev.address.clone();
                        }
                        if self.dev.is_active() && secondary_button(ui, p, "  Disconnect  ").clicked() {
                            self.dev.disconnect();
                        }
                    });
                });
                ui.add_space(14.0);
                card(ui, p, |ui| {
                    ui.set_width(col_w - 36.0);
                    setting_row(ui, p, "Settings", "Theme, about this app and updates", |ui| {
                        if secondary_button(ui, p, "  Open settings  ").clicked() {
                            self.page = crate::app::Page::Settings;
                        }
                    });
                });
            });
            ui.vertical(|ui| {
                ui.set_width(col_w);
                card(ui, p, |ui| {
                    ui.set_width(col_w - 36.0);
                    ui.label(title("Wi-Fi setup", 16.0, p.text));
                    ui.horizontal(|ui| {
                        ui.label(RichText::new("Rename the fan's own Wi-Fi and set its password.").size(12.0).color(p.muted));
                        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                            if ui.add_enabled(self.dev.is_connected(), egui::Button::new(RichText::new("Read current name").size(12.0).color(p.accent)).frame(false)).clicked() {
                                self.send(&ctx, Cmd::AskWifi);
                            }
                        });
                    });
                    ui.add_space(10.0);
                    ui.label(RichText::new("Network name (SSID)").size(12.5).color(p.text));
                    input_frame(p).show(ui, |ui| {
                        ui.add(TextEdit::singleline(&mut self.wifi_ssid).frame(Frame::NONE).desired_width(f32::INFINITY).hint_text("My Wi-Fi"));
                    });
                    ui.add_space(8.0);
                    ui.label(RichText::new("Password").size(12.5).color(p.text));
                    input_frame(p).show(ui, |ui| {
                        ui.horizontal(|ui| {
                            let w = ui.available_width() - 48.0;
                            ui.add(TextEdit::singleline(&mut self.wifi_pass).password(!self.show_pass).frame(Frame::NONE).desired_width(w).hint_text("8–16 characters"));
                            if ui.add(egui::Button::new(RichText::new(if self.show_pass { "Hide" } else { "Show" }).size(12.0).color(p.muted)).frame(false)).clicked() {
                                self.show_pass = !self.show_pass;
                            }
                        });
                    });
                    ui.add_space(10.0);
                    let pw = self.wifi_pass.chars().count();
                    let pw_ok = (8..=16).contains(&pw);
                    if !self.wifi_pass.is_empty() && !pw_ok {
                        ui.label(RichText::new("The password needs 8–16 characters.").size(12.0).color(p.warning));
                    }
                    let ok = !self.wifi_ssid.trim().is_empty() && pw_ok;
                    if primary_button(ui, p, "  Send Wi-Fi settings  ", ok).clicked() {
                        let cmd = Cmd::Wifi { ssid: self.wifi_ssid.trim().to_string(), password: self.wifi_pass.clone() };
                        self.send(&ctx, cmd);
                    }
                });
                ui.add_space(14.0);
                card(ui, p, |ui| {
                    ui.set_width(col_w - 36.0);
                    ui.label(title("Maintenance", 16.0, p.text));
                    ui.add_space(8.0);
                    setting_row(ui, p, "Clear cache", "Remove temporary data on the fan", |ui| {
                        if secondary_button(ui, p, "  Clear cache  ").clicked() {
                            self.confirm = Some(Confirm::ClearCache);
                        }
                    });
                    divider(ui, p);
                    ui.add_space(8.0);
                    setting_row(ui, p, "Format fan storage", "Erase every video on the fan", |ui| {
                        if danger_button(ui, p, "  Format  ").clicked() {
                            self.confirm = Some(Confirm::Format);
                        }
                    });
                });
            });
        });
    }

    /// The library file a fan file name stands for (the fan may show short 8-letter names).
    pub(crate) fn lib_match(&self, fan_name: &str) -> Option<&crate::library::BinFile> {
        let norm = |s: &str| -> String {
            let stem = s.trim().trim_end_matches(".BIN").trim_end_matches(".bin");
            stem.chars()
                .map(|c| {
                    if c == ' ' {
                        '_'
                    } else {
                        c.to_ascii_uppercase()
                    }
                })
                .filter(|c| c.is_alphanumeric() || *c == '_')
                .collect()
        };
        let want = norm(fan_name);
        if want.is_empty() {
            return None;
        }
        let short8 = |s: String| s.chars().take(8).collect::<String>();
        self.lib
            .files
            .iter()
            .find(|f| norm(&f.name) == want)
            .or_else(|| {
                self.lib
                    .files
                    .iter()
                    .find(|f| short8(norm(&f.name)) == short8(want.clone()))
            })
    }

    /// Clock commands only work on fans that report a clock, as in the original.
    fn send_clock(&mut self, ctx: &egui::Context, cmd: Cmd) {
        if let Some(st) = &self.dev.fan {
            if !st.clock_version {
                self.notify(ctx, "This fan doesn't support the clock display", false);
                self.dev.push_log(
                    LogKind::Warn,
                    "The connected fan reports no clock, so the clock settings weren't sent.",
                );
                return;
            }
        }
        self.send(ctx, cmd);
    }

    /// The files stored on the fan, as it reports them, with Play and Delete.
    pub(crate) fn fan_files(&mut self, ui: &mut Ui, p: &Palette) {
        let ctx = ui.ctx().clone();
        let Some(st) = self.dev.fan.clone() else {
            card(ui, p, |ui| {
                ui.set_min_width(ui.available_width());
                ui.label(title("Files on the fan", 16.0, p.text));
                ui.label(
                    RichText::new("Connect to the fan to see the files it has stored.")
                        .size(12.0)
                        .color(p.muted),
                );
            });
            return;
        };
        card(ui, p, |ui| {
            ui.set_min_width(ui.available_width());
            ui.horizontal(|ui| {
                ui.label(title("Files on the fan", 16.0, p.text));
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    chip(
                        ui,
                        &format!("{} file(s)", st.files.len()),
                        p.accent,
                        p.accent_soft,
                    );
                });
            });
            ui.add_space(6.0);
            if st.files.is_empty() {
                ui.label(
                    RichText::new(
                        "The fan has no files yet. Upload .BIN files or put them on its SD card.",
                    )
                    .size(12.0)
                    .color(p.muted),
                );
                return;
            }
            let mut action: Option<Cmd> = None;
            for (i, name) in st.files.iter().enumerate() {
                let playing = i == st.current as usize;
                Frame::new()
                    .fill(if playing {
                        p.accent_soft
                    } else {
                        Color32::TRANSPARENT
                    })
                    .corner_radius(CornerRadius::same(8))
                    .inner_margin(Margin::symmetric(10, 4))
                    .show(ui, |ui| {
                        ui.set_min_width(ui.available_width());
                        ui.horizontal(|ui| {
                            ui.label(
                                RichText::new(format!("{:02}", i + 1))
                                    .monospace()
                                    .size(12.0)
                                    .color(p.muted),
                            );
                            ui.label(RichText::new(short(name, 48)).size(13.0).color(p.text));
                            if playing {
                                chip(ui, "Playing", p.accent, p.card);
                            }
                            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                                if ui
                                    .add(
                                        egui::Button::new(
                                            RichText::new("Delete").size(12.0).color(p.danger),
                                        )
                                        .frame(false),
                                    )
                                    .clicked()
                                {
                                    action = Some(Cmd::DeleteFile(i));
                                }
                                if ui
                                    .add(
                                        egui::Button::new(
                                            RichText::new("Play").size(12.0).color(p.accent),
                                        )
                                        .frame(false),
                                    )
                                    .clicked()
                                {
                                    action = Some(Cmd::PlayFile(i));
                                }
                            });
                        });
                    });
            }
            if let Some(cmd) = action {
                self.send(&ctx, cmd);
            }
        });
    }
}
