#!/bin/sh
set -eu
DIR=$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)
ROOT=$(CDPATH= cd -- "$DIR/../.." && pwd)
BIN=$(mktemp /tmp/mithril-cargo-gen.XXXXXX)
trap 'rm -f "$BIN"' EXIT
rustc --edition 2021 -O "$DIR/generate-cargo-sources.rs" -o "$BIN"
exec "$BIN" "${1:-$ROOT/Cargo.lock}" "${2:-$DIR/cargo-sources.json}"
