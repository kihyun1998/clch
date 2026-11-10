# clch

Find the disk space developer tooling leaves behind on Windows and macOS, see how it changes over time, and reclaim it safely.

## Install

macOS (Apple silicon and Intel) and Windows (x86_64) are supported.

Homebrew:

```sh
brew tap kihyun1998/tap
brew trust kihyun1998/tap
brew install clch
```

Scoop:

```powershell
scoop bucket add kihyun1998 https://github.com/kihyun1998/scoop-bucket
scoop install clch
```

Each OS also has a binary on the [releases page](https://github.com/kihyun1998/clch/releases).

To build from this repository, you need Rust and Node.js with npm, the latter for the dashboard's UI:

```sh
git clone https://github.com/kihyun1998/clch
cd clch
cargo install --path .
```

## Usage

```sh
clch scan          # list every reclaimable with its size and its change since the last scan
clch scan --json   # the same, as JSON
clch clean         # show what cleaning would do; nothing is removed
clch clean --yes   # clean: only what nothing uses, through each tool's own command
clch clean npm-cache --full --yes   # remove all of one reclaimable; see the ids with `clch scan --json`
clch clean pnpm-store --yes        # prune the pnpm store on each disk a scan root is on
clch clean xcode-derived-data --full --yes   # delete Xcode's derived data
clch clean docker-images --full --yes   # remove every image no container uses; volumes and containers are never touched

clch roots add ~/code              # search a folder for project artifacts
clch scan --older-than-days 30     # list only artifacts in projects not changed for 30 days
clch clean node_modules            # show which node_modules would be deleted
clch clean node_modules --yes      # delete them, permanently
clch clean ~/code/app/node_modules --yes        # delete one
clch clean --older-than-days 30 --yes           # delete artifacts in projects not changed for 30 days
```

Project artifacts are deleted only when you name them or pick them by age; `clch clean --yes` alone leaves them alone. Deleting is permanent, since moving them to the Trash would give no space back. A folder counts as a project artifact only when its marker file sits beside it, such as `package.json` beside `node_modules`, and each kind asks for its own further evidence, such as `pyvenv.cfg` in a `.venv`. One git tracks files in, or that is a repository itself, is skipped. The kinds are `node_modules`, `target`, `.venv`, `.dart_tool`, and Flutter's `build`.

A cleanup that fails is reported with the tool's output and makes `clch clean` exit with status 1; the others still run.

## Dashboard

```sh
clch dashboard            # open it in the browser; Ctrl+C stops it
clch dashboard --no-open  # print the URL instead
```

It shows the same reclaimables per disk with their growth over time, and records a snapshot on each scan. Click what to clean, preview the same plan `clch clean` would print, and confirm. Scan roots can be changed there too. It listens only on 127.0.0.1, on a free port, and its page and every API request need the token in the URL it prints.

## License

Licensed under either of [Apache License, Version 2.0](LICENSE-APACHE) or [MIT license](LICENSE-MIT) at your option.
