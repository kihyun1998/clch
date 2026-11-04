# Direct deletion is permanent, not a move to the Trash

When clch deletes files itself (project artifacts, and tool caches under ADR-0001's exception), it removes them permanently instead of moving them to the Trash or Recycle Bin. A move to the Trash gives no space back until the Trash is emptied, which defeats a tool whose purpose is reclaiming space, and everything clch deletes is by definition something a tool can produce again. The safety net is the preview: `clch clean` only prints what it would remove unless given `--yes`, and the dashboard shows the same preview before a confirm button.

## Considered Options

- Trash by default with a `--permanent` flag, as sasurahime does. Rejected because the space is not reclaimed until the user empties the Trash.
