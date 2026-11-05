# Adding a kind of reclaimable

A kind is an entry in `src/catalog.rs`; scan, clean, snapshots and output pick it up from there. What follows are the rules the existing kinds were made to obey, each after a measured failure described in the note it links to.

## A tool cache

- **Ask the tool where its cache is, from the home folder**, through `Env::tool_path`, which believes only an absolute path. Fall back to the tool's documented default, read from its source or docs, including the environment variables it honours, through `asked`, which records that the tool did not say. See [Native cleanup](native-cleanup.md).
- **Never run a tool inside a project or scan root.** A project's folder can hold code and settings the tool acts on. `Located::run_from` is for a folder that holds none, such as `.pnpm-store`.
- **A scan must not make a tool download anything.** Set what stops it in `settings_for` in `src/env.rs`, as `GOTOOLCHAIN=local` does for go and `COREPACK_ENABLE_NETWORK=0` for corepack's shims. Check with a pinned version in the home folder.
- **Map each cleanup level to the tool's own command, or to none**, and confirm against the real CLI, with a file placed to see what goes: yarn's docs and its behaviour differ. Record why in the note.
- **`deletable` only where the tool's official docs allow deleting**, and name what looks like the cache, so that a setting pointing elsewhere deletes nothing (ADR-0006).
- **Say what else the command or deletion takes**, in `unused_warning` and `full_warning`, for the plan: it is the only safety net. See [Native cleanup](native-cleanup.md).
- **`one_per_disk` where each disk has its own**, which makes its path its identity.
- **A store clch cannot walk reports its size through `LocatedSize`**, as Docker does: `Reported` with the tool's figure, `Unavailable` with why not. See [Native cleanup](native-cleanup.md).

## A project artifact

- **Require evidence beyond the name and marker where one exists**, and measure it on real projects before relying on it. A wrong guess must lose a listing, not a folder. See [Project artifacts](project-artifacts.md).
- **`search_otherwise` is true only for names people also choose themselves**, such as `build`.

## Tests

Seam 1 in `tests/common/mod.rs` runs the binary in a sandbox. Fixture sizes are whole blocks (`write_file` and `blocks`), since a size is allocated space. `tool`, `prints`, `says` and `removes` make a fake tool, and `calls` shows what it was asked. On Windows a batch file must spell paths with `\`, and end each path of a fake with a top-level `exit /b`.

A probe of a real tool sets `HOME` and every variable the tool reads, such as `PNPM_HOME`, `CARGO_HOME`, `GOCACHE` and `UV_CACHE_DIR`, to a temp folder, and checks that it took before cleaning anything. `PNPM_HOME` beat a fake `HOME` once. Never run `clch clean --yes` on real folders.

**A fake tool's first run takes about 3 seconds on macOS.** Measured with 240 new scripts, 8 at a time: first run median 2959 ms, second 8 ms, and new scripts handed to `/bin/sh` rather than executed 7 ms, with `XprotectService` the busiest process during a test run. In parallel tests this reached clch's 10-second limit: over 20 runs of the suite, 90% of tool questions took up to 3.1 s, 99% up to 7.3 s, and 51 hit the limit, which was the intermittent failures once seen (#19). So the sandbox sets the limit to 120 seconds; a test about the limit sets its own with `limit_tools_to`. Why the first run is slow was inferred from that process and those timings; Apple's documentation was not read. Allow for it in any test waiting on a fake tool.
