import { useLayoutEffect, useRef, useState } from "react";
import type { Reclaimable } from "./api";
import { bounded, lastChanged, title } from "./format";
import { squarify } from "./squarify";

interface Props {
  items: Reclaimable[];
  focused: Reclaimable | null;
  selected: Set<Reclaimable>;
  onToggle: (r: Reclaimable) => void;
}

/** Below this size in pixels a tile shows no text; the list beside it does. */
const LEGIBLE = { w: 56, h: 34 };

export function colorClass(r: Reclaimable): string {
  if (r.kind === "tool-cache") return "kind-cache";
  return `kind-${r.id.replace(/[^a-z_]/g, "")}`;
}

export function DiskMap({ items, focused, selected, onToggle }: Props) {
  const box = useRef<HTMLDivElement>(null);
  const [area, setArea] = useState({ w: 0, h: 0 });

  useLayoutEffect(() => {
    const element = box.current;
    if (!element) return;
    const measure = () =>
      setArea({ w: element.clientWidth, h: element.clientHeight });
    measure();
    const observer = new ResizeObserver(measure);
    observer.observe(element);
    return () => observer.disconnect();
  }, []);

  const rects = squarify(
    items.map((r) => r.size_bytes),
    area.w,
    area.h,
  );

  return (
    <div className="map" ref={box} role="group" aria-label="Reclaimables, sized by space">
      {items.map((r, i) => {
        const rect = rects[i];
        const shown = rect.w >= LEGIBLE.w && rect.h >= LEGIBLE.h;
        const description = `${title(r)}, ${r.name}, ${bounded(r.size_bytes, r.complete)}`;
        return (
          <button
            key={`${r.id}:${r.path}`}
            className={`tile ${colorClass(r)}${selected.has(r) ? " selected" : ""}${focused === r ? " focused" : ""}`}
            style={{ left: rect.x, top: rect.y, width: rect.w, height: rect.h }}
            aria-label={description}
            aria-pressed={selected.has(r)}
            title={`${description}\n${r.path}`}
            onClick={() => onToggle(r)}
          >
            {shown && (
              <>
                <b>{title(r)}</b>
                <span>{bounded(r.size_bytes, r.complete)}</span>
                {r.kind === "project-artifact" && <span>{r.name}</span>}
                <span>{lastChanged(r.last_modified_ms)}</span>
              </>
            )}
          </button>
        );
      })}
    </div>
  );
}
