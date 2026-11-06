use std::{cmp::Ordering, path::PathBuf};

pub fn natural_cmp(a: &str, b: &str) -> Ordering {
    let (mut a, mut b) = (a.chars().peekable(), b.chars().peekable());
    loop {
        match (a.peek().copied(), b.peek().copied()) {
            (None, None) => return Ordering::Equal,
            (None, _) => return Ordering::Less,
            (_, None) => return Ordering::Greater,
            (Some(x), Some(y)) if x.is_ascii_digit() && y.is_ascii_digit() => {
                let (mut na, mut nb) = (0u128, 0u128);
                while let Some(c) = a.peek().copied().filter(|c| c.is_ascii_digit()) {
                    na = na
                        .saturating_mul(10)
                        .saturating_add(c as u128 - '0' as u128);
                    a.next();
                }
                while let Some(c) = b.peek().copied().filter(|c| c.is_ascii_digit()) {
                    nb = nb
                        .saturating_mul(10)
                        .saturating_add(c as u128 - '0' as u128);
                    b.next();
                }
                match na.cmp(&nb) {
                    Ordering::Equal => {}
                    o => return o,
                }
            }
            (Some(x), Some(y)) => {
                let (x, y) = (
                    x.to_lowercase().next().unwrap_or(x),
                    y.to_lowercase().next().unwrap_or(y),
                );
                match x.cmp(&y) {
                    Ordering::Equal => {
                        a.next();
                        b.next();
                    }
                    o => return o,
                }
            }
        }
    }
}

pub fn first_image(dir: &std::path::Path) -> Option<PathBuf> {
    const EXT: [&str; 6] = ["png", "jpg", "jpeg", "gif", "webp", "bmp"];
    std::fs::read_dir(dir)
        .ok()?
        .filter_map(|e| e.ok())
        .map(|e| e.path())
        .filter(|p| {
            p.is_file()
                && p.extension()
                    .and_then(|e| e.to_str())
                    .is_some_and(|e| EXT.iter().any(|x| e.eq_ignore_ascii_case(x)))
        })
        .min_by(|a, b| {
            let name = |p: &PathBuf| p.file_name().map(|n| n.to_string_lossy().into_owned());
            natural_cmp(&name(a).unwrap_or_default(), &name(b).unwrap_or_default())
        })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_natural_cmp() {
        assert_eq!(natural_cmp("img2.png", "img10.png"), Ordering::Less);
        assert_eq!(natural_cmp("img10.png", "img2.png"), Ordering::Greater);
        assert_eq!(natural_cmp("photo.jpg", "photo.jpg"), Ordering::Equal);
        assert_eq!(natural_cmp("a1b.png", "a01b.png"), Ordering::Equal);
    }
}
