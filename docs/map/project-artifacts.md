# Project artifacts: finding them and deleting them

## What it takes to delete one

The maintainer's decisions are in ADR-0005.

- **A name that is both a kind and a folder here stops the run.** `clean node_modules` means every one under the scan roots, which is not what someone standing in a project usually means, and `build` and `target` will make that likelier.
- **An age filter with no names cleans artifacts and no tool cache**, since a tool cache has no age to filter by.
- **A project with a part it cannot read is not picked by age**, because the newer file may be in that part.
- **Another disk mounted inside stops the deletion.** The walk that measures it does not enter that disk, but `remove_dir_all` would.
- **Deleting checks again.** Between the plan and the deletion the folder may have changed, so its marker file is looked for once more, and a folder that is now a link is refused.
- **Links inside are removed, not followed.** `std::fs::remove_dir_all` removes a symbolic link or junction itself and leaves its target; the tests pin this on each OS.
- **The cleanup level does not apply.** A project artifact is one folder that either goes or stays, so unused and full cleanup both delete it, and neither runs until selected. A plan of only project artifacts does not name a level.

## Where they are looked for

Only under scan roots, and within them:

- **A marker file must sit beside the folder**, because folders such as `build` and `target` are also names people choose.
- **Nothing inside a project artifact is searched**, since deleting the outer one removes it anyway.
- **A `node_modules` without a marker is not searched either.** Packages inside it have `package.json` and `node_modules` of their own, and would otherwise be listed as projects.
- **Hidden folders, `~/Library`, `AppData` and tool caches are not searched.** The `node_modules` in them belong to editors, applications and the tools themselves (VS Code extensions, the npx cache), which break or re-download rather than rebuild.
- **Symbolic links are not entered**, so a scan stays inside what was named.

## Age

The newest file change in the artifact or the rest of its project, leaving out hidden folders and other artifacts, which change without anyone editing the project. Folders' own times are ignored. The figure is shown as last changed because that is all it is: a project in use without edits looks old. Installs were checked against this: npm writes fresh times, and pnpm from an old store keeps old times on most files but writes a few new ones (lens, one run of each).

## Scan roots

- **Roots in the config are absolute or start with `~/`.** A relative one would change meaning with the current folder.
- **Exclusions compare real locations**, so `--root ..` or a link to the home folder still keeps out of `~/Library`. A root inside an excluded folder is reported as unavailable.
- **Paths in history are compared as written.** A root spelled two ways, through a symlink for instance, gives two histories, because making paths canonical changes how they read on macOS (`/private/var`) and Windows (`\\?\`).
- **Changes to the config hold a lock file**, since a dashboard and a terminal can change it together: 64 additions at once kept only 31 to 47 of them before. A config file that is a link stays one, and keeps its permissions.
