// Connection to the hologram fan over its Wi-Fi network, speaking the fan's own protocol
// (protocol.rs, MECHANICS.md). One background thread keeps the command connection open and
// reconnects by itself; uploads run on their own connection, as in the original.

use std::collections::VecDeque;
use std::io::{ErrorKind, Read, Write};
use std::net::{SocketAddr, TcpStream, ToSocketAddrs};
use std::path::{Path, PathBuf};
use std::sync::mpsc::{Receiver, Sender, TryRecvError, channel};
use std::thread;
use std::time::{Duration, Instant};

use eframe::egui;

use crate::protocol::{self, ClockBuffer, Command, FanStatus, FrameReader, Incoming, UploadReply};

/// What the app asks the fan to do.
#[derive(Clone, Debug)]
pub enum Cmd {
    Prev,
    Next,
    PlayPause,
    SingleLoop,
    ListLoop,
    Power,
    FormatDisk,
    ClearCache,
    BrightnessUp,
    BrightnessDown,
    RotateCw,
    RotateCcw,
    Clock {
        on: bool,
        needle_white: bool,
        dial: &'static str,
    },
    PowerOnTime {
        h: u32,
        m: u32,
        s: u32,
    },
    PictureTime(u32),
    Wifi {
        ssid: String,
        password: String,
    },
    AskWifi,
    PlayFile(usize),
    DeleteFile(usize),
    Upload(Vec<PathBuf>),
}

impl Cmd {
    pub fn label(&self) -> String {
        match self {
            Cmd::Prev => "Previous".into(),
            Cmd::Next => "Next".into(),
            Cmd::PlayPause => "Play / pause".into(),
            Cmd::SingleLoop => "Single loop".into(),
            Cmd::ListLoop => "List loop".into(),
            Cmd::Power => "Power on/off".into(),
            Cmd::FormatDisk => "Format fan storage".into(),
            Cmd::ClearCache => "Clear cache".into(),
            Cmd::BrightnessUp => "Brightness +".into(),
            Cmd::BrightnessDown => "Brightness −".into(),
            Cmd::RotateCw => "Rotate clockwise".into(),
            Cmd::RotateCcw => "Rotate counter-clockwise".into(),
            Cmd::Clock { .. } => "Clock settings".into(),
            Cmd::PowerOnTime { .. } => "Power-on display time".into(),
            Cmd::PictureTime(_) => "Picture play time".into(),
            Cmd::Wifi { .. } => "Wi-Fi settings".into(),
            Cmd::AskWifi => "Ask for the Wi-Fi name".into(),
            Cmd::PlayFile(i) => format!("Play file {}", i + 1),
            Cmd::DeleteFile(i) => format!("Delete file {}", i + 1),
            Cmd::Upload(f) => format!("Upload {} file(s)", f.len()),
        }
    }

    /// The fan commands this stands for (the clock card sends three).
    fn commands(&self) -> Vec<Command> {
        match self {
            Cmd::Prev => vec![Command::Prev],
            Cmd::Next => vec![Command::Next],
            Cmd::PlayPause => vec![Command::PlayPause],
            Cmd::SingleLoop => vec![Command::SingleLoop],
            Cmd::ListLoop => vec![Command::ListLoop],
            Cmd::Power => vec![Command::Power],
            Cmd::FormatDisk => vec![Command::FormatDisk],
            Cmd::ClearCache => vec![Command::ClearCache],
            Cmd::BrightnessUp => vec![Command::BrightnessUp],
            Cmd::BrightnessDown => vec![Command::BrightnessDown],
            Cmd::RotateCw => vec![Command::RotateCw],
            Cmd::RotateCcw => vec![Command::RotateCcw],
            Cmd::Clock {
                on,
                needle_white,
                dial,
            } => vec![
                Command::Clock {
                    which: 2,
                    value: *on as u8,
                },
                Command::Clock {
                    which: 3,
                    value: *needle_white as u8,
                },
                Command::Clock {
                    which: 4,
                    value: protocol::clock_dial_value(dial),
                },
            ],
            Cmd::PowerOnTime { h, m, s } => vec![Command::PowerOnTime(h * 3600 + m * 60 + s)],
            Cmd::PictureTime(s) => vec![Command::PictureTime((*s).clamp(5, 30) as u8)],
            Cmd::Wifi { ssid, password } => {
                vec![Command::SetWifi {
                    name: ssid.as_bytes().to_vec(),
                    password: password.as_bytes().to_vec(),
                }]
            }
            Cmd::AskWifi => vec![Command::AskWifi],
            Cmd::PlayFile(i) => vec![Command::PlayFile(*i as u8)],
            Cmd::DeleteFile(i) => vec![Command::DeleteFile(*i as u8)],
            Cmd::Upload(_) => vec![],
        }
    }
}

/// Pause between two commands (estimated; the original sends one per button click).
const COMMAND_GAP: Duration = Duration::from_millis(350);

#[derive(Clone, PartialEq, Debug)]
pub enum Status {
    Disconnected,
    Connecting,
    Connected,
    Failed,
}

#[derive(Clone, Copy, PartialEq)]
pub enum LogKind {
    Info,
    Ok,
    Warn,
    Sent,
    Fan,
}

pub struct LogLine {
    pub time: String,
    pub kind: LogKind,
    pub text: String,
}

enum Event {
    Status(Status),
    Log(LogKind, String),
    Progress(Option<f32>),
    Fan(FanStatus),
    WifiName(String),
}

pub struct Device {
    pub status: Status,
    pub log: Vec<LogLine>,
    pub upload_progress: Option<f32>,
    pub address: String,
    /// The latest status the fan reported on this connection.
    pub fan: Option<FanStatus>,
    /// Set once per new fan status, for the app to pick up (model, power, loop...).
    pub fan_changed: bool,
    pub wifi_name: Option<String>,
    cmd_tx: Option<Sender<Cmd>>,
    evt_tx: Sender<Event>,
    evt_rx: Receiver<Event>,
    started: Instant,
}

impl Device {
    pub fn new(address: String) -> Self {
        let (evt_tx, evt_rx) = channel();
        Self {
            status: Status::Disconnected,
            log: Vec::new(),
            upload_progress: None,
            address,
            fan: None,
            fan_changed: false,
            wifi_name: None,
            cmd_tx: None,
            evt_tx,
            evt_rx,
            started: Instant::now(),
        }
    }

    pub fn is_connected(&self) -> bool {
        self.status == Status::Connected
    }

    pub fn is_active(&self) -> bool {
        self.cmd_tx.is_some()
    }

    pub fn push_log(&mut self, kind: LogKind, text: impl Into<String>) {
        let t = self.started.elapsed().as_secs();
        self.log.push(LogLine {
            time: format!("{:02}:{:02}:{:02}", t / 3600, (t / 60) % 60, t % 60),
            kind,
            text: text.into(),
        });
        if self.log.len() > 500 {
            self.log.drain(0..100);
        }
    }

    /// Start connecting; the worker keeps the connection up until `disconnect`.
    pub fn connect(&mut self, ctx: &egui::Context) {
        self.disconnect();
        let (host, port) = protocol::parse_address(&self.address);
        self.address = protocol::format_address(&host, port);
        let (cmd_tx, cmd_rx) = channel();
        self.cmd_tx = Some(cmd_tx);
        self.status = Status::Connecting;
        let tx = self.evt_tx.clone();
        let ctx = ctx.clone();
        let addr = self.address.clone();
        thread::spawn(move || worker(addr, cmd_rx, tx, ctx));
    }

    pub fn disconnect(&mut self) {
        if self.cmd_tx.take().is_some() {
            self.push_log(LogKind::Info, "Disconnected from the fan.");
        }
        self.status = Status::Disconnected;
        self.upload_progress = None;
        self.fan = None;
    }

    /// Send a command; without a connection it is only logged.
    pub fn send(&mut self, cmd: Cmd) -> bool {
        if !self.is_connected() {
            self.push_log(
                LogKind::Warn,
                format!("Not connected — “{}” was not sent.", cmd.label()),
            );
            return false;
        }
        let label = cmd.label();
        if let Some(tx) = &self.cmd_tx {
            if tx.send(cmd).is_ok() {
                self.push_log(LogKind::Sent, label);
                return true;
            }
        }
        false
    }

    pub fn poll(&mut self) {
        while let Ok(e) = self.evt_rx.try_recv() {
            if self.cmd_tx.is_none() {
                continue; // late messages from a worker that was already stopped
            }
            match e {
                Event::Status(s) => {
                    if s != Status::Connected {
                        self.fan = None;
                    }
                    self.status = s;
                }
                Event::Log(k, t) => self.push_log(k, t),
                Event::Progress(p) => self.upload_progress = p,
                Event::Fan(s) => {
                    if self.fan.as_ref() != Some(&s) {
                        self.fan = Some(s);
                        self.fan_changed = true;
                    }
                }
                Event::WifiName(n) => self.wifi_name = Some(n),
            }
        }
    }
}

fn resolve(addr: &str) -> Option<SocketAddr> {
    addr.to_socket_addrs().ok().and_then(|mut a| a.next())
}

fn seconds_of_day() -> u32 {
    use chrono::Timelike;
    let now = chrono::Local::now();
    now.hour() * 3600 + now.minute() * 60 + now.second()
}

fn worker(addr: String, rx: Receiver<Cmd>, tx: Sender<Event>, ctx: egui::Context) {
    let send = |e: Event| {
        let ok = tx.send(e).is_ok();
        ctx.request_repaint();
        ok
    };
    let mut attempts = 0u32;
    let mut was_connected = false;
    loop {
        send(Event::Status(Status::Connecting));
        if attempts == 0 {
            send(Event::Log(
                LogKind::Info,
                format!("Trying to connect to the fan at {addr}…"),
            ));
        }
        let stream = match resolve(&addr) {
            Some(sa) => {
                TcpStream::connect_timeout(&sa, Duration::from_secs(3)).map_err(|e| e.to_string())
            }
            None => Err("the address is not valid".to_string()),
        };
        match stream {
            Ok(s) => {
                attempts = 0;
                match session(s, &addr, &rx, &tx, &ctx) {
                    SessionEnd::Stopped => return,
                    SessionEnd::Lost(why) => {
                        send(Event::Status(Status::Failed));
                        send(Event::Log(
                            LogKind::Warn,
                            format!("Lost the connection ({why}). Reconnecting…"),
                        ));
                        was_connected = true;
                    }
                }
            }
            Err(e) => {
                send(Event::Status(Status::Failed));
                attempts += 1;
                if attempts == 1 || attempts % 20 == 0 {
                    let hint = if was_connected {
                        ""
                    } else {
                        " Is this computer on the fan's Wi-Fi?"
                    };
                    send(Event::Log(
                        LogKind::Warn,
                        format!("No answer from the fan ({e}). Still trying.{hint}"),
                    ));
                }
            }
        }
        // short pause before the next try (the original retries at once), but stop at once
        // when the app disconnects
        let until = Instant::now() + Duration::from_millis(1000);
        while Instant::now() < until {
            match rx.try_recv() {
                Err(TryRecvError::Disconnected) => return,
                _ => thread::sleep(Duration::from_millis(50)),
            }
        }
        if tx.send(Event::Progress(None)).is_err() {
            return;
        }
    }
}

enum SessionEnd {
    Stopped,
    Lost(String),
}

/// One open command connection: say hello, then pass commands on and read what the fan says.
/// Like the original, silence from the fan never counts as a lost connection.
fn session(
    mut s: TcpStream,
    addr: &str,
    rx: &Receiver<Cmd>,
    tx: &Sender<Event>,
    ctx: &egui::Context,
) -> SessionEnd {
    let send = |e: Event| {
        let ok = tx.send(e).is_ok();
        ctx.request_repaint();
        ok
    };
    let _ = s.set_nodelay(true);
    let _ = s.set_read_timeout(Some(Duration::from_millis(50)));
    let _ = s.set_write_timeout(Some(Duration::from_secs(5)));
    if let Err(e) = s.write_all(protocol::HELLO) {
        return SessionEnd::Lost(e.to_string());
    }
    send(Event::Status(Status::Connected));
    send(Event::Log(
        LogKind::Ok,
        "Connected to the fan. Asking for its settings and file list…".into(),
    ));
    let mut clock = ClockBuffer::default();
    let mut reader = FrameReader::default();
    let mut buf = [0u8; 4096];
    let mut first_status = true;
    // The fan reads one command per network packet, so commands go out one at a time with a
    // short pause; several written at once arrive glued together and only the first counts.
    let mut pending: VecDeque<Vec<u8>> = VecDeque::new();
    let mut last_write = Instant::now() - COMMAND_GAP;
    loop {
        if !pending.is_empty() && last_write.elapsed() >= COMMAND_GAP {
            let frame = pending.pop_front().unwrap_or_default();
            if let Err(e) = s.write_all(&frame) {
                return SessionEnd::Lost(e.to_string());
            }
            last_write = Instant::now();
        }
        // pass on everything the app asked for
        loop {
            match rx.try_recv() {
                Ok(Cmd::Upload(files)) => {
                    let addr = addr.to_string();
                    let (ev, ctx) = (tx.clone(), ctx.clone());
                    thread::spawn(move || upload_files(&addr, &files, &ev, &ctx));
                }
                Ok(cmd) => {
                    for c in cmd.commands() {
                        pending.push_back(protocol::command_frame(&c.payload(&mut clock)));
                    }
                }
                Err(TryRecvError::Empty) => break,
                Err(TryRecvError::Disconnected) => return SessionEnd::Stopped,
            }
        }
        match s.read(&mut buf) {
            Ok(0) => return SessionEnd::Lost("the fan closed it".into()),
            Ok(n) => {
                reader.push(&buf[..n]);
                while let Some(msg) = reader.next() {
                    match msg {
                        Incoming::Status(st) => {
                            if first_status {
                                first_status = false;
                                let model = st.device_name().unwrap_or("unknown model");
                                send(Event::Log(
                                    LogKind::Fan,
                                    format!(
                                        "Fan reports: {model}, {} file(s) stored.",
                                        st.files.len()
                                    ),
                                ));
                            }
                            // the original sets the fan's clock whenever a clock fan reports in
                            if st.clock_version
                                && !pending
                                    .iter()
                                    .any(|f| f.get(15) == Some(&b'b') && f.get(19) == Some(&0))
                            {
                                pending.push_back(protocol::command_frame(
                                    &Command::TimeSync(seconds_of_day()).payload(&mut clock),
                                ));
                            }
                            send(Event::Fan(st));
                        }
                        Incoming::WifiName(n) => {
                            let shown = if n.is_empty() {
                                "(none)".to_string()
                            } else {
                                n.clone()
                            };
                            send(Event::Log(LogKind::Fan, format!("Fan Wi-Fi name: {shown}")));
                            send(Event::WifiName(n));
                        }
                        Incoming::Notice => {
                            send(Event::Log(
                                LogKind::Fan,
                                "The fan sent a notice that this app doesn't show.".into(),
                            ));
                        }
                        Incoming::Other(k) => {
                            send(Event::Log(
                                LogKind::Fan,
                                format!("Fan message of type “{}”.", k as char),
                            ));
                        }
                    }
                }
            }
            Err(e)
                if matches!(
                    e.kind(),
                    ErrorKind::WouldBlock | ErrorKind::TimedOut | ErrorKind::Interrupted
                ) => {}
            Err(e) => return SessionEnd::Lost(e.to_string()),
        }
    }
}

fn upload_files(addr: &str, files: &[PathBuf], ev: &Sender<Event>, ctx: &egui::Context) {
    let log = |k: LogKind, t: String| {
        let _ = ev.send(Event::Log(k, t));
        ctx.request_repaint();
    };
    let files: Vec<&PathBuf> = files.iter().collect();
    let total: u64 = files
        .iter()
        .filter_map(|f| std::fs::metadata(f).ok())
        .map(|m| m.len())
        .sum::<u64>()
        .max(1);
    let mut done = 0u64;
    let _ = ev.send(Event::Progress(Some(0.0)));
    for f in files {
        match upload_one(addr, f, ev, &mut done, total) {
            Ok(wants_audio) => {
                log(LogKind::Ok, format!("Uploaded {}", file_name(f)));
                if wants_audio {
                    let mp3 = f.with_extension("mp3");
                    if mp3.exists() {
                        match upload_one(addr, &mp3, ev, &mut done, total) {
                            Ok(_) => log(
                                LogKind::Ok,
                                format!("Uploaded its sound {}", file_name(&mp3)),
                            ),
                            Err(e) => log(LogKind::Warn, format!("Sound upload failed: {e}")),
                        }
                    } else {
                        log(
                            LogKind::Info,
                            format!(
                                "The fan also takes a sound file, but {} isn't there.",
                                file_name(&mp3)
                            ),
                        );
                    }
                }
            }
            Err(e) => {
                log(
                    LogKind::Warn,
                    format!("Upload of {} failed: {e}", file_name(f)),
                );
                break;
            }
        }
    }
    let _ = ev.send(Event::Progress(None));
    ctx.request_repaint();
}

fn file_name(p: &Path) -> String {
    p.file_name()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_default()
}

/// One file on its own connection: mark, name frame, the fan's answer, the bytes, mark.
fn upload_one(
    addr: &str,
    path: &Path,
    ev: &Sender<Event>,
    done: &mut u64,
    total: u64,
) -> Result<bool, String> {
    let sa = resolve(addr).ok_or("the address is not valid")?;
    let mut file = std::fs::File::open(path).map_err(|e| e.to_string())?;
    let mut s =
        TcpStream::connect_timeout(&sa, Duration::from_secs(5)).map_err(|e| e.to_string())?;
    let _ = s.set_nodelay(true);
    let _ = s.set_read_timeout(Some(Duration::from_secs(15)));
    thread::sleep(Duration::from_millis(200));
    let name = file_name(path);
    s.write_all(protocol::UPLOAD_MARK)
        .map_err(|e| e.to_string())?;
    s.write_all(&protocol::upload_frame(name.as_bytes()))
        .map_err(|e| e.to_string())?;
    let mut reply = Vec::new();
    let mut buf = [0u8; 1460];
    while reply.len() < 28 {
        match s.read(&mut buf) {
            Ok(0) => break,
            Ok(n) => reply.extend_from_slice(&buf[..n]),
            Err(e) => return Err(format!("no answer from the fan ({e})")),
        }
    }
    let wants_audio = match protocol::parse_upload_reply(&reply) {
        UploadReply::Accept { wants_audio } => wants_audio,
        UploadReply::Refused(code) => {
            return Err(format!("the fan refused the file (code {code})"));
        }
        UploadReply::Garbled => return Err("the fan's answer wasn't understood".into()),
    };
    let mut chunk = vec![0u8; protocol::UPLOAD_CHUNK];
    loop {
        let n = file.read(&mut chunk).map_err(|e| e.to_string())?;
        if n == 0 {
            break;
        }
        s.write_all(&chunk[..n]).map_err(|e| e.to_string())?;
        *done += n as u64;
        let _ = ev.send(Event::Progress(Some(
            (*done as f32 / total as f32).min(1.0),
        )));
    }
    s.write_all(protocol::UPLOAD_MARK)
        .map_err(|e| e.to_string())?;
    Ok(wants_audio)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::net::TcpListener;

    /// A pretend fan on this computer: checks the hello, answers with a status frame and
    /// records what the app sends.
    #[test]
    fn talks_to_a_fan() {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let port = listener.local_addr().unwrap().port();
        let fan = thread::spawn(move || {
            let (mut c, _) = listener.accept().unwrap();
            c.set_read_timeout(Some(Duration::from_secs(5))).unwrap();
            let mut hello = [0u8; 24];
            c.read_exact(&mut hello).unwrap();
            assert_eq!(&hello, protocol::HELLO);
            let mut payload = vec![b'i', 5];
            payload.extend_from_slice(b"A.BIN");
            payload.extend_from_slice(&[0, 12, 0, 0, 1, 1, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0]);
            c.write_all(&protocol::command_frame(&payload)).unwrap();
            let mut got = [0u8; 28];
            c.read_exact(&mut got).unwrap();
            got
        });
        let ctx = egui::Context::default();
        let mut dev = Device::new(format!("127.0.0.1:{port}"));
        dev.connect(&ctx);
        let until = Instant::now() + Duration::from_secs(5);
        while dev.fan.is_none() && Instant::now() < until {
            dev.poll();
            thread::sleep(Duration::from_millis(20));
        }
        let st = dev.fan.clone().expect("status from the fan");
        assert_eq!(st.files, vec!["A.BIN".to_string()]);
        assert_eq!(st.device_name(), Some("42-F2"));
        assert!(dev.send(Cmd::Power));
        let got = fan.join().unwrap();
        assert_eq!(got.to_vec(), protocol::command_frame(b"a"));
        dev.disconnect();
    }
}
