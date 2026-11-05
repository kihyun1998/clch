# Measuring reclaimables and disks

## Size

- **A size is allocated space**, from `st_blocks` on Unix and `AllocationSize` on Windows, because that is what deleting gives back. File length overstates a sparse file and understates a folder of small ones.
- **Folders themselves are not counted**, only the files in them. `du` counts them, so its figure runs slightly higher.
- **A hard-linked file counts once inside a reclaimable, and once across a scan's disk totals.** A reclaimable's own size still includes a file it shares with another reclaimable, so the sizes in a scan can add up to more than its disk totals. Measured the other way, a reclaimable's size would depend on which was scanned first.
- **Another disk mounted inside a tree is not entered**, as `du -x` does not, so a figure is always about one disk. On Unix the device number decides; on Windows a disk mounted in a folder is a reparse point, which the walk does not enter.
- **A file that disappears during the walk is not an unreadable one.** A build running during a scan would otherwise turn every comparison into `unknown`.
- **A size can exceed what cleaning frees.** A file with another name outside the reclaimable, such as a pnpm store file linked into `node_modules`, stays on disk when one name goes. APFS clones share blocks that `st_blocks` counts in full for each.
- **What cannot be read makes a size a lower bound**, shown as `at least` and `complete: false`. A lower bound is never compared, in either direction, so an unreadable folder does not record a drop and then a matching rise.

## Disk

- **A disk is named by where it is mounted.** On macOS that comes from `statfs`, not from walking up parents until the device changes: `/Users` is a firmlink into `/System/Volumes/Data`, and a walk would stop at `/Users`. `df` reports the same name. On Windows it is `GetVolumePathNameW`.
- **Free space is what this user can use**: `f_bavail` on Unix, the caller's figure from `GetDiskFreeSpaceExW` on Windows. It has not been compared with Finder, which may also count purgeable space.

## Freed space

- **What a cleanup freed is the scan before it minus the scan after it**, for each disk and in total, so a file two reclaimables share counts once, and a file another reclaimable still holds counts as not freed. A single reclaimable's figure is how much it shrank, which can include such a file.
- **It costs two measurements of the tree**, no matter how many reclaimables are cleaned, and the second one is the snapshot. A measurement of the maintainer's project folders (about 103 GB and 1.3 million files, HFS+ on an external drive, under load) took 46–57 seconds.

## Saying what it is doing

A scan of large roots printed nothing for 32 seconds, so it reports its steps through `Env`: one message before each measurement or tool question, and folders being searched at most twice a second. The CLI stays quiet for the first second, so a quick scan adds nothing to stderr, and writes to stderr so `--json` stays clean: one line replaced in place on a terminal, otherwise a line per message. The dashboard shows the same messages through `GET /api/progress`. Nothing estimates how much is left: the size of a tree is what the walk is finding out.
