# A full cleanup deletes a tool cache's folder where no command does it and the docs allow it

ADR-0001 kept direct deletion of a tool cache for when the tool is not on `PATH`, which did not cover a tool that is installed and has no command for the job. The maintainer decided on 2026-11-05 that when a tool has no command for a full cleanup, or is absent, and its official docs allow deleting the folder, a full cleanup deletes it, and an unused cleanup never does. It was shown the two cases: `cargo clean gc` fails as unstable on the stable channel while Cargo's docs say "you can always remove any part of the cache", and pnpm has no such command while its uninstall page gives `rm -rf "$(pnpm store path)"`.

## Considered Options

- Allowing Cargo only, as pnpm's words are about uninstalling. Rejected: both are the tool's own instruction for removing the folder.
- Keeping ADR-0001 as it was. Rejected: clch would list 1.2 GB of Cargo cache and have no way to reclaim it.

## Consequences

Only what the docs name is deleted, and only once it looks like that: pnpm's `v<number>` stores with a `files` folder, not the folder above them, and Cargo's `registry` and `git` holding nothing but their known entries. The first version deleted the whole folder a setting pointed at, which review reproduced with a `storeDir` pointing at `docs/` and a `CARGO_HOME` at a work folder.

Asked on 2026-11-05 about Xcode's derived data, the maintainer judged that Apple's own material counts as allowing it. They were shown that no Apple documentation page and no command for it were found, only Xcode 27's "Delete Derived Data" menu item and an Apple engineer in WWDC26's Xcode lab giving a full disk as a reason to delete it while saying it should not be needed. A further search found nothing more. So "the docs allow it" here includes what the tool's maker ships and says, not only its documentation pages.
