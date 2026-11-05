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
- **No full cleanup.** pnpm has no command for it. Its uninstall page gives `rm -rf "$(pnpm store path)"`, which is direct deletion, and ADR-0001 keeps that for when the tool is absent.
- **Its identity is its path**, since a machine can have several.

## yarn caches

Classic and Berry are separate reclaimables, because a cache left by the one not on `PATH` is still on disk, and neither can clean the other's.

- **Which runs is decided by `yarn --version`**, from the home folder. A project pinning another version is not consulted.
- **Classic's only cleanup is full, `yarn cache clean`**: per its docs, it clears the global cache.
- **Berry's is `yarn cache clean --all`.** The docs say `--mirror` removes the global cache, but with the default `enableGlobalCache: true` it does not: measured on yarn 4.18.1, a plain `cache clean` and `--all` emptied `~/.yarn/berry/cache`, and `--mirror` left it. In its source, the mirror is `null` when the global cache is the cache in use. `--all` covers both settings.
- **Default locations come from their sources**: Classic's `getCacheDir` plus `v6`; Berry's `globalFolder` plus `cache`, with the XDG variables those honour.
- **A path a tool prints must be absolute to be believed.** Berry prints `undefined` for `cacheFolder` when `enableGlobalCache` is false in the home folder (measured on 4.18.1).

## How tools are run

- **From the home folder, never from a project**, whose folder can hold settings and code a tool acts on; the home folder and a `.pnpm-store` are the only places tools run. npm reads the `.npmrc` of the folder it runs in, so a project could otherwise redirect the cache npm reports or cleans.
- **Questions time out; cleanups do not.** A question such as `npm config get cache` is abandoned after 10 seconds, its whole process tree ended. A cleanup may legitimately run for a long time on a large cache, so it runs to completion; it shares clch's process group, so a Ctrl+C in the terminal stops the tool as well as clch.
- **git is asked with `core.fsmonitor` off.** `git ls-files` in a repository with `core.fsmonitor` in its `.git/config` runs that command (measured), and a downloaded archive can carry such a `.git`.
