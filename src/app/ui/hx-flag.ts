/** Largest synthetic sample of the spike switch `?hx=<n>` (#350); more would only freeze the page. */
export const MAX_SAMPLE = 5000;

/** The spike switch `?hx` is on when the query parameter exists, even without a value; absent keeps the SVG chart. */
export function hxEnabled(value: string | undefined): boolean {
  return value !== undefined;
}

/** `?hx=400` asks for a synthetic sample of 400 points: a whole number from 1, capped at `MAX_SAMPLE`; else null. */
export function hxSamplePoints(value: string | undefined): number | null {
  if (value === undefined || !/^\d+$/.test(value)) return null;
  const n = Number(value);
  return n >= 1 ? Math.min(n, MAX_SAMPLE) : null;
}
