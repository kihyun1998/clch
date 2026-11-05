# thegraph

## What this project is

A Rust CLI for Windows and macOS that detects and tracks developer caches (npm, pnpm and others), cleans them, and carries a range of other features.

## References

| Source | Informs | Reached by | Binding |
|---|---|---|---|
| uv cache docs | how it works | docs.astral.sh/uv/concepts/cache — **summarized** | binding |
| Cargo build-cache docs | how it works | doc.rust-lang.org/cargo/reference/build-cache.html — **summarized** | binding |
| Cargo Home docs | how it works | doc.rust-lang.org/cargo/guide/cargo-home.html — **summarized** | binding |
| Docker `builder prune` / `daemon.json` builder gc | how it works | docs.docker.com — **summarized** | binding |
| `go help cache` | how it works | the `go help cache` output — raw | binding |
| npm `npm cache`, pnpm `pnpm store`, yarn `yarn cache` (Classic and Berry), pip `pip cache` | how it works | each tool's official CLI docs — **summarized** | binding |
| Apple Library Directory guide, Microsoft KNOWNFOLDERID (`FOLDERID_LocalAppData`) | how it works | developer.apple.com archive, learn.microsoft.com — **summarized** | binding |
| sasurahime (Rust, macOS cache cleaner) | how it works | its source tree, github.com/armaniacs/sasurahime — raw | example |
| cacheferret (Python cache cleaner) | how it works | its PyPI page — **summarized**; no source repo found | example |
| kondo (Rust, project build-dir cleaner) | how it works | its source tree, github.com/tbillington/kondo — raw | example |
| Homebrew `brew cleanup --help` | how it works | the installed `brew` — raw | binding |
| Dart `pub cache` docs and pub's `cache_gc.dart` | how it works | dart.dev/tools/pub/cmd/pub-cache — **summarized**; github.com/dart-lang/pub — raw | binding |
| Xcode: `xcrun simctl help`, WWDC26 Xcode Tips and Tricks Group Lab | how it works | the installed `xcrun` — raw; developer.apple.com/videos/play/wwdc2026/8013 — transcript | binding |
| Docker `image prune`, `system df`, docker/cli `formatter/disk_usage.go` | how it works | docs.docker.com — **summarized**; github.com/docker/cli — raw | binding |
| WSL: `compact vdisk`, `wsl-config`, `wsl.exe` help strings | how it works | learn.microsoft.com — **summarized**; github.com/microsoft/WSL `localization/strings/en-US/Resources.resw` — raw | binding |
| Corepack README | how it works | github.com/nodejs/corepack — raw | binding |
