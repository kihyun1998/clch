# The local dashboard can run cleanups, behind a per-run token

`clch dashboard` serves a web UI on 127.0.0.1 at a free port, and that UI can run cleanups, not only show sizes. A local HTTP server that deletes files can be reached by any web page open in the browser (cross-site requests, DNS rebinding), so every request must carry a token generated fresh for each run and passed in the URL that clch opens, and the server rejects requests whose `Origin` or `Host` is not its own. The server lives only until Ctrl+C.

## Considered Options

- A view-only dashboard with cleanup left to the CLI. Rejected because cleaning from the dashboard is part of what the dashboard is for.
