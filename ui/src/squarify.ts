export interface Rect {
  x: number;
  y: number;
  w: number;
  h: number;
}

/** Lays areas in proportion to `values` over a `width` by `height` rectangle. */
export function squarify(values: number[], width: number, height: number): Rect[] {
  const total = values.reduce((sum, v) => sum + v, 0);
  const rects: Rect[] = values.map(() => ({ x: 0, y: 0, w: 0, h: 0 }));
  if (total <= 0 || width <= 0 || height <= 0) return rects;
  const scale = (width * height) / total;
  let [x, y, w, h] = [0, 0, width, height];
  let next = 0;
  while (next < values.length) {
    const side = Math.min(w, h);
    let end = next;
    let sum = 0;
    let best = Infinity;
    while (end < values.length) {
      const area = values[end] * scale;
      const grown = sum + area;
      const largest = values[next] * scale;
      const worst = Math.max(
        (side * side * largest) / (grown * grown),
        (grown * grown) / (side * side * area),
      );
      if (worst > best) break;
      best = worst;
      sum = grown;
      end += 1;
    }
    const thickness = sum / side;
    let along = 0;
    for (let i = next; i < end; i += 1) {
      const length = (values[i] * scale) / thickness;
      rects[i] =
        w >= h
          ? { x, y: y + along, w: thickness, h: length }
          : { x: x + along, y, w: length, h: thickness };
      along += length;
    }
    if (w >= h) {
      x += thickness;
      w -= thickness;
    } else {
      y += thickness;
      h -= thickness;
    }
    next = end;
  }
  return rects;
}
