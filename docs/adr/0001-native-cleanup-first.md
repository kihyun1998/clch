# Tool caches are reclaimed through the owning tool's own command

A tool cache is cleaned by running the tool's own cleanup command (`pnpm store prune`, `npm cache clean --force`, `uv cache prune`, `docker builder prune` and the like), not by deleting its files. The tool knows its own locks, indexes and which entries a project still uses; deleting files directly means re-implementing those rules and breaking the tool when they are wrong. Direct deletion of a tool cache happens only when the tool is not on `PATH` and that tool's official documentation says the cache may be removed outright; otherwise the cache is shown as not cleanable, with the reason.

## Considered Options

- Always delete files directly, moving them to the Trash, as sasurahime does. Rejected because it bypasses each tool's own rules.
