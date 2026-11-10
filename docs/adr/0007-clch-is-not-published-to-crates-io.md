# clch is not published to crates.io after 0.1.0

The maintainer decided on 2026-11-10 that clch is released through GitHub Releases, Homebrew and Scoop, and not through crates.io. Version 0.1.0 is on crates.io and cannot be removed; no later version is published there. The reason they gave was that a further channel is not worth keeping up.

They were shown three ways to publish 0.1.1 and later: by hand with `cargo publish --allow-dirty`, which takes their credentials each time; from the release workflow with crates.io's Trusted Publishing, which needs a trusted publisher set up on crates.io by a crate owner and a publish job with `id-token: write`; and not publishing.

## Considered Options

- Publishing by hand each release. Rejected: it keeps a step in `releasing.md` that only the maintainer's credentials can do, for a channel they do not want to look after.
- Trusted Publishing from `release.yml`. Rejected for the same reason. It was read in crates.io's announcement of 2025-07-11 and RFC 3691: GitHub Actions only, set up after a first manual publish (already done for 0.1.0), a short-lived token that publishes new versions of an existing crate. Not found in what was readable: whether other triggers are refused, and whether an API token can be switched off beside it. Publishing would also happen on a tag push without a person looking at the release first, unless an approval environment were added.
- Yanking 0.1.0. Not done; it is the maintainer's call and can be undone.

## Consequences

`cargo install clch` installs 0.1.0 and will not get later versions, so the README no longer lists it as a way to install. `cargo install --path .` from a clone stays.

`publish = false` in `Cargo.toml` makes `cargo publish` refuse, so a release cannot reach crates.io by a mistake. It does not affect `cargo package` or `cargo install --path`. The `crate` job in the release workflow stays, as a check that the packaged crate builds and installs with no Node on `PATH`.

Reversing this means removing `publish = false` and choosing one of the two ways above.
