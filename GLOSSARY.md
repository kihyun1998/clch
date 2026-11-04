# clch

A command-line tool that finds the disk space developer tooling leaves behind on Windows and macOS, records how it changes over time, and reclaims it safely.

## Language

### What is found

**Reclaimable**:
Anything clch can find and give space back from: a tool cache or a project artifact.
_Avoid_: item, target, entry

**Tool cache**:
A store a developer tool keeps per user, outside any project, holding data it can download or rebuild again, such as the npm cache or the pnpm store.
_Avoid_: global cache, system cache, junk

**Project artifact**:
A directory inside a project that the project's toolchain produced and can produce again, such as `node_modules` or Cargo's `target`.
_Avoid_: build cache, project cache, junk

**Marker file**:
The file that must sit beside a directory for it to count as a project artifact, such as `package.json` beside `node_modules`.
_Avoid_: manifest, indicator

**Scan root**:
A folder the user names as the place to look for project artifacts; nothing outside the scan roots is searched for them.
_Avoid_: search path, workspace

### Over time

**Snapshot**:
The sizes recorded by one scan, kept so that later scans can be compared against it.
_Avoid_: history entry, record

### Reclaiming

**Native cleanup**:
Reclaiming space by running the owning tool's own cleanup command rather than deleting its files directly.
_Avoid_: prune (one tool's word for it)

**Direct deletion**:
Reclaiming space by deleting a reclaimable's files without going through any tool.
_Avoid_: force delete, manual cleanup

**Unused cleanup**:
A cleanup that removes only what nothing currently uses, leaving what a project still needs.
_Avoid_: soft clean, light clean

**Full cleanup**:
A cleanup that removes everything in a reclaimable, whether in use or not.
_Avoid_: hard clean, deep clean, wipe
