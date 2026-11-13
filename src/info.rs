use std::{
    path::{Path, PathBuf},
    time::{SystemTime, UNIX_EPOCH},
};

use image::ImageFormat;

use crate::i18n::tr;

#[derive(Clone)]
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
            Some(ImageFormat::Avif) => "AVIF".to_string(),
            Some(ImageFormat::Ico) => "ICO".to_string(),
            Some(ImageFormat::Tiff) => "TIFF".to_string(),
            Some(ImageFormat::Qoi) => "QOI".to_string(),
            Some(other) => format!("{other:?}"),
            None => {
                let ext = path
                    .extension()
                    .and_then(|e| e.to_str())
                    .unwrap_or_default();
                if ext.eq_ignore_ascii_case("svg") || ext.eq_ignore_ascii_case("svgz") {
                    "SVG".to_string()
                } else {
                    tr("info.unknown").to_string()
                }
            }
        };
        if has_alpha {
            fmt = format!("{fmt}, {}", tr("info.transparent"));
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
                .map(|m| human_size(m.len()))
                .unwrap_or_else(|| tr("info.unknown").to_string()),
            created: local_time(meta.as_ref().and_then(|m| m.created().ok())),
            modified: local_time(meta.as_ref().and_then(|m| m.modified().ok())),
        }
    }
}

fn human_size(n: u64) -> String {
    let units = [tr("unit.b"), tr("unit.kb"), tr("unit.mb"), tr("unit.gb")];
    if n < 1000 {
        return format!("{n} {}", units[0]);
    }
    let mut v = n as f64;
    let mut i = 0;
    while v >= 1000.0 && i < 3 {
        v /= 1000.0;
        i += 1;
    }
    format!("{v:.1} {}", units[i]).replace('.', tr("number.decimal"))
}

fn local_time(t: Option<SystemTime>) -> String {
    let unknown = || tr("info.unknown").to_string();
    let Some(t) = t else { return unknown() };
    let Ok(dur) = t.duration_since(UNIX_EPOCH) else {
        return unknown();
    };
    let secs = dur.as_secs() as libc::time_t;
    let mut tm: libc::tm = unsafe { std::mem::zeroed() };
    if unsafe { libc::localtime_r(&secs, &mut tm) }.is_null() {
        return unknown();
    }
    tr("date.format")
        .replace("{y}", &(tm.tm_year + 1900).to_string())
        .replace("{m}", &format!("{:02}", tm.tm_mon + 1))
        .replace("{d}", &format!("{:02}", tm.tm_mday))
        .replace("{H}", &format!("{:02}", tm.tm_hour))
        .replace("{M}", &format!("{:02}", tm.tm_min))
        .replace("{S}", &format!("{:02}", tm.tm_sec))
}
