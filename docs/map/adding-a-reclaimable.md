# Adding a kind of reclaimable

A kind is an entry in `src/catalog.rs`; scan, clean, snapshots and output pick it up from there. What follows are the rules the existing kinds were made to obey, each after a measured failure described in the note it links to.

## A tool cache

- **Ask the tool where its cache is, from the home folder**, through `Env::tool_path`, which believes only an absolute path. Fall back to the tool's documented default, read from its source or docs, including the environment variables it honours. See [Native cleanup](native-cleanup.md).
- **Never run a tool inside a project or scan root.** A project's folder can hold code and settings the tool acts on. `Located::run_from` is for a folder that holds none, such as `.pnpm-store`.
- **A scan must not make a tool download anything.** Set what stops it in `settings_for` in `src/env.rs`, as `GOTOOLCHAIN=local` does for go. Check with a pinned version in the home folder.
- **Map each cleanup level to the tool's own command, or to none**, and confirm against the real CLI, with a file placed to see what goes: yarn's docs and its behaviour differ. Record why in the note.
- **`deletable` only where the tool's official docs allow deleting**, and name what looks like the cache, so that a setting pointing elsewhere deletes nothing (ADR-0006).
- **`one_per_disk` where each disk has its own**, which makes its path its identity.

## A project artifact

- **Require evidence beyond the name and marker where one exists**, and measure it on real projects before relying on it. A wrong guess must lose a listing, not a folder. See [Project artifacts](project-artifacts.md).
- **`search_otherwise` is true only for names people also choose themselves**, such as `build`.

## Tests

Seam 1 in `tests/common/mod.rs` runs the binary in a sandbox. Fixture sizes are whole blocks (`write_file` and `blocks`), since a size is allocated space. `tool`, `prints`, `says` and `removes` make a fake tool, and `calls` shows what it was asked. On Windows a batch file must spell paths with `\`, and end each path of a fake with a top-level `exit /b`.

A probe of a real tool sets `HOME` and every variable the tool reads, such as `PNPM_HOME`, `CARGO_HOME`, `GOCACHE` and `UV_CACHE_DIR`, to a temp folder, and checks that it took before cleaning anything. `PNPM_HOME` beat a fake `HOME` once. Never run `clch clean --yes` on real folders.
