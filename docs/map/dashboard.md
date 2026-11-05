# Dashboard

`clch dashboard` serves a web UI over the same core as the CLI. ADR-0003 and ADR-0004 hold the decisions; this holds how they are carried out.

## Who it answers

- **Every request's `Host` must be exactly `127.0.0.1:<port>`.** That stops DNS rebinding, where another site's name resolves to this machine and the browser treats it as that site's own. `localhost` is refused too, since the URL printed never uses it.
- **An `Origin`, when sent, must be the server's own.** Browsers send one on cross-site POSTs, and on same-origin POSTs. A request with none is not from a page on another origin.
- **API requests carry the token in `X-Clch-Token`, not the query string.** A cross-site page cannot set that header without a preflight, which is never answered, and a header stays out of logs and history. The page's URL carries the token, so `Referrer-Policy: no-referrer` keeps it from leaving.
- **The page needs the token in its query string; its script and style files need none.** They hold nothing secret, and a browser cannot send a header when loading them. This is the one exception to ADR-0003's every request.
- **A second `Host` or `Origin` is refused**, since `tiny_http` keeps both and the first would otherwise decide.
- **A scan is a POST**, as it records a snapshot.
- **`127.0.0.1` is bound by the listener**, and no test can show a bind's address from outside. A port of 0 lets the system pick a free one.

## Cleaning

A CLI preview and its `--yes` are separate runs, so what runs can differ from what was shown. Here the server ties them together.

- **A preview is built from the latest scan and held under an id**, with what was selected, as `Plan` carries how each entry runs and is not sent to the browser. There is only one: a newer preview replaces it, and a cleanup uses it up.
- **A selection is exact rows, by id and path**, never kinds or an empty list. An empty one means every tool cache in the CLI, so it is refused. Two Docker rows can share a path, so a path alone will not do.
- **The cleanup scans again and builds the plan again.** If an entry's identity, action or project age differs, nothing runs and the person previews again. So a folder gone, a project edited since, or a different command refuses the plan, and something that appeared is not in it. What a tool's command acts on is not compared: Docker pointed at another local engine since the preview runs there, as it would in the CLI between its two runs. A size that changed does not refuse it: a cache grows all the time, and the entry's meaning is the same.
- **Only one cleanup runs at a time.**
- **A refused plan rescans**, since the map beside it no longer matches what the server holds.
- **The reply holds the scan after the cleanup**, which is its snapshot, so the map updates without a third measurement.

ADR-0005's rules stay in `clean::plan`: a folder git tracks is skipped as in the CLI, and picking a row is what names it.

## Scan roots

The same functions as `clch roots` change the file, under its lock. `~/` is expanded here, since no shell does it, and a relative path is refused: the server's current folder means nothing to the person. A change scans again, since the map depends on the roots. The panel lists what is configured, not a one-off's roots, as the dashboard has none.

## Stopping

Ctrl+C is the system's default for the process. Nothing needs saving: a snapshot is written in one SQLite transaction, and a tool running then shares the process group as it does for the CLI.

## Requests

Each runs on its own thread with its own `Env`, which is not shareable and which starts with no cached tool answers. Two scans at once are as safe as two CLI runs. `tiny_http` was chosen as the rest of clch is synchronous, and a server for one browser needs no async runtime.

## The UI in the binary

`build.rs` runs `npm ci` where `ui/node_modules` is missing, then `npm run build` (`tsc` and Vite), and generates a table of `include_bytes!` over `ui/dist`. It builds whenever `ui/package.json` is there and a watched source changed. A crate packaged with only `ui/dist` needs no Node. A UI that does not build stops the build, so a type error cannot ship an old UI. Only where npm cannot run at all and a `ui/dist` exists is that one embedded, with a cargo warning; with neither, the build stops and says what it needs. `npm ci` runs again when the lock file is newer than the install. `ui/dist` is not committed.

Checking Windows from a Mac (see [that note](checking-windows.md)) relies on the same rule: a copy holding only `ui/dist` builds without Node.

## What the screen shows

The maintainer picked the disk map from three variants (A, a table; B, the map; C, grouped settings), for the biggest pieces of the short disk being seen at once. Small tiles cannot hold text, which was known at the time, so a list beside the map names everything. It opens on the disk with the least free space, since that is what a person came about.

## Limits

`tiny_http` has no limit on a header line, a body or connections, and a thread goes to each request. Measured in review: a 500 MB header line took about 1 GB of memory with no token, and 3000 idle connections held 3000 threads. Only something on this machine can reach it, and the server lives until Ctrl+C.
