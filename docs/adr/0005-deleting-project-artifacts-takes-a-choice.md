# Deleting project artifacts takes a choice, and only what can be produced again goes

The maintainer decided three things on 2026-11-05, shown reproductions of what the first version deleted. A plain `clch clean --yes` cleans tool caches only: project artifacts are deleted when named by kind or path, or picked with `--older-than-days`, and a kind's name that is also a folder where clch runs stops with an error. Age is measured on the whole project, not the artifact alone, so a project edited today keeps an old `node_modules`; this replaces the spec's "artifact's last-modified time". A folder git tracks files in is skipped with the reason, because ADR-0002's permanent deletion rests on what is deleted being something a toolchain can produce again.

## Considered Options

- `--yes` deleting everything found: frees the most at once, and deletes the project being worked on.
- Age from the artifact alone, as the spec said: faster, and picks a project in active use whose dependencies have not changed.
- Deleting tracked folders once selected: simpler, and loses uncommitted edits to tracked files.

## Consequences

Scans read each project's files as well. Untracked hand edits inside a `node_modules` still cannot be detected, and a project used without edits still looks old. Reinstalling at a pnpm workspace root is needed after deleting its `node_modules`, since members link into it, and the plan says so.

The stop for a kind's name was later widened from a folder of that name to any folder inside a project of that kind, since from a workspace member `clean target --yes` reached every root. It is the same call, made to hold where it was first shown not to.
