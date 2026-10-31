use std::{error::Error, path::Path};

use image::{
    DynamicImage, ImageDecoder, ImageFormat, ImageReader, RgbaImage, metadata::Orientation,
};

pub struct Loaded {
    pub image: RgbaImage,
    pub format: Option<ImageFormat>,
    pub has_alpha: bool,
}

pub fn load(path: &Path) -> Result<Loaded, Box<dyn Error>> {
    let reader = ImageReader::open(path)?.with_guessed_format()?;
    let format = reader.format();
    let mut decoder = reader.into_decoder()?;
    let orientation = decoder.orientation().unwrap_or(Orientation::NoTransforms);
    let mut img = DynamicImage::from_decoder(decoder)?;
    img.apply_orientation(orientation);
    let has_alpha = img.color().has_alpha();
    Ok(Loaded {
        image: img.to_rgba8(),
        format,
        has_alpha,
    })
}
