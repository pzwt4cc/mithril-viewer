use std::{
    env, fs,
    os::fd::AsFd,
    path::Path,
    process::Command,
    time::{SystemTime, UNIX_EPOCH},
};

use image::{DynamicImage, RgbaImage};

fn image_to_pdf(img: &RgbaImage) -> Result<Vec<u8>, String> {
    let rgb = image::RgbImage::from_fn(img.width(), img.height(), |x, y| {
        let p = img.get_pixel(x, y);
        let a = p[3] as u32;
        if a == 255 {
            image::Rgb([p[0], p[1], p[2]])
        } else {
            let r = ((p[0] as u32 * a + 255 * (255 - a)) / 255) as u8;
            let g = ((p[1] as u32 * a + 255 * (255 - a)) / 255) as u8;
            let b = ((p[2] as u32 * a + 255 * (255 - a)) / 255) as u8;
            image::Rgb([r, g, b])
        }
    });

    let mut jpg = Vec::new();
    let enc = image::codecs::jpeg::JpegEncoder::new_with_quality(&mut jpg, 92);
    DynamicImage::ImageRgb8(rgb)
        .write_with_encoder(enc)
        .map_err(|e| e.to_string())?;

    let is_landscape = img.width() > img.height();
    let (pw, ph) = if is_landscape {
        (842.0f32, 595.0f32)
    } else {
        (595.0f32, 842.0f32)
    };
    let m = 28.0f32;
    let aw = pw - 2.0 * m;
    let ah = ph - 2.0 * m;
    let scale = (aw / img.width() as f32).min(ah / img.height() as f32);
    let dw = img.width() as f32 * scale;
    let dh = img.height() as f32 * scale;
    let dx = m + (aw - dw) / 2.0;
    let dy = m + (ah - dh) / 2.0;

    let content = format!(
        "q {:.2} 0 0 {:.2} {:.2} {:.2} cm /Im Do Q\n",
        dw, dh, dx, dy
    );
    let mut pdf = Vec::with_capacity(jpg.len() + 1024);
    pdf.extend_from_slice(b"%PDF-1.4\n");
    let o1 = pdf.len();
    pdf.extend_from_slice(b"1 0 obj\n<< /Type /Catalog /Pages 2 0 R >>\nendobj\n");
    let o2 = pdf.len();
    pdf.extend_from_slice(b"2 0 obj\n<< /Type /Pages /Kids [3 0 R] /Count 1 >>\nendobj\n");
    let o3 = pdf.len();
    pdf.extend_from_slice(
        format!(
            "3 0 obj\n<< /Type /Page /Parent 2 0 R /MediaBox [0 0 {:.0} {:.0}] /Resources << /XObject << /Im 4 0 R >> >> /Contents 5 0 R >>\nendobj\n",
            pw, ph
        )
        .as_bytes(),
    );
    let o4 = pdf.len();
    pdf.extend_from_slice(
        format!(
            "4 0 obj\n<< /Type /XObject /Subtype /Image /Width {} /Height {} /ColorSpace /DeviceRGB /BitsPerComponent 8 /Filter /DCTDecode /Length {} >>\nstream\n",
            img.width(),
            img.height(),
            jpg.len()
        )
        .as_bytes(),
    );
    pdf.extend_from_slice(&jpg);
    pdf.extend_from_slice(b"\nendstream\nendobj\n");
    let o5 = pdf.len();
    pdf.extend_from_slice(
        format!(
            "5 0 obj\n<< /Length {} >>\nstream\n{}\nendstream\nendobj\n",
            content.len(),
            content
        )
        .as_bytes(),
    );
    let xref = pdf.len();
    pdf.extend_from_slice(
        format!(
            "xref\n0 6\n0000000000 65535 f \n{:010} 00000 n \n{:010} 00000 n \n{:010} 00000 n \n{:010} 00000 n \n{:010} 00000 n \ntrailer\n<< /Size 6 /Root 1 0 R >>\nstartxref\n{}\n%%EOF\n",
            o1, o2, o3, o4, o5, xref
        )
        .as_bytes(),
    );
    Ok(pdf)
}

enum PortalOutcome {
    Printed,
    Cancelled,
    Unavailable,
}

fn try_portal(path: &Path, title: &str) -> PortalOutcome {
    let Ok(file) = fs::File::open(path) else {
        return PortalOutcome::Unavailable;
    };
    let res = pollster::block_on(async {
        let proxy = ashpd::desktop::print::PrintProxy::new().await?;
        let prep = proxy
            .prepare_print(
                None,
                title,
                Default::default(),
                Default::default(),
                None,
                true,
            )
            .await?
            .response()?;
        proxy
            .print(None, title, &file.as_fd(), Some(prep.token), true)
            .await?
            .response()?;
        Ok::<(), ashpd::Error>(())
    });
    match res {
        Ok(()) => PortalOutcome::Printed,
        Err(ashpd::Error::Response(ashpd::desktop::ResponseError::Cancelled)) => {
            PortalOutcome::Cancelled
        }
        Err(_) => PortalOutcome::Unavailable,
    }
}

fn has_tool(name: &str) -> bool {
    env::var_os("PATH").is_some_and(|paths| {
        env::split_paths(&paths).any(|dir| {
            let candidate = dir.join(name);
            candidate.is_file()
        })
    })
}

fn try_native(path: &Path) -> Result<bool, String> {
    if has_tool("gtklp")
        && let Ok(out) = Command::new("gtklp").arg(path).output()
    {
        if out.status.success() {
            return Ok(true);
        }
        let err = String::from_utf8_lossy(&out.stderr).trim().to_string();
        if !err.is_empty() {
            return Err(err);
        }
        return Ok(false);
    }

    if has_tool("lp") {
        let out = Command::new("lp")
            .arg("-o")
            .arg("fit-to-page")
            .arg(path)
            .output()
            .map_err(|e| e.to_string())?;
        if out.status.success() {
            return Ok(true);
        }
        let err = String::from_utf8_lossy(&out.stderr).trim().to_string();
        return Err(if err.is_empty() {
            "lp failed".into()
        } else {
            err
        });
    }

    Err("no print portal or lp available".into())
}

pub fn print_image(img: &RgbaImage, title: &str) -> Result<bool, String> {
    let pdf = image_to_pdf(img)?;
    let stamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| d.as_nanos());
    let path = env::temp_dir().join(format!("mithril-print-{stamp}.pdf"));
    fs::write(&path, &pdf).map_err(|e| e.to_string())?;

    let res = match try_portal(&path, title) {
        PortalOutcome::Printed => Ok(true),
        PortalOutcome::Cancelled => Ok(false),
        PortalOutcome::Unavailable => try_native(&path),
    };

    let _ = fs::remove_file(&path);
    res
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_image_to_pdf_valid() {
        let img = RgbaImage::from_fn(100, 100, |x, y| {
            image::Rgba([(x % 256) as u8, (y % 256) as u8, 128, 255])
        });
        let pdf = image_to_pdf(&img).expect("pdf generation should succeed");
        assert!(pdf.starts_with(b"%PDF-1.4"));
        assert!(pdf.ends_with(b"%%EOF\n") || pdf.ends_with(b"%%EOF"));
    }
}
