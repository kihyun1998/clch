# Project artifacts: finding them and deleting them

## What it takes to delete one

The maintainer's decisions are in ADR-0005.

- **git failing inside a repository counts as tracked**, since a damaged `.git/index` or a refused ownership check says nothing about what is tracked. Outside any repository, its failure means nothing.
- **A kind's name used from a folder of that name, or from inside a project of that kind, stops the run.** `clean node_modules` means every one under the scan roots, which is not what someone standing in a project usually means, and `build` and `target` will make that likelier.
- **An age filter with no names cleans artifacts and no tool cache**, since a tool cache has no age to filter by.
- **A project with a part it cannot read is not picked by age**, because the newer file may be in that part.
- **Another disk mounted inside stops the deletion.** The walk that measures it does not enter that disk, but `remove_dir_all` would.
- **Deleting checks again.** Between the plan and the deletion the folder may have changed, so its marker file is looked for once more, and a folder that is now a link is refused.
- **Links inside are removed, not followed.** `std::fs::remove_dir_all` removes a symbolic link or junction itself and leaves its target; the tests pin this on each OS.
- **A plan warns about a pnpm workspace root**, recognised by `pnpm-workspace.yaml` beside `node_modules`, as a completeness read saw its members fail with `MODULE_NOT_FOUND`. Other workspaces are not recognised: their members keep their own `node_modules`, or that has not been checked.
- **A deletion that fails partway says to reinstall.** `remove_dir_all` stops at the first folder it cannot remove, having taken what came before it. A failure with the size unchanged says nothing more.
- **A long plan stays readable.** Its total and warnings come first, and each row's line fits 80 columns, with its command, reason and path beneath, as a 301-row table had wrapped every row and put its totals on line 304.
- **The cleanup level does not apply.** A project artifact is one folder that either goes or stays, so unused and full cleanup both delete it, and neither runs until selected. A plan of only project artifacts does not name a level.

## Where they are looked for

Only under scan roots, and within them:

- **A marker file must sit beside the folder, and each kind asks for more where a marker is not enough**, because `build` and `target` are also names people choose. Measured on the maintainer's projects, each of these holds for every real one, so a real artifact is not missed: `target` has `CACHEDIR.TAG` or `.rustc_info.json` (11 of 11), `.dart_tool` has `package_config.json` (28 of 28), `.venv` has `pyvenv.cfg` (1 of 1, and `python -m venv` documents it). No file marks a Flutter `build`, so its `pubspec.yaml` must have a `sdk: flutter` line (every real one has); a plain Dart package's `build` is not touched, even when `flutter:` appears in its description. A hand-made folder in a real Flutter project's `build` still goes, as `flutter clean` would take it.
- **A folder with its own `.git` is never an artifact.** A repository cloned as `target` beside a `Cargo.toml` holding a `CACHEDIR.TAG` would otherwise lose its unpushed commits.
- **Deleting each is what the tool itself would do or say.** `flutter clean` deletes `build` and `.dart_tool`, Dart's docs call `.dart_tool` "generally safe to delete", Python's call a venv "disposable", and `cargo clean` with no options deletes `target`. clch deletes them itself so that a missing tool does not stop it.
- **Nothing inside a project artifact is searched**, since deleting the outer one removes it anyway.
- **A `node_modules`, `.venv` or `.dart_tool` that is not an artifact is not searched either.** Packages inside a `node_modules` have `package.json` and `node_modules` of their own, and would otherwise be listed as projects. A `build` or `target` that is not an artifact may hold real projects, so it is searched.
- **Hidden folders, `~/Library`, `AppData` and tool caches are not searched.** The `node_modules` in them belong to editors, applications and the tools themselves (VS Code extensions, the npx cache), which break or re-download rather than rebuild.
- **A folder ending in `.app` is not searched**, as an application's `node_modules` under a scan root is the application's, and npkill treats such paths as dangerous. It costs a project named that way its listing, not a folder; a search of `/Volumes/T7/GitHub` four levels down found no such folder. A scan root named so is searched, as it was named.
- **Symbolic links are not entered**, so a scan stays inside what was named.

## Age

The newest file change in the artifact or the rest of its project, leaving out hidden folders and other artifacts, which change without anyone editing the project. A folder that only has an artifact's name counts as the project's own. Folders' own times are ignored. The figure is shown as last changed because that is all it is: a project in use without edits looks old. Installs were checked against this: npm writes fresh times, and pnpm from an old store keeps old times on most files but writes a few new ones (lens, one run of each).

## Scan roots

- **Roots in the config are absolute or start with `~/`.** A relative one would change meaning with the current folder.
- **Exclusions compare real locations**, so `--root ..` or a link to the home folder still keeps out of `~/Library`. A root inside an excluded folder is reported as unavailable.
- **Paths in history are compared as written.** A root spelled two ways, through a symlink for instance, gives two histories, because making paths canonical changes how they read on macOS (`/private/var`) and Windows (`\\?\`).
- **Changes to the config hold a lock file**, since a dashboard and a terminal can change it together: 64 additions at once kept only 31 to 47 of them before. A config file that is a link stays one, and keeps its permissions.
