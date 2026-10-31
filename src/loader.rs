use std::{error::Error, fs::File, io::BufReader, path::Path, time::Duration};

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

pub fn load(path: &Path) -> Result<Loaded, Box<dyn Error>> {
    let reader = ImageReader::open(path)?.with_guessed_format()?;
    let format = reader.format();

    if format == Some(ImageFormat::Gif) {
        if let Some(loaded) = load_gif(path)? {
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
