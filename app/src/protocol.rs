// The hologram fan's network protocol, as worked out from the original Windows program
// (see MECHANICS.md for the function addresses). Pure functions only: no sockets here.
//
// Commands (port 20320):
//   frame = "C0EEB7C9BAA3" + 3 length bytes + payload + "C0EEBDF9E5B7"
//   length bytes for a payload of L bytes: [L / 323, 0x63 + (L / 17) % 19, 0x62 + L % 17]
// The fan answers with frames of the same shape; their first payload byte says what they are
// ('i' status and file list, 'r' Wi-Fi name, 'v' a notice).
// Uploads use their own connection with the header "B2DDDDED" and other length digits.

pub const DEFAULT_HOST: &str = "192.168.4.1";
pub const DEFAULT_PORT: u16 = 20320;

pub const CMD_HEAD: &[u8; 12] = b"C0EEB7C9BAA3";
pub const TAIL: &[u8; 12] = b"C0EEBDF9E5B7";
pub const UPLOAD_HEAD: &[u8; 8] = b"B2DDDDED";
/// Hello sent right after the command connection opens.
pub const HELLO: &[u8] = b"C0EEB7C9BAA3C0EEBDF9E5B7";
/// Sent when an upload connection opens and again when a file has been sent.
pub const UPLOAD_MARK: &[u8] = b"B2DDDDEDC0EEBDF9E5B7";
/// Upload chunk size used by the original.
pub const UPLOAD_CHUNK: usize = 0x3c000;

/// The 21 device-type codes the fan reports in its status (index = code).
pub const DEVICE_TYPES: [&str; 21] = [
    "45-F4", "65-F4", "85-F4", "85-F6", "115-F6", "56-F4", "18-F2", "52-F4", "30-F2", "100-F6",
    "80-F4", "14-ND", "42-F2", "32-F2", "70-F6", "28-F2", "14-SD", "65-HD", "10-SD", "10-ND",
    "60-F6",
];

/// One command frame around `payload`.
pub fn command_frame(payload: &[u8]) -> Vec<u8> {
    let l = payload.len();
    let mut f = Vec::with_capacity(l + 27);
    f.extend_from_slice(CMD_HEAD);
    f.push((l / 323) as u8);
    f.push(0x63 + ((l / 17) % 19) as u8);
    f.push(0x62 + (l % 17) as u8);
    f.extend_from_slice(payload);
    f.extend_from_slice(TAIL);
    f
}

/// The frame that announces a file on an upload connection (`name` is the file name).
pub fn upload_frame(name: &[u8]) -> Vec<u8> {
    let l = name.len();
    let mut f = Vec::with_capacity(l + 23);
    f.extend_from_slice(UPLOAD_HEAD);
    f.push((l / 15) as u8);
    f.push(0x6a + ((l / 3) % 5) as u8);
    f.push(0x63 + (l % 3) as u8);
    f.extend_from_slice(name);
    f.extend_from_slice(TAIL);
    f
}

/// What the fan said about a file it was offered.
#[derive(Debug, PartialEq)]
pub enum UploadReply {
    /// Go ahead; `wants_audio` asks for the matching .mp3 too.
    Accept {
        wants_audio: bool,
    },
    Refused(u8),
    Garbled,
}

pub fn parse_upload_reply(r: &[u8]) -> UploadReply {
    if r.len() < 28 || &r[..8] != UPLOAD_HEAD || &r[16..28] != TAIL {
        return UploadReply::Garbled;
    }
    let len = r[8] as u32 * 15 + r[9] as u32 * 3 + r[10] as u32;
    if len != 0x1a6 {
        return UploadReply::Garbled;
    }
    if r[11] == 0 {
        UploadReply::Refused(r[12])
    } else {
        UploadReply::Accept {
            wants_audio: r[11] & 0x80 != 0,
        }
    }
}

/// Every command the app can give the fan.
#[derive(Clone, Debug, PartialEq)]
pub enum Command {
    Power,
    Next,
    Prev,
    PlayPause,
    SingleLoop,
    ListLoop,
    BrightnessDown,
    BrightnessUp,
    RotateCw,
    RotateCcw,
    FormatDisk,
    ClearCache,
    /// How long a picture stays on, 5..=30 s.
    PictureTime(u8),
    /// Play the fan's file at this list position (0-based).
    PlayFile(u8),
    /// Delete the fan's file at this list position (0-based).
    DeleteFile(u8),
    /// Clock settings: `which` 2 = on/off, 3 = hand colour, 4 = dial; see `clock_payload`.
    Clock {
        which: u8,
        value: u8,
    },
    /// Set the fan's clock to this second of the day.
    TimeSync(u32),
    /// Power-on display time as seconds (h*3600 + m*60 + s).
    PowerOnTime(u32),
    /// Ask the fan for its current Wi-Fi name.
    AskWifi,
    /// New Wi-Fi name and password.
    SetWifi {
        name: Vec<u8>,
        password: Vec<u8>,
    },
}

impl Command {
    /// The payload, except for the clock commands which share a buffer (see `ClockBuffer`).
    pub fn payload(&self, clock: &mut ClockBuffer) -> Vec<u8> {
        match self {
            Command::Power => vec![b'a'],
            Command::Next => vec![b'c'],
            Command::Prev => vec![b'd'],
            Command::PlayPause => vec![b'e'],
            Command::SingleLoop => vec![b'g'],
            Command::ListLoop => vec![b'h'],
            Command::BrightnessDown => vec![b'l'],
            Command::BrightnessUp => vec![b'm'],
            Command::RotateCw => vec![b'p'],
            Command::RotateCcw => vec![b'q'],
            Command::FormatDisk => vec![b'j'],
            Command::ClearCache => vec![b'k'],
            Command::PictureTime(s) => vec![b'C', (*s).clamp(5, 30)],
            Command::PlayFile(i) => vec![b'B', *i],
            Command::DeleteFile(i) => vec![b'A', *i],
            Command::AskWifi => vec![b'r'],
            Command::SetWifi { name, password } => {
                let mut v = vec![b's', name.len() as u8];
                v.extend_from_slice(name);
                v.extend_from_slice(password);
                v
            }
            Command::Clock { which, value } => clock.option(*value, *which),
            Command::TimeSync(secs) => clock.seconds(*secs, 0),
            Command::PowerOnTime(secs) => clock.seconds(*secs, 1),
        }
    }
}

/// The original keeps one 5-byte buffer for every 'b' (clock) command: option commands only
/// change bytes 1 and 4, so bytes 2 and 3 keep what the last time command left there.
#[derive(Default)]
pub struct ClockBuffer([u8; 5]);

impl ClockBuffer {
    fn seconds(&mut self, secs: u32, kind: u8) -> Vec<u8> {
        self.0[0] = b'b';
        self.0[1..5].copy_from_slice(&secs.to_le_bytes());
        self.0[4] = kind;
        self.0.to_vec()
    }
    fn option(&mut self, value: u8, which: u8) -> Vec<u8> {
        self.0[0] = b'b';
        self.0[1] = value;
        self.0[4] = which;
        self.0.to_vec()
    }
}

/// Clock option values the fan understands.
pub fn clock_dial_value(name: &str) -> u8 {
    match name {
        "Symbol" => 1,
        "Constellation" => 2,
        "Zodiac" => 3,
        _ => 0, // Digital
    }
}

/// What the fan reports in an 'i' frame.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct FanStatus {
    pub files: Vec<String>,
    /// List position of the file playing now.
    pub current: u8,
    pub device_type: u8,
    pub audio_version: bool,
    pub sleeping: bool,
    /// 0 high, 1 medium, anything else low.
    pub brightness: u8,
    pub list_loop: bool,
    pub paused: bool,
    pub clock_version: bool,
}

impl FanStatus {
    pub fn device_name(&self) -> Option<&'static str> {
        DEVICE_TYPES.get(self.device_type as usize).copied()
    }
    pub fn brightness_label(&self) -> &'static str {
        match self.brightness {
            0 => "High",
            1 => "Medium",
            _ => "Low",
        }
    }
}

#[derive(Debug, PartialEq)]
pub enum Incoming {
    Status(FanStatus),
    WifiName(String),
    Notice,
    Other(u8),
}

/// Pulls complete frames out of the bytes received so far (a TCP stream can split or join
/// frames). Bytes before a frame header are dropped.
#[derive(Default)]
pub struct FrameReader {
    buf: Vec<u8>,
}

impl FrameReader {
    pub fn push(&mut self, data: &[u8]) {
        self.buf.extend_from_slice(data);
        if self.buf.len() > 1 << 20 {
            self.buf.clear(); // never let garbage grow without bound
        }
    }

    pub fn next(&mut self) -> Option<Incoming> {
        loop {
            let start = find(&self.buf, CMD_HEAD)?;
            if start > 0 {
                self.buf.drain(..start);
            }
            if self.buf.len() < 16 {
                return None;
            }
            let value =
                self.buf[12] as usize * 323 + self.buf[13] as usize * 17 + self.buf[14] as usize;
            // value = whole frame length + 1754 (the original's check)
            if value < 1754 + 28 {
                self.buf.drain(..1);
                continue;
            }
            let total = value - 1754;
            if total > 64 * 1024 {
                self.buf.drain(..1);
                continue;
            }
            if self.buf.len() < total {
                return None;
            }
            if &self.buf[total - 12..total] != TAIL {
                self.buf.drain(..1);
                continue;
            }
            let frame: Vec<u8> = self.buf.drain(..total).collect();
            let kind = frame[15];
            let data = &frame[16..total - 12];
            return Some(match kind {
                b'i' => match parse_status(data) {
                    Some(s) => Incoming::Status(s),
                    None => Incoming::Other(kind),
                },
                b'r' => Incoming::WifiName(text(data)),
                b'v' => Incoming::Notice,
                k => Incoming::Other(k),
            });
        }
    }
}

fn find(hay: &[u8], needle: &[u8]) -> Option<usize> {
    hay.windows(needle.len()).position(|w| w == needle)
}

fn text(b: &[u8]) -> String {
    String::from_utf8_lossy(b)
        .trim_matches(char::from(0))
        .trim()
        .to_string()
}

/// 'i' data: file names as [length][bytes]..., then 16 status bytes.
fn parse_status(data: &[u8]) -> Option<FanStatus> {
    if data.len() < 16 {
        return None;
    }
    let (list, st) = data.split_at(data.len() - 16);
    let mut files = Vec::new();
    let mut i = 0;
    while i < list.len() {
        let n = list[i] as usize;
        i += 1;
        if n == 0 || i + n > list.len() {
            break;
        }
        files.push(text(&list[i..i + n]));
        i += n;
    }
    Some(FanStatus {
        files,
        current: st[0],
        device_type: st[1],
        audio_version: st[2] != 0,
        sleeping: st[3] != 0,
        brightness: st[4],
        list_loop: st[5] != 0,
        paused: st[6] != 0,
        clock_version: st[7] != 0,
    })
}

/// Turns what the person typed into host and port. Without a port the fan's 20320 is used;
/// "192.168.4.120320" (port typed without the colon) is read as 192.168.4.1:20320.
pub fn parse_address(text: &str) -> (String, u16) {
    let t = text.trim();
    if t.is_empty() {
        return (DEFAULT_HOST.into(), DEFAULT_PORT);
    }
    if let Some((h, p)) = t.rsplit_once(|c| c == ':' || c == ' ') {
        if let Ok(port) = p.trim().parse::<u16>() {
            let h = h.trim();
            return (
                if h.is_empty() {
                    DEFAULT_HOST.into()
                } else {
                    h.into()
                },
                port,
            );
        }
    }
    let parts: Vec<&str> = t.split('.').collect();
    if parts.len() == 4 {
        let last = parts[3];
        let port_s = DEFAULT_PORT.to_string();
        // "192.168.4.20320": the fan's port typed in place of the last number; the fan's own
        // network always gives it .1
        if last == port_s {
            return (
                format!("{}.{}.{}.1", parts[0], parts[1], parts[2]),
                DEFAULT_PORT,
            );
        }
        if last.len() > 3 && last.ends_with(&port_s) && last.len() > port_s.len() {
            let octet = &last[..last.len() - port_s.len()];
            if octet.parse::<u8>().is_ok() {
                return (
                    format!("{}.{}.{}.{}", parts[0], parts[1], parts[2], octet),
                    DEFAULT_PORT,
                );
            }
        }
    }
    (t.into(), DEFAULT_PORT)
}

pub fn format_address(host: &str, port: u16) -> String {
    format!("{host}:{port}")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn power_frame() {
        let f = command_frame(&[b'a']);
        assert_eq!(&f[..12], CMD_HEAD);
        assert_eq!(&f[12..15], &[0, 0x63, 0x63]);
        assert_eq!(f[15], b'a');
        assert_eq!(&f[16..], TAIL);
    }

    #[test]
    fn reads_back_status() {
        // a fan frame: 'i', one file "A.BIN", status bytes
        let mut payload = vec![b'i', 5];
        payload.extend_from_slice(b"A.BIN");
        payload.extend_from_slice(&[0, 12, 0, 1, 2, 1, 0, 1, 0, 0, 0, 0, 0, 0, 0, 0]);
        let f = command_frame(&payload);
        let mut r = FrameReader::default();
        r.push(b"junk");
        r.push(&f[..10]);
        assert_eq!(r.next(), None);
        r.push(&f[10..]);
        match r.next() {
            Some(Incoming::Status(s)) => {
                assert_eq!(s.files, vec!["A.BIN".to_string()]);
                assert_eq!(s.device_name(), Some("42-F2"));
                assert!(s.sleeping && s.list_loop && s.clock_version && !s.paused);
                assert_eq!(s.brightness_label(), "Low");
            }
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn addresses() {
        assert_eq!(
            parse_address("192.168.4.120320"),
            ("192.168.4.1".into(), 20320)
        );
        assert_eq!(
            parse_address("192.168.4.20320"),
            ("192.168.4.1".into(), 20320)
        );
        assert_eq!(parse_address("192.168.4.1"), ("192.168.4.1".into(), 20320));
        assert_eq!(
            parse_address("192.168.4.1:5000"),
            ("192.168.4.1".into(), 5000)
        );
        assert_eq!(parse_address("10.0.0.7 20320"), ("10.0.0.7".into(), 20320));
    }

    #[test]
    fn clock_buffer() {
        let mut c = ClockBuffer::default();
        let t = Command::TimeSync(3600 * 13 + 5).payload(&mut c);
        assert_eq!(t[0], b'b');
        assert_eq!(u32::from_le_bytes([t[1], t[2], t[3], 0]), 46805);
        assert_eq!(t[4], 0);
        let o = Command::Clock { which: 4, value: 2 }.payload(&mut c);
        assert_eq!((o[1], o[2], o[3], o[4]), (2, t[2], t[3], 4));
    }
}
