// HoloFan Studio: app state, sidebar, dialogs and the frame loop. The four pages live in pages.rs.

use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicU8, Ordering};
use std::time::Duration;

use eframe::egui::{
    self, Align, Align2, Color32, CornerRadius, FontId, Frame, Id, Layout, Margin, RichText,
    ScrollArea, Sense, Stroke, Ui, pos2, vec2,
};

use crate::binfmt::ColorMode;
use crate::device::{Cmd, Device, LogKind, Status};
use crate::library::{self, Library};
use crate::models::{self, MODELS};
use crate::pages_lib::ConvertDialog;
use crate::paint::{self, modal_frame, short};
use crate::theme::{self, ACCENTS, Palette, ThemeMode, bold};
use crate::widgets::*;

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Page {
    Player,
    Library,
    Playlists,
    Fans,
    Clock,
    Device,
    Settings,
}

impl Page {
    const ALL: [Page; 7] = [
        Page::Player,
        Page::Library,
        Page::Playlists,
        Page::Fans,
        Page::Clock,
        Page::Device,
        Page::Settings,
    ];
    fn label(self) -> &'static str {
        match self {
            Page::Player => "Player",
            Page::Library => "Library",
            Page::Playlists => "Playlists",
            Page::Fans => "Fans",
            Page::Clock => "Clock",
            Page::Device => "Device",
            Page::Settings => "Settings",
        }
    }
    fn icon(self) -> &'static str {
        match self {
            Page::Player => "▶",
            Page::Library => "🗄",
            Page::Playlists => "🎞",
            Page::Fans => "🌀",
            Page::Clock => "🕐",
            Page::Device => "📡",
            Page::Settings => "⚙",
        }
    }
    fn key(self) -> &'static str {
        match self {
            Page::Player => "player",
            Page::Library => "library",
            Page::Playlists => "playlists",
            Page::Fans => "fans",
            Page::Clock => "clock",
            Page::Device => "device",
            Page::Settings => "settings",
        }
    }
    fn from_key(s: &str) -> Self {
        Page::ALL
            .into_iter()
            .find(|p| p.key() == s)
            .unwrap_or(Page::Player)
    }
}

pub const DIALS: [&str; 4] = ["Digital", "Symbol", "Zodiac", "Constellation"];

#[derive(Clone, Copy, PartialEq)]
pub enum LoopMode {
    Single,
    List,
}

#[derive(Clone, PartialEq)]
pub enum Confirm {
    Format,
    ClearCache,
    DeletePlaylist(usize),
    DeleteFile(String),
}

/// Another fan the app keeps connected (the one you control is `App::dev`).
pub struct FanSlot {
    pub name: String,
    pub dev: Device,
}

pub struct App {
    pub(crate) page: Page,
    pub(crate) theme_mode: ThemeMode,
    pub(crate) accent_idx: usize,
    applied: Option<(ThemeMode, usize)>,

    pub(crate) model: Option<usize>,
    pub(crate) picking_model: bool,

    pub(crate) dev: Device,
    pub(crate) fan_name: String,
    pub(crate) fans: Vec<FanSlot>,
    pub(crate) new_fan: (String, String),
    pub(crate) lib: Library,
    pub(crate) playlist: usize,
    pub(crate) rename: Option<String>,
    pub(crate) pending_drop: Option<String>,
    pub(crate) convert: Option<ConvertDialog>,
    pub(crate) convert_mode: ColorMode,
    pub(crate) convert_zoom: f32,
    pub(crate) updater: crate::settings::Updater,
    pub(crate) update_repo: String,
    pub(crate) update_on_start: bool,
    /// Play the blank file as soon as the fan lists it (after uploading it).
    pub(crate) blank_pending: bool,

    // player state (as far as the app knows; the fan isn't known to report it back)
    pub(crate) playing: bool,
    pub(crate) loop_mode: LoopMode,
    pub(crate) powered: bool,
    pub(crate) brightness: i32,
    pub(crate) picture_secs: u32,

    // clock
    pub(crate) clock_on: bool,
    pub(crate) needle_white: bool,
    pub(crate) dial: usize,
    pub(crate) boot_h: u32,
    pub(crate) boot_m: u32,
    pub(crate) boot_s: u32,

    // device page
    pub(crate) addr_edit: String,
    pub(crate) wifi_ssid: String,
    pub(crate) wifi_pass: String,
    pub(crate) show_pass: bool,
    pub(crate) sd_target: Option<PathBuf>,
    pub(crate) drives: Vec<(String, PathBuf, u64)>,

    pub(crate) confirm: Option<Confirm>,
    toast: Option<(String, bool, f64)>,

    shot_path: Option<PathBuf>,
    shot_frames: u32,
    shot_state: Arc<AtomicU8>,
}

impl App {
    pub fn new(cc: &eframe::CreationContext<'_>, shot_path: Option<PathBuf>) -> Self {
        theme::install_fonts(&cc.egui_ctx);
        let s = if shot_path.is_none() {
            cc.storage
        } else {
            None
        };
        let get = |k: &str| s.and_then(|s| s.get_string(k));
        let num = |k: &str, d: u32| get(k).and_then(|v| v.parse().ok()).unwrap_or(d);
        // the fan listens on 192.168.4.1:20320; addresses saved by older versions with the
        // placeholder port 5000 move to the real one
        let address = match get("address") {
            Some(a) if !a.trim().ends_with(":5000") => {
                let (h, port) = crate::protocol::parse_address(&a);
                crate::protocol::format_address(&h, port)
            }
            _ => crate::protocol::format_address(
                crate::protocol::DEFAULT_HOST,
                crate::protocol::DEFAULT_PORT,
            ),
        };

        let mut app = Self {
            page: get("page")
                .map(|v| Page::from_key(&v))
                .unwrap_or(Page::Player),
            theme_mode: get("theme")
                .map(|v| ThemeMode::from_key(&v))
                .unwrap_or(ThemeMode::System),
            accent_idx: (num("accent", 0) as usize).min(ACCENTS.len() - 1),
            applied: None,
            model: get("model").and_then(|m| models::find(&m)),
            picking_model: false,
            dev: Device::new(address.clone()),
            fan_name: get("fan_name").unwrap_or_else(|| "My fan".into()),
            fans: Vec::new(),
            new_fan: (String::new(), String::new()),
            lib: Library::new(
                get("library")
                    .map(PathBuf::from)
                    .unwrap_or_else(library::default_folder),
            ),
            playlist: num("playlist", 0) as usize,
            rename: None,
            pending_drop: None,
            convert: None,
            convert_mode: get("convert_mode")
                .map(|v| ColorMode::from_key(&v))
                .unwrap_or(ColorMode::Colorful),
            convert_zoom: get("convert_zoom")
                .and_then(|v| v.parse().ok())
                .unwrap_or(1.0f32)
                .clamp(0.3, 1.5),
            updater: crate::settings::Updater::new(),
            update_repo: get("update_repo").unwrap_or_default(),
            update_on_start: get("update_on_start").map(|v| v == "1").unwrap_or(false),
            blank_pending: false,
            playing: true,
            loop_mode: LoopMode::List,
            powered: true,
            brightness: (num("brightness", 5) as i32).clamp(1, 10),
            picture_secs: num("picture_secs", 10).clamp(5, 30),
            clock_on: get("clock_on").map(|v| v == "1").unwrap_or(false),
            needle_white: get("needle").map(|v| v != "black").unwrap_or(true),
            dial: (num("dial", 0) as usize).min(DIALS.len() - 1),
            boot_h: num("boot_h", 0).min(23),
            boot_m: num("boot_m", 0).min(59),
            boot_s: num("boot_s", 10).min(59),
            addr_edit: address,
            wifi_ssid: get("ssid").unwrap_or_default(),
            wifi_pass: String::new(),
            show_pass: false,
            sd_target: get("sd_target").map(PathBuf::from),
            drives: library::removable_drives(),
            confirm: None,
            toast: None,
            shot_path,
            shot_frames: 0,
            shot_state: Arc::new(AtomicU8::new(0)),
        };
        // the other fans: "name<TAB>address" per line
        if let Some(list) = get("fans") {
            for line in list.lines() {
                if let Some((n, a)) = line.split_once('\t') {
                    app.fans.push(FanSlot {
                        name: n.to_string(),
                        dev: Device::new(a.to_string()),
                    });
                }
            }
        }
        app.lib.preferred_model = app.model.map(|m| MODELS[m].code.to_string());
        app.lib.rescan();

        let args: Vec<String> = std::env::args().collect();
        let arg = |name: &str| {
            args.iter()
                .position(|a| a == name)
                .and_then(|i| args.get(i + 1))
                .cloned()
        };
        if let Some(v) = arg("--page") {
            app.page = Page::from_key(&v);
        }
        if let Some(v) = arg("--theme") {
            app.theme_mode = ThemeMode::from_key(&v);
        }
        // `--library <folder>`: use this folder of fan files (handy for a check picture)
        if let Some(v) = arg("--library") {
            app.lib.set_folder(PathBuf::from(v));
        }
        // `--convert <video>`: open the converter for this file
        if let Some(v) = arg("--convert") {
            app.open_converter(vec![PathBuf::from(v)]);
        }
        if app.shot_path.is_some() {
            // A check picture shows the main window with a typical fan, unless --pick-model is given.
            if args.iter().any(|a| a == "--pick-model") {
                app.model = None;
            } else if app.model.is_none() {
                app.model = models::find("42-F2");
            }
        }
        app.picking_model = app.model.is_none();
        app.dev.push_log(LogKind::Info, "HoloFan Studio started.");
        app.dev.connect(&cc.egui_ctx);
        for f in &mut app.fans {
            f.dev.connect(&cc.egui_ctx);
        }
        if app.update_on_start && app.shot_path.is_none() && !app.update_repo.trim().is_empty() {
            let repo = app.update_repo.clone();
            app.updater.check(&repo, &cc.egui_ctx);
        }
        app
    }

    pub(crate) fn accent(&self) -> Color32 {
        ACCENTS[self.accent_idx].1
    }

    pub(crate) fn model_name(&self) -> String {
        match self.model {
            Some(i) => format!("{} · {}", MODELS[i].code, MODELS[i].size_label()),
            None => "No model chosen".into(),
        }
    }

    pub(crate) fn notify(&mut self, ctx: &egui::Context, msg: impl Into<String>, ok: bool) {
        self.toast = Some((msg.into(), ok, ctx.input(|i| i.time)));
    }

    /// Send a command to the fan and tell the person what happened.
    pub(crate) fn send(&mut self, ctx: &egui::Context, cmd: Cmd) {
        let label = cmd.label();
        if self.dev.send(cmd) {
            self.notify(ctx, format!("Sent: {label}"), true);
        } else {
            self.notify(ctx, "Not connected to a fan — see the Device page", false);
        }
    }

    fn handle_screenshot(&mut self, ctx: &egui::Context) {
        let Some(path) = self.shot_path.clone() else {
            return;
        };
        ctx.request_repaint(); // keep drawing even without focus
        self.shot_frames += 1;
        match self.shot_state.load(Ordering::SeqCst) {
            0 if self.shot_frames >= 120 => {
                self.shot_state.store(1, Ordering::SeqCst);
                let state = self.shot_state.clone();
                let ctx2 = ctx.clone();
                ctx.request_screenshot(move |img| {
                    let [w, h] = img.size;
                    let mut bytes = Vec::with_capacity(w * h * 4);
                    for px in &img.pixels {
                        let [r, g, b, _] = px.to_array();
                        bytes.extend_from_slice(&[r, g, b, 255]);
                    }
                    if let Some(dir) = path.parent() {
                        let _ = std::fs::create_dir_all(dir);
                    }
                    let ok = image::save_buffer(
                        &path,
                        &bytes,
                        w as u32,
                        h as u32,
                        image::ColorType::Rgba8,
                    )
                    .is_ok();
                    state.store(if ok { 2 } else { 3 }, Ordering::SeqCst);
                    ctx2.request_repaint();
                });
            }
            2 => ctx.send_viewport_cmd(egui::ViewportCommand::Close),
            3 => {
                eprintln!("could not save the screenshot");
                std::process::exit(1);
            }
            _ => {
                if self.shot_frames > 1200 {
                    eprintln!("screenshot timed out");
                    std::process::exit(1);
                }
            }
        }
    }

    /// Take over what the fan reports (power, playing, loop mode, model), as the original does.
    fn apply_fan_status(&mut self, ctx: &egui::Context) {
        if let Some(name) = self.dev.wifi_name.take() {
            if !name.is_empty() {
                self.wifi_ssid = name;
            }
        }
        if !self.dev.fan_changed {
            return;
        }
        self.dev.fan_changed = false;
        let Some(st) = self.dev.fan.clone() else {
            return;
        };
        if self.blank_pending {
            if let Some(i) = crate::pages_lib::blank_index(&st.files) {
                self.blank_pending = false;
                self.dev.send(Cmd::PlayFile(i));
                self.notify(ctx, "Showing the blank (black) picture", true);
            }
        }
        self.powered = !st.sleeping;
        self.playing = !st.paused;
        self.loop_mode = if st.list_loop {
            LoopMode::List
        } else {
            LoopMode::Single
        };
        self.brightness = match st.brightness {
            0 => 10,
            1 => 6,
            _ => 3,
        };
        if let Some(name) = st.device_name() {
            if self.lib.preferred_model.as_deref() != Some(name) {
                self.lib.preferred_model = Some(name.to_string());
                self.lib.rescan();
            }
            if let Some(i) = models::find(name) {
                if self.model != Some(i) {
                    self.model = Some(i);
                    self.picking_model = false;
                    let msg = format!(
                        "Fan model switched to {name}, the type the connected fan reports."
                    );
                    self.dev.push_log(LogKind::Info, msg.clone());
                    self.notify(ctx, msg, true);
                }
            }
        }
    }

    // ------------------------------------------------------------------ sidebar

    fn sidebar(&mut self, ui: &mut Ui, p: &Palette) {
        #[cfg(target_os = "macos")]
        ui.add_space(22.0);
        let ctx = ui.ctx().clone();
        let t = ctx.input(|i| i.time) as f32;

        ui.horizontal(|ui| {
            let (rect, _) = ui.allocate_exact_size(vec2(36.0, 36.0), Sense::hover());
            paint::logo(ui.painter(), rect, p.accent, t);
            ui.add_space(2.0);
            ui.vertical(|ui| {
                ui.add_space(1.0);
                ui.label(title("HoloFan Studio", 15.5, p.text));
                ui.label(RichText::new("Version 13.0").size(11.5).color(p.muted));
            });
        });
        ui.add_space(18.0);

        // current fan model (click to change)
        let resp = Frame::new()
            .fill(if p.dark { p.card } else { p.bg })
            .stroke(Stroke::new(1.0, p.border))
            .corner_radius(CornerRadius::same(12))
            .inner_margin(Margin::symmetric(12, 10))
            .show(ui, |ui| {
                ui.set_width(ui.available_width());
                ui.label(
                    RichText::new("FAN MODEL")
                        .size(10.5)
                        .color(p.muted)
                        .strong(),
                );
                ui.horizontal(|ui| {
                    ui.label(
                        RichText::new(self.model_name())
                            .font(FontId::new(14.0, bold()))
                            .color(p.text),
                    );
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        ui.label(RichText::new("Change").size(12.0).color(p.accent));
                    });
                });
            })
            .response
            .interact(Sense::click());
        if resp
            .on_hover_cursor(egui::CursorIcon::PointingHand)
            .clicked()
        {
            self.picking_model = true;
        }
        ui.add_space(16.0);

        section_label(ui, p, "Menu");
        ui.add_space(4.0);
        for page in Page::ALL {
            if nav_item(ui, p, page.icon(), page.label(), self.page == page).clicked() {
                self.page = page;
            }
            ui.add_space(2.0);
        }

        // connection card at the bottom
        let card_h = 124.0;
        let spare = ui.available_height() - card_h;
        if spare > 0.0 {
            ui.add_space(spare);
        }
        let (dot, text) = status_look(p, &self.dev.status);
        Frame::new()
            .fill(if p.dark { p.card } else { p.bg })
            .stroke(Stroke::new(1.0, p.border))
            .corner_radius(CornerRadius::same(12))
            .inner_margin(Margin::same(12))
            .show(ui, |ui| {
                ui.set_width(ui.available_width());
                ui.horizontal(|ui| {
                    let (r, _) = ui.allocate_exact_size(vec2(10.0, 10.0), Sense::hover());
                    ui.painter().circle_filled(r.center(), 4.5, dot);
                    if self.dev.status == Status::Connecting {
                        let f = (t * 1.5).fract();
                        ui.painter().circle_stroke(
                            r.center(),
                            4.5 + f * 5.0,
                            Stroke::new(1.0, dot.gamma_multiply(1.0 - f)),
                        );
                    }
                    ui.label(
                        RichText::new(text)
                            .font(FontId::new(13.5, bold()))
                            .color(p.text),
                    );
                });
                ui.label(
                    RichText::new(short(&self.dev.address, 26))
                        .size(11.5)
                        .color(p.muted),
                );
                ui.add_space(6.0);
                let w = ui.available_width();
                if self.dev.is_active() {
                    if ui
                        .add(
                            egui::Button::new(RichText::new("Disconnect").color(p.text))
                                .fill(p.card)
                                .stroke(Stroke::new(1.0, p.border))
                                .corner_radius(9)
                                .min_size(vec2(w, 32.0)),
                        )
                        .clicked()
                    {
                        self.dev.disconnect();
                    }
                } else if ui
                    .add(
                        egui::Button::new(
                            RichText::new("Connect")
                                .color(p.on_accent)
                                .font(FontId::new(14.0, bold())),
                        )
                        .fill(p.accent)
                        .stroke(Stroke::NONE)
                        .corner_radius(9)
                        .min_size(vec2(w, 32.0)),
                    )
                    .clicked()
                {
                    self.dev.connect(&ctx);
                }
            });
    }

    // ------------------------------------------------------------------ dialogs

    fn model_picker(&mut self, ctx: &egui::Context, p: &Palette) {
        if !self.picking_model {
            return;
        }
        let first = self.model.is_none();
        let mut close = false;
        let resp = egui::Modal::new(Id::unique("model-picker"))
            .frame(modal_frame(p))
            .show(ctx, |ui| {
                ui.set_width(660.0);
                ui.label(title("Choose your fan model", 20.0, p.text));
                ui.label(
                    RichText::new("The code and size are printed on the fan or its box.")
                        .color(p.muted),
                );
                ui.add_space(14.0);
                let cols = 5;
                let gap = 10.0;
                let tw = (660.0 - gap * (cols as f32 - 1.0)) / cols as f32;
                for (row_i, row) in MODELS.chunks(cols).enumerate() {
                    ui.horizontal(|ui| {
                        ui.spacing_mut().item_spacing.x = gap;
                        for (j, m) in row.iter().enumerate() {
                            let idx = row_i * cols + j;
                            let (r, resp) = ui.allocate_exact_size(vec2(tw, 62.0), Sense::click());
                            let sel = self.model == Some(idx);
                            let hover = resp.hovered();
                            let painter = ui.painter();
                            painter.rect_filled(
                                r,
                                12,
                                if sel {
                                    p.accent_soft
                                } else if hover {
                                    p.card_hover
                                } else {
                                    p.bg
                                },
                            );
                            painter.rect_stroke(
                                r,
                                12,
                                Stroke::new(
                                    if sel { 2.0 } else { 1.0 },
                                    if sel || hover { p.accent } else { p.border },
                                ),
                                egui::StrokeKind::Inside,
                            );
                            painter.text(
                                pos2(r.center().x, r.top() + 22.0),
                                Align2::CENTER_CENTER,
                                m.code,
                                FontId::new(15.0, bold()),
                                p.text,
                            );
                            painter.text(
                                pos2(r.center().x, r.top() + 42.0),
                                Align2::CENTER_CENTER,
                                format!("{} · {:.0} cm", m.size_label(), m.cm()),
                                FontId::proportional(12.0),
                                p.muted,
                            );
                            if resp
                                .on_hover_cursor(egui::CursorIcon::PointingHand)
                                .clicked()
                            {
                                self.model = Some(idx);
                                self.dev.push_log(
                                    LogKind::Info,
                                    format!("Fan model set to {} ({})", m.code, m.size_label()),
                                );
                                close = true;
                            }
                        }
                    });
                    ui.add_space(gap - 8.0);
                }
                if !first {
                    ui.add_space(8.0);
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        if secondary_button(ui, p, "  Cancel  ").clicked() {
                            close = true;
                        }
                    });
                }
            });
        if close || (resp.should_close() && !first) {
            self.picking_model = false;
        }
    }

    fn confirm_dialog(&mut self, ctx: &egui::Context, p: &Palette) {
        let Some(c) = self.confirm.clone() else {
            return;
        };
        let (head, body, action) = match c {
            Confirm::Format => (
                "Format the fan's storage?",
                "Every video and picture stored on the fan will be erased. This can't be undone.",
                "  Format  ",
            ),
            Confirm::ClearCache => (
                "Clear the fan's cache?",
                "Temporary data on the fan will be removed. Your videos stay.",
                "  Clear cache  ",
            ),
            Confirm::DeletePlaylist(_) => (
                "Delete this playlist?",
                "Only the list goes; the fan files stay in the library.",
                "  Delete  ",
            ),
            Confirm::DeleteFile(_) => (
                "Delete this fan file?",
                "The .BIN file is removed from the library folder and from every playlist.",
                "  Delete  ",
            ),
        };
        let mut close = false;
        let resp = egui::Modal::new(Id::unique("confirm"))
            .frame(modal_frame(p))
            .show(ctx, |ui| {
                ui.set_width(400.0);
                ui.label(title(head, 18.0, p.text));
                ui.add_space(6.0);
                ui.label(RichText::new(body).color(p.muted));
                ui.add_space(14.0);
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    if danger_button(ui, p, action).clicked() {
                        match &c {
                            Confirm::Format => self.send(ctx, Cmd::FormatDisk),
                            Confirm::ClearCache => self.send(ctx, Cmd::ClearCache),
                            Confirm::DeletePlaylist(i) => {
                                if *i < self.lib.playlists.len() && self.lib.playlists.len() > 1 {
                                    self.lib.playlists.remove(*i);
                                    self.playlist = 0;
                                    self.lib.save_playlists();
                                }
                            }
                            Confirm::DeleteFile(name) => {
                                if let Err(e) = self.lib.delete(name) {
                                    self.dev.push_log(
                                        LogKind::Warn,
                                        format!("Couldn't delete {name}: {e}"),
                                    );
                                }
                            }
                        }
                        close = true;
                    }
                    if secondary_button(ui, p, "  Cancel  ").clicked() {
                        close = true;
                    }
                });
            });
        if close || resp.should_close() {
            self.confirm = None;
        }
    }

    fn toast_ui(&mut self, ctx: &egui::Context, p: &Palette) {
        let Some((msg, ok, at)) = self.toast.clone() else {
            return;
        };
        let age = (ctx.input(|i| i.time) - at) as f32;
        if age > 3.5 {
            self.toast = None;
            return;
        }
        ctx.request_repaint_after(Duration::from_millis(50));
        let alpha = (3.5 - age).clamp(0.0, 0.4) / 0.4;
        egui::Area::new(Id::unique("toast"))
            .anchor(Align2::CENTER_BOTTOM, vec2(122.0, -28.0))
            .order(egui::Order::Foreground)
            .show(ctx, |ui| {
                Frame::new()
                    .fill(if p.dark {
                        Color32::from_rgb(38, 43, 51)
                    } else {
                        Color32::from_rgb(17, 24, 39)
                    })
                    .corner_radius(CornerRadius::same(12))
                    .inner_margin(Margin::symmetric(16, 10))
                    .multiply_with_opacity(alpha)
                    .show(ui, |ui| {
                        ui.horizontal(|ui| {
                            ui.label(RichText::new(if ok { "✔" } else { "⚠" }).color(if ok {
                                p.success
                            } else {
                                p.warning
                            }));
                            ui.label(RichText::new(msg).color(Color32::WHITE));
                        });
                    });
            });
    }
}

pub fn status_look(p: &Palette, s: &Status) -> (Color32, &'static str) {
    match s {
        Status::Connected => (p.success, "Connected"),
        Status::Connecting => (p.warning, "Connecting…"),
        Status::Failed => (p.danger, "Not connected"),
        Status::Disconnected => (p.faint, "Disconnected"),
    }
}

impl eframe::App for App {
    fn ui(&mut self, ui: &mut Ui, _frame: &mut eframe::Frame) {
        let ctx = ui.ctx().clone();
        self.handle_screenshot(&ctx);

        if self.applied != Some((self.theme_mode, self.accent_idx)) {
            theme::apply(&ctx, self.theme_mode, self.accent());
            self.applied = Some((self.theme_mode, self.accent_idx));
        }

        self.dev.poll();
        self.apply_fan_status(&ctx);
        for f in &mut self.fans {
            f.dev.poll();
        }
        for (ok, text) in self.lib.poll(&ctx) {
            self.dev
                .push_log(if ok { LogKind::Ok } else { LogKind::Warn }, text.clone());
            self.notify(&ctx, text, ok);
        }
        // files dropped onto the window: fan files go into the library (and the open
        // playlist), videos and pictures open the converter
        let dropped: Vec<PathBuf> = ctx.input(|i| {
            i.raw
                .dropped_files
                .iter()
                .map(|f| f.path().to_path_buf())
                .filter(|p| !p.as_os_str().is_empty())
                .collect()
        });
        if !dropped.is_empty() {
            let bins: Vec<PathBuf> = dropped
                .iter()
                .filter(|p| library::is_bin(p))
                .cloned()
                .collect();
            let media: Vec<PathBuf> = dropped
                .iter()
                .filter(|p| library::is_convertible(p))
                .cloned()
                .collect();
            if !bins.is_empty() {
                let into = self.page == Page::Playlists;
                self.import_bins(&ctx, bins, into);
            }
            if !media.is_empty() {
                self.open_converter(media);
            }
        }

        let p = theme::palette(ui.visuals().dark_mode, self.accent());

        egui::Panel::left("sidebar")
            .resizable(false)
            .exact_size(244.0)
            .frame(Frame::new().fill(p.sidebar).inner_margin(Margin {
                left: 16,
                right: 16,
                top: 22,
                bottom: 16,
            }))
            .show(ui, |ui| {
                let r = ui.max_rect();
                ui.painter().vline(
                    r.right() + 16.0,
                    ui.clip_rect().y_range(),
                    Stroke::new(1.0, p.border),
                );
                self.sidebar(ui, &p);
            });

        egui::CentralPanel::default()
            .frame(Frame::new().fill(p.bg).inner_margin(Margin {
                left: 32,
                right: 32,
                top: 28,
                bottom: 20,
            }))
            .show(ui, |ui| {
                ScrollArea::vertical()
                    .auto_shrink([false, false])
                    .show(ui, |ui| {
                        ui.set_width(ui.available_width() - 8.0);
                        match self.page {
                            Page::Player => self.page_player(ui, &p),
                            Page::Library => self.page_library(ui, &p),
                            Page::Playlists => self.page_playlists(ui, &p),
                            Page::Fans => self.page_fans(ui, &p),
                            Page::Clock => self.page_clock(ui, &p),
                            Page::Device => self.page_device(ui, &p),
                            Page::Settings => self.page_settings(ui, &p),
                        }
                        ui.add_space(12.0);
                    });
            });

        self.model_picker(&ctx, &p);
        self.confirm_dialog(&ctx, &p);
        self.convert_dialog(&ctx, &p);
        self.toast_ui(&ctx, &p);

        // the fan preview, the logo and the connection pulse are animated
        ctx.request_repaint_after(Duration::from_millis(if self.page == Page::Player {
            33
        } else {
            100
        }));
    }

    fn save(&mut self, storage: &mut dyn eframe::Storage) {
        if self.shot_path.is_some() {
            return;
        }
        let mut put = |k: &str, v: String| storage.set_string(k, v);
        put("page", self.page.key().into());
        put("theme", self.theme_mode.key().into());
        put("accent", self.accent_idx.to_string());
        if let Some(m) = self.model {
            put("model", MODELS[m].code.into());
        }
        put("address", self.dev.address.clone());
        put("brightness", self.brightness.to_string());
        put("picture_secs", self.picture_secs.to_string());
        put("clock_on", if self.clock_on { "1" } else { "0" }.into());
        put(
            "needle",
            if self.needle_white { "white" } else { "black" }.into(),
        );
        put("dial", self.dial.to_string());
        put("boot_h", self.boot_h.to_string());
        put("boot_m", self.boot_m.to_string());
        put("boot_s", self.boot_s.to_string());
        put("ssid", self.wifi_ssid.clone());
        if let Some(t) = &self.sd_target {
            put("sd_target", t.display().to_string());
        }
        put("fan_name", self.fan_name.clone());
        let fans: Vec<String> = self
            .fans
            .iter()
            .map(|f| format!("{}\t{}", f.name, f.dev.address))
            .collect();
        put("fans", fans.join("\n"));
        put("library", self.lib.folder.display().to_string());
        put("playlist", self.playlist.to_string());
        put("convert_mode", self.convert_mode.key().into());
        put("convert_zoom", self.convert_zoom.to_string());
        put("update_repo", self.update_repo.clone());
        put("update_on_start", if self.update_on_start { "1" } else { "0" }.into());
        self.lib.save_playlists();
    }
}
