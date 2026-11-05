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

const token = new URLSearchParams(location.search).get("token") ?? "";

export async function post<T>(path: string): Promise<T> {
  const response = await fetch(path, {
    method: "POST",
    headers: { "X-Clch-Token": token },
  });
  const body = await response.json();
  if (!response.ok) {
    throw new Error(body.error ?? `The server answered ${response.status}`);
  }
  return body as T;
}
