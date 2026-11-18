// The hologram fan models the app can be set up for (code and blade diameter).

pub struct Model {
    pub code: &'static str,
    pub inches: f32,
}

pub const MODELS: [Model; 20] = [
    Model {
        code: "10-ND",
        inches: 4.0,
    },
    Model {
        code: "10-SD",
        inches: 4.0,
    },
    Model {
        code: "14-ND",
        inches: 5.6,
    },
    Model {
        code: "14-SD",
        inches: 5.6,
    },
    Model {
        code: "18-F2",
        inches: 7.1,
    },
    Model {
        code: "28-F2",
        inches: 11.0,
    },
    Model {
        code: "30-F2",
        inches: 11.8,
    },
    Model {
        code: "32-F2",
        inches: 12.6,
    },
    Model {
        code: "42-F2",
        inches: 16.5,
    },
    Model {
        code: "45-F4",
        inches: 17.7,
    },
    Model {
        code: "52-F4",
        inches: 20.5,
    },
    Model {
        code: "56-F4",
        inches: 22.0,
    },
    Model {
        code: "65-F4",
        inches: 25.6,
    },
    Model {
        code: "65-HD",
        inches: 25.6,
    },
    Model {
        code: "80-F4",
        inches: 31.5,
    },
    Model {
        code: "60-F6",
        inches: 23.6,
    },
    Model {
        code: "70-F6",
        inches: 27.1,
    },
    Model {
        code: "85-F6",
        inches: 33.5,
    },
    Model {
        code: "100-F6",
        inches: 39.4,
    },
    Model {
        code: "115-F6",
        inches: 45.3,
    },
];

impl Model {
    pub fn size_label(&self) -> String {
        if self.inches.fract() == 0.0 {
            format!("{:.0}\"", self.inches)
        } else {
            format!("{:.1}\"", self.inches)
        }
    }
    pub fn cm(&self) -> f32 {
        self.inches * 2.54
    }
    /// Number of blades: the letter code's digit (F2, F4, F6); ND/SD/HD fans are treated as 2.
    pub fn blades(&self) -> usize {
        self.code
            .rsplit('-')
            .next()
            .and_then(|s| s.trim_start_matches('F').parse().ok())
            .unwrap_or(2)
    }
    /// Square video size used when preparing videos for this fan (an estimate; the real
    /// panel resolution of each model isn't known).
    pub fn video_size(&self) -> u32 {
        match self.inches {
            i if i < 6.0 => 480,
            i if i < 13.0 => 720,
            i if i < 26.0 => 1024,
            _ => 1280,
        }
    }
}

pub fn find(code: &str) -> Option<usize> {
    MODELS.iter().position(|m| m.code == code)
}
