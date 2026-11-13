# Flatpak / Flathub Packaging

Manifest and build helpers for `io.github.pzwt4cc.Mithril`.

## Files

- `io.github.pzwt4cc.Mithril.yml` — Flathub manifest targeting `org.freedesktop.Platform` `25.08` + `org.freedesktop.Sdk.Extension.rust-stable`
- `cargo-sources.json` — offline Cargo crate sources generated from `../../Cargo.lock`
- `generate-cargo-sources.rs` + `generate-cargo-sources.sh` — std-only Rust generator to refresh `cargo-sources.json` after updating `Cargo.lock`
- `flathub.json` — architecture configuration (`x86_64`, `aarch64`)

## Local Build & Validation

1. Install the SDK, runtime, Rust extension, and `org.flatpak.Builder`:

```sh
flatpak install flathub \
  org.freedesktop.Platform//25.08 \
  org.freedesktop.Sdk//25.08 \
  org.freedesktop.Sdk.Extension.rust-stable//25.08 \
  org.flatpak.Builder
```

2. To test a local checkout without pushing a tag, temporarily replace the `type: git` source in `io.github.pzwt4cc.Mithril.yml` with `type: dir` pointing to `../..`, then run:

```sh
flatpak run org.flatpak.Builder --force-clean --user --install --repo=repo build-dir io.github.pzwt4cc.Mithril.yml
flatpak run io.github.pzwt4cc.Mithril
```

3. Run the Flathub linter before submitting:

```sh
flatpak run --command=flatpak-builder-lint org.flatpak.Builder manifest io.github.pzwt4cc.Mithril.yml
flatpak run --command=flatpak-builder-lint org.flatpak.Builder repo repo
```

## Checklist Before Opening a Flathub PR

1. **Release Tag:** Tag a release (`git tag v0.1.0 && git push --tags`) so the `<screenshots>` URLs in `data/io.github.pzwt4cc.Mithril.metainfo.xml` (`v0.1.0`) resolve on GitHub, and replace `REPLACE_WITH_TAG_COMMIT_SHA` in `io.github.pzwt4cc.Mithril.yml` with the exact commit hash of that tag.
2. **Refresh `cargo-sources.json`:** Whenever `Cargo.lock` changes, run `./generate-cargo-sources.sh`.
3. **Full Host Filesystem Access (Optional):** The default `finish-args` allow standard XDG user directories (`xdg-pictures`, `xdg-download`, `xdg-documents`, `xdg-desktop`, `/run/media`) so `flatpak-builder-lint` passes without exemptions. If you want `←`/`→` folder browsing across arbitrary host paths (similar to `org.gnome.Loupe`), replace those directory entries with `--filesystem=host` and request the exception in your Flathub submission.
