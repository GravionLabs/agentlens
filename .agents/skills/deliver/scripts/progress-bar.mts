// The progress line of the loop: a bar and `x of y`, printed when an issue starts.
const FULL = '█';
const EMPTY = '░';
const MAX_BAR = 20;
/** Below this width the line is only `x of y`. */
export const MIN_WIDTH = 30;

/**
 * `[████░░░░░░] 3 of 12`. `x` is the issue that begins now (1-based), `y` the number of issues of the
 * run; a repair Bug can make `x` exceed `y`, then `y` follows `x`. `width` is the room of the line.
 */
export function progressLine(x: number, y: number, width: number): string {
  const total = Math.max(y, x, 1);
  const text = `${x} of ${total}`;
  if (width < MIN_WIDTH) return text;
  const bar = Math.min(MAX_BAR, width - text.length - 3);
  const filled = Math.round((bar * x) / total);
  return `[${FULL.repeat(filled)}${EMPTY.repeat(bar - filled)}] ${text}`;
}
