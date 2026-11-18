// HoloFan Studio: control and program 3D hologram fans, on Windows and macOS.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod app;
mod binfmt;
mod device;
mod library;
mod models;
mod pages;
mod pages_lib;
mod paint;
mod protocol;
mod settings;
mod theme;
#[allow(dead_code)]
mod widgets;

use std::path::PathBuf;

use eframe::egui;

fn main() -> eframe::Result {
    let args: Vec<String> = std::env::args().collect();
    let screenshot: Option<PathBuf> = args
        .iter()
        .position(|a| a == "--screenshot")
        .and_then(|i| args.get(i + 1))
        .map(PathBuf::from);

    // `--write-icon <file.png> [size]` saves the app icon (used to build the macOS .app/.dmg icon).
    if let Some(i) = args.iter().position(|a| a == "--write-icon") {
        let path = args
            .get(i + 1)
            .map(PathBuf::from)
            .unwrap_or_else(|| PathBuf::from("icon.png"));
        let size: u32 = args.get(i + 2).and_then(|v| v.parse().ok()).unwrap_or(1024);
        let ok = image::save_buffer(&path, &icon_rgba(size), size, size, image::ColorType::Rgba8)
            .is_ok();
        std::process::exit(if ok { 0 } else { 1 });
    }

    let mut viewport = egui::ViewportBuilder::default()
        .with_title("HoloFan Studio")
        .with_app_id("holofan_studio")
        .with_inner_size([1200.0, 800.0])
        .with_min_inner_size([1040.0, 640.0])
        .with_drag_and_drop(true)
        .with_icon(app_icon());

    // macOS: blend the title bar into the window for a modern look (traffic lights stay).
    #[cfg(target_os = "macos")]
    {
        viewport = viewport
            .with_fullsize_content_view(true)
            .with_title_shown(false)
            .with_titlebar_shown(false);
    }

    if screenshot.is_some() {
        // Never steal focus from the person while a check takes a picture.
        viewport = viewport.with_active(false);
        #[cfg(target_os = "windows")]
        {
            viewport = viewport.with_window_level(egui::WindowLevel::AlwaysOnBottom);
        }
    }

    #[allow(unused_mut)]
    let mut options = eframe::NativeOptions {
        viewport,
        persist_window: screenshot.is_none(),
        ..Default::default()
    };

    #[cfg(target_os = "macos")]
    if screenshot.is_some() {
        options.event_loop_builder = Some(Box::new(|builder| {
            use winit::platform::macos::EventLoopBuilderExtMacOS;
            builder.with_activate_ignoring_other_apps(false);
        }));
    }

    eframe::run_native(
        "HoloFan Studio",
        options,
        Box::new(move |cc| Ok(Box::new(app::App::new(cc, screenshot)))),
    )
}

fn app_icon() -> egui::IconData {
    let size = 64u32;
    egui::IconData {
        rgba: icon_rgba(size),
        width: size,
        height: size,
    }
}

/// The app icon at any size: a rounded square with an indigo-to-violet gradient, a white ring
/// (the fan's rim) and a white LED bar through the hub.
pub fn icon_rgba(size: u32) -> Vec<u8> {
    let n = size as usize;
    let s = size as f32;
    let mut rgba = vec![0u8; n * n * 4];
    let corner = s * 0.22;
    let inset = s * 0.06; // macOS-style margin around the tile
    let (c, ring_r, ring_w) = (s / 2.0, s * 0.30, s * 0.045);
    let bar_w = s * 0.07;
    let (ax, ay) = (0.80f32, -0.60f32); // bar direction (normalised)
    for y in 0..n {
        for x in 0..n {
            let fx = x as f32 + 0.5;
            let fy = y as f32 + 0.5;
            // rounded-rect mask
            let lo = inset + corner;
            let hi = s - inset - corner;
            let cx = fx.clamp(lo, hi);
            let cy = fy.clamp(lo, hi);
            let d = ((fx - cx).powi(2) + (fy - cy).powi(2)).sqrt();
            let alpha = (corner - d + 0.5).clamp(0.0, 1.0);
            let t = (fx + fy) / (2.0 * s);
            let mut col = [99.0 + 69.0 * t, 102.0 - 17.0 * t, 241.0 + 6.0 * t];
            // white ring and bar, anti-aliased
            let (dx, dy) = (fx - c, fy - c);
            let dist = (dx * dx + dy * dy).sqrt();
            let ring = (ring_w / 2.0 - (dist - ring_r).abs() + 0.5).clamp(0.0, 1.0);
            let along = dx * ax + dy * ay;
            let across = (dx * -ay + dy * ax).abs();
            // two short blades with rounded ends that stop well inside the rim
            let blade_len = ring_r * 0.62;
            let along_c = along.abs().min(blade_len);
            let blade_d = ((along.abs() - along_c).powi(2) + across.powi(2)).sqrt();
            let bar = (bar_w / 2.0 - blade_d + 0.5).clamp(0.0, 1.0);
            let hub = (s * 0.075 - dist + 0.5).clamp(0.0, 1.0);
            // faint inner glow ring (the "hologram")
            let glow = (s * 0.012 - (dist - ring_r * 0.78).abs() + 0.5).clamp(0.0, 1.0) * 0.55;
            let white = ring.max(bar).max(hub).max(glow);
            for ch in &mut col {
                *ch += (255.0 - *ch) * white;
            }
            let i = (y * n + x) * 4;
            rgba[i] = col[0] as u8;
            rgba[i + 1] = col[1] as u8;
            rgba[i + 2] = col[2] as u8;
            rgba[i + 3] = (alpha * 255.0) as u8;
        }
    }
    rgba
}
