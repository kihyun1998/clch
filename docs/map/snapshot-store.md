# Snapshot store: keeping and comparing scans

Each `clch scan`, and each `clch clean --yes` once it finishes, writes a snapshot to one SQLite file in the OS application-data folder. A `clch clean` preview writes nothing: it changes nothing on disk, and a user only looking at a plan has not asked for a measurement.

## Writing at the same time

- **The switch to WAL is retried by hand.** Runs racing on a new database got `database is locked` at once, ignoring the 10-second busy timeout. Measured on macOS with no retry: 2 runs at once lost a snapshot in 94 of 100 rounds, 8 at once lost 63 in 100 rounds. With the retry around `PRAGMA journal_mode = WAL`, both lost none. Removing only that retry brings the failure back.
- **Migrations and writes do not need the retry.** They start with `BEGIN IMMEDIATE`; removing a retry around the migration changed nothing in the racing test.
- **SQLite's WAL does not work on a network file system** ([sqlite.org/wal.html](https://www.sqlite.org/wal.html)), and when the switch fails SQLite keeps the previous journal mode without an error. clch then runs in that mode. Not measured on a network home folder.

## Comparing

- **A tool cache is the same reclaimable wherever it lives**, so its history follows a moved cache. Path spelling, a symlink or npm falling back to its default location would otherwise break it.
- **What the latest snapshot had and this scan did not find is reported as gone**, once, with its last size. It is compared only with the latest snapshot, so one that returns later is `new`.
- **`new` and `unknown` differ.** `new` means the store has no earlier size for it. `unknown` means either the store could not be used, which is also `history_available: false` in JSON, or one of the two measurements is a lower bound. A store that cannot be used never fails a scan.
- **Order snapshots by id, not time.** `taken_at_ms` is when the scan started, from the system clock, which can go backwards and differ between overlapping runs.

## Changing the schema

`user_version` holds the schema version and `migrate` brings an older store up. Sizes from schema 1 are marked incomplete by the upgrade: they are file lengths, so comparing them with allocated sizes would show a change that did not happen. A store from a newer clch is left alone and reported, because snapshots are kept forever.

## What a snapshot keeps

Each reclaimable's size, disk and completeness, and each disk's free space and reclaimable total. The disk totals are stored because they cannot be rebuilt: adding up reclaimables counts a shared hard-linked file more than once.
