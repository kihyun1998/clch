import { useState } from "react";
import { size } from "./format";

export interface TrendPoint {
  at: number;
  value: number;
}

const WIDTH = 300;
const HEIGHT = 56;
const PAD = 4;

/** A line over the order of snapshots, with the value under the pointer. */
export function Trend({ label, points }: { label: string; points: TrendPoint[] }) {
  const [hover, setHover] = useState<number | null>(null);
  if (points.length < 2) return null;
  const max = Math.max(...points.map((p) => p.value), 1);
  const x = (i: number) => PAD + (i / (points.length - 1)) * (WIDTH - 2 * PAD);
  const y = (v: number) => HEIGHT - PAD - (v / max) * (HEIGHT - 2 * PAD);
  const line = points.map((p, i) => `${x(i).toFixed(1)},${y(p.value).toFixed(1)}`).join(" ");
  const shown = points[hover ?? points.length - 1];
  return (
    <figure className="trend">
      <figcaption>
        <span>{label}</span>
        <span>
          {size(shown.value)} on {new Date(shown.at).toLocaleDateString()}
        </span>
      </figcaption>
      <svg
        viewBox={`0 0 ${WIDTH} ${HEIGHT}`}
        role="img"
        aria-label={`${label}: from ${size(points[0].value)} to ${size(points[points.length - 1].value)} over ${points.length} scans`}
        onPointerMove={(e) => {
          const box = e.currentTarget.getBoundingClientRect();
          const part = (e.clientX - box.left) / box.width;
          setHover(Math.round(part * (points.length - 1)));
        }}
        onPointerLeave={() => setHover(null)}
      >
        <line className="baseline" x1={PAD} x2={WIDTH - PAD} y1={HEIGHT - PAD} y2={HEIGHT - PAD} />
        <polyline className="series" points={line} />
        {hover !== null && (
          <circle className="mark" cx={x(hover)} cy={y(shown.value)} r={4} />
        )}
      </svg>
    </figure>
  );
}
