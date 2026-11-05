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

export interface PlanEntry {
  id: string;
  name: string;
  cleanable: boolean;
  action?: string;
  expected_bytes?: number;
  complete: boolean;
  reason?: string;
  path?: string;
  last_modified_ms?: number;
}

export interface Preview {
  plan_id: string;
  level: "unused" | "full";
  uses_level: boolean;
  plan: PlanEntry[];
  not_selected: number;
  expected_total_bytes: number;
  tool_notes: ToolNote[];
}

export interface CleanResult {
  id: string;
  name: string;
  path?: string;
  status: "cleaned" | "skipped" | "failed";
  freed_bytes: number;
  complete: boolean;
  reason?: string;
  output?: string;
}

export interface Outcome {
  results: CleanResult[];
  disks: { disk: string; freed_bytes: number; complete: boolean }[];
  freed_bytes: number;
  complete: boolean;
  notes: string[];
  tool_notes: ToolNote[];
}

export interface Cleaned {
  outcome: Outcome;
  scan: Scan;
}

async function call<T>(method: string, path: string, json?: unknown): Promise<T> {
  const headers: Record<string, string> = { "X-Clch-Token": token };
  if (json !== undefined) headers["Content-Type"] = "application/json";
  const response = await fetch(path, {
    method,
    headers,
    body: json === undefined ? undefined : JSON.stringify(json),
  });
  const body = await response.json();
  if (!response.ok) {
    throw new Error(body.error ?? `The server answered ${response.status}`);
  }
  return body as T;
}

export const post = <T,>(path: string, json?: unknown) => call<T>("POST", path, json);
export const get = <T,>(path: string) => call<T>("GET", path);
