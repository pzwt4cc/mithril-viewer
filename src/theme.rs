use crate::draw::Rgb;
use std::sync::RwLock;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum ThemeMode {
    System,
    Neutral,
    Dark,
    Light,
}

impl ThemeMode {
    pub fn as_str(&self) -> &'static str {
        match self {
            ThemeMode::System => "system",
            ThemeMode::Neutral => "neutral",
            ThemeMode::Dark => "dark",
            ThemeMode::Light => "light",
        }
    }

    pub fn parse(s: &str) -> Self {
        match s.trim().to_lowercase().as_str() {
            "neutral" => ThemeMode::Neutral,
            "dark" => ThemeMode::Dark,
            "light" => ThemeMode::Light,
            _ => ThemeMode::System,
        }
    }
}

#[derive(Clone, Copy, PartialEq, Debug)]
pub struct Theme {
    pub mode: ThemeMode,
    pub is_light: bool,
    pub surf_top: Rgb,
    pub surf_bot: Rgb,
    pub edge_top: Rgb,
    pub edge_bot: Rgb,
    pub border: Rgb,
    pub hover: Rgb,
    pub active: Rgb,
    pub text: Rgb,
    pub accent: Rgb,
    pub dim: Rgb,
    pub danger: Rgb,
    pub bg: Rgb,
    pub btn_bg: Rgb,
    pub btn_bg_hover: Rgb,
    pub on_accent: Rgb,
}

impl Theme {
    pub const NEUTRAL: Self = Self {
        mode: ThemeMode::Neutral,
        is_light: false,
        surf_top: [0x3b, 0x3e, 0x44],
        surf_bot: [0x36, 0x39, 0x3f],
        edge_top: [0x4b, 0x4f, 0x57],
        edge_bot: [0x46, 0x4a, 0x51],
        border: [0x4b, 0x4f, 0x57],
        hover: [0x4a, 0x4e, 0x57],
        active: [0x56, 0x5e, 0x73],
        text: [0xee, 0xef, 0xf2],
        accent: [0xff, 0xff, 0xff],
        dim: [0xa3, 0xa8, 0xb1],
        danger: [0xd9, 0x4f, 0x4f],
        bg: [0x28, 0x2a, 0x2f],
        btn_bg: [0x45, 0x49, 0x51],
        btn_bg_hover: [0x55, 0x5a, 0x64],
        on_accent: [0x1c, 0x1f, 0x26],
    };

    pub const DARK: Self = Self {
        mode: ThemeMode::Dark,
        is_light: false,
        surf_top: [0x23, 0x25, 0x2b],
        surf_bot: [0x1d, 0x1e, 0x23],
        edge_top: [0x3a, 0x3d, 0x47],
        edge_bot: [0x29, 0x2b, 0x32],
        border: [0x34, 0x37, 0x40],
        hover: [0x2f, 0x32, 0x3a],
        active: [0x3c, 0x45, 0x56],
        text: [0xee, 0xf0, 0xf4],
        accent: [0xff, 0xff, 0xff],
        dim: [0x90, 0x96, 0xa2],
        danger: [0xe5, 0x55, 0x55],
        bg: [0x14, 0x15, 0x18],
        btn_bg: [0x2b, 0x2d, 0x35],
        btn_bg_hover: [0x37, 0x3a, 0x44],
        on_accent: [0x18, 0x1a, 0x1f],
    };

    pub const LIGHT: Self = Self {
        mode: ThemeMode::Light,
        is_light: true,
        surf_top: [0xff, 0xff, 0xff],
        surf_bot: [0xf6, 0xf7, 0xf9],
        edge_top: [0xe2, 0xe5, 0xeb],
        edge_bot: [0xd7, 0xdb, 0xe2],
        border: [0xd0, 0xd4, 0xdc],
        hover: [0xec, 0xef, 0xf4],
        active: [0xde, 0xe3, 0xeb],
        text: [0x1c, 0x1f, 0x24],
        accent: [0x20, 0x23, 0x29],
        dim: [0x6e, 0x74, 0x80],
        danger: [0xd6, 0x38, 0x38],
        bg: [0xec, 0xef, 0xf4],
        btn_bg: [0xe8, 0xeb, 0xf0],
        btn_bg_hover: [0xdc, 0xe0, 0xe6],
        on_accent: [0xff, 0xff, 0xff],
    };
}

fn detect_system_dark() -> Option<bool> {
    if let Ok(out) = std::process::Command::new("busctl")
        .args([
            "--user",
            "call",
            "org.freedesktop.portal.Desktop",
            "/org/freedesktop/portal/desktop",
            "org.freedesktop.portal.Settings",
            "Read",
            "ss",
            "org.freedesktop.appearance",
            "color-scheme",
        ])
        .output()
        && out.status.success() {
            let s = String::from_utf8_lossy(&out.stdout);
            if s.contains("u 1") {
                return Some(true);
            } else if s.contains("u 2") {
                return Some(false);
            }
        }

    if let Ok(out) = std::process::Command::new("gsettings")
        .args(["get", "org.gnome.desktop.interface", "color-scheme"])
        .output()
        && out.status.success() {
            let s = String::from_utf8_lossy(&out.stdout).to_lowercase();
            if s.contains("dark") {
                return Some(true);
            } else if s.contains("light") || s.contains("default") {
                return Some(false);
            }
        }

    if let Ok(colorfgbg) = std::env::var("COLORFGBG") {
        let parts: Vec<&str> = colorfgbg.split(';').collect();
        if let Some(bg) = parts.last()
            && let Ok(val) = bg.parse::<u32>() {
                if val < 8 {
                    return Some(true);
                } else {
                    return Some(false);
                }
            }
    }

    None
}

pub fn resolve(mode: ThemeMode) -> Theme {
    let mut t = match mode {
        ThemeMode::Neutral => Theme::NEUTRAL,
        ThemeMode::Dark => Theme::DARK,
        ThemeMode::Light => Theme::LIGHT,
        ThemeMode::System => match detect_system_dark() {
            Some(true) | None => Theme::DARK,
            Some(false) => Theme::LIGHT,
        },
    };
    t.mode = mode;
    t
}

static CURRENT: RwLock<Theme> = RwLock::new(Theme::NEUTRAL);

pub fn init(mode: ThemeMode) {
    let t = resolve(mode);
    *CURRENT.write().unwrap() = t;
}

pub fn set(mode: ThemeMode) -> Theme {
    let t = resolve(mode);
    *CURRENT.write().unwrap() = t;
    t
}

pub fn current() -> Theme {
    *CURRENT.read().unwrap()
}

pub fn bg_texture() -> [u32; 1] {
    let t = current();
    [0xff00_0000 | (t.bg[0] as u32) << 16 | (t.bg[1] as u32) << 8 | (t.bg[2] as u32)]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_theme_switch() {
        set(ThemeMode::Neutral);
        assert_eq!(current().mode, ThemeMode::Neutral);
        assert!(!current().is_light);

        set(ThemeMode::Light);
        assert_eq!(current().mode, ThemeMode::Light);
        assert!(current().is_light);

        set(ThemeMode::Dark);
        assert_eq!(current().mode, ThemeMode::Dark);
        assert!(!current().is_light);

        set(ThemeMode::System);
        assert_eq!(current().mode, ThemeMode::System);
    }
}
