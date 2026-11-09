use std::{
    collections::HashMap,
    sync::{OnceLock, RwLock},
};

use crate::config::parse_pairs;

const BUNDLED: &[(&str, &str)] = &[
    ("en", include_str!("../lang/en.txt")),
    ("ru", include_str!("../lang/ru.txt")),
    ("uk", include_str!("../lang/uk.txt")),
    ("de", include_str!("../lang/de.txt")),
    ("fr", include_str!("../lang/fr.txt")),
    ("es", include_str!("../lang/es.txt")),
];

type Table = HashMap<&'static str, &'static str>;

static ENGLISH: OnceLock<Table> = OnceLock::new();
static CURRENT: RwLock<Option<Table>> = RwLock::new(None);

fn table(code: &str) -> Option<Table> {
    BUNDLED
        .iter()
        .find(|(c, _)| *c == code)
        .map(|(_, text)| parse_pairs(text).into_iter().collect())
}

fn detect() -> Option<String> {
    for var in ["LC_ALL", "LC_MESSAGES", "LANGUAGE", "LANG"] {
        if let Ok(v) = std::env::var(var) {
            let code = v
                .split(['_', '.', '@', ':'])
                .next()
                .unwrap_or("")
                .to_lowercase();
            if !code.is_empty() && code != "c" && code != "posix" {
                return Some(code);
            }
        }
    }
    None
}

pub fn init(preferred: Option<&str>) {
    let english = ENGLISH.get_or_init(|| table("en").unwrap_or_default());
    let code = preferred
        .map(str::to_owned)
        .or_else(detect)
        .unwrap_or_else(|| "en".into());
    let current = table(&code).unwrap_or_else(|| english.clone());
    *CURRENT.write().unwrap() = Some(current);
}

pub fn available() -> Vec<(&'static str, &'static str)> {
    BUNDLED
        .iter()
        .map(|(code, text)| {
            let name = parse_pairs(text)
                .into_iter()
                .find(|(k, _)| *k == "lang.name")
                .map(|(_, v)| v)
                .unwrap_or(code);
            (*code, name)
        })
        .collect()
}

pub fn tr(key: &'static str) -> &'static str {
    if let Some(t) = CURRENT.read().unwrap().as_ref()
        && let Some(v) = t.get(key)
    {
        return v;
    }
    ENGLISH
        .get()
        .and_then(|t| t.get(key))
        .copied()
        .unwrap_or(key)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_all_languages_loaded() {
        assert_eq!(available().len(), 6);
        for (code, _) in available() {
            let t = table(code).expect("failed to load table");
            assert!(t.contains_key("lang.name"));
            assert!(t.contains_key("menu.reset_zoom"));
        }
    }
}
