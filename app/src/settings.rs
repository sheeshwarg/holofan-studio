// The Settings page: appearance, about, and checking a GitHub repository for a newer release.

use std::process::{Command, Stdio};
use std::sync::mpsc::{Receiver, channel};
use std::thread;

use eframe::egui::{self, Align, Frame, Layout, RichText, Sense, Stroke, TextEdit, Ui, vec2};

use crate::app::App;
use crate::paint::{input_frame, setting_row, short};
use crate::theme::{ACCENTS, Palette, ThemeMode};
use crate::widgets::*;

pub const VERSION: &str = env!("CARGO_PKG_VERSION");

#[derive(Clone, Debug, PartialEq)]
pub struct Release {
    pub tag: String,
    pub name: String,
    pub url: String,
    pub published: String,
    /// A .dmg (on a Mac) or .exe (on Windows) attached to the release.
    pub download: Option<String>,
}

#[derive(Clone, Debug, PartialEq)]
pub enum UpdateState {
    Idle,
    Checking,
    UpToDate(Release),
    Available(Release),
    Failed(String),
}

pub struct Updater {
    pub state: UpdateState,
    rx: Option<Receiver<UpdateState>>,
}

impl Updater {
    pub fn new() -> Self {
        Updater { state: UpdateState::Idle, rx: None }
    }

    /// Ask GitHub for the repository's latest release (in the background, with curl, which
    /// macOS and Windows 10+ include).
    pub fn check(&mut self, repo: &str, ctx: &egui::Context) {
        let repo = clean_repo(repo);
        if repo.is_empty() {
            self.state = UpdateState::Failed("Enter the repository first (owner/name).".into());
            return;
        }
        let (tx, rx) = channel();
        self.rx = Some(rx);
        self.state = UpdateState::Checking;
        let ctx = ctx.clone();
        thread::spawn(move || {
            let _ = tx.send(fetch_latest(&repo));
            ctx.request_repaint();
        });
    }

    pub fn poll(&mut self) {
        if let Some(rx) = &self.rx {
            if let Ok(s) = rx.try_recv() {
                self.state = s;
                self.rx = None;
            }
        }
    }
}

/// "https://github.com/owner/name(.git)" or "owner/name" → "owner/name".
pub fn clean_repo(s: &str) -> String {
    let s = s.trim().trim_end_matches('/').trim_end_matches(".git");
    let s = s.strip_prefix("https://").or_else(|| s.strip_prefix("http://")).unwrap_or(s);
    let s = s.strip_prefix("github.com/").or_else(|| s.strip_prefix("www.github.com/")).unwrap_or(s);
    let parts: Vec<&str> = s.split('/').filter(|p| !p.is_empty()).collect();
    if parts.len() >= 2 { format!("{}/{}", parts[0], parts[1]) } else { String::new() }
}

fn fetch_latest(repo: &str) -> UpdateState {
    let url = format!("https://api.github.com/repos/{repo}/releases/latest");
    let mut c = Command::new("curl");
    c.args(["-sS", "-L", "--max-time", "15", "-H", "Accept: application/vnd.github+json", "-H", "User-Agent: HoloFan-Studio", "-w", "\n%{http_code}"])
        .arg(&url)
        .stdin(Stdio::null());
    #[cfg(target_os = "windows")]
    {
        use std::os::windows::process::CommandExt;
        c.creation_flags(0x0800_0000);
    }
    let out = match c.output() {
        Ok(o) => o,
        Err(e) => return UpdateState::Failed(format!("Couldn't run curl: {e}")),
    };
    let text = String::from_utf8_lossy(&out.stdout).to_string();
    let (body, code) = text.rsplit_once('\n').unwrap_or((&text, ""));
    match code.trim() {
        "200" => {}
        "404" => return UpdateState::Failed("No published release found in this repository (or it is private).".into()),
        "403" => return UpdateState::Failed("GitHub refused the request (too many checks?). Try again later.".into()),
        "" => {
            let err = String::from_utf8_lossy(&out.stderr).trim().to_string();
            return UpdateState::Failed(if err.is_empty() { "No answer from GitHub. Is this computer online?".into() } else { err });
        }
        c => return UpdateState::Failed(format!("GitHub answered with code {c}.")),
    }
    let Some(rel) = parse_release(body) else { return UpdateState::Failed("GitHub's answer wasn't understood.".into()) };
    if is_newer(&rel.tag, VERSION) { UpdateState::Available(rel) } else { UpdateState::UpToDate(rel) }
}

/// The string value of `"key": "…"` (first one) in GitHub's JSON.
fn json_str(body: &str, key: &str) -> Option<String> {
    let pat = format!("\"{key}\"");
    let i = body.find(&pat)? + pat.len();
    let rest = body[i..].trim_start().strip_prefix(':')?.trim_start();
    let rest = rest.strip_prefix('"')?;
    let mut out = String::new();
    let mut chars = rest.chars();
    while let Some(c) = chars.next() {
        match c {
            '"' => return Some(out),
            '\\' => match chars.next()? {
                'n' => out.push('\n'),
                't' => out.push('\t'),
                'u' => {
                    let hex: String = chars.by_ref().take(4).collect();
                    out.push(u32::from_str_radix(&hex, 16).ok().and_then(char::from_u32).unwrap_or('?'));
                }
                o => out.push(o),
            },
            c => out.push(c),
        }
    }
    None
}

fn parse_release(body: &str) -> Option<Release> {
    let tag = json_str(body, "tag_name")?;
    let url = json_str(body, "html_url").unwrap_or_default();
    let name = json_str(body, "name").unwrap_or_else(|| tag.clone());
    let published = json_str(body, "published_at").unwrap_or_default().chars().take(10).collect();
    let want = if cfg!(target_os = "macos") { ".dmg" } else if cfg!(windows) { ".exe" } else { ".AppImage" };
    let mut download = None;
    let mut rest = body;
    while let Some(i) = rest.find("\"browser_download_url\"") {
        let tail = &rest[i..];
        if let Some(u) = json_str(tail, "browser_download_url") {
            if u.to_lowercase().ends_with(want) {
                download = Some(u);
                break;
            }
        }
        rest = &rest[i + 22..];
    }
    Some(Release { tag, name, url, published, download })
}

fn version_parts(v: &str) -> Vec<u64> {
    v.trim()
        .trim_start_matches(|c: char| !c.is_ascii_digit())
        .split(|c: char| !c.is_ascii_digit())
        .filter(|s| !s.is_empty())
        .filter_map(|s| s.parse().ok())
        .collect()
}

/// Is the release tag ("v13.1", "13.0.2") newer than this version?
pub fn is_newer(tag: &str, current: &str) -> bool {
    let (a, b) = (version_parts(tag), version_parts(current));
    for i in 0..a.len().max(b.len()) {
        let (x, y) = (a.get(i).copied().unwrap_or(0), b.get(i).copied().unwrap_or(0));
        if x != y {
            return x > y;
        }
    }
    false
}

pub fn open_url(url: &str) {
    #[cfg(target_os = "macos")]
    let _ = Command::new("open").arg(url).spawn();
    #[cfg(target_os = "windows")]
    let _ = Command::new("cmd").args(["/C", "start", "", url]).spawn();
    #[cfg(all(unix, not(target_os = "macos")))]
    let _ = Command::new("xdg-open").arg(url).spawn();
}

impl App {
    pub(crate) fn page_settings(&mut self, ui: &mut Ui, p: &Palette) {
        let ctx = ui.ctx().clone();
        self.updater.poll();
        page_header(ui, p, "Settings", "Appearance, information and updates");
        let total_w = ui.available_width();
        let col_w = ((total_w - 16.0) / 2.0).floor();
        ui.horizontal_top(|ui| {
            ui.spacing_mut().item_spacing.x = 16.0;
            ui.vertical(|ui| {
                ui.set_width(col_w);
                card(ui, p, |ui| {
                    ui.set_width(col_w - 36.0);
                    ui.label(title("Appearance", 16.0, p.text));
                    ui.add_space(8.0);
                    setting_row(ui, p, "Theme", "Follow the system, or always dark or light", |ui| {
                        let modes = [ThemeMode::System, ThemeMode::Dark, ThemeMode::Light];
                        let idx = modes.iter().position(|m| *m == self.theme_mode).unwrap_or(0);
                        let labels: Vec<&str> = modes.iter().map(|m| m.label()).collect();
                        if let Some(i) = segmented(ui, p, &labels, idx) {
                            self.theme_mode = modes[i];
                        }
                    });
                    divider(ui, p);
                    ui.add_space(8.0);
                    setting_row(ui, p, "Accent colour", ACCENTS[self.accent_idx].0, |ui| {
                        for i in (0..ACCENTS.len()).rev() {
                            let (rect, resp) = ui.allocate_exact_size(vec2(24.0, 24.0), Sense::click());
                            if i == self.accent_idx {
                                ui.painter().circle_stroke(rect.center(), 11.0, Stroke::new(2.0, ACCENTS[i].1));
                            }
                            ui.painter().circle_filled(rect.center(), 8.0, ACCENTS[i].1);
                            if resp.on_hover_text(ACCENTS[i].0).clicked() {
                                self.accent_idx = i;
                            }
                        }
                    });
                });
                ui.add_space(14.0);
                card(ui, p, |ui| {
                    ui.set_width(col_w - 36.0);
                    ui.label(title("About", 16.0, p.text));
                    ui.add_space(6.0);
                    kv(ui, p, "App", &format!("HoloFan Studio {VERSION}"));
                    kv(ui, p, "System", &format!("{} ({})", std::env::consts::OS, std::env::consts::ARCH));
                    kv(ui, p, "Fan protocol", "TCP port 20320, from the original V13.0 app");
                    let ff = if self.lib.ffmpeg().is_some() { "found" } else { "not found (needed to convert videos)" };
                    kv(ui, p, "ffmpeg", ff);
                    kv(ui, p, "Library", &short(&self.lib.folder.display().to_string(), 34));
                    ui.add_space(6.0);
                    ui.label(
                        RichText::new("Controls and programs 3D hologram fans: playback, brightness, clock, Wi-Fi, video conversion to .BIN, playlists, SD cards and several fans at once.")
                            .size(12.0)
                            .color(p.muted),
                    );
                });
            });
            ui.vertical(|ui| {
                ui.set_width(col_w);
                card(ui, p, |ui| {
                    ui.set_width(col_w - 36.0);
                    ui.label(title("Updates", 16.0, p.text));
                    ui.label(RichText::new(format!("You have version {VERSION}. The app looks for a newer release in a GitHub repository.")).size(12.0).color(p.muted));
                    ui.add_space(10.0);
                    ui.label(RichText::new("GitHub repository").size(12.5).color(p.text));
                    input_frame(p).show(ui, |ui| {
                        ui.add(TextEdit::singleline(&mut self.update_repo).frame(Frame::NONE).desired_width(f32::INFINITY).hint_text("owner/name or https://github.com/owner/name"));
                    });
                    ui.add_space(8.0);
                    setting_row(ui, p, "Check when the app starts", "Once per start, quietly", |ui| {
                        toggle(ui, p, &mut self.update_on_start);
                    });
                    ui.add_space(8.0);
                    ui.horizontal(|ui| {
                        let checking = self.updater.state == UpdateState::Checking;
                        if primary_button(ui, p, if checking { "  Checking…  " } else { "  Check for updates  " }, !checking).clicked() {
                            let repo = self.update_repo.clone();
                            self.updater.check(&repo, &ctx);
                        }
                        let repo = clean_repo(&self.update_repo);
                        if !repo.is_empty() && ui.add(egui::Button::new(RichText::new("Open releases page").size(12.5).color(p.accent)).frame(false)).clicked() {
                            open_url(&format!("https://github.com/{repo}/releases"));
                        }
                    });
                    ui.add_space(10.0);
                    match self.updater.state.clone() {
                        UpdateState::Idle => {}
                        UpdateState::Checking => {
                            ui.label(RichText::new("Asking GitHub…").size(12.5).color(p.muted));
                            ctx.request_repaint_after(std::time::Duration::from_millis(200));
                        }
                        UpdateState::UpToDate(r) => {
                            ui.horizontal(|ui| {
                                chip(ui, "Up to date", p.success, crate::theme::mix(p.card, p.success, 0.16));
                                ui.label(RichText::new(format!("Latest release: {} ({})", r.tag, r.published)).size(12.5).color(p.muted));
                            });
                        }
                        UpdateState::Available(r) => {
                            ui.horizontal(|ui| {
                                chip(ui, "Update available", p.accent, p.accent_soft);
                                ui.label(RichText::new(format!("{} · {}", r.name, r.published)).size(12.5).color(p.text));
                            });
                            ui.add_space(6.0);
                            ui.horizontal(|ui| {
                                if let Some(d) = &r.download {
                                    if primary_button(ui, p, "  Download  ", true).clicked() {
                                        open_url(d);
                                    }
                                }
                                if secondary_button(ui, p, "  Release notes  ").clicked() {
                                    open_url(&r.url);
                                }
                            });
                        }
                        UpdateState::Failed(e) => {
                            ui.label(RichText::new(e).size(12.5).color(p.warning));
                        }
                    }
                });
                ui.add_space(14.0);
                card(ui, p, |ui| {
                    ui.set_width(col_w - 36.0);
                    ui.horizontal(|ui| {
                        ui.label(title("Fan files", 16.0, p.text));
                        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                            if ui.add(egui::Button::new(RichText::new("Change folder…").size(12.5).color(p.accent)).frame(false)).clicked() {
                                if let Some(d) = rfd::FileDialog::new().set_title("Library folder").pick_folder() {
                                    self.lib.set_folder(d);
                                }
                            }
                        });
                    });
                    ui.label(RichText::new(self.lib.folder.display().to_string()).size(12.0).color(p.muted));
                });
            });
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn versions() {
        assert!(is_newer("v13.1", "13.0.0"));
        assert!(is_newer("13.0.1", "13.0.0"));
        assert!(!is_newer("v13.0", "13.0.0"));
        assert!(!is_newer("12.9", "13.0.0"));
    }

    #[test]
    fn repos() {
        assert_eq!(clean_repo("https://github.com/acme/holofan.git"), "acme/holofan");
        assert_eq!(clean_repo("acme/holofan"), "acme/holofan");
        assert_eq!(clean_repo("holofan"), "");
    }

    #[test]
    fn release_json() {
        let body = r#"{"html_url":"https://github.com/a/b/releases/tag/v13.1","tag_name":"v13.1","name":"HoloFan 13.1","published_at":"2026-12-01T10:00:00Z","assets":[{"browser_download_url":"https://x/HoloFan.exe"},{"browser_download_url":"https://x/HoloFan.dmg"}]}"#;
        let r = parse_release(body).unwrap();
        assert_eq!(r.tag, "v13.1");
        assert_eq!(r.published, "2026-12-01");
        if cfg!(target_os = "macos") {
            assert_eq!(r.download.as_deref(), Some("https://x/HoloFan.dmg"));
        }
    }
}
