# clch

Find the disk space developer tooling leaves behind on Windows and macOS, see how it changes over time, and reclaim it safely.

## Usage

```sh
clch scan          # list every reclaimable with its size and its change since the last scan
clch scan --json   # the same, as JSON
clch clean         # show what cleaning would do; nothing is removed
clch clean --yes   # clean: only what nothing uses, through each tool's own command
clch clean npm-cache --full --yes   # remove all of one reclaimable
```

A cleanup that fails is reported with the tool's output and makes `clch clean` exit with status 1; the others still run.

## License

Licensed under either of [Apache License, Version 2.0](LICENSE-APACHE) or [MIT license](LICENSE-MIT) at your option.
