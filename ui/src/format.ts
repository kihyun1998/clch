import type { Reclaimable } from "./api";

const UNITS = ["B", "kB", "MB", "GB", "TB"];

export function size(bytes: number): string {
  let value = bytes;
  let unit = 0;
  while (value >= 1000 && unit < UNITS.length - 1) {
    value /= 1000;
    unit += 1;
  }
  return unit === 0 ? `${bytes} B` : `${value.toFixed(1)} ${UNITS[unit]}`;
}

export function bounded(bytes: number, complete: boolean): string {
  return complete ? size(bytes) : `at least ${size(bytes)}`;
}

export function lastChanged(ms: number | null): string {
  if (ms === null) return "";
  const days = Math.floor(Math.max(0, Date.now() - ms) / 86_400_000);
  if (days === 0) return "changed today";
  return days === 1 ? "changed 1 day ago" : `changed ${days} days ago`;
}

/** What a person calls this: a project's folder name, or the tool cache's name. */
export function title(r: Reclaimable): string {
  if (r.kind === "tool-cache") return r.name;
  return r.path.split(/[\\/]/).slice(-2, -1)[0] ?? r.path;
}

export function diskLabel(disk: string): string {
  if (disk === "/System/Volumes/Data" || disk === "/") return "Internal disk";
  if (disk === "unknown") return "Unknown disk";
  return disk.replace(/[\\/]+$/, "").split(/[\\/]/).pop() || disk;
}
