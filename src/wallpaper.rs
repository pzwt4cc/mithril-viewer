use std::{
    env, fs,
    path::{Path, PathBuf},
    process::{Command, Stdio},
    time::{SystemTime, UNIX_EPOCH},
};

use image::{DynamicImage, ImageFormat, RgbaImage};

fn cache_dir() -> Option<PathBuf> {
    let base = env::var_os("XDG_CACHE_HOME")
        .filter(|v| !v.is_empty())
        .map(PathBuf::from)
        .or_else(|| env::var_os("HOME").map(|h| PathBuf::from(h).join(".cache")))?;
    Some(base.join("mithril"))
}

fn export_temp_png(img: &RgbaImage) -> Result<PathBuf, String> {
    let dir = cache_dir().ok_or("no cache directory")?;
    fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    if let Ok(entries) = fs::read_dir(&dir) {
        for entry in entries.flatten() {
            let p = entry.path();
            if p.file_name()
                .and_then(|n| n.to_str())
                .is_some_and(|n| n.starts_with("wallpaper-") && n.ends_with(".png"))
            {
                let _ = fs::remove_file(p);
            }
        }
    }
    let stamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| d.as_millis());
    let out = dir.join(format!("wallpaper-{stamp}.png"));
    DynamicImage::ImageRgba8(img.clone())
        .save_with_format(&out, ImageFormat::Png)
        .map_err(|e| e.to_string())?;
    Ok(out)
}

fn needs_export(format: Option<ImageFormat>, modified: bool) -> bool {
    modified
        || !matches!(
            format,
            Some(ImageFormat::Png | ImageFormat::Jpeg | ImageFormat::WebP | ImageFormat::Bmp)
        )
}

fn command_ok(cmd: &mut Command) -> bool {
    let Ok(out) = cmd.output() else {
        return false;
    };
    if !out.status.success() {
        return false;
    }
    let stdout = String::from_utf8_lossy(&out.stdout);
    let stderr = String::from_utf8_lossy(&out.stderr);
    let has_err = |s: &str| {
        let lower = s.trim_start().to_ascii_lowercase();
        lower.starts_with("error:") || lower.starts_with("invalid") || lower.starts_with("failed")
    };
    !has_err(&stdout) && !has_err(&stderr)
}

fn has_tool(name: &str) -> bool {
    env::var_os("PATH").is_some_and(|paths| {
        env::split_paths(&paths).any(|dir| {
            let candidate = dir.join(name);
            candidate.is_file()
        })
    })
}

fn try_swww_bin(bin: &str, path: &Path) -> bool {
    if !has_tool(bin) {
        return false;
    }
    if !command_ok(Command::new(bin).arg("query")) {
        return false;
    }
    command_ok(
        Command::new(bin)
            .arg("img")
            .arg(path)
            .arg("--transition-type")
            .arg("grow")
            .arg("--transition-duration")
            .arg("0.6"),
    ) || command_ok(Command::new(bin).arg("img").arg(path))
}

fn try_hyprpaper(path: &Path) -> bool {
    if env::var_os("HYPRLAND_INSTANCE_SIGNATURE").is_none() || !has_tool("hyprctl") {
        return false;
    }
    let p = path.to_string_lossy();
    if command_ok(
        Command::new("hyprctl")
            .arg("hyprpaper")
            .arg("wallpaper")
            .arg(format!(",{p},cover")),
    ) {
        return true;
    }
    if command_ok(
        Command::new("hyprctl")
            .arg("hyprpaper")
            .arg("reload")
            .arg(format!(",{p}")),
    ) {
        return true;
    }
    command_ok(
        Command::new("hyprctl")
            .arg("hyprpaper")
            .arg("preload")
            .arg(p.as_ref()),
    ) && command_ok(
        Command::new("hyprctl")
            .arg("hyprpaper")
            .arg("wallpaper")
            .arg(format!(",{p}")),
    )
}

fn try_swaybg(path: &Path) -> bool {
    if env::var_os("WAYLAND_DISPLAY").is_none() || !has_tool("swaybg") {
        return false;
    }
    let _ = Command::new("pkill")
        .args(["-x", "swaybg"])
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status();
    Command::new("swaybg")
        .arg("-i")
        .arg(path)
        .arg("-m")
        .arg("fill")
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .is_ok()
}

fn try_feh(path: &Path) -> bool {
    if env::var_os("DISPLAY").is_none() || !has_tool("feh") {
        return false;
    }
    command_ok(Command::new("feh").arg("--bg-fill").arg(path))
}

fn try_plasma(path: &Path) -> bool {
    has_tool("plasma-apply-wallpaperimage")
        && command_ok(Command::new("plasma-apply-wallpaperimage").arg(path))
}

fn try_gnome(path: &Path) -> bool {
    if !has_tool("gsettings") {
        return false;
    }
    let desk = env::var("XDG_CURRENT_DESKTOP")
        .unwrap_or_default()
        .to_ascii_lowercase();
    let is_gtk_de = ["gnome", "unity", "budgie", "cinnamon", "mate"]
        .iter()
        .any(|d| desk.contains(d));
    if !is_gtk_de {
        return false;
    }
    let uri = format!("file://{}", path.to_string_lossy());
    let ok_light = command_ok(Command::new("gsettings").args([
        "set",
        "org.gnome.desktop.background",
        "picture-uri",
        &uri,
    ]));
    let _ = command_ok(Command::new("gsettings").args([
        "set",
        "org.gnome.desktop.background",
        "picture-uri-dark",
        &uri,
    ]));
    ok_light
}

pub fn set(
    path: &Path,
    img: Option<&RgbaImage>,
    format: Option<ImageFormat>,
    modified: bool,
) -> Result<(), String> {
    let target = if needs_export(format, modified) {
        if let Some(rgba) = img {
            export_temp_png(rgba)?
        } else {
            fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf())
        }
    } else {
        fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf())
    };

    if try_swww_bin("swww", &target)
        || try_swww_bin("awww", &target)
        || try_hyprpaper(&target)
        || try_swaybg(&target)
        || try_feh(&target)
        || try_plasma(&target)
        || try_gnome(&target)
    {
        return Ok(());
    }

    Err("no active wallpaper daemon (swww, hyprpaper, swaybg, feh)".into())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_needs_export_rules() {
        assert!(!needs_export(Some(ImageFormat::Png), false));
        assert!(!needs_export(Some(ImageFormat::Jpeg), false));
        assert!(!needs_export(Some(ImageFormat::WebP), false));
        assert!(needs_export(Some(ImageFormat::Png), true));
        assert!(needs_export(Some(ImageFormat::Avif), false));
        assert!(needs_export(None, false));
    }
}
