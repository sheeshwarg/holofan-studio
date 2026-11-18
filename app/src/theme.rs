// Colours, fonts and widget styling for the modern look (light and dark).

use eframe::egui::{
    self, Color32, CornerRadius, FontData, FontDefinitions, FontFamily, FontId, Margin, Shadow,
    Stroke, TextStyle, Theme, Visuals,
};

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum ThemeMode {
    System,
    Light,
    Dark,
}

impl ThemeMode {
    pub fn label(self) -> &'static str {
        match self {
            ThemeMode::System => "System",
            ThemeMode::Light => "Light",
            ThemeMode::Dark => "Dark",
        }
    }
    pub fn key(self) -> &'static str {
        match self {
            ThemeMode::System => "system",
            ThemeMode::Light => "light",
            ThemeMode::Dark => "dark",
        }
    }
    pub fn from_key(s: &str) -> Self {
        match s {
            "light" => ThemeMode::Light,
            "dark" => ThemeMode::Dark,
            _ => ThemeMode::System,
        }
    }
}

pub const ACCENTS: [(&str, Color32); 6] = [
    ("Indigo", Color32::from_rgb(99, 102, 241)),
    ("Blue", Color32::from_rgb(59, 130, 246)),
    ("Teal", Color32::from_rgb(20, 184, 166)),
    ("Green", Color32::from_rgb(34, 197, 94)),
    ("Orange", Color32::from_rgb(249, 115, 22)),
    ("Pink", Color32::from_rgb(236, 72, 153)),
];

/// The colours every page paints with.
#[derive(Clone, Copy)]
pub struct Palette {
    pub dark: bool,
    pub bg: Color32,
    pub sidebar: Color32,
    pub card: Color32,
    pub card_hover: Color32,
    pub border: Color32,
    pub text: Color32,
    pub muted: Color32,
    pub faint: Color32,
    pub accent: Color32,
    pub accent_soft: Color32,
    pub on_accent: Color32,
    pub success: Color32,
    pub warning: Color32,
    pub danger: Color32,
    pub track: Color32,
}

pub fn palette(dark: bool, accent: Color32) -> Palette {
    if dark {
        Palette {
            dark,
            bg: Color32::from_rgb(15, 17, 21),
            sidebar: Color32::from_rgb(20, 23, 28),
            card: Color32::from_rgb(26, 30, 36),
            card_hover: Color32::from_rgb(32, 37, 45),
            border: Color32::from_rgb(38, 43, 51),
            text: Color32::from_rgb(230, 232, 235),
            muted: Color32::from_rgb(139, 147, 161),
            faint: Color32::from_rgb(90, 97, 110),
            accent,
            accent_soft: mix(Color32::from_rgb(26, 30, 36), accent, 0.22),
            on_accent: Color32::WHITE,
            success: Color32::from_rgb(52, 211, 153),
            warning: Color32::from_rgb(251, 191, 36),
            danger: Color32::from_rgb(248, 113, 113),
            track: Color32::from_rgb(40, 45, 54),
        }
    } else {
        Palette {
            dark,
            bg: Color32::from_rgb(245, 246, 248),
            sidebar: Color32::from_rgb(255, 255, 255),
            card: Color32::from_rgb(255, 255, 255),
            card_hover: Color32::from_rgb(249, 250, 251),
            border: Color32::from_rgb(228, 231, 236),
            text: Color32::from_rgb(17, 24, 39),
            muted: Color32::from_rgb(107, 114, 128),
            faint: Color32::from_rgb(156, 163, 175),
            accent,
            accent_soft: mix(Color32::WHITE, accent, 0.14),
            on_accent: Color32::WHITE,
            success: Color32::from_rgb(16, 185, 129),
            warning: Color32::from_rgb(217, 119, 6),
            danger: Color32::from_rgb(220, 38, 38),
            track: Color32::from_rgb(234, 236, 240),
        }
    }
}

pub fn mix(a: Color32, b: Color32, t: f32) -> Color32 {
    let l = |x: u8, y: u8| (x as f32 + (y as f32 - x as f32) * t).round() as u8;
    Color32::from_rgb(l(a.r(), b.r()), l(a.g(), b.g()), l(a.b(), b.b()))
}

fn visuals_for(p: &Palette) -> Visuals {
    let mut v = if p.dark {
        Visuals::dark()
    } else {
        Visuals::light()
    };
    v.panel_fill = p.bg;
    v.window_fill = p.card;
    v.window_stroke = Stroke::new(1.0, p.border);
    v.window_corner_radius = CornerRadius::same(14);
    v.menu_corner_radius = CornerRadius::same(10);
    v.window_shadow = Shadow {
        offset: [0, 12],
        blur: 32,
        spread: 0,
        color: Color32::from_black_alpha(if p.dark { 120 } else { 40 }),
    };
    v.popup_shadow = Shadow {
        offset: [0, 6],
        blur: 16,
        spread: 0,
        color: Color32::from_black_alpha(if p.dark { 100 } else { 30 }),
    };
    v.extreme_bg_color = if p.dark {
        Color32::from_rgb(18, 21, 26)
    } else {
        Color32::from_rgb(248, 249, 251)
    };
    v.faint_bg_color = p.card_hover;
    v.hyperlink_color = p.accent;
    v.selection.bg_fill = p.accent.gamma_multiply(0.45);
    v.selection.stroke = Stroke::new(1.0, p.accent);
    v.striped = false;
    v.slider_trailing_fill = true;
    v.handle_shape = egui::style::HandleShape::Circle;

    let r = CornerRadius::same(8);
    let w = &mut v.widgets;
    w.noninteractive.bg_fill = p.card;
    w.noninteractive.weak_bg_fill = p.card;
    w.noninteractive.bg_stroke = Stroke::new(1.0, p.border);
    w.noninteractive.fg_stroke = Stroke::new(1.0, p.text);
    w.noninteractive.corner_radius = r;

    w.inactive.bg_fill = p.track;
    w.inactive.weak_bg_fill = if p.dark {
        Color32::from_rgb(34, 39, 47)
    } else {
        Color32::from_rgb(241, 243, 246)
    };
    w.inactive.bg_stroke = Stroke::new(1.0, p.border);
    w.inactive.fg_stroke = Stroke::new(1.0, p.text);
    w.inactive.corner_radius = r;

    w.hovered.bg_fill = mix(p.track, p.accent, 0.25);
    w.hovered.weak_bg_fill = if p.dark {
        Color32::from_rgb(42, 48, 58)
    } else {
        Color32::from_rgb(232, 235, 240)
    };
    w.hovered.bg_stroke = Stroke::new(1.0, mix(p.border, p.accent, 0.5));
    w.hovered.fg_stroke = Stroke::new(1.5, p.text);
    w.hovered.corner_radius = r;
    w.hovered.expansion = 0.0;

    w.active.bg_fill = p.accent;
    w.active.weak_bg_fill = mix(p.track, p.accent, 0.35);
    w.active.bg_stroke = Stroke::new(1.0, p.accent);
    w.active.fg_stroke = Stroke::new(1.5, p.text);
    w.active.corner_radius = r;
    w.active.expansion = 0.0;

    w.open = w.hovered;
    v
}

/// Apply both themes' visuals, spacing and text sizes.
pub fn apply(ctx: &egui::Context, mode: ThemeMode, accent: Color32) {
    ctx.set_visuals_of(Theme::Dark, visuals_for(&palette(true, accent)));
    ctx.set_visuals_of(Theme::Light, visuals_for(&palette(false, accent)));
    ctx.set_theme(match mode {
        ThemeMode::System => egui::ThemePreference::System,
        ThemeMode::Light => egui::ThemePreference::Light,
        ThemeMode::Dark => egui::ThemePreference::Dark,
    });
    ctx.all_styles_mut(|style| {
        style.spacing.item_spacing = egui::vec2(10.0, 8.0);
        style.spacing.button_padding = egui::vec2(14.0, 7.0);
        style.spacing.interact_size.y = 32.0;
        style.spacing.window_margin = Margin::same(20);
        style.spacing.menu_margin = Margin::same(8);
        style.spacing.combo_height = 240.0;
        style.text_styles = [
            (
                TextStyle::Heading,
                FontId::new(26.0, FontFamily::Proportional),
            ),
            (TextStyle::Body, FontId::new(14.0, FontFamily::Proportional)),
            (
                TextStyle::Button,
                FontId::new(14.0, FontFamily::Proportional),
            ),
            (
                TextStyle::Small,
                FontId::new(12.0, FontFamily::Proportional),
            ),
            (
                TextStyle::Monospace,
                FontId::new(13.0, FontFamily::Monospace),
            ),
        ]
        .into();
    });
}

pub fn bold() -> FontFamily {
    FontFamily::Name("bold".into())
}

/// Use the operating system's own UI font when it can be found (Segoe UI on Windows,
/// San Francisco on macOS), so the app feels native; fall back to egui's built-in font.
pub fn install_fonts(ctx: &egui::Context) {
    let mut fonts = FontDefinitions::default();

    #[cfg(target_os = "windows")]
    let (regular, semibold): (&[&str], &[&str]) = (
        &["C:\\Windows\\Fonts\\segoeui.ttf"],
        &[
            "C:\\Windows\\Fonts\\seguisb.ttf",
            "C:\\Windows\\Fonts\\segoeuib.ttf",
        ],
    );
    #[cfg(target_os = "macos")]
    let (regular, semibold): (&[&str], &[&str]) = (
        &[
            "/System/Library/Fonts/SFNS.ttf",
            "/System/Library/Fonts/Supplemental/Arial.ttf",
        ],
        &[
            "/System/Library/Fonts/SFNS.ttf",
            "/System/Library/Fonts/Supplemental/Arial Bold.ttf",
        ],
    );
    #[cfg(not(any(target_os = "windows", target_os = "macos")))]
    let (regular, semibold): (&[&str], &[&str]) = (
        &["/usr/share/fonts/truetype/dejavu/DejaVuSans.ttf"],
        &["/usr/share/fonts/truetype/dejavu/DejaVuSans-Bold.ttf"],
    );

    let load = |paths: &[&str]| paths.iter().find_map(|p| std::fs::read(p).ok());

    let regular_name = if let Some(bytes) = load(regular) {
        fonts
            .font_data
            .insert("ui-regular".into(), FontData::from_owned(bytes).into());
        fonts
            .families
            .entry(FontFamily::Proportional)
            .or_default()
            .insert(0, "ui-regular".into());
        Some("ui-regular")
    } else {
        None
    };

    let mut bold_chain: Vec<String> = Vec::new();
    if let Some(bytes) = load(semibold) {
        fonts
            .font_data
            .insert("ui-bold".into(), FontData::from_owned(bytes).into());
        bold_chain.push("ui-bold".into());
    } else if let Some(r) = regular_name {
        bold_chain.push(r.into());
    }
    // fall back to the proportional chain for missing glyphs
    if let Some(prop) = fonts.families.get(&FontFamily::Proportional) {
        bold_chain.extend(prop.iter().cloned());
    }
    fonts.families.insert(bold(), bold_chain);
    ctx.set_fonts(fonts);
}
