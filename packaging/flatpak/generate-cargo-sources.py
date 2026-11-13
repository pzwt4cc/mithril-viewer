#!/usr/bin/env python3
"""Generate Flathub cargo-sources.json from Cargo.lock using Python stdlib."""

import argparse
import json
import sys
import tomllib
from pathlib import Path

CRATES_IO = "https://static.crates.io/crates"
CARGO_HOME = "cargo"
CARGO_CRATES = f"{CARGO_HOME}/vendor"
VENDORED_SOURCES = "vendored-sources"


def generate(cargo_lock_path: Path) -> list[dict]:
    with cargo_lock_path.open("rb") as f:
        lock = tomllib.load(f)

    packages = lock.get("package", [])
    metadata = lock.get("metadata", {})
    sources: list[dict] = []

    for pkg in packages:
        source = pkg.get("source")
        if not source:
            continue
        if source.startswith("git+"):
            raise RuntimeError(
                f"Git dependency {pkg['name']} found; use upstream flatpak-cargo-generator.py instead."
            )

        name = pkg["name"]
        version = pkg["version"]
        checksum = pkg.get("checksum") or metadata.get(f"checksum {name} {version} ({source})")
        if not checksum:
            raise RuntimeError(f"Missing checksum for {name} {version}")

        sources.append(
            {
                "type": "archive",
                "archive-type": "tar-gzip",
                "url": f"{CRATES_IO}/{name}/{name}-{version}.crate",
                "sha256": checksum,
                "dest": f"{CARGO_CRATES}/{name}-{version}",
            }
        )
        sources.append(
            {
                "type": "inline",
                "contents": json.dumps({"package": checksum, "files": {}}),
                "dest": f"{CARGO_CRATES}/{name}-{version}",
                "dest-filename": ".cargo-checksum.json",
            }
        )

    sources.append(
        {
            "type": "inline",
            "contents": (
                f'[source.{VENDORED_SOURCES}]\n'
                f'directory = "{CARGO_CRATES}"\n\n'
                f'[source.crates-io]\n'
                f'replace-with = "{VENDORED_SOURCES}"\n'
            ),
            "dest": CARGO_HOME,
            "dest-filename": "config",
        }
    )
    return sources


def main() -> int:
    default_dir = Path(__file__).resolve().parent
    parser = argparse.ArgumentParser(description="Generate Flathub cargo-sources.json from Cargo.lock")
    parser.add_argument(
        "cargo_lock",
        nargs="?",
        type=Path,
        default=default_dir.parent.parent / "Cargo.lock",
        help="Path to Cargo.lock",
    )
    parser.add_argument(
        "-o",
        "--output",
        type=Path,
        default=default_dir / "cargo-sources.json",
        help="Output JSON path",
    )
    args = parser.parse_args()

    sources = generate(args.cargo_lock)
    with args.output.open("w", encoding="utf-8") as f:
        json.dump(sources, f, indent=4)
        f.write("\n")

    print(f"Wrote {len(sources)} source entries to {args.output}")
    return 0


if __name__ == "__main__":
    sys.exit(main())
