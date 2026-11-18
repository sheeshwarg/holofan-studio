// The collection of fan files (.BIN) in one folder, turning videos and pictures into .BIN
// files (with ffmpeg reading the video), previews of every file, playlists, and copying a
// playlist to an SD card. Long work runs on background threads.

use std::collections::HashMap;
use std::io::{BufWriter, Read, Write};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{Receiver, Sender, channel};
use std::thread;

use eframe::egui;

use crate::binfmt::{self, Circle, ColorMode, FramePicker, Spec};

pub const VIDEO_EXT: [&str; 9] = [
    "mp4", "mov", "avi", "mkv", "wmv", "m4v", "webm", "flv", "gif",
];
pub const IMAGE_EXT: [&str; 5] = ["png", "jpg", "jpeg", "bmp", "webp"];

pub fn is_bin(p: &Path) -> bool {
    p.extension()
        .map(|e| e.eq_ignore_ascii_case("bin"))
        .unwrap_or(false)
}

pub fn is_convertible(p: &Path) -> bool {
    let ext = p
        .extension()
        .map(|e| e.to_string_lossy().to_lowercase())
        .unwrap_or_default();
    VIDEO_EXT.contains(&ext.as_str()) || IMAGE_EXT.contains(&ext.as_str())
}

pub struct BinFile {
    pub path: PathBuf,
    pub name: String,
    pub size: u64,
    pub spec: Option<Spec>,
    pub thumb: Option<egui::TextureHandle>,
    thumb_asked: bool,
}

impl BinFile {
    pub fn frames(&self) -> u64 {
        self.spec
            .map(|s| self.size / s.frame_bytes() as u64)
            .unwrap_or(0)
    }
    pub fn seconds(&self) -> f32 {
        self.spec
            .map(|s| self.frames() as f32 / s.fps)
            .unwrap_or(0.0)
    }
    /// Name without ".BIN".
    pub fn stem(&self) -> String {
        Path::new(&self.name)
            .file_stem()
            .map(|s| s.to_string_lossy().to_string())
            .unwrap_or_default()
    }
}

/// One second of a single colour (the fan plays 12 frames per second).
const PRESET_FRAMES: usize = 12;

/// Ready-made one-colour fan files. Black is the "blank": the fan spins but shows nothing.
#[derive(Clone, Copy, PartialEq, Debug)]
pub enum Preset {
    Black,
    White,
    Red,
    Blue,
}

impl Preset {
    pub const ALL: [Preset; 4] = [Preset::Black, Preset::White, Preset::Red, Preset::Blue];
    pub fn label(self) -> &'static str {
        match self {
            Preset::Black => "Blank (black)",
            Preset::White => "White",
            Preset::Red => "Red",
            Preset::Blue => "Blue",
        }
    }
    /// Short upper-case names, as the fan shows them.
    pub fn file_name(self) -> &'static str {
        match self {
            Preset::Black => "BLANK.BIN",
            Preset::White => "WHITE.BIN",
            Preset::Red => "RED.BIN",
            Preset::Blue => "BLUE.BIN",
        }
    }
    pub fn rgb(self) -> [u8; 3] {
        match self {
            Preset::Black => [0, 0, 0],
            Preset::White => [255, 255, 255],
            Preset::Red => [255, 0, 0],
            Preset::Blue => [0, 60, 255],
        }
    }
}

pub struct Playlist {
    pub name: String,
    /// File names in the library folder, in play order.
    pub items: Vec<String>,
}

pub struct Job {
    pub id: u64,
    pub source: PathBuf,
    pub output: PathBuf,
    pub progress: f32,
    pub done: bool,
    pub error: Option<String>,
    cancel: Arc<AtomicBool>,
}

impl Job {
    pub fn cancel(&self) {
        self.cancel.store(true, Ordering::SeqCst);
    }
}

/// Settings for turning a video into a fan file.
#[derive(Clone, Copy)]
pub struct ConvertSettings {
    pub spec: Spec,
    pub mode: ColorMode,
    /// 1.0 = the largest circle in the middle; smaller zooms in.
    pub zoom: f32,
}

enum Msg {
    Thumb {
        path: PathBuf,
        img: Option<egui::ColorImage>,
    },
    Progress {
        id: u64,
        p: f32,
    },
    Finished {
        id: u64,
        res: Result<(), String>,
    },
    CopyProgress(f32),
    CopyDone(Result<String, String>),
}

pub struct Library {
    pub folder: PathBuf,
    pub files: Vec<BinFile>,
    pub playlists: Vec<Playlist>,
    pub jobs: Vec<Job>,
    pub copying: Option<f32>,
    pub preferred_model: Option<String>,
    ffmpeg: Option<Option<(PathBuf, PathBuf)>>,
    next_id: u64,
    tx: Sender<Msg>,
    rx: Receiver<Msg>,
}

pub fn default_folder() -> PathBuf {
    let home = std::env::var_os("HOME")
        .or_else(|| std::env::var_os("USERPROFILE"))
        .map(PathBuf::from)
        .unwrap_or_else(std::env::temp_dir);
    home.join("Documents").join("HoloFan Library")
}

impl Library {
    pub fn new(folder: PathBuf) -> Self {
        let (tx, rx) = channel();
        let mut lib = Library {
            folder,
            files: Vec::new(),
            playlists: Vec::new(),
            jobs: Vec::new(),
            copying: None,
            preferred_model: None,
            ffmpeg: None,
            next_id: 1,
            tx,
            rx,
        };
        lib.rescan();
        lib.load_playlists();
        lib
    }

    pub fn set_folder(&mut self, folder: PathBuf) {
        self.save_playlists();
        self.folder = folder;
        self.files.clear();
        self.rescan();
        self.load_playlists();
    }

    /// Read the folder again (keeps previews of files that are still there).
    pub fn rescan(&mut self) {
        let mut old: HashMap<PathBuf, BinFile> =
            self.files.drain(..).map(|f| (f.path.clone(), f)).collect();
        let mut found = Vec::new();
        if let Ok(rd) = std::fs::read_dir(&self.folder) {
            for e in rd.flatten() {
                let path = e.path();
                if !is_bin(&path) || !path.is_file() {
                    continue;
                }
                let size = e.metadata().map(|m| m.len()).unwrap_or(0);
                // a file being written by a running job isn't finished yet
                if self.jobs.iter().any(|j| !j.done && j.output == path) {
                    continue;
                }
                let f = match old.remove(&path) {
                    Some(mut f) if f.size == size => {
                        f.spec = binfmt::spec_for_size(size, self.preferred_model.as_deref());
                        f
                    }
                    _ => BinFile {
                        name: path
                            .file_name()
                            .map(|n| n.to_string_lossy().to_string())
                            .unwrap_or_default(),
                        spec: binfmt::spec_for_size(size, self.preferred_model.as_deref()),
                        path,
                        size,
                        thumb: None,
                        thumb_asked: false,
                    },
                };
                found.push(f);
            }
        }
        found.sort_by_key(|f| f.name.to_lowercase());
        self.files = found;
    }

    pub fn file(&self, name: &str) -> Option<&BinFile> {
        self.files.iter().find(|f| f.name == name)
    }

    /// Copy .BIN files from elsewhere into the folder; returns the names now in the folder.
    pub fn import(&mut self, paths: &[PathBuf]) -> (Vec<String>, Vec<String>) {
        let _ = std::fs::create_dir_all(&self.folder);
        let (mut ok, mut failed) = (Vec::new(), Vec::new());
        for p in paths.iter().filter(|p| is_bin(p)) {
            let name = p
                .file_name()
                .map(|n| n.to_string_lossy().to_string())
                .unwrap_or_default();
            let dest = self.folder.join(&name);
            if p.parent() == Some(self.folder.as_path()) || dest.exists() {
                ok.push(name);
                continue;
            }
            match std::fs::copy(p, &dest) {
                Ok(_) => ok.push(name),
                Err(e) => failed.push(format!("{name}: {e}")),
            }
        }
        self.rescan();
        (ok, failed)
    }

    pub fn delete(&mut self, name: &str) -> std::io::Result<()> {
        std::fs::remove_file(self.folder.join(name))?;
        for pl in &mut self.playlists {
            pl.items.retain(|i| i != name);
        }
        self.save_playlists();
        self.rescan();
        Ok(())
    }

    /// Make (or remake) a one-colour fan file in the library; returns its file name.
    pub fn make_preset(&mut self, preset: Preset, spec: &Spec) -> Result<String, String> {
        std::fs::create_dir_all(&self.folder).map_err(|e| e.to_string())?;
        let name = preset.file_name().to_string();
        let [r, g, b] = preset.rgb();
        let polar: Vec<u8> = (0..spec.rows * spec.leds).flat_map(|_| [r, g, b]).collect();
        let frame = binfmt::encode_frame(&polar, spec, &ColorMode::Common.table());
        let mut data = Vec::with_capacity(frame.len() * PRESET_FRAMES);
        for _ in 0..PRESET_FRAMES {
            data.extend_from_slice(&frame);
        }
        std::fs::write(self.folder.join(&name), data).map_err(|e| e.to_string())?;
        // a remade file needs a fresh preview
        self.files.retain(|f| f.name != name);
        self.rescan();
        Ok(name)
    }

    /// Ask for previews of files that don't have one yet (decoded on a background thread).
    pub fn request_thumbs(&mut self, ctx: &egui::Context) {
        for f in self.files.iter_mut().filter(|f| !f.thumb_asked) {
            f.thumb_asked = true;
            let Some(spec) = f.spec else { continue };
            let (path, size, tx, ctx) = (f.path.clone(), f.size, self.tx.clone(), ctx.clone());
            thread::spawn(move || {
                let img = thumbnail(&path, size, &spec, 160);
                let _ = tx.send(Msg::Thumb { path, img });
                ctx.request_repaint();
            });
        }
    }

    // ------------------------------------------------------------- converting

    /// ffmpeg and ffprobe: on the PATH, from Homebrew, or next to this program.
    pub fn ffmpeg(&mut self) -> Option<(PathBuf, PathBuf)> {
        self.ffmpeg.get_or_insert_with(find_ffmpeg).clone()
    }

    pub fn convert(
        &mut self,
        source: PathBuf,
        name: &str,
        settings: ConvertSettings,
        ctx: &egui::Context,
    ) -> Result<PathBuf, String> {
        let (ffmpeg, ffprobe) = self.ffmpeg().ok_or("ffmpeg isn't installed (it reads the videos). Install it from ffmpeg.org or with Homebrew: brew install ffmpeg")?;
        let _ = std::fs::create_dir_all(&self.folder);
        let output = self.folder.join(format!("{}.BIN", clean_name(name)));
        let id = self.next_id;
        self.next_id += 1;
        let cancel = Arc::new(AtomicBool::new(false));
        self.jobs.push(Job {
            id,
            source: source.clone(),
            output: output.clone(),
            progress: 0.0,
            done: false,
            error: None,
            cancel: cancel.clone(),
        });
        let (tx, ctx) = (self.tx.clone(), ctx.clone());
        let out2 = output.clone();
        thread::spawn(move || {
            let res = convert_file(
                &ffmpeg,
                &ffprobe,
                &source,
                &out2,
                settings,
                &cancel,
                &mut |p| {
                    let _ = tx.send(Msg::Progress { id, p });
                    ctx.request_repaint();
                },
            );
            if res.is_err() {
                let _ = std::fs::remove_file(&out2);
            }
            let _ = tx.send(Msg::Finished { id, res });
            ctx.request_repaint();
        });
        Ok(output)
    }

    pub fn busy(&self) -> bool {
        self.jobs.iter().any(|j| !j.done)
    }

    // ------------------------------------------------------------- playlists

    fn playlists_file(&self) -> PathBuf {
        self.folder.join("playlists.txt")
    }

    /// Saved next to the files as plain text: "# name" and then one file name per line.
    pub fn save_playlists(&self) {
        // nothing to keep yet: don't create the folder just for an empty list
        if !self.folder.exists() && self.playlists.iter().all(|p| p.items.is_empty()) {
            return;
        }
        let mut s = String::new();
        for pl in &self.playlists {
            s.push_str("# ");
            s.push_str(&pl.name);
            s.push('\n');
            for i in &pl.items {
                s.push_str(i);
                s.push('\n');
            }
        }
        let _ = std::fs::create_dir_all(&self.folder);
        let _ = std::fs::write(self.playlists_file(), s);
    }

    fn load_playlists(&mut self) {
        self.playlists.clear();
        if let Ok(s) = std::fs::read_to_string(self.playlists_file()) {
            for line in s.lines() {
                if let Some(n) = line.strip_prefix("# ") {
                    self.playlists.push(Playlist {
                        name: n.to_string(),
                        items: Vec::new(),
                    });
                } else if !line.trim().is_empty() {
                    if let Some(pl) = self.playlists.last_mut() {
                        pl.items.push(line.to_string());
                    }
                }
            }
        }
        if self.playlists.is_empty() {
            self.playlists.push(Playlist {
                name: "My playlist".into(),
                items: Vec::new(),
            });
        }
    }

    /// Copy a playlist's files in order onto an SD card (or folder), numbered so the fan
    /// plays them in that order, like the cards that come with the fan ("0…", "1…").
    pub fn copy_to_card(&mut self, playlist: usize, dest: PathBuf, ctx: &egui::Context) {
        if self.copying.is_some() {
            return;
        }
        let Some(pl) = self.playlists.get(playlist) else {
            return;
        };
        let files: Vec<PathBuf> = pl
            .items
            .iter()
            .map(|n| self.folder.join(n))
            .filter(|p| p.exists())
            .collect();
        self.copying = Some(0.0);
        let (tx, ctx) = (self.tx.clone(), ctx.clone());
        thread::spawn(move || {
            let res = (|| -> Result<String, String> {
                std::fs::create_dir_all(&dest)
                    .map_err(|e| format!("Can't write to {}: {e}", dest.display()))?;
                let total: u64 = files
                    .iter()
                    .filter_map(|f| std::fs::metadata(f).ok())
                    .map(|m| m.len())
                    .sum::<u64>()
                    .max(1);
                let mut done = 0u64;
                let width = if files.len() > 10 { 2 } else { 1 };
                for (i, f) in files.iter().enumerate() {
                    let base = f
                        .file_name()
                        .map(|n| n.to_string_lossy().to_string())
                        .unwrap_or_default();
                    let name = format!("{i:0width$}{base}");
                    done += std::fs::copy(f, dest.join(&name))
                        .map_err(|e| format!("Copying {name} failed: {e}"))?;
                    let _ = tx.send(Msg::CopyProgress(done as f32 / total as f32));
                    ctx.request_repaint();
                }
                Ok(format!(
                    "Copied {} file(s) to {}",
                    files.len(),
                    dest.display()
                ))
            })();
            let _ = tx.send(Msg::CopyDone(res));
            ctx.request_repaint();
        });
    }

    /// Apply finished background work; returns messages for the activity log (ok, text).
    pub fn poll(&mut self, ctx: &egui::Context) -> Vec<(bool, String)> {
        let mut out = Vec::new();
        let mut rescan = false;
        while let Ok(m) = self.rx.try_recv() {
            match m {
                Msg::Thumb { path, img } => {
                    if let (Some(f), Some(img)) =
                        (self.files.iter_mut().find(|f| f.path == path), img)
                    {
                        f.thumb = Some(ctx.load_texture(
                            format!("bin-{}", path.display()),
                            img,
                            egui::TextureOptions::LINEAR,
                        ));
                    }
                }
                Msg::Progress { id, p } => {
                    if let Some(j) = self.jobs.iter_mut().find(|j| j.id == id) {
                        j.progress = p;
                    }
                }
                Msg::Finished { id, res } => {
                    if let Some(j) = self.jobs.iter_mut().find(|j| j.id == id) {
                        j.done = true;
                        let name = j
                            .output
                            .file_name()
                            .map(|n| n.to_string_lossy().to_string())
                            .unwrap_or_default();
                        match res {
                            Ok(()) => {
                                j.progress = 1.0;
                                out.push((true, format!("Converted into {name}")));
                            }
                            Err(e) => {
                                out.push((false, format!("Converting into {name} failed: {e}")));
                                j.error = Some(e);
                            }
                        }
                    }
                    rescan = true;
                }
                Msg::CopyProgress(p) => self.copying = Some(p),
                Msg::CopyDone(r) => {
                    self.copying = None;
                    match r {
                        Ok(t) => out.push((true, t)),
                        Err(e) => out.push((false, e)),
                    }
                }
            }
        }
        if rescan {
            self.rescan();
        }
        out
    }
}

/// A file name the fan and an SD card both accept.
pub fn clean_name(name: &str) -> String {
    let s: String = name
        .trim()
        .trim_end_matches(".BIN")
        .trim_end_matches(".bin")
        .chars()
        .map(|c| {
            if c.is_alphanumeric() || c == '_' || c == '-' || c == ' ' {
                c
            } else {
                '_'
            }
        })
        .collect();
    let s = s.trim().chars().take(99).collect::<String>();
    if s.is_empty() { "video".into() } else { s }
}

fn find_ffmpeg() -> Option<(PathBuf, PathBuf)> {
    let works = |p: &Path| {
        let mut c = Command::new(p);
        c.arg("-version")
            .stdout(Stdio::null())
            .stderr(Stdio::null());
        no_window(&mut c);
        c.status().map(|s| s.success()).unwrap_or(false)
    };
    let exe = if cfg!(windows) { ".exe" } else { "" };
    let mut dirs: Vec<PathBuf> = vec![PathBuf::new()];
    if let Ok(me) = std::env::current_exe() {
        if let Some(d) = me.parent() {
            dirs.push(d.to_path_buf());
            dirs.push(d.join("../Resources"));
        }
    }
    for d in ["/opt/homebrew/bin", "/usr/local/bin", "/usr/bin"] {
        dirs.push(PathBuf::from(d));
    }
    for d in dirs {
        let ff = d.join(format!("ffmpeg{exe}"));
        let fp = d.join(format!("ffprobe{exe}"));
        if works(&ff) && works(&fp) {
            return Some((ff, fp));
        }
    }
    None
}

fn no_window(_c: &mut Command) {
    #[cfg(target_os = "windows")]
    {
        use std::os::windows::process::CommandExt;
        _c.creation_flags(0x0800_0000);
    }
}

struct Probe {
    w: usize,
    h: usize,
    fps: f32,
    frames: Option<u64>,
}

fn probe(ffprobe: &Path, src: &Path) -> Result<Probe, String> {
    let mut c = Command::new(ffprobe);
    c.args([
        "-v",
        "error",
        "-select_streams",
        "v:0",
        "-show_entries",
        "stream=width,height,avg_frame_rate,r_frame_rate,nb_frames:format=duration",
        "-of",
        "default=noprint_wrappers=1",
    ])
    .arg(src);
    no_window(&mut c);
    let o = c.output().map_err(|e| e.to_string())?;
    if !o.status.success() {
        return Err("this file can't be read as a video or picture".into());
    }
    let text = String::from_utf8_lossy(&o.stdout);
    let mut kv = HashMap::new();
    for l in text.lines() {
        if let Some((k, v)) = l.split_once('=') {
            kv.entry(k.trim().to_string())
                .or_insert_with(|| v.trim().to_string());
        }
    }
    let rate = |s: &str| -> Option<f32> {
        let (a, b) = s.split_once('/').unwrap_or((s, "1"));
        let (a, b) = (a.parse::<f32>().ok()?, b.parse::<f32>().ok()?);
        (b > 0.0 && a > 0.0).then(|| a / b)
    };
    let w = kv
        .get("width")
        .and_then(|v| v.parse().ok())
        .ok_or("no picture in this file")?;
    let h = kv
        .get("height")
        .and_then(|v| v.parse().ok())
        .ok_or("no picture in this file")?;
    let fps = kv
        .get("avg_frame_rate")
        .and_then(|v| rate(v))
        .or_else(|| kv.get("r_frame_rate").and_then(|v| rate(v)))
        .unwrap_or(25.0);
    let duration: Option<f32> = kv.get("duration").and_then(|v| v.parse().ok());
    let frames = kv
        .get("nb_frames")
        .and_then(|v| v.parse().ok())
        .or_else(|| duration.map(|d| (d * fps) as u64));
    Ok(Probe { w, h, fps, frames })
}

fn convert_file(
    ffmpeg: &Path,
    ffprobe: &Path,
    src: &Path,
    out: &Path,
    s: ConvertSettings,
    cancel: &AtomicBool,
    progress: &mut dyn FnMut(f32),
) -> Result<(), String> {
    let info = probe(ffprobe, src)?;
    // read the frames a little larger than the fan needs, never larger than the source
    let target = (s.spec.leds * 4).max(256) as f32;
    let scale = (target / info.w.min(info.h) as f32).min(1.0);
    let w = (((info.w as f32 * scale) as usize) / 2 * 2).max(2);
    let h = (((info.h as f32 * scale) as usize) / 2 * 2).max(2);
    let mut c = Command::new(ffmpeg);
    c.args(["-v", "error", "-i"])
        .arg(src)
        .args([
            "-an",
            "-vf",
            &format!("scale={w}:{h}:flags=bicubic"),
            "-f",
            "rawvideo",
            "-pix_fmt",
            "rgb24",
            "-",
        ])
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .stdin(Stdio::null());
    no_window(&mut c);
    let mut child = c.spawn().map_err(|e| format!("ffmpeg didn't start: {e}"))?;
    let mut stdout = child.stdout.take().ok_or("no output from ffmpeg")?;
    let file =
        std::fs::File::create(out).map_err(|e| format!("can't write {}: {e}", out.display()))?;
    let mut writer = BufWriter::with_capacity(1 << 20, file);
    let curve = s.mode.table();
    let circle = Circle::centred(w, h, s.zoom);
    let mut picker = FramePicker::new(info.fps, s.spec.fps);
    let mut buf = vec![0u8; w * h * 3];
    let (mut read, mut written) = (0u64, 0u64);
    let total = info.frames.unwrap_or(0).max(1);
    loop {
        if cancel.load(Ordering::SeqCst) {
            let _ = child.kill();
            return Err("cancelled".into());
        }
        if let Err(e) = stdout.read_exact(&mut buf) {
            if e.kind() == std::io::ErrorKind::UnexpectedEof {
                break;
            }
            return Err(e.to_string());
        }
        read += 1;
        if !picker.keep() {
            continue;
        }
        let polar = binfmt::unwrap(&buf, w, h, circle, &s.spec);
        writer
            .write_all(&binfmt::encode_frame(&polar, &s.spec, &curve))
            .map_err(|e| e.to_string())?;
        written += 1;
        if read % 4 == 0 {
            progress((read as f32 / total as f32).min(0.99));
        }
    }
    let status = child.wait().map_err(|e| e.to_string())?;
    writer.flush().map_err(|e| e.to_string())?;
    if written == 0 {
        let mut err = String::new();
        if let Some(mut e) = child.stderr.take() {
            let _ = e.read_to_string(&mut err);
        }
        return Err(err
            .lines()
            .last()
            .map(str::to_string)
            .unwrap_or_else(|| format!("no frames could be read ({status})")));
    }
    Ok(())
}

/// A round preview of a .BIN file (its middle frame).
fn thumbnail(path: &Path, size: u64, spec: &Spec, px: usize) -> Option<egui::ColorImage> {
    use std::io::{Seek, SeekFrom};
    let fb = spec.frame_bytes() as u64;
    let frames = size / fb;
    if frames == 0 {
        return None;
    }
    let mut f = std::fs::File::open(path).ok()?;
    let mut frame = vec![0u8; fb as usize];
    // the middle frame, or the first that isn't black
    for k in [frames / 2, frames / 3, frames * 2 / 3, 0] {
        f.seek(SeekFrom::Start(k * fb)).ok()?;
        f.read_exact(&mut frame).ok()?;
        if frame.iter().any(|&b| b != 0) {
            break;
        }
    }
    let polar = binfmt::decode_frame(&frame, spec);
    let rgba = binfmt::render_disc(&polar, spec, px);
    Some(egui::ColorImage::from_rgba_unmultiplied([px, px], &rgba))
}

/// A still preview of how a picture would look on the fan (used by the converter dialog).
pub fn preview_of(
    rgb: &[u8],
    w: usize,
    h: usize,
    s: ConvertSettings,
    px: usize,
) -> egui::ColorImage {
    let polar = binfmt::unwrap(rgb, w, h, Circle::centred(w, h, s.zoom), &s.spec);
    let frame = binfmt::encode_frame(&polar, &s.spec, &s.mode.table());
    let back = binfmt::decode_frame(&frame, &s.spec);
    egui::ColorImage::from_rgba_unmultiplied([px, px], &binfmt::render_disc(&back, &s.spec, px))
}

/// One picture from the middle of a video (for the converter's preview), via ffmpeg.
pub fn grab_frame(ffmpeg: &Path, ffprobe: &Path, src: &Path) -> Option<(Vec<u8>, usize, usize)> {
    let info = probe(ffprobe, src).ok()?;
    let scale = (480.0 / info.w.min(info.h) as f32).min(1.0);
    let w = (((info.w as f32 * scale) as usize) / 2 * 2).max(2);
    let h = (((info.h as f32 * scale) as usize) / 2 * 2).max(2);
    let at = info
        .frames
        .map(|f| f as f32 / info.fps / 2.0)
        .unwrap_or(0.0);
    let mut c = Command::new(ffmpeg);
    c.args(["-v", "error", "-ss", &format!("{at:.2}"), "-i"])
        .arg(src)
        .args([
            "-frames:v",
            "1",
            "-vf",
            &format!("scale={w}:{h}"),
            "-f",
            "rawvideo",
            "-pix_fmt",
            "rgb24",
            "-",
        ])
        .stdin(Stdio::null());
    no_window(&mut c);
    let o = c.output().ok()?;
    (o.stdout.len() >= w * h * 3).then(|| (o.stdout[..w * h * 3].to_vec(), w, h))
}

/// Removable drives (SD cards, USB sticks) currently attached.
pub fn removable_drives() -> Vec<(String, PathBuf, u64)> {
    let Ok(disks) = sysinfo::Disks::new_with_refreshed_list() else {
        return Vec::new();
    };
    disks
        .iter()
        .filter(|d| d.is_removable() && d.total_space() > 0)
        .map(|d| {
            let mount = d.mount_point().to_path_buf();
            let name = d.name().to_string_lossy().to_string();
            let label = if name.is_empty() {
                mount.display().to_string()
            } else {
                format!("{name} ({})", mount.display())
            };
            (label, mount, d.available_space())
        })
        .collect()
}

pub fn fmt_bytes(b: u64) -> String {
    let units = ["B", "KB", "MB", "GB", "TB"];
    let mut v = b as f64;
    let mut i = 0;
    while v >= 1024.0 && i < units.len() - 1 {
        v /= 1024.0;
        i += 1;
    }
    if i == 0 {
        format!("{b} B")
    } else if v >= 100.0 {
        format!("{v:.0} {}", units[i])
    } else {
        format!("{v:.1} {}", units[i])
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Converts one of the person's clips with ffmpeg and saves a preview of a frame
    /// (run with: cargo test convert_clip -- --ignored).
    #[test]
    #[ignore]
    fn convert_clip() {
        let home = PathBuf::from(std::env::var_os("HOME").unwrap());
        let src = home.join("Documents/holo-clips/343831_medium.mp4");
        let out = std::env::temp_dir().join("holo-test.BIN");
        let (ff, fp) = find_ffmpeg().expect("ffmpeg");
        let spec = binfmt::spec_for_code("42-F2").unwrap();
        let s = ConvertSettings {
            spec,
            mode: ColorMode::Colorful,
            zoom: 1.0,
        };
        let cancel = AtomicBool::new(false);
        convert_file(&ff, &fp, &src, &out, s, &cancel, &mut |_| {}).unwrap();
        let len = std::fs::metadata(&out).unwrap().len();
        assert_eq!(len % spec.frame_bytes() as u64, 0);
        let img = thumbnail(&out, len, &spec, 320).unwrap();
        let rgba: Vec<u8> = img.pixels.iter().flat_map(|c| c.to_array()).collect();
        image::save_buffer(
            std::env::temp_dir().join("holo-test.png"),
            &rgba,
            320,
            320,
            image::ColorType::Rgba8,
        )
        .unwrap();
        eprintln!("frames: {}", len / spec.frame_bytes() as u64);
    }
}
