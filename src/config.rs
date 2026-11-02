use std::{
    env, fs,
    path::PathBuf,
    sync::atomic::{AtomicU32, Ordering},
};

#[derive(Clone, Copy, PartialEq)]
pub enum DeleteMode {
    Trash,
    Permanent,
}

#[derive(Clone, Copy, PartialEq)]
pub enum Accent {
    Blue,
    Green,
    Purple,
    Orange,
    Gray,
}

impl Accent {
    pub const ALL: [Accent; 5] = [
        Accent::Blue,
        Accent::Green,
        Accent::Purple,
        Accent::Orange,
        Accent::Gray,
    ];

    pub fn rgb(self) -> [u8; 3] {
        match self {
            Accent::Blue => [0x8a, 0xa8, 0xff],
            Accent::Green => [0x9c, 0xc4, 0x8a],
            Accent::Purple => [0xb8, 0x9d, 0xf2],
            Accent::Orange => [0xee, 0xa8, 0x70],
            Accent::Gray => [0xc3, 0xc7, 0xcf],
        }
    }

    pub fn key(self) -> &'static str {
        match self {
            Accent::Blue => "blue",
            Accent::Green => "green",
            Accent::Purple => "purple",
            Accent::Orange => "orange",
            Accent::Gray => "gray",
        }
    }

    fn parse(s: &str) -> Option<Accent> {
        Accent::ALL
            .into_iter()
            .find(|a| a.key().eq_ignore_ascii_case(s))
    }
}

static ACCENT: AtomicU32 = AtomicU32::new(0x8aa8ff);

pub fn set_accent(a: Accent) {
    let c = a.rgb();
    let v = (c[0] as u32) << 16 | (c[1] as u32) << 8 | c[2] as u32;
    ACCENT.store(v, Ordering::Relaxed);
}

pub fn accent() -> [u8; 3] {
    let v = ACCENT.load(Ordering::Relaxed);
    [(v >> 16) as u8, (v >> 8) as u8, v as u8]
}

pub struct Config {
    pub delete_mode: DeleteMode,
    pub language: Option<String>,
    pub opacity: u8,
    pub accent: Accent,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            delete_mode: DeleteMode::Trash,
            language: None,
            opacity: 100,
            accent: Accent::Blue,
        }
    }
}

pub fn parse_pairs(text: &str) -> Vec<(&str, &str)> {
    text.lines()
        .filter_map(|line| {
            let line = line.trim();
            if line.is_empty() || line.starts_with('#') {
                return None;
            }
            let (k, v) = line.split_once('=')?;
            Some((k.trim(), v.trim()))
        })
        .collect()
}

fn config_path() -> Option<PathBuf> {
    let base = env::var_os("XDG_CONFIG_HOME")
        .filter(|v| !v.is_empty())
        .map(PathBuf::from)
        .or_else(|| env::var_os("HOME").map(|h| PathBuf::from(h).join(".config")))?;
    Some(base.join("mithril").join("config.conf"))
}

pub fn load() -> Config {
    let mut cfg = Config::default();
    let Some(text) = config_path().and_then(|p| fs::read_to_string(p).ok()) else {
        return cfg;
    };
    for (k, v) in parse_pairs(&text) {
        match k {
            "delete_mode" => {
                cfg.delete_mode = if v.eq_ignore_ascii_case("permanent") {
                    DeleteMode::Permanent
                } else {
                    DeleteMode::Trash
                };
            }
            "language" if !v.is_empty() && !v.eq_ignore_ascii_case("auto") => {
                cfg.language = Some(v.to_lowercase());
            }
            "opacity" => {
                if let Ok(p) = v.trim_end_matches('%').parse::<u8>() {
                    cfg.opacity = p.clamp(40, 100);
                }
            }
            "accent" => {
                if let Some(a) = Accent::parse(v) {
                    cfg.accent = a;
                }
            }
            _ => {}
        }
    }
    cfg
}

pub fn save(cfg: &Config) {
    let Some(path) = config_path() else { return };
    if let Some(dir) = path.parent() {
        let _ = fs::create_dir_all(dir);
    }
    let mode = match cfg.delete_mode {
        DeleteMode::Trash => "trash",
        DeleteMode::Permanent => "permanent",
    };
    let lang = cfg.language.as_deref().unwrap_or("auto");
    let tmp = path.with_extension("conf.tmp");
    if fs::write(
        &tmp,
        format!(
            "delete_mode = {mode}\nlanguage = {lang}\nopacity = {}\naccent = {}\n",
            cfg.opacity,
            cfg.accent.key()
        ),
    )
    .is_ok()
    {
        let _ = fs::rename(&tmp, &path);
    }
}
