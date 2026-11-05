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
- **Deleting during a build breaks that build, and the plan says so.** Measured: `cargo build` failed with status 101, "could not execute process rustc", when its git cache went mid-compile; the next build succeeded. A running build cannot be detected from the lock: `lsof` showed nothing holding `.package-cache` during that compile, as Cargo takes it to fetch, not to build. Cargo's source for the lock was not read.

## Go

- **`GOTOOLCHAIN=local` is set on every go command.** A `go.mod` in the home folder or `GOTOOLCHAIN` set to a version made `go env` download a toolchain: 330 MB and 28 seconds in one measurement, cut short by the 10-second limit in another, leaving part of it behind.
- **Only full cleanups**: `go clean -cache` and `go clean -modcache`. The go command deletes unused build cache entries itself. Its docs do not allow deleting by hand, and most of the module cache is read-only.

## Docker

- **Sizes are Docker's own report**, since the data is in the engine's virtual disk and clch cannot walk it, and they are rounded to four digits (`units.HumanSize` in docker/cli), so a change between scans is that coarse.
- **Unused images are the `UniqueSize` of images with no container, from `docker system df --verbose`.** The `Reclaimable` of the plain report is not used: measured here, 2.18 GB with all three images in use, while `image prune --all` would remove none of them. Sizes an unused image shares with another are not counted, so what a full cleanup frees can be more than shown.
- **The build cache is `Reclaimable` in `docker system df`.** It can read 0 with `docker buildx du` showing gigabytes: measured here, 0B beside 3.186 GB shared with an image in use, which pruning would not give back. At 0 a plain `clch clean --yes` passes over it.
- **Images: unused cleanup is `docker image prune`, full is `--all`.** Docker's docs: without `-a` only dangling images, with it "all images not referenced by any container". Measured on labelled probe images with `--filter`: the plain command left tagged images, `--all` removed them, and images of containers stayed. The filter is the one difference from what clch runs.
- **Build cache: unused cleanup is `docker builder prune`, full is `--all`.** On this machine it is `buildx prune`, whose source warns "This will remove all dangling build cache", and with `--all` "all build cache"; its help words `--all` differently ("Include internal/frontend images"). Not measured: running it clears the real cache. It acts on the current builder, which `system df` may not describe with a `docker-container` builder.
- **`--force` is required.** Both ask for confirmation, and tools run without a stdin.
- **Volumes and containers have no entry**, which is the whole of how they are never offered: they hold data no tool can produce again.
- **An engine not on this machine is left alone.** `docker context inspect` names the endpoint (it follows `DOCKER_HOST` and `DOCKER_CONTEXT`, measured), and anything but `unix://` or `npipe://` gets a note and no row, so a shell pointed at a shared server cannot prune it. A glossary tool cache is the user's own, and it would give this machine no space. One it cannot place is treated the same.
- **A disk is named only for Docker Desktop.** `docker info` calling the engine `Docker Desktop` and the folder Docker's docs give for its disk image (`~/Library/Containers/com.docker.docker/Data/vms/0/data`, `%LOCALAPPDATA%\Docker\wsl`) being there places it on that folder's disk. Otherwise, as with a folder moved in Docker Desktop's settings, the disk is unknown and the path reads `docker engine`, which is no path: it names no disk and no `clean` argument matches it. The Windows engine name has not been read on a real machine.
- **A daemon that does not answer leaves no row and a note** (`tool_notes`). A size of zero would record a drop and then a matching rise, and the rows are not reported gone while the note stands. That scan's disk totals leave Docker out, which #13's per-disk history will show as a dip.
- **A cleanup whose after-scan has no answer from Docker frees nothing on paper**, marked as an estimate, since a row missing there says nothing about the data.
- **What a cleanup freed is what Docker's report dropped by**, not the host's free space, which can stay as it was for a while.
- **On Windows the result explains compaction.** Microsoft's `compact vdisk` page: dynamically expanding VHDs "do not automatically reduce in size when you delete files", and it works only on one detached or attached read-only, hence quitting Docker Desktop and `wsl --shutdown` first. `wsl.exe`'s own help says `--set-sparse` makes the VHD "allowing disk space to be automatically reclaimed" and that sparse VHDs "are currently experimental", and `wsl-config`'s `sparseVhd` applies to newly created VHDs, so the text says to check first. Docker's release notes for 4.59.0 say "Temporarily disabled VHDX compaction for the WSL data disk", so Docker Desktop may or may not do it itself; later notes do not say it came back. `wsl --manage <distro> --compact` is in WSL's source but in no doc read, and Docker's data disk is not shown to be a distro's own VHD, so diskpart is what is given. The disk's file name is not in Docker's docs, so the text names the folder. None of this was run on a real Windows machine; Microsoft also recommends against touching WSL's files in `AppData` with Windows tools, and clch only guides, per the spec.

## pub cache

- **Unused cleanup is `dart pub cache gc --force`, full is `dart pub cache clean --force`.** `gc` keeps what the projects pub knows of still use, and anything changed in the last 2 hours (pub's `cache_gc.dart`), read here with `--dry-run`, which found 32 projects. On an older Dart without `gc`, the run fails with Dart's own message. No version was pinned, since `dart --version` does not print a bare number and the first version with `gc` was not read.
- **`gc` counts a project it cannot read as using nothing**, per its source, so a project on a disk not attached can lose its packages, which `dart pub get` brings back.
- **A full cleanup also takes what `dart pub global activate` installed**, since `clean` removes the whole folder and both are in it, as measured on a temp `PUB_CACHE`.
- **There is no command that says where it is.** `PUB_CACHE` is the setting and the defaults are in Dart's glossary, `~/.pub-cache` and `%LOCALAPPDATA%\Pub\Cache`, "though its exact location might vary depending on the Windows version". Windows has not been measured.
- **Without dart, a full cleanup deletes it** (ADR-0006). Dart's docs: "you can manually delete the PUB_CACHE folder". Only while it has `hosted` and holds nothing but pub's entries, listed as measured here.

## Homebrew cache

- **Unused cleanup is `brew cleanup --scrub`, full is `brew cleanup --prune=all`.** Help: `--scrub` "including downloads for even the latest versions. Note that downloads for any installed formulae or casks will still not be deleted"; `--prune` "If you want to remove everything, use --prune=all". Dry runs here: 24.2 MB and 160.2 MB of a 197 MB cache, the second including a download for an installed formula. Together they printed nothing, so they are not combined. Both also remove old versions of installed formulae, which is outside the cache and comes with the command.
- **Without brew it is not deleted.** The help's `rm -rf "$(brew --cache)"` would allow it under ADR-0006, but the folder has no fixed set of names to recognise, and a first version deleting `downloads` and `api` from wherever `HOMEBREW_CACHE` pointed took a project's folders in review.
- **Detected only on macOS**, per the spec. Windows has no Homebrew.

## Xcode data

- **Derived data has no command.** `xcodebuild clean` is per project. A full cleanup deletes the folder: the maintainer judged Xcode 27's "Delete Derived Data" menu item and what Apple's engineers said in WWDC26's Xcode lab as allowing it under ADR-0006; see that record. Only a folder called `DerivedData` that git tracks nothing in goes, because `IDECustomDerivedDataLocation` can point at a folder of the user's. A location Xcode keeps relative to a project is not an absolute path, so it is not believed and the default is looked at. No derived data was on the maintainer's Mac to measure, so a rebuild afterwards is taken from forum answers, not seen.
- **Simulators get `xcrun simctl delete unavailable` at either level**, which its help describes as devices "not supported by the current Xcode SDK". `simctl delete all` would take the apps and their data in every simulator, so `--full` does not run it (maintainer's call, 2026-11-05). Runtimes (`simctl runtime delete`) are not listed: they live outside the home folder, and there were none here.
- **iOS device support is shown and not cleaned.** No command and no doc was found, so ADR-0006's rule leaves it. Other platforms' device support is not listed; it was not seen here.
- **Archives are never listed.** They hold what shipped, with its symbols, and nothing can produce them again, so they are not reclaimables.

## How tools are run

- **From the home folder, never from a project**, whose folder can hold settings and code a tool acts on; the home folder and a `.pnpm-store` are the only places tools run. npm reads the `.npmrc` of the folder it runs in, so a project could otherwise redirect the cache npm reports or cleans.
- **Questions time out; cleanups do not.** A question such as `npm config get cache` is abandoned after 10 seconds, its whole process tree ended. A cleanup may legitimately run for a long time on a large cache, so it runs to completion; it shares clch's process group, so a Ctrl+C in the terminal stops the tool as well as clch. Each one's command is printed to stderr as it starts, since a tool can wait silently: `uv cache clean` waits for whichever `uv run` holds its lock (measured 19 and 21 seconds behind a 25-second run).
- **git is asked with `core.fsmonitor` off.** `git ls-files` in a repository with `core.fsmonitor` in its `.git/config` runs that command (measured), and a downloaded archive can carry such a `.git`.
