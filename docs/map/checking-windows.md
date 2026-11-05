# Checking Windows from a Mac

CI runs everything on `windows-latest`, and nothing here replaces that. This catches what would fail there before pushing.

`cargo clippy --target x86_64-pc-windows-msvc` stops on the Mac because rusqlite's `bundled` feature compiles SQLite's C code, for which there is no cross-compiler here. Type checking needs no C, so check a copy of the crate without that feature:

```sh
copy="$(mktemp -d)"
cp -R src tests Cargo.toml Cargo.lock README.md "$copy/"
sed -i '' 's/features = \["bundled"\]/features = []/' "$copy/Cargo.toml"
(cd "$copy" && cargo clippy --all-targets --target x86_64-pc-windows-msvc -- -D warnings)
```

It needs `rustup target add x86_64-pc-windows-msvc` once. It covers `#[cfg(windows)]` code and tests, and it runs nothing: how Windows behaves is known only from CI. CI's clippy is also newer than a local one can be, so a pass here does not promise a pass there.
