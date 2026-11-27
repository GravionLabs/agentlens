// Names kept for one release: DELIVER_MIN_<X> is read as DELIVER_<X> (the new name wins) with a warning, and the
// keys of `deliver` in .agents/config.json that the loop no longer knows are ignored with one warning.
import { REVIEW_KEYS } from './review.mts';

export const CONFIG_KEYS = [
  'base',
  'tracker',
  'harness',
  'protectedPaths',
  'handoverMinutes',
  'review',
];

/** Copies each DELIVER_MIN_<X> to DELIVER_<X> when that is unset. Returns one warning per old name found. */
export function adoptLegacyEnv(env: NodeJS.ProcessEnv = process.env): string[] {
  const warnings: string[] = [];
  for (const old of Object.keys(env).filter((name) => name.startsWith('DELIVER_MIN_'))) {
    const name = `DELIVER_${old.slice('DELIVER_MIN_'.length)}`;
    warnings.push(`${old} is deprecated and read for one more release: rename it to ${name}`);
    if (env[name] === undefined) env[name] = env[old];
    delete env[old];
  }
  return warnings;
}

/** One warning line when `deliver` (or `deliver.review`) holds keys the loop does not know, else nothing. */
export function unknownConfigKeys(deliver: unknown): string | undefined {
  if (!deliver || typeof deliver !== 'object') return undefined;
  const { review } = deliver as { review?: unknown };
  const unknown = [
    ...Object.keys(deliver).filter((key) => !CONFIG_KEYS.includes(key)),
    ...(review && typeof review === 'object' ? Object.keys(review) : [])
      .filter((key) => !REVIEW_KEYS.includes(key))
      .map((key) => `review.${key}`),
  ];
  return unknown.length
    ? `.agents/config.json: deliver.${unknown.join(', deliver.')} ignored (the loop reads ${CONFIG_KEYS.join(', ')}; review: ${REVIEW_KEYS.join(', ')})`
    : undefined;
}
