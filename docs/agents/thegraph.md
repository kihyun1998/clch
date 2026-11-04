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
