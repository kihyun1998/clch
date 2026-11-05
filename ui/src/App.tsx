import { useCallback, useEffect, useState } from "react";
import { post, type DiskTotal, type Reclaimable, type Scan } from "./api";
import { colorClass, DiskMap } from "./DiskMap";
import { bounded, diskLabel, lastChanged, size, title } from "./format";

const UNKNOWN_DISK = "unknown";

function onDisk(scan: Scan, disk: string): Reclaimable[] {
  return scan.reclaimables
    .filter((r) => (r.disk ?? UNKNOWN_DISK) === disk)
    .sort((a, b) => b.size_bytes - a.size_bytes);
}

function shortest(disks: DiskTotal[]): string | null {
  const known = [...disks].sort(
    (a, b) => (a.free_bytes ?? Infinity) - (b.free_bytes ?? Infinity),
  );
  return known[0]?.disk ?? null;
}

export function App() {
  const [scan, setScan] = useState<Scan | null>(null);
  const [failure, setFailure] = useState<string | null>(null);
  const [scanning, setScanning] = useState(true);
  const [disk, setDisk] = useState<string | null>(null);
  const [focused, setFocused] = useState<Reclaimable | null>(null);

  const run = useCallback(async () => {
    setScanning(true);
    setFailure(null);
    setFocused(null);
    try {
      const result = await post<Scan>("/api/scan");
      setScan(result);
      setDisk((now) =>
        now !== null && result.disks.some((d) => d.disk === now)
          ? now
          : shortest(result.disks),
      );
    } catch (e) {
      setFailure(e instanceof Error ? e.message : String(e));
    } finally {
      setScanning(false);
    }
  }, []);

  useEffect(() => {
    void run();
  }, [run]);

  const items = scan && disk ? onDisk(scan, disk) : [];
  const shown = items.filter((r) => r.size_bytes > 0);

  return (
    <div className="page">
      <header className="top">
        {scan?.disks.map((d) => (
          <button
            key={d.disk}
            className="disk"
            aria-pressed={d.disk === disk}
            onClick={() => {
              setDisk(d.disk);
              setFocused(null);
            }}
          >
            <small>{diskLabel(d.disk)}</small>
            <b>{d.free_bytes === null ? "Free space unknown" : `${size(d.free_bytes)} free`}</b>
            <small>{bounded(d.reclaimable_bytes, d.complete)} can come back</small>
          </button>
        ))}
        <span className="grow" />
        <button className="link" onClick={() => void run()} disabled={scanning}>
          Scan again
        </button>
      </header>

      {failure && <p className="problem" role="alert">The scan failed: {failure}</p>}

      {scanning && (
        <div className="scanning" role="status">
          <h1>Measuring your disks</h1>
          <div className="progress"><i /></div>
          <p>With large scan roots this takes about a minute.</p>
        </div>
      )}

      {scan && !scanning && (
        <main className="body">
          {shown.length > 0 ? (
            <DiskMap items={shown} focused={focused} onFocus={setFocused} />
          ) : (
            <p className="map empty">Nothing to reclaim on this disk.</p>
          )}
          <aside className="side">
            <Notes scan={scan} />
            {focused && <Details r={focused} />}
            <h2>On this disk</h2>
            <ul className="list">
              {items.map((r) => (
                <li key={`${r.id}:${r.path}`}>
                  <button
                    className={focused === r ? "focused" : ""}
                    title={r.path}
                    onClick={() => setFocused(r)}
                  >
                    <i className={`swatch ${colorClass(r)}`} />
                    <span className="name">{title(r)}</span>
                    <span>{bounded(r.size_bytes, r.complete)}</span>
                  </button>
                </li>
              ))}
            </ul>
          </aside>
        </main>
      )}
    </div>
  );
}

function Details({ r }: { r: Reclaimable }) {
  return (
    <section className="panel">
      <h2>{title(r)}</h2>
      <p className="figure">{bounded(r.size_bytes, r.complete)}</p>
      <p>{r.name}</p>
      {r.last_modified_ms !== null && <p>{lastChanged(r.last_modified_ms)}</p>}
      <p className="path">{r.path}</p>
    </section>
  );
}

function Notes({ scan }: { scan: Scan }) {
  return (
    <>
      {scan.project_artifacts_skipped && (
        <p className="problem">
          Project artifacts were not looked for: no scan roots are set. Add one
          with <code>clch roots add &lt;folder&gt;</code>.
        </p>
      )}
      {scan.unavailable_roots.map((root) => (
        <p className="problem" key={root}>Scan root not available: {root}</p>
      ))}
      {scan.incomplete_roots.map((root) => (
        <p className="problem" key={root}>Part of this scan root could not be read: {root}</p>
      ))}
      {scan.tool_notes.map((note) => (
        <p className="problem" key={note.tool}>Nothing from {note.tool}: {note.reason}.</p>
      ))}
    </>
  );
}
