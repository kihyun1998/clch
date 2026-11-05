export interface Reclaimable {
  id: string;
  kind: "tool-cache" | "project-artifact";
  name: string;
  path: string;
  size_bytes: number;
  complete: boolean;
  disk: string | null;
  last_modified_ms: number | null;
  change_bytes: number | null;
  change_state: "known" | "new" | "unknown";
}

export interface DiskTotal {
  disk: string;
  free_bytes: number | null;
  reclaimable_bytes: number;
  complete: boolean;
}

export interface ToolNote {
  tool: string;
  reason: string;
}

export interface Scan {
  reclaimables: Reclaimable[];
  disks: DiskTotal[];
  tool_notes: ToolNote[];
  scan_roots: string[];
  unavailable_roots: string[];
  incomplete_roots: string[];
  project_artifacts_skipped: boolean;
}

export interface SizePoint {
  snapshot: number;
  taken_at_ms: number;
  size_bytes: number;
  complete: boolean;
  gone: boolean;
}

export interface ReclaimableHistory {
  id: string;
  kind: Reclaimable["kind"];
  path: string;
  points: SizePoint[];
}

export interface DiskPoint {
  snapshot: number;
  taken_at_ms: number;
  free_bytes: number | null;
  reclaimable_bytes: number;
  complete: boolean;
}

export interface History {
  snapshots: { id: number; taken_at_ms: number; partial: boolean }[];
  reclaimables: ReclaimableHistory[];
  disks: { disk: string; points: DiskPoint[] }[];
}

const token = new URLSearchParams(location.search).get("token") ?? "";

async function call<T>(method: string, path: string): Promise<T> {
  const response = await fetch(path, {
    method,
    headers: { "X-Clch-Token": token },
  });
  const body = await response.json();
  if (!response.ok) {
    throw new Error(body.error ?? `The server answered ${response.status}`);
  }
  return body as T;
}

export const post = <T,>(path: string) => call<T>("POST", path);
export const get = <T,>(path: string) => call<T>("GET", path);
