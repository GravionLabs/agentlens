// The review of a delivered issue is its own session (`/deliver-review`, ADR 0009): its settings and the
// verdict it writes to DELIVER_REVIEW_FILE. The file is written by an agent: it is validated, never trusted.
import { HARNESSES, type Harness } from './harness.mts';

export type Verdict = {
  verdict: 'approve' | 'changes';
  findings: { severity: 'blocking'; file: string; summary: string }[];
  unmet: string[];
};

const text = (value: unknown): value is string => typeof value === 'string' && value.trim() !== '';
const object = (value: unknown): value is Record<string, unknown> =>
  !!value && typeof value === 'object' && !Array.isArray(value);

/**
 * The verdict of the file content, `undefined` when it breaks the contract. An `approve` with a finding or an
 * unmet criterion, and a `changes` with neither, contradict themselves and are invalid too.
 */
export function parseVerdict(content: string): Verdict | undefined {
  let value: unknown;
  try {
    value = JSON.parse(content);
  } catch {
    return undefined;
  }
  if (!object(value)) return undefined;
  const { verdict, findings, unmet } = value;
  if (verdict !== 'approve' && verdict !== 'changes') return undefined;
  if (!Array.isArray(findings) || !Array.isArray(unmet)) return undefined;
  const valid = findings.every(
    (f) => object(f) && f.severity === 'blocking' && text(f.file) && text(f.summary),
  );
  if (!valid || !unmet.every(text)) return undefined;
  if ((findings.length + unmet.length === 0) !== (verdict === 'approve')) return undefined;
  return { verdict, findings, unmet } as Verdict;
}

/** The findings and unmet criteria as one line of the report, without control characters. */
export function findingsLine(verdict: Verdict): string {
  const parts = [
    ...verdict.findings.map((f) => `${f.file}: ${f.summary}`),
    ...verdict.unmet.map((criterion) => `unmet: ${criterion}`),
  ];
  const line = parts.join('; ').replace(/[\u0000-\u001f\u007f-\u009f]+/g, ' ');
  return line.length > 500 ? `${line.slice(0, 500)}…` : line;
}

/** The risk levels of an implementer result (`risk.level`). */
export const RISK_LEVELS = ['low', 'medium', 'high'];

/** A verdict the loop recorded on a pull request; `review` is the validated verdict of a `changes` mark. */
export type ReviewMark = {
  verdict: Verdict['verdict'];
  head: string;
  risk?: string;
  review?: Verdict;
};

const MARK = /^<!-- deliver-review: (approve|changes) (\S+?)(?: risk=([a-z]+))? -->/;
const VERDICT_LINE = /^<!-- deliver-review-verdict: ([A-Za-z0-9+/]+={0,2}) -->$/m;
// a comment holds at most 65536 characters: a longer verdict line is left out
const VERDICT_LINE_MAX = 60_000;

/**
 * The comment that records a verdict: the mark line, for `changes` the verdict as base64 JSON (agent text could
 * close an HTML comment), then one readable line. The risk token is left out without a level.
 */
export function markBody(review: Verdict, head: string, risk?: string): string {
  const level = risk && RISK_LEVELS.includes(risk) ? ` risk=${risk}` : '';
  const lines = [`<!-- deliver-review: ${review.verdict} ${head}${level} -->`];
  if (review.verdict === 'changes') {
    const { verdict, unmet } = review;
    const findings = review.findings.map(({ severity, file, summary }) => ({
      severity,
      file,
      summary,
    }));
    const encoded = Buffer.from(JSON.stringify({ verdict, findings, unmet })).toString('base64');
    if (encoded.length <= VERDICT_LINE_MAX)
      lines.push(`<!-- deliver-review-verdict: ${encoded} -->`);
  }
  lines.push(`The deliver loop recorded the review verdict \`${review.verdict}\` for ${head}.`);
  return lines.join('\n');
}

/**
 * The mark of a comment body, `undefined` when it does not start with one. An unknown risk and a verdict line that
 * is not a valid verdict of the same kind are left out: the body is read, never trusted.
 */
export function parseMark(body: string): ReviewMark | undefined {
  const mark = MARK.exec(body);
  if (!mark) return undefined;
  const result: ReviewMark = { verdict: mark[1] as Verdict['verdict'], head: mark[2] as string };
  if (mark[3] && RISK_LEVELS.includes(mark[3])) result.risk = mark[3];
  const line = VERDICT_LINE.exec(body);
  if (result.verdict === 'changes' && line) {
    const review = parseVerdict(Buffer.from(line[1] as string, 'base64').toString('utf8'));
    if (review?.verdict === 'changes') result.review = review;
  }
  return result;
}

export const REVIEW_KEYS = ['model', 'harness', 'effort'];

export type ReviewSettings = {
  harnessName: string;
  harness: Harness;
  model: string;
  effort?: string;
};

/**
 * The settings of the review session: the flag, then `deliver.review`, else the harness of the implementer with
 * a model that is not the implementer's. Throws an Error whose message names the wrong setting.
 */
export function reviewSettings(input: {
  flags: { model?: string; harness?: string };
  config: unknown;
  implementer: { harnessName: string; model: string };
}): ReviewSettings {
  const config = object(input.config) ? input.config : {};
  const harnessName = String(
    input.flags.harness ?? config.harness ?? input.implementer.harnessName,
  );
  if (!Object.hasOwn(HARNESSES, harnessName))
    throw new Error(
      `unknown review harness "${harnessName}": one of ${Object.keys(HARNESSES).join(', ')}`,
    );
  const harness = HARNESSES[harnessName];
  const [first, second] = harness.reviewModels;
  // the same family counts as the same model: `claude-opus-…` and `opus`
  const family = (model: string) => /fable|opus|sonnet|haiku/i.exec(model)?.[0].toLowerCase();
  const other = family(input.implementer.model) === family(first) ? second : first;
  const model = input.flags.model ?? config.model ?? other;
  // a model name goes into the command line: never one that reads as an option
  if (typeof model !== 'string' || !/^[A-Za-z0-9][\w.:/[\]-]*$/.test(model))
    throw new Error('the review model (--review-model, deliver.review.model) is no model name');
  const { effort } = config;
  if (effort !== undefined && !harness.efforts.includes(effort as string))
    throw new Error(
      `deliver.review.effort of ${harnessName} is one of ${harness.efforts.join(', ')}`,
    );
  return { harnessName, harness, model, effort: effort as string | undefined };
}
