// What the loop writes on the issue: the issue is the surface a person reads, the feature pull request is transport.
// Every comment starts with a marker, and the tracker posts a marker once (no repeat on resume).
import type { Verdict } from './review.mts';

/** The reason of the loop as one word of the marker; the first match wins. */
const KINDS: [RegExp, string][] = [
  [/^needs-decision: review still asks/, 'changes-remain'],
  [/^needs-decision/, 'needs-decision'],
  [/more than one open pull request/, 'more-pulls'],
  [/changes protected paths/, 'protected-paths'],
  [/could not read the changed files/, 'files-unreadable'],
  [/pushed after the review/, 'pushed-after-review'],
  [/^high risk/, 'high-risk'],
  [/^review failed/, 'review-failed'],
  [/^red: /, 'red-checks'],
  [/conflict/, 'conflict'],
  [/^merge refused/, 'merge-refused'],
  [/^checks did not finish/, 'checks-unfinished'],
  [/^session timeout/, 'session-timeout'],
  [/^session/, 'session-failed'],
  [/^harness failed/, 'harness-failed'],
  [/^pull request closed/, 'pull-closed'],
];
export const reasonKind = (reason: string): string => {
  // the text a blocked session adds to the reason names no kind
  const own = reason.replace(/ \(session: blocked .*/s, '');
  return KINDS.find(([pattern]) => pattern.test(own))?.[1] ?? 'other';
};

const NEXT: Record<string, string> = {
  'changes-remain':
    'Read the findings on the pull request, fix them or adjust this issue, then start the loop again.',
  'needs-decision':
    'Answer the open question on this issue, remove the label `needs-decision` if it is set, then start the loop again.',
  'more-pulls': 'Close the stale pull request, then start the loop again.',
  'high-risk': 'The pull request is merged: check the change.',
};
const MERGE = 'Check the pull request and merge it yourself, then start the loop again.';
const FIX = 'Remove the cause, then start the loop again.';

export type Note = { marker: string; body: string };

export function handoverNote(input: {
  reason: string;
  /** Handed over by rule: a person merges the pull request. */
  planned: boolean;
  /** The head of the pull request, `none` without one. */
  head: string;
  urls: string[];
  risk?: string;
}): Note {
  const kind = reasonKind(input.reason);
  const marker = `<!-- deliver-handover: ${kind} ${input.head} -->`;
  // the reason may quote an agent or a file name: in a code span it mentions nobody and opens no marker
  const reason = input.reason.replace(/[\u0000-\u001f\u007f-\u009f`]+/g, ' ').slice(0, 1000);
  const body = [
    marker,
    'The deliver loop hands this issue to you.',
    '',
    `- Reason: \`${reason}\``,
    `- Pull request: ${input.urls.length ? input.urls.join(', ') : 'none'}`,
    ...(input.risk ? [`- Risk: ${input.risk}`] : []),
    `- Next step: ${NEXT[kind] ?? (input.planned ? MERGE : FIX)}`,
  ].join('\n');
  return { marker, body };
}

/** One review round: the verdict and the counts; the findings themselves are in the comment on the pull request. */
export function reviewNote(round: number, verdict: Verdict, head: string, url: string): Note {
  const marker = `<!-- deliver-review-summary: ${round} ${verdict.verdict} ${head} -->`;
  const body = [
    marker,
    `Review round ${round}: \`${verdict.verdict}\``,
    '',
    `- Blocking findings: ${verdict.findings.length}`,
    `- Unmet criteria: ${verdict.unmet.length}`,
    `- Pull request: ${url} (the findings are in its review comment)`,
  ].join('\n');
  return { marker, body };
}
