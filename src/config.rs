use std::{env, fs, path::PathBuf};
use crate::theme::ThemeMode;

#[derive(Clone, Copy, PartialEq)]
pub enum DeleteMode {
    Trash,
    Permanent,
}

pub struct Config {
    pub delete_mode: DeleteMode,
    pub language: Option<String>,
    pub opacity: u8,
    pub theme: ThemeMode,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            delete_mode: DeleteMode::Trash,
            language: None,
            opacity: 100,
            theme: ThemeMode::System,
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
            "theme" => {
                cfg.theme = ThemeMode::parse(v);
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
    let theme_str = cfg.theme.as_str();
    if fs::write(
        &tmp,
        format!(
            "delete_mode = {mode}\nlanguage = {lang}\nopacity = {}\ntheme = {theme_str}\n",
            cfg.opacity
        ),
    )
    .is_ok()
    {
        let _ = fs::rename(&tmp, &path);
    }
}
