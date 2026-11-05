# Snapshot store: keeping and comparing scans

Each `clch scan`, and each `clch clean --yes` once it finishes, writes a snapshot to one SQLite file in the OS application-data folder. A `clch clean` preview writes nothing: it changes nothing on disk, and a user only looking at a plan has not asked for a measurement.

## Writing at the same time

- **The switch to WAL is retried by hand.** Runs racing on a new database got `database is locked` at once, ignoring the 10-second busy timeout. Measured on macOS with no retry: 2 runs at once lost a snapshot in 94 of 100 rounds, 8 at once lost 63 in 100 rounds. With the retry around `PRAGMA journal_mode = WAL`, both lost none. Removing only that retry brings the failure back.
- **Migrations and writes do not need the retry.** They start with `BEGIN IMMEDIATE`; removing a retry around the migration changed nothing in the racing test.
- **SQLite's WAL does not work on a network file system** ([sqlite.org/wal.html](https://www.sqlite.org/wal.html)), and when the switch fails SQLite keeps the previous journal mode without an error. clch then runs in that mode. Not measured on a network home folder.

## Comparing

- **A tool cache is the same reclaimable wherever it lives**, so its history follows a moved cache. Path spelling, a symlink or npm falling back to its default location would otherwise break it.
- **Each reclaimable is compared with the last size recorded for it, not with the latest snapshot**, so one a one-off scan root left out, or one on a disk that was unplugged, keeps its history.
- **What this scan should have found and did not is reported as gone**, once, with its last size, and a marker row in the snapshot records that. One that returns after that is `new`. A project artifact should have been found only if a scan root covers it and that root was there and fully readable; a tool cache, if its folder is gone and its disk attached.
- **`new` and `unknown` differ.** `new` means the store has no earlier size for it. `unknown` means either the store could not be used, which is also `history_available: false` in JSON, or one of the two measurements is a lower bound. A store that cannot be used never fails a scan.
- **Order snapshots by id, not time.** `taken_at_ms` is when the scan started, from the system clock, which can go backwards and differ between overlapping runs.

## Reading it back

`GET /api/history` returns every snapshot, each reclaimable's sizes under the same identity comparing uses, and each disk's totals. It sends everything, as the data is small and the screen needs most of it; nothing yet shows how large it gets over years. Snapshots from one-off scan roots are in the reclaimables' series, since each row is a true size, and left out of the disks' series. The chart spaces points by scan, not by date, so a gap in use does not read as a flat stretch.

A disk stays named by where it is mounted, as the glossary has it. So a Windows drive letter used by another disk, or a macOS remount as `T7 1`, would mix or split that disk's series. That was inferred in a completeness read on #5 and has not been seen. The cost of a volume identifier is a schema change and a per-OS lookup, for a case not yet met.

## Changing the schema

`user_version` holds the schema version and `migrate` brings an older store up. Sizes from schema 1 are marked incomplete by the upgrade: they are file lengths, so comparing them with allocated sizes would show a change that did not happen. A store from a newer clch is left alone and reported, because snapshots are kept forever.

## What a snapshot keeps

Each reclaimable's size, disk and completeness, and each disk's free space and reclaimable total. The disk totals are stored because they cannot be rebuilt: adding up reclaimables counts a shared hard-linked file more than once.

A snapshot from one-off scan roots is marked `partial`, for #13 to leave out of per-disk history, since its disk totals cover only part of what the configured roots would.

After a cleanup, only what the cleanup itself removed is marked gone, so something deleted by hand still gets reported by the next scan.
