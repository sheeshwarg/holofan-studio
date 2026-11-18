// Library (fan files and the video converter), Playlists (drag files in, upload, SD card)
// and Fans (several fans at once).

use std::path::PathBuf;

use eframe::egui::{
    self, Align, Align2, Color32, CornerRadius, FontId, Frame, Id, Layout, Margin, RichText,
    ScrollArea, Sense, Stroke, TextEdit, Ui, vec2,
};

use crate::app::{App, Confirm, FanSlot, status_look};
use crate::binfmt::{self, ColorMode};
use crate::device::{Cmd, Device, LogKind};
use crate::library::{self, ConvertSettings, fmt_bytes};
use crate::paint::{self, input_frame, modal_frame, short};
use crate::theme::{Palette, bold, mix};
use crate::widgets::*;

/// The converter window's state.
pub struct ConvertDialog {
    pub files: Vec<PathBuf>,
    pub name: String,
    pub model: usize,
    pub mode: ColorMode,
    pub zoom: f32,
    frame: Option<(Vec<u8>, usize, usize)>,
    preview: Option<egui::TextureHandle>,
    preview_for: Option<(usize, ColorMode, i32)>,
    grabbed: bool,
}

impl ConvertDialog {
    pub fn new(files: Vec<PathBuf>, model: usize, mode: ColorMode, zoom: f32) -> Self {
        let name = if files.len() == 1 {
            files[0]
                .file_stem()
                .map(|s| library::clean_name(&s.to_string_lossy()))
                .unwrap_or_default()
        } else {
            String::new()
        };
        ConvertDialog {
            files,
            name,
            model,
            mode,
            zoom,
            frame: None,
            preview: None,
            preview_for: None,
            grabbed: false,
        }
    }
}

/// Payload when a library file is dragged onto a playlist.
#[derive(Clone)]
struct DragFile(String);
/// Payload when a playlist entry is dragged to a new place.
#[derive(Clone)]
struct DragEntry(usize);

fn fmt_secs(s: f32) -> String {
    let s = s.round() as u32;
    if s >= 60 {
        format!("{}:{:02} min", s / 60, s % 60)
    } else {
        format!("{s} s")
    }
}

fn link_button(ui: &mut Ui, text: &str, color: Color32) -> egui::Response {
    ui.add(egui::Button::new(RichText::new(text).size(12.5).color(color)).frame(false))
}

fn outline(ui: &mut Ui, p: &Palette, text: &str, enabled: bool) -> egui::Response {
    ui.add_enabled(
        enabled,
        egui::Button::new(RichText::new(text).color(p.text))
            .fill(p.card)
            .stroke(Stroke::new(1.0, p.border))
            .corner_radius(10)
            .min_size(vec2(0.0, 36.0)),
    )
}

/// Round preview of a fan file, or a placeholder disc.
fn disc(ui: &mut Ui, p: &Palette, tex: Option<&egui::TextureHandle>, size: f32) -> egui::Response {
    let (rect, resp) = ui.allocate_exact_size(vec2(size, size), Sense::click());
    let painter = ui.painter();
    painter.circle_filled(rect.center(), size / 2.0, Color32::from_rgb(6, 8, 12));
    match tex {
        Some(t) => paint::image_circle(painter, t, rect.center(), size / 2.0 - 1.0, 1.0),
        None => {
            painter.circle_stroke(
                rect.center(),
                size / 2.0 - 6.0,
                Stroke::new(1.5, p.accent.gamma_multiply(0.4)),
            );
            painter.text(
                rect.center(),
                Align2::CENTER_CENTER,
                "BIN",
                FontId::new(size * 0.16, bold()),
                p.faint,
            );
        }
    }
    painter.circle_stroke(rect.center(), size / 2.0, Stroke::new(1.0, p.border));
    resp
}

/// Where the blank (black) file sits in the fan's file list.
pub fn blank_index(files: &[String]) -> Option<usize> {
    files.iter().position(|f| f.trim().to_ascii_uppercase().trim_end_matches(".BIN") == "BLANK")
}

impl App {
    /// The converter's fan type for presets (the fan's own when it is supported).
    fn preset_spec(&self) -> binfmt::Spec {
        binfmt::SPECS[self.converter_model()]
    }

    pub(crate) fn add_preset(&mut self, ctx: &egui::Context, preset: library::Preset) -> Option<String> {
        let spec = self.preset_spec();
        match self.lib.make_preset(preset, &spec) {
            Ok(name) => {
                let msg = format!("{name} is in the library ({})", spec.code);
                self.dev.push_log(LogKind::Ok, msg.clone());
                self.notify(ctx, msg, true);
                Some(name)
            }
            Err(e) => {
                self.notify(ctx, format!("Couldn't make the preset: {e}"), false);
                None
            }
        }
    }

    /// Show nothing on the fan: play its BLANK file, or upload one first and play it as
    /// soon as the fan lists it.
    pub(crate) fn show_blank(&mut self, ctx: &egui::Context) {
        if let Some(i) = self.dev.fan.as_ref().and_then(|st| blank_index(&st.files)) {
            self.send(ctx, Cmd::PlayFile(i));
            return;
        }
        if !self.dev.is_connected() {
            self.notify(ctx, "Not connected to a fan — see the Device page", false);
            return;
        }
        let name = library::Preset::Black.file_name();
        let have = self.lib.file(name).is_some();
        if have || self.add_preset(ctx, library::Preset::Black).is_some() {
            self.blank_pending = true;
            self.send(ctx, Cmd::Upload(vec![self.lib.folder.join(name)]));
            self.notify(ctx, "Sending the blank file to the fan; it plays once it arrives", true);
        }
    }

    pub(crate) fn open_converter(&mut self, files: Vec<PathBuf>) {
        let files: Vec<PathBuf> = files
            .into_iter()
            .filter(|f| library::is_convertible(f))
            .collect();
        if files.is_empty() {
            return;
        }
        let model = self.converter_model();
        self.convert = Some(ConvertDialog::new(
            files,
            model,
            self.convert_mode,
            self.convert_zoom,
        ));
    }

    /// The fan type to convert for: the active fan's, if the converter knows it.
    fn converter_model(&self) -> usize {
        let code = self
            .dev
            .fan
            .as_ref()
            .and_then(|s| s.device_name())
            .or(self.model.map(|m| crate::models::MODELS[m].code));
        code.and_then(|c| binfmt::SPECS.iter().position(|s| s.code == c))
            .unwrap_or(0)
    }

    pub(crate) fn pick_videos(&mut self) {
        let mut all: Vec<&str> = library::VIDEO_EXT.to_vec();
        all.extend(library::IMAGE_EXT);
        if let Some(files) = rfd::FileDialog::new()
            .set_title("Videos or pictures to convert")
            .add_filter("Videos and pictures", &all)
            .pick_files()
        {
            self.open_converter(files);
        }
    }

    pub(crate) fn import_bins(
        &mut self,
        ctx: &egui::Context,
        files: Vec<PathBuf>,
        into_playlist: bool,
    ) {
        let (ok, failed) = self.lib.import(&files);
        if into_playlist {
            if let Some(pl) = self.lib.playlists.get_mut(self.playlist) {
                pl.items.extend(ok.iter().cloned());
            }
            self.lib.save_playlists();
        }
        for f in &failed {
            self.dev
                .push_log(LogKind::Warn, format!("Couldn't add {f}"));
        }
        let msg = format!("Added {} fan file(s) to the library", ok.len());
        self.dev.push_log(LogKind::Info, msg.clone());
        self.notify(ctx, msg, failed.is_empty());
    }

    // ------------------------------------------------------------------ library

    pub(crate) fn page_library(&mut self, ui: &mut Ui, p: &Palette) {
        let ctx = ui.ctx().clone();
        page_header(
            ui,
            p,
            "Library",
            "Your fan files (.BIN): convert videos, preview and send them",
        );
        self.lib.request_thumbs(&ctx);

        ui.horizontal(|ui| {
            if primary_button(ui, p, "  + Convert videos…  ", true).clicked() {
                self.pick_videos();
            }
            if outline(ui, p, "Add .BIN files…", true).clicked() {
                if let Some(files) = rfd::FileDialog::new()
                    .set_title("Fan files")
                    .add_filter("Fan files (*.BIN)", &["BIN", "bin"])
                    .pick_files()
                {
                    self.import_bins(&ctx, files, false);
                }
            }
            if outline(ui, p, "Refresh", true).clicked() {
                self.lib.rescan();
            }
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                if link_button(ui, "Change folder…", p.accent).clicked() {
                    if let Some(d) = rfd::FileDialog::new()
                        .set_title("Library folder")
                        .pick_folder()
                    {
                        self.lib.set_folder(d);
                    }
                }
                ui.label(
                    RichText::new(short(&self.lib.folder.display().to_string(), 46))
                        .size(12.0)
                        .color(p.muted),
                );
            });
        });
        ui.add_space(12.0);

        // ready-made one-colour files
        card(ui, p, |ui| {
            ui.set_min_width(ui.available_width());
            ui.horizontal(|ui| {
                ui.vertical(|ui| {
                    ui.label(title("Presets", 16.0, p.text));
                    ui.label(
                        RichText::new("One-second files in a single colour. Blank (black) lets the fan spin without showing anything.")
                            .size(12.0)
                            .color(p.muted),
                    );
                });
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    for preset in library::Preset::ALL.into_iter().rev() {
                        let [r, g, b] = preset.rgb();
                        let resp = ui.add(
                            egui::Button::new(RichText::new(preset.label()).color(p.text))
                                .fill(p.card)
                                .stroke(Stroke::new(1.0, p.border))
                                .corner_radius(10)
                                .min_size(vec2(0.0, 34.0)),
                        );
                        // a small swatch of the colour on the button's left
                        let c = resp.rect.left_center() + vec2(-8.0, 0.0);
                        ui.painter().circle_filled(c, 6.0, Color32::from_rgb(r, g, b));
                        ui.painter().circle_stroke(c, 6.0, Stroke::new(1.0, p.border));
                        if resp.on_hover_text(format!("Adds {} to the library", preset.file_name())).clicked() {
                            self.add_preset(&ctx, preset);
                        }
                        ui.add_space(14.0);
                    }
                });
            });
        });
        ui.add_space(14.0);

        // conversions in progress
        if !self.lib.jobs.is_empty() {
            card(ui, p, |ui| {
                ui.set_min_width(ui.available_width());
                ui.horizontal(|ui| {
                    ui.label(title("Converting", 16.0, p.text));
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        if !self.lib.busy() && link_button(ui, "Clear", p.muted).clicked() {
                            self.lib.jobs.clear();
                        }
                    });
                });
                let mut cancel = None;
                for j in &self.lib.jobs {
                    ui.add_space(6.0);
                    ui.horizontal(|ui| {
                        let src = j
                            .source
                            .file_name()
                            .map(|n| n.to_string_lossy().to_string())
                            .unwrap_or_default();
                        let out = j
                            .output
                            .file_name()
                            .map(|n| n.to_string_lossy().to_string())
                            .unwrap_or_default();
                        ui.label(
                            RichText::new(format!("{} → {}", short(&src, 34), short(&out, 30)))
                                .size(12.5)
                                .color(p.text),
                        );
                        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                            if !j.done && link_button(ui, "Cancel", p.danger).clicked() {
                                cancel = Some(j.id);
                            }
                            let state = match (&j.error, j.done) {
                                (Some(e), _) => short(e, 40),
                                (None, true) => "Done".into(),
                                (None, false) => format!("{:.0}%", j.progress * 100.0),
                            };
                            ui.label(RichText::new(state).size(12.0).color(if j.error.is_some() {
                                p.danger
                            } else {
                                p.muted
                            }));
                        });
                    });
                    bar(
                        ui,
                        p,
                        j.progress,
                        if j.error.is_some() {
                            p.danger
                        } else {
                            p.accent
                        },
                        5.0,
                    );
                }
                if let Some(id) = cancel {
                    if let Some(j) = self.lib.jobs.iter().find(|j| j.id == id) {
                        j.cancel();
                    }
                }
                ctx.request_repaint_after(std::time::Duration::from_millis(200));
            });
            ui.add_space(14.0);
        }

        // the collection
        card(ui, p, |ui| {
            ui.set_min_width(ui.available_width());
            ui.horizontal(|ui| {
                ui.label(title("Collection", 16.0, p.text));
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    let total: u64 = self.lib.files.iter().map(|f| f.size).sum();
                    chip(
                        ui,
                        &format!("{} file(s) · {}", self.lib.files.len(), fmt_bytes(total)),
                        p.accent,
                        p.accent_soft,
                    );
                });
            });
            ui.add_space(8.0);
            if self.lib.files.is_empty() {
                ui.add_space(16.0);
                ui.vertical_centered(|ui| {
                    ui.label(
                        RichText::new("No fan files yet")
                            .font(FontId::new(15.0, bold()))
                            .color(p.text),
                    );
                    ui.label(
                        RichText::new(
                            "Convert a video, add .BIN files, or drop files onto this window",
                        )
                        .color(p.muted),
                    );
                });
                ui.add_space(16.0);
                return;
            }
            let tile = 172.0;
            let gap = 12.0;
            let cols = (((ui.available_width() + gap) / (tile + gap)).floor() as usize).max(1);
            let names: Vec<String> = self.lib.files.iter().map(|f| f.name.clone()).collect();
            let mut action: Option<(String, u8)> = None;
            for row in names.chunks(cols) {
                ui.horizontal_top(|ui| {
                    ui.spacing_mut().item_spacing.x = gap;
                    for name in row {
                        let Some(f) = self.lib.file(name) else {
                            continue;
                        };
                        Frame::new()
                            .fill(p.bg)
                            .stroke(Stroke::new(1.0, p.border))
                            .corner_radius(CornerRadius::same(12))
                            .inner_margin(Margin::same(10))
                            .show(ui, |ui| {
                                ui.set_width(tile - 20.0);
                                ui.vertical_centered(|ui| {
                                    disc(ui, p, f.thumb.as_ref(), 120.0);
                                    ui.add_space(4.0);
                                    ui.label(
                                        RichText::new(short(&f.stem(), 20))
                                            .font(FontId::new(13.0, bold()))
                                            .color(p.text),
                                    );
                                    let info = match f.spec {
                                        Some(s) => format!(
                                            "{} · {} · {}",
                                            fmt_secs(f.seconds()),
                                            fmt_bytes(f.size),
                                            s.code
                                        ),
                                        None => format!("{} · unknown fan type", fmt_bytes(f.size)),
                                    };
                                    ui.label(RichText::new(info).size(11.0).color(p.muted));
                                    ui.horizontal(|ui| {
                                        ui.spacing_mut().item_spacing.x = 6.0;
                                        if link_button(ui, "+ Playlist", p.accent)
                                            .on_hover_text("Add to the selected playlist")
                                            .clicked()
                                        {
                                            action = Some((name.clone(), 0));
                                        }
                                        if link_button(ui, "Upload", p.accent)
                                            .on_hover_text("Send to the fan you control now")
                                            .clicked()
                                        {
                                            action = Some((name.clone(), 1));
                                        }
                                        if link_button(ui, "Delete", p.danger).clicked() {
                                            action = Some((name.clone(), 2));
                                        }
                                    });
                                });
                            });
                    }
                });
                ui.add_space(gap);
            }
            if let Some((name, a)) = action {
                match a {
                    0 => {
                        let pl_name = self
                            .lib
                            .playlists
                            .get(self.playlist)
                            .map(|pl| pl.name.clone())
                            .unwrap_or_default();
                        if let Some(pl) = self.lib.playlists.get_mut(self.playlist) {
                            pl.items.push(name.clone());
                        }
                        self.lib.save_playlists();
                        self.notify(&ctx, format!("Added {name} to “{pl_name}”"), true);
                    }
                    1 => {
                        let path = self.lib.folder.join(&name);
                        self.send(&ctx, Cmd::Upload(vec![path]));
                    }
                    _ => self.confirm = Some(Confirm::DeleteFile(name)),
                }
            }
        });
        ui.add_space(14.0);
        self.fan_files(ui, p);
    }

    // ------------------------------------------------------------------ playlists

    pub(crate) fn page_playlists(&mut self, ui: &mut Ui, p: &Palette) {
        let ctx = ui.ctx().clone();
        page_header(
            ui,
            p,
            "Playlists",
            "Drag fan files into a playlist, then send it to the fan or an SD card",
        );
        self.lib.request_thumbs(&ctx);
        if self.playlist >= self.lib.playlists.len() {
            self.playlist = 0;
        }
        let total_w = ui.available_width();
        let left_w = 270.0;
        let mid_w = (total_w - left_w - 16.0).max(360.0);
        let mut changed = false;

        ui.horizontal_top(|ui| {
            ui.spacing_mut().item_spacing.x = 16.0;
            // ---- list of playlists
            ui.vertical(|ui| {
                ui.set_width(left_w);
                card(ui, p, |ui| {
                    ui.set_width(left_w - 36.0);
                    ui.label(title("Playlists", 16.0, p.text));
                    ui.add_space(6.0);
                    for i in 0..self.lib.playlists.len() {
                        let pl = &self.lib.playlists[i];
                        let label = format!("{}  ({})", short(&pl.name, 18), pl.items.len());
                        if nav_item(ui, p, "≡", &label, i == self.playlist).clicked() {
                            self.playlist = i;
                            self.rename = None;
                        }
                    }
                    ui.add_space(6.0);
                    if link_button(ui, "+ New playlist", p.accent).clicked() {
                        let n = self.lib.playlists.len() + 1;
                        self.lib.playlists.push(library::Playlist {
                            name: format!("Playlist {n}"),
                            items: Vec::new(),
                        });
                        self.playlist = self.lib.playlists.len() - 1;
                        self.rename = Some(self.lib.playlists[self.playlist].name.clone());
                        changed = true;
                    }
                });
                ui.add_space(14.0);
                self.drag_list(ui, p, left_w, &mut changed);
            });

            // ---- the selected playlist (drop zone)
            ui.vertical(|ui| {
                ui.set_width(mid_w);
                card(ui, p, |ui| {
                    ui.set_width(mid_w - 36.0);
                    let idx = self.playlist;
                    ui.horizontal(|ui| {
                        if let Some(text) = &mut self.rename {
                            let r = input_frame(p)
                                .show(ui, |ui| {
                                    ui.add(
                                        TextEdit::singleline(text)
                                            .frame(Frame::NONE)
                                            .desired_width(200.0),
                                    )
                                })
                                .inner;
                            if r.lost_focus() || ui.input(|i| i.key_pressed(egui::Key::Enter)) {
                                let name = text.trim().to_string();
                                if !name.is_empty() {
                                    self.lib.playlists[idx].name = name;
                                    changed = true;
                                }
                                self.rename = None;
                            } else {
                                r.request_focus();
                            }
                        } else {
                            ui.label(title(&self.lib.playlists[idx].name, 17.0, p.text));
                            if link_button(ui, "Rename", p.accent).clicked() {
                                self.rename = Some(self.lib.playlists[idx].name.clone());
                            }
                        }
                        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                            if self.lib.playlists.len() > 1
                                && link_button(ui, "Delete playlist", p.danger).clicked()
                            {
                                self.confirm = Some(Confirm::DeletePlaylist(idx));
                            }
                        });
                    });
                    let secs: f32 = self.lib.playlists[idx]
                        .items
                        .iter()
                        .filter_map(|n| self.lib.file(n))
                        .map(|f| f.seconds())
                        .sum();
                    ui.label(
                        RichText::new(format!(
                            "{} item(s) · {}",
                            self.lib.playlists[idx].items.len(),
                            fmt_secs(secs)
                        ))
                        .size(12.0)
                        .color(p.muted),
                    );
                    ui.add_space(8.0);

                    let frame = Frame::new()
                        .fill(p.bg)
                        .corner_radius(CornerRadius::same(10))
                        .inner_margin(Margin::same(8))
                        .stroke(Stroke::new(1.0, p.border));
                    let mut drop_at: Option<(usize, Option<usize>)> = None; // (insert index, moved entry)
                    let items = self.lib.playlists[idx].items.clone();
                    let mut remove: Option<usize> = None;
                    let (_, dropped_file) = ui.dnd_drop_zone::<DragFile, ()>(frame, |ui| {
                        ui.set_min_width(ui.available_width());
                        ui.set_min_height(220.0);
                        if items.is_empty() {
                            ui.add_space(70.0);
                            ui.vertical_centered(|ui| {
                                ui.label(
                                    RichText::new("Drag fan files here")
                                        .font(FontId::new(15.0, bold()))
                                        .color(p.text),
                                );
                                ui.label(
                                    RichText::new(
                                        "from the list on the left, or from Finder / Explorer",
                                    )
                                    .color(p.muted),
                                );
                            });
                        }
                        for (i, name) in items.iter().enumerate() {
                            let f = self.lib.file(name);
                            let row = ui.dnd_drag_source(
                                Id::unique(("pl-entry", idx, i)),
                                DragEntry(i),
                                |ui| {
                                    ui.horizontal(|ui| {
                                        ui.set_min_width(ui.available_width());
                                        ui.label(RichText::new("⠿").color(p.faint));
                                        ui.label(
                                            RichText::new(format!("{:02}", i + 1))
                                                .monospace()
                                                .size(12.0)
                                                .color(p.muted),
                                        );
                                        disc(ui, p, f.and_then(|f| f.thumb.as_ref()), 34.0);
                                        ui.vertical(|ui| {
                                            let stem = name
                                                .trim_end_matches(".BIN")
                                                .trim_end_matches(".bin");
                                            ui.label(
                                                RichText::new(short(stem, 34))
                                                    .font(FontId::new(13.0, bold()))
                                                    .color(if f.is_some() {
                                                        p.text
                                                    } else {
                                                        p.danger
                                                    }),
                                            );
                                            let info = match f {
                                                Some(f) => format!(
                                                    "{} · {}",
                                                    fmt_secs(f.seconds()),
                                                    fmt_bytes(f.size)
                                                ),
                                                None => "missing from the library folder".into(),
                                            };
                                            ui.label(RichText::new(info).size(11.0).color(p.muted));
                                        });
                                        ui.with_layout(
                                            Layout::right_to_left(Align::Center),
                                            |ui| {
                                                if link_button(ui, "✕", p.muted)
                                                    .on_hover_text("Remove from the playlist")
                                                    .clicked()
                                                {
                                                    remove = Some(i);
                                                }
                                            },
                                        );
                                    });
                                },
                            );
                            // a dragged entry or file dropped onto this row goes before it
                            let r = row.response;
                            if let (Some(pointer), true) =
                                (ui.input(|i| i.pointer.interact_pos()), r.contains_pointer())
                            {
                                let before = pointer.y < r.rect.center().y;
                                let at = if before { i } else { i + 1 };
                                if let Some(entry) = r.dnd_release_payload::<DragEntry>() {
                                    drop_at = Some((at, Some(entry.0)));
                                }
                                if r.dnd_hover_payload::<DragEntry>().is_some()
                                    || r.dnd_hover_payload::<DragFile>().is_some()
                                {
                                    let y = if before {
                                        r.rect.top()
                                    } else {
                                        r.rect.bottom()
                                    };
                                    ui.painter().hline(
                                        r.rect.x_range(),
                                        y,
                                        Stroke::new(2.0, p.accent),
                                    );
                                }
                                if let Some(file) = r.dnd_release_payload::<DragFile>() {
                                    drop_at = Some((at, None));
                                    self.pending_drop = Some(file.0.clone());
                                }
                            }
                        }
                    });
                    if let Some(i) = remove {
                        self.lib.playlists[idx].items.remove(i);
                        changed = true;
                    }
                    let list = &mut self.lib.playlists[idx].items;
                    match drop_at {
                        Some((at, Some(from))) if from < list.len() => {
                            let it = list.remove(from);
                            let at = if at > from { at - 1 } else { at };
                            list.insert(at.min(list.len()), it);
                            changed = true;
                        }
                        Some((at, None)) => {
                            if let Some(name) = self.pending_drop.take() {
                                list.insert(at.min(list.len()), name);
                                changed = true;
                            }
                        }
                        _ => {
                            if let Some(file) = dropped_file {
                                list.push(file.0.clone());
                                changed = true;
                            }
                        }
                    }
                    self.pending_drop = None;

                    ui.add_space(12.0);
                    let paths: Vec<PathBuf> = self.lib.playlists[idx]
                        .items
                        .iter()
                        .map(|n| self.lib.folder.join(n))
                        .filter(|p| p.exists())
                        .collect();
                    ui.horizontal(|ui| {
                        let can = !paths.is_empty()
                            && self.dev.is_connected()
                            && self.dev.upload_progress.is_none();
                        if primary_button(ui, p, "  Send to fan  ", can)
                            .on_hover_text("Uploads every file, in this order, over Wi-Fi")
                            .clicked()
                        {
                            self.send(&ctx, Cmd::Upload(paths.clone()));
                        }
                        if self.fans.iter().any(|f| f.dev.is_connected())
                            && outline(ui, p, "Send to all fans", !paths.is_empty()).clicked()
                        {
                            self.send(&ctx, Cmd::Upload(paths.clone()));
                            for f in &mut self.fans {
                                f.dev.send(Cmd::Upload(paths.clone()));
                            }
                        }
                        if let Some(pr) = self.dev.upload_progress {
                            ui.label(
                                RichText::new(format!("Uploading… {:.0}%", pr * 100.0))
                                    .size(12.0)
                                    .color(p.muted),
                            );
                        }
                    });
                });
                ui.add_space(14.0);
                self.sd_card(ui, p, mid_w);
            });
        });
        if changed {
            self.lib.save_playlists();
        }
    }

    /// The library as a list to drag from (left column of the Playlists page).
    fn drag_list(&mut self, ui: &mut Ui, p: &Palette, w: f32, changed: &mut bool) {
        card(ui, p, |ui| {
            ui.set_width(w - 36.0);
            ui.label(title("Library", 16.0, p.text));
            ui.label(
                RichText::new("Drag into the playlist, or click +")
                    .size(11.5)
                    .color(p.muted),
            );
            ui.add_space(6.0);
            ScrollArea::vertical()
                .id_salt("pl-lib")
                .max_height(520.0)
                .show(ui, |ui| {
                    let names: Vec<String> =
                        self.lib.files.iter().map(|f| f.name.clone()).collect();
                    if names.is_empty() {
                        ui.label(
                            RichText::new("No fan files yet — see the Library page.")
                                .size(12.0)
                                .color(p.muted),
                        );
                    }
                    for name in names {
                        let f = self.lib.file(&name);
                        let tex = f.and_then(|f| f.thumb.clone());
                        let secs = f.map(|f| f.seconds()).unwrap_or(0.0);
                        let mut add = false;
                        ui.dnd_drag_source(
                            Id::unique(("lib-file", &name)),
                            DragFile(name.clone()),
                            |ui| {
                                ui.horizontal(|ui| {
                                    ui.set_min_width(ui.available_width());
                                    disc(ui, p, tex.as_ref(), 30.0);
                                    ui.vertical(|ui| {
                                        ui.label(
                                            RichText::new(short(name.trim_end_matches(".BIN"), 20))
                                                .size(12.5)
                                                .color(p.text),
                                        );
                                        ui.label(
                                            RichText::new(fmt_secs(secs)).size(10.5).color(p.muted),
                                        );
                                    });
                                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                                        add = link_button(ui, "+", p.accent).clicked();
                                    });
                                });
                            },
                        );
                        if add {
                            self.lib.playlists[self.playlist].items.push(name.clone());
                            *changed = true;
                        }
                    }
                });
        });
    }

    fn sd_card(&mut self, ui: &mut Ui, p: &Palette, w: f32) {
        let ctx = ui.ctx().clone();
        card(ui, p, |ui| {
            ui.set_width(w - 36.0);
            ui.label(title("Save to SD card", 16.0, p.text));
            ui.label(
                RichText::new(
                    "Copies the playlist's files, numbered in play order, onto the card.",
                )
                .size(12.0)
                .color(p.muted),
            );
            ui.add_space(8.0);
            ui.horizontal(|ui| {
                let label = self
                    .sd_target
                    .as_ref()
                    .map(|t| t.display().to_string())
                    .unwrap_or_else(|| "Choose a card or folder".into());
                egui::ComboBox::from_id_salt("sd-target")
                    .selected_text(short(&label, 30))
                    .width(220.0)
                    .show_ui(ui, |ui| {
                        if self.drives.is_empty() {
                            ui.label(
                                RichText::new("No SD card found — insert one and press Refresh")
                                    .color(p.muted),
                            );
                        }
                        for (name, mount, free) in self.drives.clone() {
                            if ui
                                .selectable_label(
                                    self.sd_target.as_ref() == Some(&mount),
                                    format!("{name} — {} free", fmt_bytes(free)),
                                )
                                .clicked()
                            {
                                self.sd_target = Some(mount);
                            }
                        }
                    });
                if link_button(ui, "Refresh", p.accent).clicked() {
                    self.drives = library::removable_drives();
                }
                if link_button(ui, "Folder…", p.accent).clicked() {
                    if let Some(d) = rfd::FileDialog::new()
                        .set_title("Choose the SD card or a folder")
                        .pick_folder()
                    {
                        self.sd_target = Some(d);
                    }
                }
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    let has = self
                        .lib
                        .playlists
                        .get(self.playlist)
                        .map(|pl| !pl.items.is_empty())
                        .unwrap_or(false);
                    let can = self.sd_target.is_some() && has && self.lib.copying.is_none();
                    if primary_button(ui, p, "  Copy  ", can).clicked() {
                        if let Some(t) = self.sd_target.clone() {
                            self.dev.push_log(
                                LogKind::Info,
                                format!("Copying the playlist to {}…", t.display()),
                            );
                            self.lib.copy_to_card(self.playlist, t, &ctx);
                        }
                    }
                });
            });
            if let Some(pr) = self.lib.copying {
                ui.add_space(6.0);
                bar(ui, p, pr, p.accent, 5.0);
            }
        });
    }

    // ------------------------------------------------------------------ fans

    pub(crate) fn page_fans(&mut self, ui: &mut Ui, p: &Palette) {
        let ctx = ui.ctx().clone();
        page_header(ui, p, "Fans", "All your fans at a glance");
        ui.label(
            RichText::new("Each fan makes its own Wi-Fi at 192.168.4.1. To reach several at once, give each its own address (for example through a Raspberry Pi bridge per fan).")
                .size(12.0)
                .color(p.muted),
        );
        ui.add_space(10.0);

        // actions for every fan at once
        card(ui, p, |ui| {
            ui.set_min_width(ui.available_width());
            ui.horizontal(|ui| {
                ui.label(title("All fans", 16.0, p.text));
                ui.add_space(12.0);
                let all: [(&str, Cmd); 5] = [
                    ("Power", Cmd::Power),
                    ("Previous", Cmd::Prev),
                    ("Play / pause", Cmd::PlayPause),
                    ("Next", Cmd::Next),
                    ("Brightness +", Cmd::BrightnessUp),
                ];
                for (label, cmd) in all {
                    if secondary_button(ui, p, &format!(" {label} ")).clicked() {
                        let n = self.dev.is_connected() as usize
                            + self.fans.iter().filter(|f| f.dev.is_connected()).count();
                        self.dev.send(cmd.clone());
                        for f in &mut self.fans {
                            f.dev.send(cmd.clone());
                        }
                        self.notify(&ctx, format!("{label}: sent to {n} fan(s)"), n > 0);
                    }
                }
                if secondary_button(ui, p, " Brightness − ").clicked() {
                    self.dev.send(Cmd::BrightnessDown);
                    for f in &mut self.fans {
                        f.dev.send(Cmd::BrightnessDown);
                    }
                }
            });
        });
        ui.add_space(14.0);

        let tile = 300.0;
        let gap = 14.0;
        let cols = (((ui.available_width() + gap) / (tile + gap)).floor() as usize).max(1);
        let count = 1 + self.fans.len();
        let mut make_active: Option<usize> = None;
        let mut remove: Option<usize> = None;
        let t = ctx.input(|i| i.time) as f32;
        let indices: Vec<usize> = (0..count).collect();
        for row in indices.chunks(cols) {
            ui.horizontal_top(|ui| {
                ui.spacing_mut().item_spacing.x = gap;
                for &k in row {
                    let active = k == 0;
                    let (name, dev): (String, &mut Device) = if active {
                        (self.fan_name.clone(), &mut self.dev)
                    } else {
                        (self.fans[k - 1].name.clone(), &mut self.fans[k - 1].dev)
                    };
                    Frame::new()
                        .fill(p.card)
                        .stroke(Stroke::new(
                            if active { 2.0 } else { 1.0 },
                            if active { p.accent } else { p.border },
                        ))
                        .corner_radius(CornerRadius::same(14))
                        .inner_margin(Margin::same(16))
                        .show(ui, |ui| {
                            ui.set_width(tile - 32.0);
                            ui.vertical(|ui| {
                                ui.horizontal(|ui| {
                                    let (r, _) =
                                        ui.allocate_exact_size(vec2(40.0, 40.0), Sense::hover());
                                    let blades = dev
                                        .fan
                                        .as_ref()
                                        .and_then(|s| s.device_name())
                                        .and_then(crate::models::find)
                                        .map(|m| crate::models::MODELS[m].blades())
                                        .unwrap_or(2);
                                    paint::fan_icon(
                                        ui.painter(),
                                        r,
                                        p,
                                        blades,
                                        if dev
                                            .fan
                                            .as_ref()
                                            .map(|s| !s.sleeping && !s.paused)
                                            .unwrap_or(false)
                                        {
                                            t
                                        } else {
                                            0.0
                                        },
                                    );
                                    ui.vertical(|ui| {
                                        ui.label(
                                            RichText::new(short(&name, 22))
                                                .font(FontId::new(15.0, bold()))
                                                .color(p.text),
                                        );
                                        ui.label(
                                            RichText::new(&dev.address).size(11.5).color(p.muted),
                                        );
                                    });
                                    ui.with_layout(Layout::right_to_left(Align::Min), |ui| {
                                        let (dot, text) = status_look(p, &dev.status);
                                        chip(ui, text, dot, mix(p.card, dot, 0.16));
                                    });
                                });
                                ui.add_space(6.0);
                                match dev.fan.clone() {
                                    Some(st) => {
                                        kv(ui, p, "Model", st.device_name().unwrap_or("unknown"));
                                        kv(
                                            ui,
                                            p,
                                            "State",
                                            if st.sleeping {
                                                "Sleeping (off)"
                                            } else if st.paused {
                                                "Paused"
                                            } else {
                                                "Playing"
                                            },
                                        );
                                        kv(ui, p, "Brightness", st.brightness_label());
                                        let now = st
                                            .files
                                            .get(st.current as usize)
                                            .cloned()
                                            .unwrap_or_else(|| "—".into());
                                        kv(
                                            ui,
                                            p,
                                            "Showing",
                                            &format!(
                                                "{} ({} file(s))",
                                                short(&now, 14),
                                                st.files.len()
                                            ),
                                        );
                                    }
                                    None => {
                                        ui.label(
                                            RichText::new("No report from this fan yet.")
                                                .size(12.0)
                                                .color(p.muted),
                                        );
                                    }
                                }
                                ui.add_space(8.0);
                                ui.horizontal(|ui| {
                                    ui.spacing_mut().item_spacing.x = 6.0;
                                    for (icon, cmd, tip) in [
                                        ("⏻", Cmd::Power, "On / off"),
                                        ("⏮", Cmd::Prev, "Previous"),
                                        ("⏯", Cmd::PlayPause, "Play / pause"),
                                        ("⏭", Cmd::Next, "Next"),
                                    ] {
                                        if paint::small_icon_button(ui, p, icon)
                                            .on_hover_text(tip)
                                            .clicked()
                                        {
                                            dev.send(cmd);
                                        }
                                    }
                                    if paint::small_icon_button(ui, p, "−")
                                        .on_hover_text("Brightness −")
                                        .clicked()
                                    {
                                        dev.send(Cmd::BrightnessDown);
                                    }
                                    if paint::small_icon_button(ui, p, "+")
                                        .on_hover_text("Brightness +")
                                        .clicked()
                                    {
                                        dev.send(Cmd::BrightnessUp);
                                    }
                                });
                                ui.add_space(6.0);
                                ui.horizontal(|ui| {
                                    if active {
                                        ui.label(
                                            RichText::new("You control this fan")
                                                .size(12.0)
                                                .color(p.accent),
                                        );
                                    } else if link_button(ui, "Control this fan", p.accent)
                                        .clicked()
                                    {
                                        make_active = Some(k - 1);
                                    }
                                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                                        if !active && link_button(ui, "Remove", p.danger).clicked()
                                        {
                                            remove = Some(k - 1);
                                        }
                                        if dev.is_active() {
                                            if link_button(ui, "Disconnect", p.muted).clicked() {
                                                dev.disconnect();
                                            }
                                        } else if link_button(ui, "Connect", p.accent).clicked() {
                                            dev.connect(&ctx);
                                        }
                                    });
                                });
                            });
                        });
                }
            });
            ui.add_space(gap);
        }

        // add a fan
        card(ui, p, |ui| {
            ui.set_min_width(ui.available_width());
            ui.label(title("Add a fan", 16.0, p.text));
            ui.add_space(6.0);
            ui.horizontal(|ui| {
                ui.label(RichText::new("Name").color(p.text));
                input_frame(p).show(ui, |ui| {
                    ui.add(
                        TextEdit::singleline(&mut self.new_fan.0)
                            .frame(Frame::NONE)
                            .desired_width(180.0)
                            .hint_text("Shop window"),
                    )
                });
                ui.add_space(8.0);
                ui.label(RichText::new("Address").color(p.text));
                input_frame(p).show(ui, |ui| {
                    ui.add(
                        TextEdit::singleline(&mut self.new_fan.1)
                            .frame(Frame::NONE)
                            .desired_width(180.0)
                            .hint_text("192.168.1.50:20320"),
                    )
                });
                ui.add_space(8.0);
                let ok = !self.new_fan.1.trim().is_empty();
                if primary_button(ui, p, "  Add  ", ok).clicked() {
                    let name = if self.new_fan.0.trim().is_empty() {
                        format!("Fan {}", self.fans.len() + 2)
                    } else {
                        self.new_fan.0.trim().to_string()
                    };
                    let mut dev = Device::new(self.new_fan.1.trim().to_string());
                    dev.connect(&ctx);
                    self.fans.push(FanSlot { name, dev });
                    self.new_fan = (String::new(), String::new());
                }
            });
        });

        if let Some(i) = remove {
            let mut f = self.fans.remove(i);
            f.dev.disconnect();
        }
        if let Some(i) = make_active {
            std::mem::swap(&mut self.dev, &mut self.fans[i].dev);
            std::mem::swap(&mut self.fan_name, &mut self.fans[i].name);
            self.addr_edit = self.dev.address.clone();
            self.dev.fan_changed = true;
            let msg = format!("You now control “{}”", self.fan_name);
            self.notify(&ctx, msg, true);
        }
        ctx.request_repaint_after(std::time::Duration::from_millis(100));
    }

    // ------------------------------------------------------------------ converter window

    pub(crate) fn convert_dialog(&mut self, ctx: &egui::Context, p: &Palette) {
        let Some(mut d) = self.convert.take() else {
            return;
        };
        // a picture from the video for the preview, once
        if !d.grabbed {
            d.grabbed = true;
            if let Some((ff, fp)) = self.lib.ffmpeg() {
                d.frame = library::grab_frame(&ff, &fp, &d.files[0]);
            }
        }
        let spec = binfmt::SPECS[d.model];
        let key = (d.model, d.mode, (d.zoom * 100.0) as i32);
        if d.preview_for != Some(key) {
            d.preview_for = Some(key);
            d.preview = d.frame.as_ref().map(|(rgb, w, h)| {
                let img = library::preview_of(
                    rgb,
                    *w,
                    *h,
                    ConvertSettings {
                        spec,
                        mode: d.mode,
                        zoom: d.zoom,
                    },
                    240,
                );
                ctx.load_texture("convert-preview", img, egui::TextureOptions::LINEAR)
            });
        }
        let has_ffmpeg = self.lib.ffmpeg().is_some();
        let mut close = false;
        let mut start = false;
        egui::Modal::new(Id::unique("converter")).frame(modal_frame(p)).show(ctx, |ui| {
            ui.set_width(640.0);
            ui.label(title("Convert for the fan", 20.0, p.text));
            let n = d.files.len();
            let first = d.files[0].file_name().map(|s| s.to_string_lossy().to_string()).unwrap_or_default();
            ui.label(RichText::new(if n == 1 { first } else { format!("{n} files: {first} and more") }).color(p.muted));
            ui.add_space(12.0);
            ui.horizontal_top(|ui| {
                ui.vertical(|ui| {
                    ui.set_width(250.0);
                    let (rect, _) = ui.allocate_exact_size(vec2(240.0, 240.0), Sense::hover());
                    ui.painter().circle_filled(rect.center(), 120.0, Color32::from_rgb(6, 8, 12));
                    match &d.preview {
                        Some(t) => paint::image_circle(ui.painter(), t, rect.center(), 119.0, 1.0),
                        None => {
                            ui.painter().text(rect.center(), Align2::CENTER_CENTER, if has_ffmpeg { "No preview" } else { "Needs ffmpeg" }, FontId::proportional(13.0), p.muted);
                        }
                    }
                    ui.painter().circle_stroke(rect.center(), 120.0, Stroke::new(1.0, p.border));
                    ui.label(RichText::new("How the fan will show it (one frame)").size(11.0).color(p.muted));
                });
                ui.vertical(|ui| {
                    if n == 1 {
                        ui.label(RichText::new("File name").size(12.5).color(p.text));
                        input_frame(p).show(ui, |ui| ui.add(TextEdit::singleline(&mut d.name).frame(Frame::NONE).desired_width(f32::INFINITY)));
                        ui.add_space(8.0);
                    }
                    ui.label(RichText::new("Fan type").size(12.5).color(p.text));
                    egui::ComboBox::from_id_salt("conv-model").selected_text(format!("{}{}", spec.code, if spec.verified { "" } else { " (untested)" })).width(300.0).show_ui(ui, |ui| {
                        for (i, s) in binfmt::SPECS.iter().enumerate() {
                            let label = format!("{} — {} LEDs, {} angles{}", s.code, s.leds, s.rows, if s.verified { "" } else { " (untested)" });
                            if ui.selectable_label(d.model == i, label).clicked() {
                                d.model = i;
                            }
                        }
                    });
                    if !spec.verified {
                        ui.label(RichText::new("Made like the original makes it, but not yet checked on this fan type.").size(11.0).color(p.warning));
                    }
                    ui.add_space(8.0);
                    ui.label(RichText::new("Colour").size(12.5).color(p.text));
                    let labels: Vec<&str> = ColorMode::ALL.iter().map(|m| m.label()).collect();
                    let cur = ColorMode::ALL.iter().position(|m| *m == d.mode).unwrap_or(0);
                    if let Some(i) = segmented(ui, p, &labels, cur) {
                        d.mode = ColorMode::ALL[i];
                    }
                    ui.label(RichText::new(match d.mode {
                        ColorMode::Colorful => "Deep colours, darker shadows",
                        ColorMode::Common => "Natural",
                        ColorMode::Highlight => "Brightest; light areas turn white",
                    }).size(11.0).color(p.muted));
                    ui.add_space(8.0);
                    ui.label(RichText::new("Zoom").size(12.5).color(p.text));
                    let mut z = 1.0 / d.zoom;
                    if ui.add(egui::Slider::new(&mut z, 0.7..=2.5).fixed_decimals(2).suffix("×")).changed() {
                        d.zoom = 1.0 / z;
                    }
                    ui.label(RichText::new("1× shows the largest circle in the middle of the video").size(11.0).color(p.muted));
                    ui.add_space(8.0);
                    ui.label(RichText::new(format!("{} frames per second, as the fan plays them", spec.fps)).size(11.0).color(p.faint));
                });
            });
            ui.add_space(14.0);
            if !has_ffmpeg {
                ui.label(RichText::new("ffmpeg is needed to read videos. Install it (Mac: brew install ffmpeg; Windows: ffmpeg.org) and open this window again.").size(12.0).color(p.warning));
                ui.add_space(8.0);
            }
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                let ok = has_ffmpeg && (n > 1 || !d.name.trim().is_empty());
                if primary_button(ui, p, "  Convert  ", ok).clicked() {
                    start = true;
                }
                if secondary_button(ui, p, "  Cancel  ").clicked() {
                    close = true;
                }
            });
        });
        if start {
            let settings = ConvertSettings {
                spec,
                mode: d.mode,
                zoom: d.zoom,
            };
            self.convert_mode = d.mode;
            self.convert_zoom = d.zoom;
            for (i, f) in d.files.iter().enumerate() {
                let name = if d.files.len() == 1 {
                    d.name.clone()
                } else {
                    f.file_stem()
                        .map(|s| s.to_string_lossy().to_string())
                        .unwrap_or_else(|| format!("video {i}"))
                };
                match self.lib.convert(f.clone(), &name, settings, ctx) {
                    Ok(out) => self.dev.push_log(
                        LogKind::Info,
                        format!(
                            "Converting {} for the {}…",
                            out.file_name()
                                .map(|n| n.to_string_lossy().to_string())
                                .unwrap_or_default(),
                            spec.code
                        ),
                    ),
                    Err(e) => {
                        self.notify(ctx, e.clone(), false);
                        self.dev.push_log(LogKind::Warn, e);
                        break;
                    }
                }
            }
            self.page = crate::app::Page::Library;
            return;
        }
        if !close {
            self.convert = Some(d);
        }
    }
}
