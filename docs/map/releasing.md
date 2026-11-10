# Releasing

A tag `v<version>` runs `.github/workflows/release.yml`. It builds the macOS (arm64, x86_64) and Windows (x86_64) binaries, packages the crate and installs it with no Node on `PATH`, then makes a GitHub Release holding the archives and a rendered Homebrew formula and Scoop manifest.

The version in `Cargo.toml` must match the tag, or the crate step looks for a folder that is not there.

## After the tag

- **Nothing goes to crates.io** (ADR-0007), and `publish = false` in `Cargo.toml` makes `cargo publish` refuse. The `crate` job still packages the crate and installs it, as a check that a source install works with no Node. It needs `--allow-dirty`, since `ui/dist` is ignored by git and carried in the crate, which Cargo counts as uncommitted.
- **Homebrew and Scoop update themselves.** clch lives in the shared `kihyun1998/homebrew-tap` and `kihyun1998/scoop-bucket`, as the maintainer's other CLI tools do. Each has an hourly `auto-update` workflow that reads the latest release and hashes its archives, and a `test-install` workflow that installs what changed.

The rendered `clch.rb` and `clch.json` on a release are not what those repositories use: their workflows write their own.

## What v0.1.0 checked

- The release workflow passed on its first run.
- `test-install` installed and ran clch from Homebrew on `macos-latest` and `macos-15-intel`, and from Scoop on `windows-latest`. A Scoop install on a real Windows machine worked too.
- The `auto-update` steps were run locally from a `0.0.0` seed against the release and wrote the files in use, with hashes matching the archives. They have not yet run in Actions against a new version.
