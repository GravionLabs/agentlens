// The stage of an issue, kept by the loop in one `agent:*` label (ADR 0009). Only the loop writes these labels.
// A label is a hint where to resume, never an authorization: a person can set it.

/** In the order of progress. */
export const STAGES = ['implementing', 'changes', 'review', 'approved', 'handover'] as const;
export type Stage = (typeof STAGES)[number];

export const LABEL_PREFIX = 'agent:';
export const labelOf = (stage: Stage): string => `${LABEL_PREFIX}${stage}`;

export const STAGE_LABELS: Record<Stage, { color: string; description: string }> = {
  implementing: { color: '1d76db', description: 'deliver loop: the implementer works on it' },
  changes: { color: 'd93f0b', description: 'deliver loop: the review asked for changes' },
  review: { color: 'fbca04', description: 'deliver loop: the pull request is in review' },
  approved: { color: '0e8a16', description: 'deliver loop: approved, waits for the merge' },
  handover: { color: '5319e7', description: 'deliver loop: handed to a person' },
};

export type StageEvent =
  | 'session-started'
  | 'pull-opened'
  | 'verdict-changes'
  | 'verdict-approve'
  | 'set-aside'
  | 'handed-over'
  | 'merged';

const AFTER: Record<StageEvent, Stage | undefined> = {
  'session-started': 'implementing',
  'pull-opened': 'review',
  'verdict-changes': 'changes',
  'verdict-approve': 'approved',
  'set-aside': 'handover',
  'handed-over': 'handover',
  merged: undefined,
};

/** The stage of an issue after the event; `undefined`: no `agent:*` label is left. */
export const stageAfter = (event: StageEvent): Stage | undefined => AFTER[event];

/** The stage the labels name. Several labels (set by a person): the least advanced one. */
export const stageOf = (labels: string[]): Stage | undefined => {
  const names = labels.map((name) => name.toLowerCase());
  return STAGES.find((stage) => names.includes(labelOf(stage)));
};
