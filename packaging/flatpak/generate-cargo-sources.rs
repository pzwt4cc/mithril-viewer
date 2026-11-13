use std::{
    collections::HashMap,
    env, fs,
    io::Write,
    path::{Path, PathBuf},
    process,
};

const CRATES_IO: &str = "https://static.crates.io/crates";
const CARGO_HOME: &str = "cargo";
const CARGO_CRATES: &str = "cargo/vendor";
const VENDORED_SOURCES: &str = "vendored-sources";

#[derive(Default)]
struct Package {
    name: String,
    version: String,
    source: Option<String>,
    checksum: Option<String>,
}

fn unquote_toml(s: &str) -> Option<String> {
    let s = s.trim();
    if s.len() >= 2 && s.starts_with('"') && s.ends_with('"') {
        let inner = &s[1..s.len() - 1];
        let mut out = String::with_capacity(inner.len());
        let mut chars = inner.chars();
        while let Some(c) = chars.next() {
            if c == '\\' {
                match chars.next() {
                    Some('\\') => out.push('\\'),
                    Some('"') => out.push('"'),
                    Some('n') => out.push('\n'),
                    Some('r') => out.push('\r'),
                    Some('t') => out.push('\t'),
                    Some(other) => {
                        out.push('\\');
                        out.push(other);
                    }
                    None => out.push('\\'),
                }
            } else {
                out.push(c);
            }
        }
        Some(out)
    } else {
        None
    }
}

fn json_escape(s: &str) -> String {
    let mut out = String::with_capacity(s.len() + 2);
    out.push('"');
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c if (c as u32) < 0x20 => {
                out.push_str(&format!("\\u{:04x}", c as u32));
            }
            c => out.push(c),
        }
    }
    out.push('"');
    out
}

fn parse_cargo_lock(content: &str) -> (Vec<Package>, HashMap<String, String>) {
    let mut packages = Vec::new();
    let mut metadata = HashMap::new();
    let mut current: Option<Package> = None;
    let mut in_metadata = false;

    for raw_line in content.lines() {
        let line = raw_line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        if line == "[[package]]" {
            if let Some(pkg) = current.take() {
                packages.push(pkg);
            }
            current = Some(Package::default());
            in_metadata = false;
            continue;
        }
        if line == "[metadata]" {
            if let Some(pkg) = current.take() {
                packages.push(pkg);
            }
            in_metadata = true;
            continue;
        }
        if line.starts_with('[') {
            if let Some(pkg) = current.take() {
                packages.push(pkg);
            }
            in_metadata = false;
            continue;
        }
        let Some((k, v)) = line.split_once('=') else {
            continue;
        };
        let key = k.trim();
        let Some(val) = unquote_toml(v) else {
            continue;
        };
        if let Some(pkg) = current.as_mut() {
            match key {
                "name" => pkg.name = val,
                "version" => pkg.version = val,
                "source" => pkg.source = Some(val),
                "checksum" => pkg.checksum = Some(val),
                _ => {}
            }
        } else if in_metadata {
            if let Some(unquoted_key) = unquote_toml(key) {
                metadata.insert(unquoted_key, val);
            }
        }
    }
    if let Some(pkg) = current.take() {
        packages.push(pkg);
    }

    (packages, metadata)
}

fn generate(lock_path: &Path, out_path: &Path) -> Result<usize, String> {
    let lock_text = fs::read_to_string(lock_path)
        .map_err(|e| format!("failed to read {}: {e}", lock_path.display()))?;
    let (packages, metadata) = parse_cargo_lock(&lock_text);

    let mut entries = Vec::new();
    for pkg in packages {
        let Some(ref source) = pkg.source else {
            continue;
        };
        if source.starts_with("git+") {
            return Err(format!(
                "git dependency `{}` found; use upstream flatpak-cargo-generator",
                pkg.name
            ));
        }
        let meta_key = format!("checksum {} {} ({source})", pkg.name, pkg.version);
        let checksum = pkg
            .checksum
            .as_deref()
            .or_else(|| metadata.get(&meta_key).map(String::as_str))
            .ok_or_else(|| format!("missing checksum for {} {}", pkg.name, pkg.version))?;

        let dest = format!("{CARGO_CRATES}/{}-{}", pkg.name, pkg.version);
        let url = format!(
            "{CRATES_IO}/{}/{}-{}.crate",
            pkg.name, pkg.name, pkg.version
        );
        let inline_json = format!("{{\"package\": \"{checksum}\", \"files\": {{}}}}");

        entries.push(format!(
            concat!(
                "    {{\n",
                "        \"type\": \"archive\",\n",
                "        \"archive-type\": \"tar-gzip\",\n",
                "        \"url\": {},\n",
                "        \"sha256\": {},\n",
                "        \"dest\": {}\n",
                "    }}"
            ),
            json_escape(&url),
            json_escape(checksum),
            json_escape(&dest),
        ));
        entries.push(format!(
            concat!(
                "    {{\n",
                "        \"type\": \"inline\",\n",
                "        \"contents\": {},\n",
                "        \"dest\": {},\n",
                "        \"dest-filename\": \".cargo-checksum.json\"\n",
                "    }}"
            ),
            json_escape(&inline_json),
            json_escape(&dest),
        ));
    }

    let config_toml = format!(
        "[source.{VENDORED_SOURCES}]\ndirectory = \"{CARGO_CRATES}\"\n\n[source.crates-io]\nreplace-with = \"{VENDORED_SOURCES}\"\n"
    );
    entries.push(format!(
        concat!(
            "    {{\n",
            "        \"type\": \"inline\",\n",
            "        \"contents\": {},\n",
            "        \"dest\": {},\n",
            "        \"dest-filename\": \"config\"\n",
            "    }}"
        ),
        json_escape(&config_toml),
        json_escape(CARGO_HOME),
    ));

    let count = entries.len();
    let mut file = fs::File::create(out_path)
        .map_err(|e| format!("failed to write {}: {e}", out_path.display()))?;
    writeln!(file, "[\n{}\n]", entries.join(",\n")).map_err(|e| e.to_string())?;
    Ok(count)
}

fn main() {
    let args: Vec<String> = env::args().skip(1).collect();
    let lock_path = args
        .first()
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("Cargo.lock"));
    let out_path = args
        .get(1)
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("packaging/flatpak/cargo-sources.json"));

    match generate(&lock_path, &out_path) {
        Ok(n) => println!("Wrote {n} source entries to {}", out_path.display()),
        Err(e) => {
            eprintln!("error: {e}");
            process::exit(1);
        }
    }
}
