# Native cleanup: which tool command serves which cleanup level

clch reclaims a tool cache through the tool's own command (ADR-0001). The catalog maps each tool cache to the command for each cleanup level, or to none. This note holds why each mapping is what it is; the catalog holds only the commands.

## npm cache (`<cache>/_cacache`)

- **Unused cleanup is `npm cache verify`.** It garbage-collects content that npm's cache index no longer refers to, removes content that fails its integrity check and clears `tmp/` (npm 11.6.2, `cacache/lib/verify.js`). It never removes anything the index refers to, so it removes only what nothing uses, as the glossary defines unused cleanup. It is conservative: indexed tarballs that no project needs stay. How much it frees cannot be known before it runs, so the plan shows no figure for it.
- **Full cleanup is `npm cache clean --force`.** It removes `_cacache` and nothing else; the npx cache beside it stays (npm 11.6.2, `lib/commands/cache.js`).

## npx cache (`<cache>/_npx`)

- **No unused cleanup.** npm's commands for the npx cache are `npm cache npx ls`, `rm <key>…` and `rm --force` (everything); none removes only unused entries.
- **Full cleanup is `npm cache npx rm --force`, from npm 11.2.0.** `npm cache npx` does not exist in npm 10.9.2, 11.0.0 or 11.1.0 and first appears in 11.2.0 (npm/cli `lib/commands/cache.js` at each tag). Older npm is told the version it needs instead of being run and failing.

## How tools are run

- **From the home folder.** npm reads the `.npmrc` of the folder it runs in, so a project could otherwise redirect the cache npm reports or cleans.
- **Questions time out; cleanups do not.** A question such as `npm config get cache` is abandoned after 10 seconds, its whole process tree ended. A cleanup may legitimately run for a long time on a large cache, so it runs to completion; it shares clch's process group, so a Ctrl+C in the terminal stops the tool as well as clch.
