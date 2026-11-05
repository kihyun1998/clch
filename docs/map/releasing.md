# Releasing

A tag `v<version>` runs `.github/workflows/release.yml`. It builds the macOS (arm64, x86_64) and Windows (x86_64) binaries, packages the crate and installs it with no Node on `PATH`, then makes a GitHub Release holding the archives and a rendered Homebrew formula and Scoop manifest.

The version in `Cargo.toml` must match the tag, or the crate step looks for a folder that is not there.

## By hand

These take a person's credentials, so the workflow does not do them:

- `cargo publish`, from a tree where `ui/dist` is built. It checks the same packaging.
- Copying `clch.rb` from the release into the tap repository as `Formula/clch.rb`.
- Copying `clch.json` into the bucket repository as `bucket/clch.json`. Its `checkver` and `autoupdate` let Scoop's tools find later versions.

Homebrew resolves `brew install <owner>/clch/clch` to a repository named `homebrew-clch`.

## README

The README gives only the build from source until a release exists, as its install commands would otherwise fail. With the first release, it lists these, which #16 asks for:

```sh
brew install kihyun1998/clch/clch
scoop bucket add clch https://github.com/kihyun1998/scoop-clch
scoop install clch/clch
cargo install clch
```

and a binary for each OS from GitHub Releases. The repository names `homebrew-clch` and `scoop-clch` are assumed, and neither exists yet.

## Not yet run

The workflow has never run: it needs a tag. The rendered files were checked with `ruby -c`, as JSON and for their hashes, not by installing them.
