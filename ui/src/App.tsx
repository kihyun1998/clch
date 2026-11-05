import { useCallback, useEffect, useState } from "react";
import {
  get,
  post,
  type DiskTotal,
  type History,
  type Reclaimable,
  type ReclaimableHistory,
  type Scan,
} from "./api";
import { colorClass, DiskMap } from "./DiskMap";
import { bounded, diskLabel, lastChanged, size, title } from "./format";
import { Trend, type TrendPoint } from "./Trend";

const UNKNOWN_DISK = "unknown";
const MONTH_MS = 30 * 86_400_000;

function seriesOf(history: History | null, r: Reclaimable): ReclaimableHistory | undefined {
  const same = history?.reclaimables.filter((s) => s.id === r.id) ?? [];
  return same.length === 1 ? same[0] : same.find((s) => s.path === r.path);
}

/** How much it grew over the last 30 days, between readings that are not lower bounds. */
function growth(series: ReclaimableHistory | undefined): number | null {
  const points = series?.points.filter((p) => p.complete) ?? [];
  const last = points[points.length - 1];
  if (!last) return null;
  const first = points.find((p) => p.taken_at_ms >= last.taken_at_ms - MONTH_MS);
  return first && first !== last ? last.size_bytes - first.size_bytes : null;
}

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
  const [history, setHistory] = useState<History | null>(null);
  const [historyFailure, setHistoryFailure] = useState<string | null>(null);

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
      try {
        setHistory(await get<History>("/api/history"));
        setHistoryFailure(null);
      } catch (e) {
        setHistoryFailure(e instanceof Error ? e.message : String(e));
      }
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
            {historyFailure && <p className="problem">History is not available: {historyFailure}</p>}
            {focused ? (
              <Details r={focused} series={seriesOf(history, focused)} />
            ) : (
              <DiskTrends history={history} disk={disk} />
            )}
            <Growing history={history} items={items} onFocus={setFocused} />
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

function DiskTrends({ history, disk }: { history: History | null; disk: string | null }) {
  const points = history?.disks.find((d) => d.disk === disk)?.points ?? [];
  if (points.length < 2) {
    return <p className="quiet">Growth appears after the next scan.</p>;
  }
  const free: TrendPoint[] = points
    .filter((p) => p.free_bytes !== null)
    .map((p) => ({ at: p.taken_at_ms, value: p.free_bytes ?? 0 }));
  const back: TrendPoint[] = points
    .filter((p) => p.complete)
    .map((p) => ({ at: p.taken_at_ms, value: p.reclaimable_bytes }));
  return (
    <section className="panel">
      <Trend label="Can come back" points={back} />
      <Trend label="Free" points={free} />
    </section>
  );
}

function Growing({
  history,
  items,
  onFocus,
}: {
  history: History | null;
  items: Reclaimable[];
  onFocus: (r: Reclaimable) => void;
}) {
  const grew = items
    .map((r) => ({ r, by: growth(seriesOf(history, r)) }))
    .filter((g): g is { r: Reclaimable; by: number } => g.by !== null && g.by > 0)
    .sort((a, b) => b.by - a.by)
    .slice(0, 5);
  if (grew.length === 0) return null;
  return (
    <>
      <h2>Grew most in 30 days</h2>
      <ul className="list">
        {grew.map(({ r, by }) => (
          <li key={`${r.id}:${r.path}`}>
            <button title={r.path} onClick={() => onFocus(r)}>
              <span className="name">{title(r)}</span>
              <span>+{size(by)}</span>
            </button>
          </li>
        ))}
      </ul>
    </>
  );
}

function Details({ r, series }: { r: Reclaimable; series?: ReclaimableHistory }) {
  const points: TrendPoint[] = (series?.points ?? [])
    .filter((p) => p.complete)
    .map((p) => ({ at: p.taken_at_ms, value: p.size_bytes }));
  return (
    <section className="panel">
      <h2>{title(r)}</h2>
      <p className="figure">{bounded(r.size_bytes, r.complete)}</p>
      <p>{r.name}</p>
      {r.last_modified_ms !== null && <p>{lastChanged(r.last_modified_ms)}</p>}
      <p className="path">{r.path}</p>
      <Trend label="Size over time" points={points} />
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
