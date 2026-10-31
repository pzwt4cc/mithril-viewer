use std::{
    path::{Path, PathBuf},
    time::{SystemTime, UNIX_EPOCH},
};

use image::ImageFormat;

pub struct Labels {
    pub name: &'static str,
    pub folder: &'static str,
    pub resolution: &'static str,
    pub format: &'static str,
    pub size: &'static str,
    pub created: &'static str,
    pub modified: &'static str,
    transparent: &'static str,
    unknown: &'static str,
    units: [&'static str; 4],
    pub ru: bool,
}

static EN: Labels = Labels {
    name: "Name",
    folder: "Folder",
    resolution: "Image size",
    format: "Image format",
    size: "File size",
    created: "File created",
    modified: "File modified",
    transparent: "transparent",
    unknown: "—",
    units: ["B", "KB", "MB", "GB"],
    ru: false,
};

static RU: Labels = Labels {
    name: "Имя",
    folder: "Папка",
    resolution: "Размер изображения",
    format: "Формат изображения",
    size: "Размер файла",
    created: "Файл создан",
    modified: "Файл изменён",
    transparent: "прозрачное",
    unknown: "—",
    units: ["Б", "КБ", "МБ", "ГБ"],
    ru: true,
};

pub fn labels() -> &'static Labels {
    let lang = ["LC_ALL", "LC_MESSAGES", "LANG"]
        .iter()
        .find_map(|k| std::env::var(k).ok().filter(|v| !v.is_empty()))
        .unwrap_or_default();
    if lang.starts_with("ru") { &RU } else { &EN }
}

pub struct Info {
    pub name: String,
    pub folder: String,
    pub folder_path: PathBuf,
    pub resolution: String,
    pub format: String,
    pub size: String,
    pub created: String,
    pub modified: String,
}

impl Info {
    pub fn gather(
        path: &Path,
        dims: (u32, u32),
        format: Option<ImageFormat>,
        has_alpha: bool,
    ) -> Self {
        let lb = labels();
        let meta = std::fs::metadata(path).ok();

        let abs = std::fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf());
        let folder_path = abs
            .parent()
            .map(Path::to_path_buf)
            .unwrap_or_else(|| PathBuf::from("/"));
        let folder = folder_path
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_else(|| "/".into());

        let mut fmt = match format {
            Some(ImageFormat::Png) => "PNG".to_string(),
            Some(ImageFormat::Jpeg) => "JPEG".to_string(),
            Some(ImageFormat::Gif) => "GIF".to_string(),
            Some(ImageFormat::WebP) => "WebP".to_string(),
            Some(ImageFormat::Bmp) => "BMP".to_string(),
            Some(other) => format!("{other:?}"),
            None => lb.unknown.to_string(),
        };
        if has_alpha {
            fmt = format!("{fmt}, {}", lb.transparent);
        }

        Self {
            name: path
                .file_name()
                .map(|n| n.to_string_lossy().into_owned())
                .unwrap_or_default(),
            folder,
            folder_path,
            resolution: format!("{} × {}", dims.0, dims.1),
            format: fmt,
            size: meta
                .as_ref()
                .map(|m| human_size(m.len(), lb))
                .unwrap_or_else(|| lb.unknown.to_string()),
            created: local_time(meta.as_ref().and_then(|m| m.created().ok()), lb),
            modified: local_time(meta.as_ref().and_then(|m| m.modified().ok()), lb),
        }
    }
}

fn human_size(n: u64, lb: &Labels) -> String {
    if n < 1000 {
        return format!("{n} {}", lb.units[0]);
    }
    let mut v = n as f64;
    let mut i = 0;
    while v >= 1000.0 && i < 3 {
        v /= 1000.0;
        i += 1;
    }
    let s = format!("{v:.1} {}", lb.units[i]);
    if lb.ru { s.replace('.', ",") } else { s }
}

fn local_time(t: Option<SystemTime>, lb: &Labels) -> String {
    let unknown = || lb.unknown.to_string();
    let Some(t) = t else { return unknown() };
    let Ok(dur) = t.duration_since(UNIX_EPOCH) else {
        return unknown();
    };
    let secs = dur.as_secs() as libc::time_t;
    let mut tm: libc::tm = unsafe { std::mem::zeroed() };
    if unsafe { libc::localtime_r(&secs, &mut tm) }.is_null() {
        return unknown();
    }
    let (y, mo, d) = (tm.tm_year + 1900, tm.tm_mon + 1, tm.tm_mday);
    let (h, mi, s) = (tm.tm_hour, tm.tm_min, tm.tm_sec);
    if lb.ru {
        format!("{d:02}.{mo:02}.{y} {h:02}:{mi:02}:{s:02}")
    } else {
        format!("{y}-{mo:02}-{d:02} {h:02}:{mi:02}:{s:02}")
    }
}
