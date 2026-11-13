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
    codecs::{gif::GifDecoder, png::PngDecoder, webp::WebPDecoder},
    metadata::Orientation,
};

use crate::anim::Anim;

const ANIM_MEMORY_LIMIT: usize = 256 * 1024 * 1024;

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

static EMPTY_FONT_DB: LazyLock<Arc<resvg::usvg::fontdb::Database>> =
    LazyLock::new(|| Arc::new(resvg::usvg::fontdb::Database::new()));

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
    let rows = count.div_ceil(cols);
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

#[allow(clippy::manual_checked_ops)]
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
    let fontdb = if data.windows(5).any(|w| w == b"<text" || w == b"<tspa") {
        FONT_DB.clone()
    } else {
        EMPTY_FONT_DB.clone()
    };
    let opt = resvg::usvg::Options {
        fontdb,
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
        ((base_w * scale).ceil() as u32).clamp(1, 4096),
        ((base_h * scale).ceil() as u32).clamp(1, 4096),
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
    for pixel in raw.as_chunks_mut::<4>().0 {
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

    match format {
        Some(ImageFormat::Gif) => {
            let decoder = GifDecoder::new(BufReader::new(File::open(path)?))?;
            if let Some(loaded) = load_animated(decoder, ImageFormat::Gif)? {
                return Ok(loaded);
            }
        }
        Some(ImageFormat::Png) => {
            let decoder = PngDecoder::new(BufReader::new(File::open(path)?))?;
            if decoder.is_apng().unwrap_or(false)
                && let Some(loaded) = load_animated(decoder.apng()?, ImageFormat::Png)?
            {
                return Ok(loaded);
            }
        }
        Some(ImageFormat::WebP) => {
            let decoder = WebPDecoder::new(BufReader::new(File::open(path)?))?;
            if decoder.has_animation()
                && let Some(loaded) = load_animated(decoder, ImageFormat::WebP)?
            {
                return Ok(loaded);
            }
        }
        _ => {}
    }

    if format.is_none()
        && let Ok(loaded) = load_svg(path) {
            return Ok(loaded);
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

fn load_animated<'a, D: AnimationDecoder<'a>>(
    decoder: D,
    format: ImageFormat,
) -> Result<Option<Loaded>, Box<dyn Error>> {
    let mut frames = Vec::new();
    let mut delays = Vec::new();
    let mut bytes = 0usize;

    for frame in decoder.into_frames() {
        let frame = frame?;
        let (n, d) = frame.delay().numer_denom_ms();
        let ms = n as f32 / d.max(1) as f32;
        let delay_ms = if format == ImageFormat::Gif {
            if ms < 20.0 { 100 } else { ms as u64 }
        } else if ms <= 0.0 {
            100
        } else {
            (ms.round() as u64).max(10)
        };
        let delay = Duration::from_millis(delay_ms);
        let buf = frame.into_buffer();
        bytes += buf.as_raw().len();
        frames.push(buf);
        delays.push(delay);
        if bytes > ANIM_MEMORY_LIMIT {
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
        format: Some(format),
        has_alpha,
        anim: Some(Anim::new(frames, delays)),
    }))
}

const EXTENSIONS: &[&str] = &[
    "jpg", "jpeg", "png", "apng", "gif", "webp", "bmp", "svg", "svgz", "avif", "ico", "tiff", "tif", "qoi",
];

fn is_supported(path: &Path) -> bool {
    path.extension()
        .and_then(|e| e.to_str())
        .is_some_and(|e| EXTENSIONS.iter().any(|x| x.eq_ignore_ascii_case(e)))
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
    list.sort_by(|a, b| {
        let an = a.file_name().and_then(|n| n.to_str()).unwrap_or_default();
        let bn = b.file_name().and_then(|n| n.to_str()).unwrap_or_default();
        crate::nav::natural_cmp(an, bn)
    });
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
    fn test_supported_extensions() {
        for ext in ["avif", "ico", "tiff", "tif", "qoi", "png", "jpg", "svg"] {
            let path = PathBuf::from(format!("image.{ext}"));
            assert!(is_supported(&path));
        }
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

    fn crc32(data: &[u8]) -> u32 {
        let mut crc = 0xffff_ffffu32;
        for &b in data {
            crc ^= b as u32;
            for _ in 0..8 {
                let mask = (crc & 1).wrapping_neg();
                crc = (crc >> 1) ^ (0xedb8_8320 & mask);
            }
        }
        !crc
    }

    fn png_chunk(kind: &[u8; 4], data: &[u8]) -> Vec<u8> {
        let mut out = Vec::with_capacity(12 + data.len());
        out.extend_from_slice(&(data.len() as u32).to_be_bytes());
        out.extend_from_slice(kind);
        out.extend_from_slice(data);
        let mut crc_buf = Vec::with_capacity(4 + data.len());
        crc_buf.extend_from_slice(kind);
        crc_buf.extend_from_slice(data);
        out.extend_from_slice(&crc32(&crc_buf).to_be_bytes());
        out
    }

    #[test]
    fn test_load_apng_animation() {
        let img1 = RgbaImage::from_pixel(1, 1, image::Rgba([255, 0, 0, 255]));
        let img2 = RgbaImage::from_pixel(1, 1, image::Rgba([0, 255, 0, 255]));
        let mut buf1 = Vec::new();
        let mut buf2 = Vec::new();
        DynamicImage::ImageRgba8(img1)
            .write_to(&mut std::io::Cursor::new(&mut buf1), ImageFormat::Png)
            .unwrap();
        DynamicImage::ImageRgba8(img2)
            .write_to(&mut std::io::Cursor::new(&mut buf2), ImageFormat::Png)
            .unwrap();

        let find_chunk = |png: &[u8], name: &[u8; 4]| -> Vec<u8> {
            let mut pos = 8;
            while pos + 12 <= png.len() {
                let len = u32::from_be_bytes(png[pos..pos + 4].try_into().unwrap()) as usize;
                let kind = &png[pos + 4..pos + 8];
                if kind == name {
                    return png[pos + 8..pos + 8 + len].to_vec();
                }
                pos += 12 + len;
            }
            panic!("chunk not found");
        };

        let ihdr = find_chunk(&buf1, b"IHDR");
        let idat1 = find_chunk(&buf1, b"IDAT");
        let idat2 = find_chunk(&buf2, b"IDAT");

        let mut apng = Vec::new();
        apng.extend_from_slice(&buf1[..8]);
        apng.extend_from_slice(&png_chunk(b"IHDR", &ihdr));
        let mut actl = Vec::new();
        actl.extend_from_slice(&2u32.to_be_bytes());
        actl.extend_from_slice(&0u32.to_be_bytes());
        apng.extend_from_slice(&png_chunk(b"acTL", &actl));

        let make_fctl = |seq: u32| -> Vec<u8> {
            let mut d = Vec::new();
            d.extend_from_slice(&seq.to_be_bytes());
            d.extend_from_slice(&1u32.to_be_bytes());
            d.extend_from_slice(&1u32.to_be_bytes());
            d.extend_from_slice(&0u32.to_be_bytes());
            d.extend_from_slice(&0u32.to_be_bytes());
            d.extend_from_slice(&50u16.to_be_bytes());
            d.extend_from_slice(&1000u16.to_be_bytes());
            d.push(0);
            d.push(0);
            d
        };

        apng.extend_from_slice(&png_chunk(b"fcTL", &make_fctl(0)));
        apng.extend_from_slice(&png_chunk(b"IDAT", &idat1));
        apng.extend_from_slice(&png_chunk(b"fcTL", &make_fctl(1)));
        let mut fdat = Vec::new();
        fdat.extend_from_slice(&2u32.to_be_bytes());
        fdat.extend_from_slice(&idat2);
        apng.extend_from_slice(&png_chunk(b"fdAT", &fdat));
        apng.extend_from_slice(&png_chunk(b"IEND", &[]));

        let path = std::env::temp_dir().join("mithril_test_anim.png");
        std::fs::write(&path, &apng).unwrap();
        let loaded = load(&path).expect("failed to load apng");
        let _ = std::fs::remove_file(&path);
        assert!(loaded.anim.is_some());
        assert_eq!(loaded.format, Some(ImageFormat::Png));
        assert!(is_supported(Path::new("sticker.apng")));
    }
}