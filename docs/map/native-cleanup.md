# Native cleanup: which tool command serves which cleanup level

clch reclaims a tool cache through the tool's own command (ADR-0001). The catalog maps each tool cache to the command for each cleanup level, or to none. This note holds why each mapping is what it is; the catalog holds only the commands.

## npm cache (`<cache>/_cacache`)

- **Unused cleanup is `npm cache verify`.** It garbage-collects content that npm's cache index no longer refers to, removes content that fails its integrity check and clears `tmp/` (npm 11.6.2, `cacache/lib/verify.js`). It never removes anything the index refers to, so it removes only what nothing uses, as the glossary defines unused cleanup. It is conservative: indexed tarballs that no project needs stay. How much it frees cannot be known before it runs, so the plan shows no figure for it.
- **Full cleanup is `npm cache clean --force`.** It removes `_cacache` and nothing else; the npx cache beside it stays (npm 11.6.2, `lib/commands/cache.js`).

## npx cache (`<cache>/_npx`)

- **No unused cleanup.** npm's commands for the npx cache are `npm cache npx ls`, `rm <key>…` and `rm --force` (everything); none removes only unused entries.
- **Full cleanup is `npm cache npx rm --force`, from npm 11.2.0.** `npm cache npx` does not exist in npm 10.9.2, 11.0.0 or 11.1.0 and first appears in 11.2.0 (npm/cli `lib/commands/cache.js` at each tag). Older npm is told the version it needs instead of being run and failing.

## pnpm store

- **One store per disk.** pnpm's docs: "pnpm creates one store per disk", hard-linked into `node_modules`; on a disk other than the one with the pnpm home, "a `.pnpm-store` directory near the root of the disk". On the maintainer's Mac, `~/Library/pnpm/store` is empty and `/Volumes/T7/.pnpm-store` holds 4.5 GB. So clch asks from the home folder, and looks for `.pnpm-store` at the root of each disk a scan root is on. A disk no scan root is on has its store go unseen.
- **pnpm is never run inside a project.** Asking from a scan root ran that folder's `.pnpmfile.cjs`, downloaded the pnpm version its `packageManager` pinned (114 MB in one scan, measured), and followed its `storeDir`. So a scan could run code from a cloned repository. On another disk, pnpm runs from inside `.pnpm-store`, where it still reports that disk's store (pnpm 10.33.0). A store a project configures for itself is not found, which matches the glossary: a tool cache lives outside any project.
- **The reclaimable is the folder holding every version**, not the `v10` pnpm reports. Otherwise an upgrade to `v11` hides what `v10` still occupies and breaks the history. `pnpm store prune` only touches the current version, so a leftover version stays listed.
- **Unused cleanup is `pnpm store prune`**: it removes unreferenced packages, which pnpm's docs call not harmful. It also removes pnpm's metadata cache.
- **Full cleanup deletes each `v<number>` folder that has `files` in it** (ADR-0006), and nothing else there. pnpm has no command for it; its uninstall page gives `rm -rf "$(pnpm store path)"`. A setting can point `storeDir` at any folder, so the folder itself is never deleted. Projects should keep working, since their files are hard links, and later installs download again; that was not tried with pnpm's global virtual store.
- **Its identity is its path**, since a machine can have several.

## yarn caches

Classic and Berry are separate reclaimables, because a cache left by the one not on `PATH` is still on disk, and neither can clean the other's.

- **Which runs is decided by `yarn --version`**, from the home folder. A project pinning another version is not consulted.
- **Classic's only cleanup is full, `yarn cache clean`**: per its docs, it clears the global cache.
- **Berry's is `yarn cache clean --all`.** The docs say `--mirror` removes the global cache, but with the default `enableGlobalCache: true` it does not: measured on yarn 4.18.1, a plain `cache clean` and `--all` emptied `~/.yarn/berry/cache`, and `--mirror` left it. In its source, the mirror is `null` when the global cache is the cache in use. `--all` covers both settings.
- **Default locations come from their sources**: Classic's `getCacheDir` plus `v6`; Berry's `globalFolder` plus `cache`, with the XDG variables those honour.
- **A path a tool prints must be absolute to be believed.** Berry prints `undefined` for `cacheFolder` when `enableGlobalCache` is false in the home folder (measured on 4.18.1).

## pip and uv

- **pip has only a full cleanup**, `pip cache purge`. Its other command removes named packages, which is not the same as what nothing uses.
- **`pip3` stands in for `pip`**, which Homebrew's Python does not install.
- **uv has both**: `uv cache prune` removes unused entries, and `uv cache clean` everything. Its docs: "it's _never_ safe to modify the cache directly", so without uv its cache is left alone.
- **pip's docs do not allow deleting by hand**, and call the cache's layout an implementation detail.

## Cargo

- **No command.** `cargo clean gc` exists but fails on the stable channel as unstable (cargo 1.97), so `cargo` is not run at all.
- **A full cleanup deletes the folder** (ADR-0006). Cargo Home's docs: "you can always remove any part of the cache and Cargo will do its best to restore sources".
- **`registry` and `git` are the reclaimables**, not all of Cargo Home, which also holds installed binaries and credentials. Either is deleted only while it holds nothing but Cargo's own entries, since `CARGO_HOME` can point at a folder with other things called `git`.
- **Deleting during a build breaks that build.** Measured: `cargo build` failed with status 101 when its git cache went mid-compile; the next build succeeded. Nothing checks for a running build.

## Go

- **`GOTOOLCHAIN=local` is set on every go command.** A `go.mod` in the home folder or `GOTOOLCHAIN` set to a version made `go env` download a toolchain: 330 MB and 28 seconds in one measurement, cut short by the 10-second limit in another, leaving part of it behind.
- **Only full cleanups**: `go clean -cache` and `go clean -modcache`. The go command deletes unused build cache entries itself. Its docs do not allow deleting by hand, and most of the module cache is read-only.

## How tools are run

- **From the home folder, never from a project**, whose folder can hold settings and code a tool acts on; the home folder and a `.pnpm-store` are the only places tools run. npm reads the `.npmrc` of the folder it runs in, so a project could otherwise redirect the cache npm reports or cleans.
- **Questions time out; cleanups do not.** A question such as `npm config get cache` is abandoned after 10 seconds, its whole process tree ended. A cleanup may legitimately run for a long time on a large cache, so it runs to completion; it shares clch's process group, so a Ctrl+C in the terminal stops the tool as well as clch. Each one's command is printed to stderr as it starts, since a tool can wait silently: `uv cache clean` waits for whichever `uv run` holds its lock (measured 19 and 21 seconds behind a 25-second run).
- **git is asked with `core.fsmonitor` off.** `git ls-files` in a repository with `core.fsmonitor` in its `.git/config` runs that command (measured), and a downloaded archive can carry such a `.git`.
