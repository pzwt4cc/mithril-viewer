use std::{
    error::Error,
    fmt::Write,
    fs::File,
    io::{BufReader, Read},
    path::{Path, PathBuf},
    sync::{Arc, LazyLock},
    time::Duration,
};

use image::{
    AnimationDecoder, DynamicImage, ImageDecoder, ImageFormat, ImageReader, RgbaImage,
    codecs::gif::GifDecoder, metadata::Orientation,
};

use crate::anim::Anim;

const GIF_MEMORY_LIMIT: usize = 1 << 30;

pub struct Loaded {
    pub image: RgbaImage,
    pub format: Option<ImageFormat>,
    pub has_alpha: bool,
    pub anim: Option<Anim>,
}

static FONT_DB: LazyLock<Arc<resvg::usvg::fontdb::Database>> = LazyLock::new(|| {
    let mut fontdb = resvg::usvg::fontdb::Database::new();
    fontdb.load_system_fonts();
    Arc::new(fontdb)
});

fn extract_symbol_ids(text: &str) -> Vec<String> {
    let mut ids = Vec::new();
    let mut rest = text;
    while let Some(pos) = rest.to_ascii_lowercase().find("<symbol") {
        rest = &rest[pos + 7..];
        let end_tag = match rest.find('>') {
            Some(e) => e,
            None => break,
        };
        let tag_content = &rest[..end_tag];
        rest = &rest[end_tag + 1..];
        let lower_tag = tag_content.to_ascii_lowercase();
        if let Some(id_pos) = lower_tag.find("id=") {
            let val = tag_content[id_pos + 3..].trim_start();
            let quote = val.chars().next();
            if quote == Some('"') || quote == Some('\'') {
                let quote = quote.unwrap();
                let inner = &val[1..];
                if let Some(q_end) = inner.find(quote) {
                    let id = &inner[..q_end];
                    if !id.is_empty() && !ids.iter().any(|existing: &String| existing == id) {
                        ids.push(id.to_string());
                    }
                }
            }
        }
    }
    ids
}

fn synthesize_symbols_grid(text: &str, ids: &[String]) -> Option<String> {
    if ids.is_empty() {
        return None;
    }
    let count = ids.len();
    let cols = (count as f32).sqrt().ceil().max(1.0) as usize;
    let rows = (count + cols - 1) / cols;
    let cell_size = 128.0;
    let pad = 16.0;
    let icon_size = cell_size - 2.0 * pad;
    let total_w = cols as f32 * cell_size;
    let total_h = rows as f32 * cell_size;

    let open_end = text.find('>')?;
    let close_start = text.rfind("</svg>").or_else(|| text.rfind("</SVG>"))?;
    if open_end >= close_start {
        return None;
    }
    let inner = &text[open_end + 1..close_start];

    let mut out = String::with_capacity(text.len() + count * 128 + 256);
    let _ = write!(
        out,
        r##"<svg xmlns="http://www.w3.org/2000/svg" xmlns:xlink="http://www.w3.org/1999/xlink" viewBox="0 0 {} {}" width="{}" height="{}"><defs>{}</defs><g>"##,
        total_w, total_h, total_w, total_h, inner
    );
    for (i, id) in ids.iter().enumerate() {
        let col = i % cols;
        let row = i / cols;
        let x = col as f32 * cell_size + pad;
        let y = row as f32 * cell_size + pad;
        let _ = write!(
            out,
            r##"<use href="#{}" xlink:href="#{}" x="{}" y="{}" width="{}" height="{}"/>"##,
            id, id, x, y, icon_size, icon_size
        );
    }
    out.push_str("</g></svg>");
    Some(out)
}

fn load_svg(path: &Path) -> Result<Loaded, Box<dyn Error>> {
    let mut data = std::fs::read(path)?;
    if data.starts_with(&[0x1f, 0x8b]) {
        let mut decompressed = Vec::new();
        if flate2::read::GzDecoder::new(&data[..])
            .read_to_end(&mut decompressed)
            .is_ok()
        {
            data = decompressed;
        }
    }
    let opt = resvg::usvg::Options {
        fontdb: FONT_DB.clone(),
        ..Default::default()
    };
    let tree = match resvg::usvg::Tree::from_data(&data, &opt) {
        Ok(t) if !t.root().children().is_empty() => t,
        _ => {
            if let Ok(text) = std::str::from_utf8(&data) {
                let ids = extract_symbol_ids(text);
                if !ids.is_empty() {
                    if let Some(synth) = synthesize_symbols_grid(text, &ids) {
                        resvg::usvg::Tree::from_data(synth.as_bytes(), &opt)?
                    } else {
                        resvg::usvg::Tree::from_data(&data, &opt)?
                    }
                } else {
                    resvg::usvg::Tree::from_data(&data, &opt)?
                }
            } else {
                resvg::usvg::Tree::from_data(&data, &opt)?
            }
        }
    };
    let size = tree.size();
    let (base_w, base_h) = (size.width().max(1.0), size.height().max(1.0));
    let max_dim = base_w.max(base_h);
    let scale = if max_dim < 1024.0 {
        (1024.0 / max_dim).clamp(1.0, 32.0)
    } else {
        1.0
    };
    let (w, h) = (
        ((base_w * scale).ceil() as u32).clamp(1, 16384),
        ((base_h * scale).ceil() as u32).clamp(1, 16384),
    );
    let mut pixmap =
        resvg::tiny_skia::Pixmap::new(w, h).ok_or("Failed to allocate pixel buffer for SVG")?;
    resvg::render(
        &tree,
        resvg::tiny_skia::Transform::from_scale(scale, scale),
        &mut pixmap.as_mut(),
    );
    let mut raw = pixmap.take();
    let mut has_alpha = false;
    for pixel in raw.chunks_exact_mut(4) {
        let a = pixel[3] as u32;
        if a < 255 {
            has_alpha = true;
            if a > 0 {
                pixel[0] = ((pixel[0] as u32 * 255 + a / 2) / a).min(255) as u8;
                pixel[1] = ((pixel[1] as u32 * 255 + a / 2) / a).min(255) as u8;
                pixel[2] = ((pixel[2] as u32 * 255 + a / 2) / a).min(255) as u8;
            }
        }
    }
    let image =
        RgbaImage::from_raw(w, h, raw).ok_or("Failed to construct RGBA image from SVG buffer")?;
    Ok(Loaded {
        image,
        format: None,
        has_alpha,
        anim: None,
    })
}

pub fn load(path: &Path) -> Result<Loaded, Box<dyn Error>> {
    let is_svg = path
        .extension()
        .and_then(|e| e.to_str())
        .is_some_and(|e| e.eq_ignore_ascii_case("svg") || e.eq_ignore_ascii_case("svgz"));

    if is_svg {
        return load_svg(path);
    }

    let reader = match ImageReader::open(path)?.with_guessed_format() {
        Ok(r) => r,
        Err(_) => return load_svg(path),
    };
    let format = reader.format();

    if format == Some(ImageFormat::Gif) {
        if let Some(loaded) = load_gif(path)? {
            return Ok(loaded);
        }
    }

    if format.is_none() {
        if let Ok(loaded) = load_svg(path) {
            return Ok(loaded);
        }
    }

    let mut decoder = reader.into_decoder()?;
    let orientation = decoder.orientation().unwrap_or(Orientation::NoTransforms);
    let mut img = DynamicImage::from_decoder(decoder)?;
    img.apply_orientation(orientation);
    let has_alpha = img.color().has_alpha();
    Ok(Loaded {
        image: img.to_rgba8(),
        format,
        has_alpha,
        anim: None,
    })
}

fn load_gif(path: &Path) -> Result<Option<Loaded>, Box<dyn Error>> {
    let decoder = GifDecoder::new(BufReader::new(File::open(path)?))?;
    let mut frames = Vec::new();
    let mut delays = Vec::new();
    let mut bytes = 0usize;

    for frame in decoder.into_frames() {
        let frame = frame?;
        let (n, d) = frame.delay().numer_denom_ms();
        let ms = n as f32 / d.max(1) as f32;
        let delay = Duration::from_millis(if ms < 20.0 { 100 } else { ms as u64 });
        let buf = frame.into_buffer();
        bytes += buf.as_raw().len();
        frames.push(buf);
        delays.push(delay);
        if bytes > GIF_MEMORY_LIMIT {
            break;
        }
    }

    if frames.len() < 2 {
        return Ok(None);
    }

    let image = frames[0].clone();
    let has_alpha = image.pixels().any(|p| p.0[3] < 255);
    Ok(Some(Loaded {
        image,
        format: Some(ImageFormat::Gif),
        has_alpha,
        anim: Some(Anim::new(frames, delays)),
    }))
}

const EXTENSIONS: &[&str] = &["jpg", "jpeg", "png", "gif", "webp", "bmp", "svg", "svgz"];

fn is_supported(path: &Path) -> bool {
    path.extension()
        .and_then(|e| e.to_str())
        .is_some_and(|e| EXTENSIONS.iter().any(|x| x.eq_ignore_ascii_case(e)))
}

fn natural_key(path: &Path) -> String {
    let name = path
        .file_name()
        .map(|n| n.to_string_lossy().to_lowercase())
        .unwrap_or_default();
    let mut key = String::with_capacity(name.len() + 16);
    let mut digits = String::new();
    for c in name.chars() {
        if c.is_ascii_digit() {
            digits.push(c);
            continue;
        }
        if !digits.is_empty() {
            key.push_str(&format!("{digits:0>12}"));
            digits.clear();
        }
        key.push(c);
    }
    if !digits.is_empty() {
        key.push_str(&format!("{digits:0>12}"));
    }
    key
}

pub fn siblings(path: &Path) -> (Vec<PathBuf>, usize) {
    let dir = match path.parent() {
        Some(d) if !d.as_os_str().is_empty() => d,
        _ => Path::new("."),
    };
    let mut list: Vec<PathBuf> = std::fs::read_dir(dir)
        .into_iter()
        .flatten()
        .flatten()
        .map(|e| e.path())
        .filter(|p| is_supported(p) && p.is_file())
        .collect();
    list.sort_by_cached_key(|p| natural_key(p));
    let index = list
        .iter()
        .position(|p| p.file_name() == path.file_name())
        .unwrap_or_else(|| {
            list.push(path.to_path_buf());
            list.len() - 1
        });
    (list, index)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_load_svg() {
        let loaded = load(Path::new("assets/logo.svg")).expect("failed to load svg");
        assert!(loaded.image.width() > 0);
        assert!(loaded.image.height() > 0);
    }

    #[test]
    fn test_load_svg_spritesheet() {
        let svg = r##"<svg xmlns="http://www.w3.org/2000/svg">
  <symbol id="icon-a" viewBox="0 0 20 20">
    <rect width="20" height="20" fill="#ff0000"/>
  </symbol>
  <symbol id="icon-b" viewBox="0 0 20 20">
    <circle cx="10" cy="10" r="10" fill="#00ff00"/>
  </symbol>
</svg>"##;
        let path = std::env::temp_dir().join("mithril_test_icons.svg");
        std::fs::write(&path, svg).expect("failed to write test svg");
        let loaded = load(&path).expect("failed to load svg spritesheet");
        let _ = std::fs::remove_file(&path);
        assert!(loaded.image.width() > 0);
        assert!(loaded.image.height() > 0);
        assert!(loaded.image.pixels().any(|p| p.0[3] > 0));
    }
}
