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

- **A disk is named by where it is mounted.** On macOS that comes from `statfs`, not from walking up parents until the device changes: `/Users` is a firmlink into `/System/Volumes/Data`, and a walk would stop at `/Users`. `df` reports the same name. On Windows it is `GetVolumePathNameW` with the drive letter in upper case, because that call repeats the letter as the path spells it, and pip reports a lower-case path: a real scan listed `C:\` and `c:\` as two disks.
- **Free space is what this user can use**: `f_bavail` on Unix, the caller's figure from `GetDiskFreeSpaceExW` on Windows. It has not been compared with Finder, which may also count purgeable space.

## Freed space

- **What a cleanup freed is the scan before it minus the scan after it**, for each disk and in total, so a file two reclaimables share counts once, and a file another reclaimable still holds counts as not freed. A single reclaimable's figure is how much it shrank, which can include such a file.
- **It costs two measurements of the tree**, no matter how many reclaimables are cleaned, and the second one is the snapshot. A measurement of the maintainer's project folders (about 103 GB and 1.3 million files, HFS+ on an external drive, under load) took 46–57 seconds.

## Saying what it is doing

A scan of large roots printed nothing for 32 seconds, so it reports its steps through `Env`: one message before each measurement or tool question, and folders being searched at most twice a second. The CLI stays quiet for the first second, so a quick scan adds nothing to stderr, and writes to stderr so `--json` stays clean: one line replaced in place on a terminal, otherwise a line per message. The dashboard shows the same messages through `GET /api/progress`. Nothing estimates how much is left: the size of a tree is what the walk is finding out.

`clch clean` does neither of those. It names its steps (`[1/3] Scanning`, `[2/3] Cleaning N reclaimables`, `[3/3] Measuring what was freed`; a preview has `[1/1]`) and every line stays on screen, the same on a terminal and in a pipe. The rewritten line changed faster than it could be read and left no record of which reclaimable was being cleaned, whether it failed or how long it took; stacked lines, a rewritten line with a step prefix and a mix of both were tried side by side, and stacked lines won. `scan` keeps the rewritten line and its quiet first second, and a test holds that a quick `scan` prints nothing.

- **A scan has three parts, and which one is slow depends on the machine.** Each tool is asked where its cache is, then the scan roots are walked, then what was found is measured. A preview of one small project, with npm, pnpm, pip, uv, go, dart and wsl on `PATH`, took 12.5 seconds, 12.4 of them before the first `searching` line; the walk and the measuring took about 60 milliseconds. On a large tree the measuring is the 46–57 seconds above. So `clch clean` says `searching <root>` per root, then `measuring N project artifacts` once and `found N reclaimables (x s)` at the end, not a line per artifact, which would run to hundreds. Each tool cache gets a `looking for the <name>` line as its tool is asked, whether or not the tool is installed (18 lines a scan, 36 for a `clch clean --yes`), so the wait names what it is waiting on. The maintainer chose these lines over a rewritten line for that stretch and over listing only the slow ones.
- **An item line has no freed size.** What was freed is the scan before minus the scan after, known only once every reclaimable has been cleaned, so it stays in the final result.
