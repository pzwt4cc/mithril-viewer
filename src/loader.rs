use std::{
    error::Error,
    fs::File,
    io::BufReader,
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

fn load_svg(path: &Path) -> Result<Loaded, Box<dyn Error>> {
    let data = std::fs::read(path)?;
    let opt = resvg::usvg::Options {
        fontdb: FONT_DB.clone(),
        ..Default::default()
    };
    let tree = resvg::usvg::Tree::from_data(&data, &opt)?;
    let size = tree.size();
    let (w, h) = (
        (size.width().ceil() as u32).max(1),
        (size.height().ceil() as u32).max(1),
    );
    let mut pixmap =
        resvg::tiny_skia::Pixmap::new(w, h).ok_or("Failed to allocate pixel buffer for SVG")?;
    resvg::render(
        &tree,
        resvg::tiny_skia::Transform::default(),
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
}
