// The fan's .BIN video format, as the original's "Decode video" writes it (MECHANICS.md,
// functions 0x402b60 and 0x405dc0). Each video frame is unwrapped around the fan's hub into
// `rows` angles × `leds` LEDs of RGB, shaped with a colour curve, thinned near the hub, then
// written as bit planes. Pure functions: no files or threads here.

use std::f32::consts::TAU;

/// One fan type the converter knows.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Spec {
    /// The fan's own type number (status byte 1).
    pub type_id: u8,
    pub code: &'static str,
    /// LEDs along one blade (pixels from hub to rim).
    pub leds: usize,
    /// Angles per turn (rows per frame).
    pub rows: usize,
    /// Colour bits kept per channel: 8, or 6 for the small fans.
    pub bits: usize,
    /// Frames per second the fan plays.
    pub fps: f32,
    /// The 42-F2's extra bit interleave (0x407a77).
    pub interleave: bool,
    /// Checked against .BIN files made by the original.
    pub verified: bool,
}

impl Spec {
    pub fn groups(&self) -> usize {
        self.leds / 8
    }
    /// Bytes for one angle in the file.
    pub fn stride(&self) -> usize {
        self.leds * 3 * self.bits / 8
    }
    pub fn frame_bytes(&self) -> usize {
        self.rows * self.stride()
    }
}

/// Fan types whose layout needs no model-specific extra step besides the 42-F2's, taken
/// from 0x402b60. Other types use packing steps not rebuilt yet.
pub const SPECS: [Spec; 7] = [
    Spec {
        type_id: 12,
        code: "42-F2",
        leds: 112,
        rows: 512,
        bits: 6,
        fps: 12.0,
        interleave: true,
        verified: true,
    },
    Spec {
        type_id: 0,
        code: "45-F4",
        leds: 128,
        rows: 1024,
        bits: 8,
        fps: 12.0,
        interleave: false,
        verified: false,
    },
    Spec {
        type_id: 1,
        code: "65-F4",
        leds: 144,
        rows: 1020,
        bits: 8,
        fps: 12.0,
        interleave: false,
        verified: false,
    },
    Spec {
        type_id: 2,
        code: "85-F4",
        leds: 192,
        rows: 700,
        bits: 8,
        fps: 12.0,
        interleave: false,
        verified: false,
    },
    Spec {
        type_id: 5,
        code: "56-F4",
        leds: 144,
        rows: 1020,
        bits: 8,
        fps: 12.0,
        interleave: false,
        verified: false,
    },
    Spec {
        type_id: 7,
        code: "52-F4",
        leds: 136,
        rows: 1024,
        bits: 8,
        fps: 12.0,
        interleave: false,
        verified: false,
    },
    Spec {
        type_id: 8,
        code: "30-F2",
        leds: 128,
        rows: 1024,
        bits: 8,
        fps: 12.0,
        interleave: false,
        verified: false,
    },
];

pub fn spec_for_code(code: &str) -> Option<Spec> {
    SPECS.iter().copied().find(|s| s.code == code)
}

/// The fan type whose frame size divides this file size (the preferred one first).
pub fn spec_for_size(len: u64, preferred: Option<&str>) -> Option<Spec> {
    // some files from the original carry 20 more bytes at the end (an upload marker)
    let fits = |s: &Spec| len >= s.frame_bytes() as u64 && len % s.frame_bytes() as u64 <= 64;
    if let Some(p) = preferred.and_then(spec_for_code) {
        if fits(&p) {
            return Some(p);
        }
    }
    SPECS.iter().copied().find(fits)
}

/// The three colour curves of the original's "decoding mode" dialog (0x405f80).
#[derive(Clone, Copy, PartialEq, Debug)]
pub enum ColorMode {
    /// x·(x+1)² / 65536: deep colours, darker mid-tones.
    Colorful,
    /// x·(x+1) / 256.
    Common,
    /// x·(x+1) / 128, full white above 180.
    Highlight,
}

impl ColorMode {
    pub const ALL: [ColorMode; 3] = [ColorMode::Colorful, ColorMode::Common, ColorMode::Highlight];
    pub fn label(self) -> &'static str {
        match self {
            ColorMode::Colorful => "Colorful",
            ColorMode::Common => "Common",
            ColorMode::Highlight => "Highlight",
        }
    }
    pub fn key(self) -> &'static str {
        match self {
            ColorMode::Colorful => "colorful",
            ColorMode::Common => "common",
            ColorMode::Highlight => "highlight",
        }
    }
    pub fn from_key(s: &str) -> Self {
        Self::ALL
            .into_iter()
            .find(|m| m.key() == s)
            .unwrap_or(ColorMode::Colorful)
    }
    pub fn table(self) -> [u8; 256] {
        let mut t = [0u8; 256];
        for (x, v) in t.iter_mut().enumerate() {
            let x = x as u32;
            *v = match self {
                ColorMode::Colorful => ((x * (x + 1) * (x + 1)) >> 16).min(255) as u8,
                ColorMode::Common => ((x * (x + 1)) >> 8).min(255) as u8,
                ColorMode::Highlight => {
                    if x > 180 {
                        255
                    } else {
                        ((x * (x + 1)) >> 7).min(255) as u8
                    }
                }
            };
        }
        t
    }
}

/// Where the circle sits in the source picture: centre and radius in pixels.
#[derive(Clone, Copy, Debug)]
pub struct Circle {
    pub cx: f32,
    pub cy: f32,
    pub r: f32,
}

impl Circle {
    /// The original's default: the largest circle in the middle of the picture, scaled by
    /// `zoom` (1.0 = touches the shorter edges; smaller zooms in).
    pub fn centred(w: usize, h: usize, zoom: f32) -> Self {
        Circle {
            cx: w as f32 / 2.0,
            cy: h as f32 / 2.0,
            r: w.min(h) as f32 / 2.0 * zoom.max(0.05),
        }
    }
}

/// Unwrap an RGB picture around the circle: rows = angles, columns = distance from the hub
/// (like OpenCV's linearPolar, which the original uses; bilinear sampling, black outside).
pub fn unwrap(rgb: &[u8], w: usize, h: usize, circle: Circle, spec: &Spec) -> Vec<u8> {
    let mut out = vec![0u8; spec.rows * spec.leds * 3];
    for a in 0..spec.rows {
        let ang = a as f32 * TAU / spec.rows as f32;
        let (s, c) = ang.sin_cos();
        for l in 0..spec.leds {
            let rho = l as f32 * circle.r / spec.leds as f32;
            let x = circle.cx + rho * c;
            let y = circle.cy + rho * s;
            if x < 0.0 || y < 0.0 || x > (w - 1) as f32 || y > (h - 1) as f32 {
                continue;
            }
            let (x0, y0) = (x.floor() as usize, y.floor() as usize);
            let (x1, y1) = ((x0 + 1).min(w - 1), (y0 + 1).min(h - 1));
            let (fx, fy) = (x - x0 as f32, y - y0 as f32);
            let o = (a * spec.leds + l) * 3;
            for ch in 0..3 {
                let p = |xx: usize, yy: usize| rgb[(yy * w + xx) * 3 + ch] as f32;
                let top = p(x0, y0) * (1.0 - fx) + p(x1, y0) * fx;
                let bot = p(x0, y1) * (1.0 - fx) + p(x1, y1) * fx;
                out[o + ch] = (top * (1.0 - fy) + bot * fy).round().clamp(0.0, 255.0) as u8;
            }
        }
    }
    out
}

/// Turn off some of the five LEDs nearest the hub on most angles, where the blades pass
/// close together (0x4067a1): LED 0 stays on every 24th angle, LED 1 every 6th, LED 2 every
/// 3rd, LED 3 every 2nd, LED 4 is off on every 12th angle.
pub fn thin_hub(polar: &mut [u8], spec: &Spec) {
    let (mut a, mut b, mut c, mut d, mut e) = (0u8, 0u8, 0u8, 0u8, 0u8);
    for row in 0..spec.rows {
        let o = row * spec.leds * 3;
        let mut off = |led: usize| polar[o + led * 3..o + led * 3 + 3].fill(0);
        if a != 0 {
            off(0);
        }
        if b != 0 {
            off(1);
        }
        if c != 0 {
            off(2);
        }
        if d == 0 {
            off(3);
        }
        if e == 0 {
            off(4);
        }
        e = if e == 11 { 0 } else { e + 1 };
        a = if a == 23 { 0 } else { a + 1 };
        b = if b == 5 { 0 } else { b + 1 };
        c = if c == 2 { 0 } else { c + 1 };
        d ^= 1;
    }
}

/// One finished frame from an unwrapped picture (curve applied here).
pub fn encode_frame(polar: &[u8], spec: &Spec, curve: &[u8; 256]) -> Vec<u8> {
    let mut px: Vec<u8> = polar.iter().map(|&v| curve[v as usize]).collect();
    thin_hub(&mut px, spec);
    pack(&px, spec)
}

/// Bit planes: 8 colour bytes become one byte per bit; angles are stored last-to-first and
/// each angle's 8-byte chunks in reverse order (0x407390).
pub fn pack(px: &[u8], spec: &Spec) -> Vec<u8> {
    let g = spec.groups();
    let chunks = 3 * g;
    let fb = spec.frame_bytes();
    let stride = spec.stride();
    let mut out = vec![0u8; fb];
    for row in 0..spec.rows {
        let base = fb - (row + 1) * stride;
        let src = &px[row * spec.leds * 3..(row + 1) * spec.leds * 3];
        for k in 0..chunks {
            let x = &src[k * 8..k * 8 + 8];
            let pos = base + chunks - 1 - k;
            for bit in (8 - spec.bits)..8 {
                let mut v = 0u8;
                for (j, &xj) in x.iter().enumerate() {
                    v |= ((xj >> bit) & 1) << j;
                }
                out[pos + (7 - bit) * chunks] = v;
            }
        }
    }
    if spec.interleave {
        interleave(&mut out, chunks);
    }
    out
}

/// The 42-F2 step: in every plane chunk, from its 3rd byte on, groups of four bytes have
/// the bits of bytes 0/2 and 1/3 woven together (0x407a77).
fn interleave(buf: &mut [u8], chunk: usize) {
    for start in (0..buf.len()).step_by(chunk) {
        let mut k = start + 2;
        while k + 4 <= start + chunk {
            let [b0, b1, b2, b3] = [buf[k], buf[k + 1], buf[k + 2], buf[k + 3]];
            let weave = |hi: u8, lo: u8, top: bool| -> u8 {
                let mut v = 0u8;
                for i in 0..4 {
                    let s = if top { 7 - i } else { 3 - i };
                    v |= ((hi >> s) & 1) << (7 - 2 * i);
                    v |= ((lo >> s) & 1) << (6 - 2 * i);
                }
                v
            };
            buf[k] = weave(b0, b2, true);
            buf[k + 1] = weave(b0, b2, false);
            buf[k + 2] = weave(b1, b3, true);
            buf[k + 3] = weave(b1, b3, false);
            k += 4;
        }
    }
}

fn unweave(buf: &mut [u8], chunk: usize) {
    for start in (0..buf.len()).step_by(chunk) {
        let mut k = start + 2;
        while k + 4 <= start + chunk {
            let [o0, o1, o2, o3] = [buf[k], buf[k + 1], buf[k + 2], buf[k + 3]];
            let split = |a: u8, b: u8| -> (u8, u8) {
                let (mut hi, mut lo) = (0u8, 0u8);
                for i in 0..4 {
                    hi |= ((a >> (7 - 2 * i)) & 1) << (7 - i);
                    lo |= ((a >> (6 - 2 * i)) & 1) << (7 - i);
                    hi |= ((b >> (7 - 2 * i)) & 1) << (3 - i);
                    lo |= ((b >> (6 - 2 * i)) & 1) << (3 - i);
                }
                (hi, lo)
            };
            let (b0, b2) = split(o0, o1);
            let (b1, b3) = split(o2, o3);
            buf[k..k + 4].copy_from_slice(&[b0, b1, b2, b3]);
            k += 4;
        }
    }
}

/// Back from a frame to the unwrapped RGB picture (for previews of .BIN files).
pub fn decode_frame(frame: &[u8], spec: &Spec) -> Vec<u8> {
    let g = spec.groups();
    let chunks = 3 * g;
    let fb = spec.frame_bytes();
    let stride = spec.stride();
    let mut buf = frame[..fb.min(frame.len())].to_vec();
    buf.resize(fb, 0);
    if spec.interleave {
        unweave(&mut buf, chunks);
    }
    let mut px = vec![0u8; spec.rows * spec.leds * 3];
    for row in 0..spec.rows {
        let base = fb - (row + 1) * stride;
        for k in 0..chunks {
            let pos = base + chunks - 1 - k;
            for j in 0..8 {
                let mut v = 0u8;
                for bit in (8 - spec.bits)..8 {
                    v |= ((buf[pos + (7 - bit) * chunks] >> j) & 1) << bit;
                }
                px[row * spec.leds * 3 + k * 8 + j] = v;
            }
        }
    }
    px
}

/// Draw an unwrapped picture back as a round image (RGBA, `size`×`size`, transparent outside).
pub fn render_disc(px: &[u8], spec: &Spec, size: usize) -> Vec<u8> {
    let mut out = vec![0u8; size * size * 4];
    let half = size as f32 / 2.0;
    for y in 0..size {
        for x in 0..size {
            let dx = x as f32 + 0.5 - half;
            let dy = y as f32 + 0.5 - half;
            let rho = (dx * dx + dy * dy).sqrt() / half;
            let o = (y * size + x) * 4;
            if rho >= 1.0 {
                continue;
            }
            let ang = dy.atan2(dx).rem_euclid(TAU);
            let row = ((ang / TAU * spec.rows as f32) as usize).min(spec.rows - 1);
            let led = ((rho * spec.leds as f32) as usize).min(spec.leds - 1);
            let i = (row * spec.leds + led) * 3;
            out[o..o + 3].copy_from_slice(&px[i..i + 3]);
            out[o + 3] = 255;
        }
    }
    out
}

/// Which source frames the fan keeps when the video runs at `src_fps` (0x40651a): a frame
/// is skipped while the counter is above 1.
pub struct FramePicker {
    acc: f32,
    step: f32,
}

impl FramePicker {
    pub fn new(src_fps: f32, fan_fps: f32) -> Self {
        let step = if src_fps > 0.0 {
            (src_fps - fan_fps) / fan_fps
        } else {
            0.0
        };
        FramePicker { acc: 0.0, step }
    }
    pub fn keep(&mut self) -> bool {
        if self.acc > 1.0 {
            self.acc -= 1.0;
            false
        } else {
            self.acc += self.step;
            true
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sizes_match_the_original() {
        let s = spec_for_code("42-F2").unwrap();
        assert_eq!(s.frame_bytes(), 0x1f800);
        assert_eq!(spec_for_code("45-F4").unwrap().frame_bytes(), 0x60000);
        assert_eq!(spec_for_code("65-F4").unwrap().frame_bytes(), 0x6b940);
        assert_eq!(spec_for_code("85-F4").unwrap().frame_bytes(), 0x62700);
        assert_eq!(spec_for_code("52-F4").unwrap().frame_bytes(), 0x66000);
    }

    #[test]
    fn round_trip() {
        for s in SPECS {
            let mut px: Vec<u8> = (0..s.rows * s.leds * 3)
                .map(|i| (i * 37 % 251) as u8)
                .collect();
            let f = pack(&px, &s);
            assert_eq!(f.len(), s.frame_bytes());
            let back = decode_frame(&f, &s);
            let mask = !((1u8 << (8 - s.bits)) - 1);
            for v in px.iter_mut() {
                *v &= mask;
            }
            assert_eq!(back, px, "{}", s.code);
        }
    }

    #[test]
    fn curves() {
        assert_eq!(ColorMode::Colorful.table()[255], 255);
        assert_eq!(ColorMode::Common.table()[255], 255);
        assert_eq!(ColorMode::Highlight.table()[181], 255);
        assert_eq!(ColorMode::Common.table()[128], 64);
    }

    #[test]
    fn keeps_twelve_of_thirty() {
        let mut p = FramePicker::new(30.0, 12.0);
        let kept = (0..300).filter(|_| p.keep()).count();
        assert!((118..=122).contains(&kept), "{kept}");
    }

    /// A frame of a .BIN made by the original decodes back into a picture, and encoding that
    /// picture again gives the same bytes.
    #[test]
    fn matches_original_file() {
        let Some(home) = std::env::var_os("HOME") else {
            return;
        };
        let path = std::path::Path::new(&home).join("Documents/Holofan/4Tiger.BIN");
        let Ok(data) = std::fs::read(&path) else {
            return;
        };
        let s = spec_for_code("42-F2").unwrap();
        let fb = s.frame_bytes();
        let frame = &data[60 * fb..61 * fb];
        let px = decode_frame(frame, &s);
        assert_eq!(pack(&px, &s), frame);
        assert!(px.iter().filter(|&&v| v > 0).count() > 10_000);
    }
}
