use std::{cell::RefCell, collections::HashMap, ops::Deref, process::Command};

use fontdue::{Font, FontSettings, Metrics};

pub struct Fonts {
    primary: Font,
    extra: RefCell<Vec<Font>>,
    loaded: RefCell<HashMap<String, usize>>,
    cache: RefCell<HashMap<char, (usize, char)>>,
}

impl Deref for Fonts {
    type Target = Font;
    fn deref(&self) -> &Font {
        &self.primary
    }
}

pub fn invisible(c: char) -> bool {
    matches!(c as u32,
        0x00..=0x1f | 0x7f | 0x200b..=0x200f | 0x2060 | 0xfe00..=0xfe0f | 0xfeff)
}

fn fold(c: char) -> Option<char> {
    let cp = c as u32;
    let letter = |i: u32| {
        char::from_u32(if i < 26 {
            'A' as u32 + i
        } else {
            'a' as u32 + i - 26
        })
    };
    match cp {
        0x1d400..=0x1d6a3 => letter((cp - 0x1d400) % 52),
        0x1d7ce..=0x1d7ff => char::from_u32('0' as u32 + (cp - 0x1d7ce) % 10),
        0xff01..=0xff5e => char::from_u32(cp - 0xfee0),
        0x24b6..=0x24cf => char::from_u32('A' as u32 + cp - 0x24b6),
        0x24d0..=0x24e9 => char::from_u32('a' as u32 + cp - 0x24d0),
        0x2100..=0x214f => Some(match cp {
            0x2102 | 0x212d => 'C',
            0x210a => 'g',
            0x210b..=0x210d => 'H',
            0x210e => 'h',
            0x2110 | 0x2111 => 'I',
            0x2112 => 'L',
            0x2115 => 'N',
            0x2119 => 'P',
            0x211a => 'Q',
            0x211b..=0x211d => 'R',
            0x2124 | 0x2128 => 'Z',
            0x212c => 'B',
            0x212f | 0x2130 => 'E',
            0x2131 => 'F',
            0x2133 => 'M',
            0x2134 => 'o',
            _ => return None,
        }),
        _ => None,
    }
}

impl Fonts {
    pub fn new(primary: Font) -> Self {
        Self {
            primary,
            extra: RefCell::new(Vec::new()),
            loaded: RefCell::new(HashMap::new()),
            cache: RefCell::new(HashMap::new()),
        }
    }

    fn resolve(&self, ch: char) -> (usize, char) {
        let cached = self.cache.borrow().get(&ch).copied();
        if let Some(r) = cached {
            return r;
        }
        let r = self.find(ch);
        self.cache.borrow_mut().insert(ch, r);
        r
    }

    fn find(&self, ch: char) -> (usize, char) {
        if self.primary.lookup_glyph_index(ch) != 0 {
            return (0, ch);
        }
        if let Some(i) = self.loaded_with(ch) {
            return (i, ch);
        }
        if let Some(i) = self.system_font_for(ch) {
            return (i, ch);
        }
        if let Some(a) = fold(ch)
            && self.primary.lookup_glyph_index(a) != 0 {
                return (0, a);
            }
        (0, ch)
    }

    fn loaded_with(&self, ch: char) -> Option<usize> {
        self.extra
            .borrow()
            .iter()
            .position(|f| f.lookup_glyph_index(ch) != 0)
            .map(|i| i + 1)
    }

    fn system_font_for(&self, ch: char) -> Option<usize> {
        let out = Command::new("fc-match")
            .args([
                "-f",
                "%{file}\n%{index}",
                &format!(":charset={:x}", ch as u32),
            ])
            .output()
            .ok()?;
        if !out.status.success() {
            return None;
        }
        let s = String::from_utf8(out.stdout).ok()?;
        let (path, index) = s.split_once('\n')?;
        let index: u32 = index.trim().parse().unwrap_or(0);
        let key = format!("{path}#{index}");

        let known = self.loaded.borrow().get(&key).copied();
        let i = match known {
            Some(i) => i,
            None => {
                let data = std::fs::read(path).ok()?;
                let font = Font::from_bytes(
                    data,
                    FontSettings {
                        collection_index: index,
                        ..Default::default()
                    },
                )
                .ok()?;
                let mut extra = self.extra.borrow_mut();
                extra.push(font);
                let i = extra.len();
                self.loaded.borrow_mut().insert(key, i);
                i
            }
        };
        (self.extra.borrow()[i - 1].lookup_glyph_index(ch) != 0).then_some(i)
    }

    fn with<R>(&self, ch: char, f: impl FnOnce(&Font, char) -> R) -> R {
        let (i, c) = self.resolve(ch);
        if i == 0 {
            f(&self.primary, c)
        } else {
            f(&self.extra.borrow()[i - 1], c)
        }
    }

    pub fn metrics(&self, ch: char, px: f32) -> Metrics {
        let mut m = self.with(ch, |f, c| f.metrics(c, px));
        if invisible(ch) {
            m.advance_width = 0.0;
        }
        m
    }

    pub fn rasterize(&self, ch: char, px: f32) -> (Metrics, Vec<u8>) {
        self.with(ch, |f, c| f.rasterize(c, px))
    }
}
