#!/usr/bin/env node
// deliver loop: a fresh agent session per issue, a review session for its pull request, merge, then the next issue.
// The state is read from the tracker before every issue. The run only remembers what it set aside.
// Tracker: tracker.mts. Progress events: events.mts. Review settings and verdict: review.mts. Issue stage: stage.mts.
// Comments on the issue: handover.mts.
import { execFileSync } from 'node:child_process';
import {
  appendFileSync,
  createWriteStream,
  existsSync,
  mkdirSync,
  readFileSync,
  realpathSync,
  rmSync,
  writeFileSync,
} from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';
import { createInterface } from 'node:readline';
import { parseArgs } from 'node:util';
import { appSettings, tokenSource as makeSource } from './app-token.mts';
import { type Emit, openEvents } from './events.mts';
import { type Note, handoverNote, reviewNote } from './handover.mts';
import { finishReport, programStatus } from './program-status.mts';
import { cargoTomlKeys } from './cargo-toml.mts';
import { directoryPackagesPropsKeys } from './directory-packages-props.mts';
import { goModKeys } from './go-mod.mts';
import { pyprojectTomlKeys } from './pyproject-toml.mts';
import { pnpmWorkspaceKeys } from './pnpm-workspace.mts';
import { HARNESSES, type Harness } from './harness.mts';
import { adoptLegacyEnv, unknownConfigKeys } from './legacy.mts';
import { killTree, launch, promptPath } from './process.mts';
import { progressLine } from './progress-bar.mts';
import {
  RISK_LEVELS,
  type ReviewMark,
  type ReviewSettings,
  type Verdict,
  findingsLine,
  parseVerdict,
  reviewSettings,
} from './review.mts';
import { type StageEvent, stageAfter, stageOf } from './stage.mts';
import { type BaseState, type Gate, type Item, type Pull, loadTracker } from './tracker.mts';

// before anything reads the environment; the warnings are printed once the log exists
const legacyWarnings = adoptLegacyEnv();

const parsed = (() => {
  try {
    return parseArgs({
      options: {
        milestone: { type: 'string' },
        issues: { type: 'string' },
        base: { type: 'string' },
        tracker: { type: 'string' },
        harness: { type: 'string' },
        max: { type: 'string', default: '0' },
        model: { type: 'string' },
        'review-model': { type: 'string' },
        'review-harness': { type: 'string' },
        'session-minutes': { type: 'string', default: '90' },
        'merge-minutes': { type: 'string', default: '30' },
        'handover-minutes': { type: 'string' },
        'dry-run': { type: 'boolean', default: false },
        'allow-no-required-checks': { type: 'boolean', default: false },
        'trust-authors': { type: 'boolean', default: false },
      },
    });
  } catch (error) {
    // an unknown option or a missing value: one line, no stack trace
    const message = (error as Error).message.split('\n')[0];
    console.error(
      `deliver-loop: ${message}. Options: --milestone, --issues, --base, --tracker, --harness, --max, --model, --review-model, --review-harness, --session-minutes, --merge-minutes, --handover-minutes, --dry-run, --allow-no-required-checks, --trust-authors.`,
    );
    process.exit(1);
  }
})();
const opt = parsed.values;

type Session = {
  exit: number | null;
  turns: number;
  costUsd: number;
  peakContext: number;
  /** The loop killed the session at the session timeout. */
  timedOut?: boolean;
  /** The reason of a final `result: blocked <reason>` line of the session. */
  blocked?: string;
  /** Set for a review session. */
  role?: 'review';
};
type Tone = 'ok' | 'bad' | 'warn' | 'info' | 'dim' | 'bold';

const run = (cmd: string, args: string[], cwd?: string): string =>
  execFileSync(cmd, args, {
    cwd,
    encoding: 'utf8',
    stdio: ['ignore', 'pipe', 'pipe'],
    maxBuffer: 256 * 1024 * 1024,
  }).trim();
const attempt = (cmd: string, args: string[], cwd?: string): string | undefined => {
  try {
    return run(cmd, args, cwd);
  } catch {
    return undefined;
  }
};
// Tests only: the length of one second of waiting, so a test does not wait for real.
const TICK_MS = Number(process.env.DELIVER_TICK_MS ?? 1000);
const sleep = (seconds: number) => new Promise((done) => setTimeout(done, seconds * TICK_MS));
// Tests only: the session timeout in milliseconds instead of --session-minutes.
const SESSION_MS_OVERRIDE = Number(process.env.DELIVER_SESSION_MS) || undefined;
/** The message of a failed command: its stderr, else the error itself, first line only. */
const firstLine = (error: unknown): string =>
  String((error as { stderr?: string }).stderr || (error as Error).message || error)
    .trim()
    .split('\n')[0];

// Output: the terminal gets color when it is a terminal, the log file (tail -f) unless NO_COLOR is set.
const CODES: Record<Tone, string> = {
  ok: '32',
  bad: '31',
  warn: '33',
  info: '36',
  dim: '2',
  bold: '1',
};
const paint = (line: string, tone: Tone | undefined, on: boolean) =>
  on && tone ? `\x1b[${CODES[tone]}m${line}\x1b[0m` : line;
let logFile = '';
const say = (line: string, tone?: Tone) => {
  console.log(paint(line, tone, Boolean(process.stdout.isTTY) && !process.env.NO_COLOR));
  if (logFile) appendFileSync(logFile, `${paint(line, tone, !process.env.NO_COLOR)}\n`);
};
const log = (line: string, tone?: Tone) =>
  say(`${new Date().toTimeString().slice(0, 8)} ${line}`, tone);
const fail = (message: string, code: number): never => {
  console.error(message);
  process.exit(code);
};

if (!opt.milestone === !opt.issues)
  fail('give exactly one of --milestone <M…> or --issues <nr,…>', 64);
const whole = (name: 'max' | 'session-minutes' | 'merge-minutes', least: number): number => {
  const value = Number(opt[name]);
  if (!Number.isInteger(value) || value < least)
    fail(`--${name} needs a whole number from ${least}`, 64);
  return value;
};
const MAX = whole('max', 0);
const SESSION_MINUTES = whole('session-minutes', 1);
const MERGE_MINUTES = whole('merge-minutes', 1);
let stopped = false;
let stopSession: (() => void) | undefined;
// a stop request: Ctrl-C, or on Windows (no signal reaches the loop from another terminal) a file `stop` in the run folder
const requestStop = () => {
  stopped = true;
  stopSession?.();
};
const root =
  attempt('git', ['rev-parse', '--show-toplevel']) ??
  fail(
    'deliver-loop: not a git repository. Run it from the repository whose issues it should deliver.',
    1,
  );
const commonDir = path.resolve(root, run('git', ['rev-parse', '--git-common-dir'], root));
let config: {
  deliver?: {
    base?: string;
    harness?: string;
    tracker?: string;
    protectedPaths?: string[];
    handoverMinutes?: number;
    review?: unknown;
  };
} = {};
try {
  config = JSON.parse(readFileSync(path.join(root, '.agents/config.json'), 'utf8'));
} catch {
  // no project config
}
// How long the loop waits for handed-over pull requests when nothing else can start: the flag, then the config, else 120.
const HANDOVER_MINUTES = Number(opt['handover-minutes'] ?? config.deliver?.handoverMinutes ?? 120);
if (!Number.isInteger(HANDOVER_MINUTES) || HANDOVER_MINUTES < 0)
  fail('--handover-minutes (deliver.handoverMinutes) needs a whole number from 0', 64);
for (const warning of legacyWarnings) log(warning, 'warn');
const configWarning = unknownConfigKeys(config.deliver);
if (configWarning) log(configWarning, 'warn');
// The base: the flag, then the project config; else release/next for a milestone and the default branch otherwise.
const BASE: string =
  opt.base ??
  config.deliver?.base ??
  (opt.milestone
    ? 'release/next'
    : (attempt('git', ['ls-remote', '--symref', 'origin', 'HEAD'], root)?.match(
        /^ref: refs\/heads\/(\S+)\s+HEAD/m,
      )?.[1] ?? 'main'));
// The harness is a name (harness.mts builds the command). A command from the project config would let
// a change in the repository decide what the loop runs.
const harnessName = String(opt.harness ?? config.deliver?.harness ?? 'claude');
const harness =
  HARNESSES[harnessName] ??
  fail(`unknown harness "${harnessName}": one of ${Object.keys(HARNESSES).join(', ')}`, 64);
const MODEL = opt.model ?? harness.defaultModel;
const REVIEW: ReviewSettings = (() => {
  try {
    return reviewSettings({
      flags: { model: opt['review-model'], harness: opt['review-harness'] },
      config: config.deliver?.review,
      implementer: { harnessName, model: MODEL },
    });
  } catch (error) {
    return fail((error as Error).message, 64);
  }
})();
// A fix round per `changes` verdict, at most this many per issue and run; the next `changes` sets the issue aside.
const FIX_ROUNDS = 2;
// Tests only: a command from the environment of the user (JSON array, the prompt is appended).
const HARNESS_OVERRIDE: string[] | undefined = process.env.DELIVER_HARNESS
  ? JSON.parse(process.env.DELIVER_HARNESS)
  : undefined;
// Neither test hook may point into the repository: a merged pull request would then decide what the next run executes.
// Compared as real paths: git names the repository by its real path, a temp directory is often a symlink.
const real = (file: string): string => {
  const full = path.resolve(file);
  try {
    // native: only it expands Windows short names (RUNNER~1)
    return realpathSync.native(full);
  } catch {
    const parent = path.dirname(full);
    return parent === full ? full : path.join(real(parent), path.basename(full));
  }
};
// Another drive (Windows) gives an absolute relative path: outside as well.
const inside = (file: string) => {
  const rel = path.relative(real(root), real(file));
  return !path.isAbsolute(rel) && rel !== '..' && !rel.startsWith(`..${path.sep}`);
};
if (HARNESS_OVERRIDE?.some((part) => path.isAbsolute(part) && inside(part))) {
  fail('DELIVER_HARNESS must not name a file inside the repository', 64);
}
if (process.env.DELIVER_TRACKER_DIR && inside(process.env.DELIVER_TRACKER_DIR)) {
  fail('DELIVER_TRACKER_DIR must not be inside the repository', 64);
}
// A pull request that changes one of these is never merged by the loop: a person looks at it first.
// `dir/` is a directory at the root, anything else a file name anywhere. deliver.protectedPaths replaces the list.
// The default list is `protected-paths.json` next to the skill (`planner` reads the same file).
const PROTECTED: string[] =
  config.deliver?.protectedPaths ??
  JSON.parse(
    readFileSync(
      path.join(path.dirname(fileURLToPath(import.meta.url)), '../protected-paths.json'),
      'utf8',
    ),
  );
// Without regard to case (a checkout on macOS or Windows writes `.Claude/` into `.claude/`), and a file named
// like a protected directory counts too (a symlink `.claude`).
const protectedHits = (files: string[]): string[] =>
  files.filter((file) => {
    const name = file.toLowerCase();
    return PROTECTED.some((entry) => {
      const guarded = entry.toLowerCase();
      return guarded.endsWith('/')
        ? name.startsWith(guarded) || name === guarded.slice(0, -1)
        : name === guarded || path.posix.basename(name) === guarded;
    });
  });
// A `package.json` whose difference is only in these keys is no hit. A fixed list: a repository cannot widen it.
const DEPENDENCY_KEYS = [
  'dependencies',
  'devDependencies',
  'peerDependencies',
  'optionalDependencies',
];
const sameJson = (a: unknown, b: unknown) =>
  JSON.stringify(sortKeys(a)) === JSON.stringify(sortKeys(b));
function sortKeys(value: unknown): unknown {
  if (Array.isArray(value)) return value.map(sortKeys);
  if (value && typeof value === 'object') {
    return Object.fromEntries(
      Object.entries(value as Record<string, unknown>)
        .sort(([x], [y]) => (x < y ? -1 : x > y ? 1 : 0))
        .map(([k, v]) => [k, sortKeys(v)]),
    );
  }
  return value;
}
/**
 * The keys outside the dependency maps in which `package.json` differs between the merge base and `head`;
 * `undefined` when unknown (added, deleted, renamed, not JSON), then the file is a plain hit.
 * Throws when a read fails: the caller treats that as unknown too.
 */
function packageJsonKeys(file: string, refs: { base: string; head: string }): string[] | undefined {
  const before = tracker.fileAt(refs.base, file);
  const after = tracker.fileAt(refs.head, file);
  if (before === undefined || after === undefined) return undefined;
  let a: unknown;
  let b: unknown;
  try {
    a = JSON.parse(before);
    b = JSON.parse(after);
  } catch {
    return undefined;
  }
  const isObject = (v: unknown): v is Record<string, unknown> =>
    !!v && typeof v === 'object' && !Array.isArray(v);
  if (!isObject(a) || !isObject(b)) return undefined;
  return [...new Set([...Object.keys(a), ...Object.keys(b)])]
    .filter((key) => !DEPENDENCY_KEYS.includes(key) && !sameJson(a[key], b[key]))
    .sort();
}
/**
 * The parts outside the dependency entries in which `Cargo.toml` differs between the merge base and `head`;
 * `undefined` when unknown (added, deleted, a file the parser does not understand), then the file is a plain hit.
 */
function cargoTomlParts(file: string, refs: { base: string; head: string }): string[] | undefined {
  const before = tracker.fileAt(refs.base, file);
  const after = tracker.fileAt(refs.head, file);
  if (before === undefined || after === undefined) return undefined;
  try {
    return cargoTomlKeys(before, after);
  } catch {
    return undefined;
  }
}
/**
 * The top-level parts outside the catalog entries in which `pnpm-workspace.yaml` differs between the merge base and
 * `head`; `undefined` when unknown (added, deleted, a file the strict parser does not understand).
 */
function pnpmWorkspaceParts(
  file: string,
  refs: { base: string; head: string },
): string[] | undefined {
  const before = tracker.fileAt(refs.base, file);
  const after = tracker.fileAt(refs.head, file);
  if (before === undefined || after === undefined) return undefined;
  try {
    return pnpmWorkspaceKeys(before, after);
  } catch {
    return undefined;
  }
}
/**
 * The parts outside the requirement lists in which `pyproject.toml` differs between the merge base and `head`;
 * `undefined` when unknown (added, deleted, a file the strict parser does not understand).
 */
function pyprojectTomlParts(
  file: string,
  refs: { base: string; head: string },
): string[] | undefined {
  const before = tracker.fileAt(refs.base, file);
  const after = tracker.fileAt(refs.head, file);
  if (before === undefined || after === undefined) return undefined;
  try {
    return pyprojectTomlKeys(before, after);
  } catch {
    return undefined;
  }
}
/**
 * The directives other than `require` in which `go.mod` differs between the merge base and `head`;
 * `undefined` when unknown (added, deleted, a line the strict parser does not understand).
 */
function goModParts(file: string, refs: { base: string; head: string }): string[] | undefined {
  const before = tracker.fileAt(refs.base, file);
  const after = tracker.fileAt(refs.head, file);
  if (before === undefined || after === undefined) return undefined;
  try {
    return goModKeys(before, after);
  } catch {
    return undefined;
  }
}
/**
 * The parts outside the `PackageVersion` items in which `Directory.Packages.props` differs between the merge base and
 * `head`; `undefined` when unknown (added, deleted, markup the strict parser does not understand).
 */
function directoryPackagesPropsParts(
  file: string,
  refs: { base: string; head: string },
): string[] | undefined {
  const before = tracker.fileAt(refs.base, file);
  const after = tracker.fileAt(refs.head, file);
  if (before === undefined || after === undefined) return undefined;
  try {
    return directoryPackagesPropsKeys(before, after);
  } catch {
    return undefined;
  }
}
/** The files with a comparator, by lower-case name: what differs outside the allowed parts, `undefined` when unknown. */
const COMPARATORS: Record<string, typeof packageJsonKeys> = {
  'package.json': packageJsonKeys,
  'cargo.toml': cargoTomlParts,
  'pnpm-workspace.yaml': pnpmWorkspaceParts,
  'pyproject.toml': pyprojectTomlParts,
  'go.mod': goModParts,
  'directory.packages.props': directoryPackagesPropsParts,
};
/** The protected hits, a `package.json`, `Cargo.toml`, `pnpm-workspace.yaml`, `pyproject.toml`, `go.mod` or `Directory.Packages.props` that differs only in its dependencies dropped, the others named with their parts. */
function judgeHits(pull: Pull, files: string[], head?: string): string[] {
  const hits = protectedHits(files);
  if (!hits.some((file) => Object.hasOwn(COMPARATORS, path.posix.basename(file.toLowerCase()))))
    return hits;
  let refs: { base: string; head: string } | undefined;
  try {
    refs = tracker.compareRefs(pull, head);
  } catch {
    return hits;
  }
  const out: string[] = [];
  for (const file of hits) {
    // the case-folded name matches the manifest; the file itself is read under its own name
    const lower = file.toLowerCase();
    const inDirectory = PROTECTED.some(
      (entry) => entry.endsWith('/') && lower.startsWith(entry.toLowerCase()),
    );
    const name = path.posix.basename(lower);
    const compare = Object.hasOwn(COMPARATORS, name) ? COMPARATORS[name] : undefined;
    if (inDirectory || !compare) {
      out.push(file);
      continue;
    }
    let keys: string[] | undefined;
    try {
      keys = compare(file, refs);
    } catch {
      keys = undefined;
    }
    if (keys === undefined) out.push(file);
    else if (keys.length > 0) out.push(`${file}: ${keys.join(', ')}`);
  }
  return out;
}
/** Why the loop does not merge the pull request as it is now, else nothing. A person decides then, no repair session. */
const HANDS_OFF = ', merge it yourself';
const GUARD_READS = 3;
async function guard(pull: Pull, head?: string): Promise<string | undefined> {
  let hits: string[] = [];
  for (let read = 1; ; read++) {
    try {
      hits = judgeHits(pull, tracker.changedFiles(pull), head);
      break;
    } catch (error) {
      // a read that fails once (server load, rate limit) is asked again, as the gate is
      if (read < GUARD_READS && !stopped) {
        for (let second = 0; second < 5 && !stopped; second++) await sleep(1);
        continue;
      }
      // unknown is not safe: a person looks
      return `could not read the changed files (${firstLine(error)})${HANDS_OFF}`;
    }
  }
  if (hits.length === 0) return undefined;
  return `changes protected paths (${hits.slice(0, 3).join(', ')}${hits.length > 3 ? ', …' : ''})${HANDS_OFF}`;
}
const MORE_PULLS = 'more than one open pull request';
/** Two open pull requests of one item: a person closes the stale one, the loop merges neither. */
function morePulls(pull: Pull | undefined): string | undefined {
  if (!pull?.others?.length) return undefined;
  return `${MORE_PULLS} (${[pull.url, ...pull.others].join(', ')})${HANDS_OFF}`;
}
/** The reason of a `result: blocked` session is kept next to the reason of the loop. */
function withBlocked(reason: string, session: Session): string {
  if (!session.blocked) return reason;
  const note = `session: blocked ${session.blocked}`;
  return reason.endsWith(HANDS_OFF)
    ? `${reason.slice(0, -HANDS_OFF.length)} (${note})${HANDS_OFF}`
    : `${reason} (${note})`;
}
// The session acts with a token instead of the login of the user: an installation token of the GitHub App
// (DELIVER_APP_ID and a key file or command), else DELIVER_SESSION_TOKEN (a token limited to this repository).
let app: ReturnType<typeof appSettings>;
try {
  app = appSettings();
} catch (error) {
  fail(`github app: ${(error as Error).message}`, 64);
}
const FIXED_TOKEN = app ? undefined : process.env.DELIVER_SESSION_TOKEN;
const TOKEN_SOURCE = app ? 'GitHub App' : FIXED_TOKEN ? 'DELIVER_SESSION_TOKEN' : undefined;
const runId = `${new Date().toISOString().replace(/[-:]/g, '').slice(0, 15)}-${process.pid}`;
const runDir = path.join(commonDir, 'deliver', runId);
const tracker = await loadTracker(opt.tracker ?? config.deliver?.tracker ?? 'github', {
  cwd: root,
  base: BASE,
  selection: { milestone: opt.milestone, issues: opt.issues?.split(',').map((nr) => nr.trim()) },
  trustAuthors: opt['trust-authors'],
  stopFile: path.join(runDir, 'stop'),
  onStop: requestStop,
  onExhausted: (error) => {
    emit('run.finished', { outcome: 'failed', text: firstLine(error).slice(0, 300) }, 'danger');
    log(`run failed: rate limit, ${firstLine(error)}`, 'bad');
    process.exit(1);
  },
  log: (text) => log(text, 'warn'),
}).catch((error) => fail(`tracker: ${firstLine(error)}`, 64));
const tokenSource = app ? makeSource(app, tracker.repo, SESSION_MINUTES * 60_000) : undefined;
let emit: Emit = () => {};

/** One line of the stream-json of Claude Code, as far as the loop reads it. */
type StreamRecord = {
  type?: string;
  num_turns?: number;
  total_cost_usd?: number;
  is_error?: boolean;
  result?: string;
  parent_tool_use_id?: string | null;
  message?: {
    usage?: Record<string, number | undefined>;
    content?: { type: string; text?: string; name?: string; input?: Record<string, unknown> }[];
  };
};

const noteBlocked = (text: string, session: Session): void => {
  const blocked = /^result:\s*blocked\s+(.+)$/.exec(text.trim());
  if (blocked) session.blocked = blocked[1]?.slice(0, 300);
};

const RESULT_GRACE_MS = Number(process.env.DELIVER_RESULT_GRACE_MS ?? 30_000);

/** True for the final `result` record of a session whose text ends with a `result:` line. */
function finishedResult(line: string): boolean {
  if (!line.startsWith('{')) return false;
  try {
    const event: StreamRecord = JSON.parse(line);
    if (event?.type !== 'result' || event.is_error || typeof event.result !== 'string')
      return false;
    const last = event.result.trimEnd().split('\n').pop() ?? '';
    return /^result: (pr \S+|needs-decision|reviewed (approve|changes)|blocked\b.*)$/.test(
      last.trim(),
    );
  } catch {
    return false;
  }
}

/** Prints the readable part of one stream record and keeps the numbers of the session. */
function render(line: string, session: Session): void {
  let event: StreamRecord;
  try {
    event = JSON.parse(line);
  } catch {
    noteBlocked(line, session);
    say(`  ${line}`);
    return;
  }
  if (!event || typeof event !== 'object') return;
  if (event.type === 'result') {
    session.turns = event.num_turns ?? 0;
    session.costUsd = event.total_cost_usd ?? 0;
  }
  if (event.type !== 'assistant') return;
  const main = !event.parent_tool_use_id;
  const usage = event.message?.usage;
  if (main && usage) {
    const context =
      (usage.input_tokens ?? 0) +
      (usage.cache_read_input_tokens ?? 0) +
      (usage.cache_creation_input_tokens ?? 0);
    session.peakContext = Math.max(session.peakContext, context);
  }
  for (const block of event.message?.content ?? []) {
    if (block.type === 'text' && main) {
      for (const text of String(block.text).split('\n')) {
        if (!text.trim()) continue;
        noteBlocked(text, session);
        say(
          `  ${text.trim().slice(0, 160)}`,
          text.trim().startsWith('result:') ? 'info' : undefined,
        );
      }
    }
    if (block.type === 'tool_use') {
      const input = block.input ?? {};
      const what = String(input.description ?? input.file_path ?? input.command ?? '');
      say(`  ${main ? '' : '  '}· ${block.name} ${what.split('\n')[0].slice(0, 100)}`, 'dim');
    }
  }
}

/** Writes the stage of the issue after the event. A failing call is logged: the label never decides about the run. */
function mark(item: Item, event: StageEvent): void {
  try {
    tracker.setStage(item, stageAfter(event));
  } catch (error) {
    log(`#${item.id} stage label not written: ${firstLine(error)}`, 'warn');
  }
}

/** Comments on the issue, once per marker. A failing call is logged, as for the label. */
function note(item: Item, { marker, body }: Note): void {
  try {
    tracker.commentOnce(item, marker, body);
  } catch (error) {
    log(`#${item.id} issue comment not written: ${firstLine(error)}`, 'warn');
  }
}

/** Tells the person on the issue why it is theirs now, once per reason and head of the pull request. */
function tell(item: Item, pull: Pull | undefined, reason: string, planned: boolean): void {
  let head = 'none';
  if (pull) {
    try {
      head = tracker.compareRefs(pull).head;
    } catch {
      head = 'unknown';
    }
  }
  const urls = pull ? [pull.url, ...(pull.others ?? [])] : [];
  note(item, handoverNote({ reason, planned, head, urls, risk: risks.get(item.id) }));
}

/**
 * The next step of an issue; `merge` names the approved commit, `reviewFile` holds the findings the implementer
 * fixes.
 */
type Step =
  | { step: 'implement'; reviewFile?: string }
  | { step: 'review' }
  | { step: 'merge'; head: string };

/**
 * Where the label of an earlier run lets the loop go on with an open pull request. `approved` counts only with the
 * approving verdict the loop recorded for the current head, else the pull request is reviewed. The marks of the
 * earlier run also give back its risk level and, for `changes`, the findings of the last review.
 */
function resumeAt(item: Item, pull: Pull): Step {
  const stage = stageOf(item.labels);
  let marks: ReviewMark[] | undefined;
  try {
    marks = tracker.recordedVerdicts(pull);
  } catch (error) {
    log(`#${item.id} review marks unreadable: ${firstLine(error)}`, 'warn');
  }
  const risk = [...(marks ?? [])].reverse().find((m) => m.risk)?.risk;
  if (risk && !risks.has(item.id)) risks.set(item.id, risk);
  if (stage === 'approved') {
    if (marks) {
      try {
        const { head } = tracker.compareRefs(pull);
        const latest = [...marks].reverse().find((m) => m.head === head);
        if (latest?.verdict === 'approve') return { step: 'merge', head };
        log(
          `#${item.id} agent:approved without an approving review of ${head}: reviews it`,
          'warn',
        );
      } catch (error) {
        log(`#${item.id} review verdict unreadable: ${firstLine(error)}, reviews it`, 'warn');
      }
    }
    return { step: 'review' };
  }
  if (stage === 'review') return { step: 'review' };
  const last = marks?.at(-1);
  if (stage === 'changes' && last?.review) {
    const reviewFile = path.join(runDir, `${item.id}.review-recorded.json`);
    writeFileSync(reviewFile, `${JSON.stringify(last.review, null, 2)}\n`);
    return { step: 'implement', reviewFile };
  }
  if (stage === 'changes')
    log(`#${item.id} no recorded findings: the implementer starts without them`, 'warn');
  return { step: 'implement' };
}

const risks = new Map<number, string>();
/** The implementer result the session writes (`risk.level`); the session never reports anything else to the loop. */
const resultFile = (item: Item) => path.join(runDir, `${item.id}.result.json`);
function riskOf(item: Item): string | undefined {
  try {
    const level = JSON.parse(readFileSync(resultFile(item), 'utf8'))?.risk?.level;
    return RISK_LEVELS.includes(level) ? level : undefined;
  } catch {
    return undefined;
  }
}

/** What one session is for: the implementer of the issue, or the review of its pull request. */
type Task = {
  /** The slash command; the loop appends the issue file. */
  prompt: string;
  harness: Harness;
  model?: string;
  effort?: string;
  /** The file the session writes its answer to, and the variable that names it. */
  output: { name: 'DELIVER_RESULT_FILE' | 'DELIVER_REVIEW_FILE'; file: string };
  /** A failed session that left its work behind is no harness failure. */
  delivered(): boolean;
  role?: 'review';
};

/** The implementer session; `reviewFile` is the verdict whose findings it fixes. */
const implementer = (item: Item, reviewFile?: string): Task => ({
  prompt: `/deliver ${item.id} --base ${BASE}${reviewFile ? ` --review-file ${promptPath(reviewFile)}` : ''}`,
  harness,
  model: opt.model,
  output: { name: 'DELIVER_RESULT_FILE', file: resultFile(item) },
  // a pull request means the session did its work: the normal rules for a failed session apply
  delivered: () => Boolean(tracker.pullOf(item)),
});

async function startSession(item: Item, cwd: string, file: string, task: Task): Promise<Session> {
  // the session works from this text, not from what the tracker shows when it runs
  const issueFile = path.join(runDir, `${item.id}.issue.md`);
  writeFileSync(issueFile, `# ${item.title}\n\n${item.url}\n\n${item.body}\n`);
  const prompt = `${task.prompt} --issue-file ${promptPath(issueFile)}`;
  const argv = HARNESS_OVERRIDE
    ? [...HARNESS_OVERRIDE, prompt]
    : task.harness.command({ prompt, model: task.model, effort: task.effort });
  // an app token is renewed when less than the session timeout is left; the merge keeps the login of the user
  const sessionToken = tokenSource ? await tokenSource.current() : undefined;
  if (sessionToken?.renewed) log('app token minted for this session', 'dim');
  const token = sessionToken?.token ?? FIXED_TOKEN;
  const env: NodeJS.ProcessEnv = {
    // -p stops subagents 10 minutes after the last turn unless the ceiling is off
    CLAUDE_CODE_PRINT_BG_WAIT_CEILING_MS: '0',
    ...process.env,
    DELIVER_RUN: runId,
    ...(token ? tracker.sessionEnv(token) : {}),
  };
  // the session gets the token, never the way to mint one
  for (const name of [
    'DELIVER_SESSION_TOKEN',
    'DELIVER_APP_ID',
    'DELIVER_APP_KEY_FILE',
    'DELIVER_APP_KEY_COMMAND',
    'DELIVER_APP_API_URL',
    // a session gets only the file it writes
    'DELIVER_RESULT_FILE',
    'DELIVER_REVIEW_FILE',
  ])
    delete env[name];
  env[task.output.name] = task.output.file;
  // the answer of an earlier session is not the answer of this one
  rmSync(task.output.file, { force: true });
  const session: Session = {
    exit: null,
    turns: 0,
    costUsd: 0,
    peakContext: 0,
    ...(task.role && { role: task.role }),
  };
  const out = createWriteStream(file);
  return new Promise((done) => {
    const child = launch(argv, {
      cwd,
      stdio: ['ignore', 'pipe', 'inherit'],
      env,
    });
    const timer = setTimeout(
      () => {
        session.timedOut = true;
        killTree(child);
      },
      SESSION_MS_OVERRIDE ?? SESSION_MINUTES * 60_000,
    );
    const stop = () => killTree(child);
    stopSession = stop;
    // a session that scheduled a wakeup prints its final result and then stays alive until the wakeup fires:
    // once the final `result:` line is in and nothing follows for a moment, the loop ends the process itself
    let grace: ReturnType<typeof setTimeout> | undefined;
    let finished = false;
    createInterface({ input: child.stdout! }).on('line', (line) => {
      out.write(`${line}\n`);
      clearTimeout(grace);
      if (finishedResult(line)) {
        grace = setTimeout(() => {
          finished = true;
          log(`#${item.id} session printed its result and stays alive: ended by the loop`, 'dim');
          killTree(child);
        }, RESULT_GRACE_MS);
      }
      if (task.harness.claudeStream) render(line, session);
      else {
        // no stream format: the plain lines of the harness, without color codes and empty lines
        const plain = line.replace(/\u001b\[[0-9;]*[A-Za-z]/g, '').trimEnd();
        if (plain) {
          noteBlocked(plain, session);
          say(`  ${plain}`);
        }
      }
    });
    child.on('error', (error) => log(`#${item.id} session: ${error.message}`, 'bad'));
    child.on('close', (code) => {
      clearTimeout(timer);
      clearTimeout(grace);
      if (stopSession === stop) stopSession = undefined;
      out.end();
      session.exit = finished ? 0 : code;
      done(session);
    });
  });
}

const CLOSED = 'pull request closed';

// Harness failures (exit ≠ 0, no pull request, no timeout kill): wait 5, 15, 60 minutes, the third in a row ends the run.
const HARNESS_WAITS_MINUTES = [5, 15, 60];
const HARNESS_FAILED = 'harness failed';
let harnessFailures = 0;
let harnessDead: { count: number; exit: number | null } | undefined;
// a session timeout gets one more session per PBI and run, the second sets the PBI aside
const timeouts = new Map<number, number>();

/** Starts sessions for the item until one ends normally. Returns a reason when the item is to be set aside. */
async function runSession(
  item: Item,
  worktree: string,
  sessions: Session[],
  task: Task = implementer(item),
): Promise<{ session: Session; reason?: string }> {
  for (;;) {
    const session = await startSession(
      item,
      worktree,
      path.join(runDir, `${item.id}-${sessions.length + 1}.jsonl`),
      task,
    );
    sessions.push(session);
    if (stopped) return { session };
    if (session.timedOut) {
      const count = (timeouts.get(item.id) ?? 0) + 1;
      timeouts.set(item.id, count);
      if (count >= 2) return { session, reason: 'session timeout' };
      log(`#${item.id} session timeout (${SESSION_MINUTES} min), one more session`, 'warn');
      continue;
    }
    if (session.exit === 0) {
      harnessFailures = 0;
      return { session };
    }
    if (task.delivered()) return { session };
    harnessFailures += 1;
    if (harnessFailures >= HARNESS_WAITS_MINUTES.length) {
      harnessDead = { count: harnessFailures, exit: session.exit };
      return { session, reason: HARNESS_FAILED };
    }
    const minutes = HARNESS_WAITS_MINUTES[harnessFailures - 1];
    log(
      `#${item.id} harness failed (exit ${session.exit}, ${harnessFailures} in a row): waits ${minutes} min, then a fresh session`,
      'warn',
    );
    for (let half = 0; half < minutes * 2 && !stopped; half++) await sleep(30);
    if (stopped) return { session };
  }
}

// A review without a valid verdict gets one more session, then the issue is set aside.
const REVIEW_ATTEMPTS = 2;

type Review = { round: number; verdict: Verdict['verdict'] | 'failed' };
type Reviewed = { verdict: Verdict; file: string; head: string } | { reason: string };

/**
 * One review round: a session of its own in a fresh worktree at the head of the pull request, which writes its
 * verdict to a file. `head` is the commit that was reviewed: the only one the loop may merge.
 */
async function review(
  item: Item,
  pull: Pull,
  round: number,
  sessions: Session[],
): Promise<Reviewed> {
  const worktree = path.join(path.dirname(root), `${path.basename(root)}-wt-${item.id}-review`);
  const drop = () => attempt('git', ['worktree', 'remove', '--force', worktree], root);
  let problem = 'no valid verdict';
  for (let tries = 1; tries <= REVIEW_ATTEMPTS; tries++) {
    const file = path.join(runDir, `${item.id}.review-${round}-${tries}.json`);
    try {
      const { head } = tracker.compareRefs(pull);
      // a failed fetch shows in the checkout: the commit is then missing
      attempt('git', ['fetch', '--quiet', 'origin', pull.branch], root);
      drop();
      run('git', ['worktree', 'add', '--quiet', '--detach', worktree, head], root);
      const ran = await runSession(item, worktree, sessions, {
        prompt: `/deliver-review ${item.id} --base ${BASE} --pr ${pull.url}`,
        harness: REVIEW.harness,
        model: REVIEW.model,
        effort: REVIEW.effort,
        output: { name: 'DELIVER_REVIEW_FILE', file },
        delivered: () => existsSync(file),
        role: 'review',
      });
      if (stopped) return { reason: 'stopped' };
      if (ran.reason) return { reason: ran.reason };
      if (ran.session.exit !== 0) problem = `session failed (exit ${ran.session.exit})`;
      else {
        const verdict = existsSync(file) ? parseVerdict(readFileSync(file, 'utf8')) : undefined;
        if (verdict) return { verdict, file, head };
        problem = existsSync(file) ? 'invalid verdict' : 'no verdict';
      }
    } catch (error) {
      problem = firstLine(error);
    } finally {
      drop();
    }
    if (stopped) return { reason: 'stopped' };
    if (tries < REVIEW_ATTEMPTS) log(`#${item.id} review: ${problem}, one retry`, 'warn');
  }
  return { reason: `review failed: ${problem}` };
}

/**
 * Tracker calls block the event loop: a stop request that came in meanwhile is only read in the poll phase.
 * A timer runs before the poll phase, and from a poll callback the check phase follows without a new poll:
 * two setImmediate turns always include one full poll.
 */
async function seeStop(): Promise<void> {
  await new Promise((turn) => setImmediate(turn));
  await new Promise((turn) => setImmediate(turn));
}

// a handed-over pull request with a conflict gets one repair session per run, never a merge by the loop
const conflictTried = new Set<number>();
/**
 * Returns nothing when the gate of the pull request is no conflict (or it was tried in this run), else one line:
 * `conflict repaired` or `conflict not repaired: <why>` after one session (`/deliver <nr>` merges the base in).
 */
async function repairConflict(
  item: Item,
  pull: Pull,
  worktree: string,
  sessions: Session[],
): Promise<string | undefined> {
  if (conflictTried.has(item.id) || stopped) return undefined;
  const read = (): Gate | undefined => {
    try {
      return tracker.gate(pull);
    } catch {
      // a failing read is no verdict: no session
      return undefined;
    }
  };
  if (read()?.state !== 'conflict') return undefined;
  conflictTried.add(item.id);
  log(`#${item.id} ${pull.url} has a conflict, one repair session`, 'warn');
  mark(item, 'session-started');
  const ran = await runSession(item, worktree, sessions);
  if (stopped) return undefined;
  if (ran.reason) return `conflict not repaired: ${ran.reason}`;
  if (ran.session.exit !== 0)
    return `conflict not repaired: session failed (exit ${ran.session.exit})`;
  const after = read();
  if (after?.state === 'conflict') return `conflict not repaired: ${after.detail}`;
  return 'conflict repaired';
}

/** Waits for the gate and merges the reviewed commit. Returns nothing when merged, else the reason. */
async function merge(pull: Pull, reviewed: string): Promise<string | undefined> {
  const start = Date.now();
  let problem = '';
  while (!stopped) {
    let gate: Gate;
    try {
      gate = tracker.gate(pull);
    } catch (error) {
      // a tracker call that fails once (network, rate limit) is not a verdict on the pull request
      problem = `tracker error: ${firstLine(error)}`;
      gate = { state: 'pending', reported: false };
    }
    if (gate.state === 'merged') return undefined;
    if (gate.state === 'closed') return CLOSED;
    if (gate.state === 'conflict') return gate.detail;
    if (gate.state === 'red') return `red: ${gate.detail}`;
    // checks register a moment after the pull request opens: nothing reported is not green yet
    if (gate.state === 'ready' && (gate.reported || Date.now() - start > 90_000)) {
      await seeStop();
      if (stopped) break;
      // only the approved commit is merged: what was pushed after the review was seen by nobody
      if (gate.head !== reviewed) return `pushed after the review${HANDS_OFF}`;
      // the files of the commit that is merged, not of the one the session left: a later push is checked too.
      // The merge names gate.head, so a push after this read is refused by the tracker.
      const handsOff = await guard(pull, gate.head);
      if (handsOff) return handsOff;
      // the read of the files blocks too: a stop request that came in during it must be seen before the merge
      await seeStop();
      if (stopped) break;
      try {
        tracker.merge(pull, gate.head);
        return undefined;
      } catch (error) {
        problem = `merge refused: ${(error as Error).message}`;
      }
    }
    if (Date.now() - start > MERGE_MINUTES * 60_000) return problem || 'checks did not finish';
    for (let second = 0; second < 30 && !stopped; second++) await sleep(1);
  }
  return 'stopped';
}

function cleanup(item: Item, pull: Pull): void {
  const worktree = path.join(path.dirname(root), `${path.basename(root)}-wt-${item.id}`);
  attempt('git', ['worktree', 'remove', '--force', worktree], root);
  attempt('git', ['branch', '-D', pull.branch], root);
  tracker.dropBranch(pull);
}

async function deliver(item: Item): Promise<{ reason?: string; pull?: Pull }> {
  const task = `issue-${item.id}`;
  // merged by someone else a moment ago, the issue is not closed yet: nothing to start
  const already = tracker.mergedPullOf(item);
  if (already) {
    log(`#${item.id} merged by someone else: ${already.url}, no session`, 'info');
    cleanup(item, already);
    appendFileSync(
      path.join(runDir, 'results.jsonl'),
      `${JSON.stringify({ issue: item.id, title: item.title, outcome: 'merged', pr: already.url, minutes: 0, sessions: [] })}\n`,
    );
    return { pull: already };
  }
  const worktree = path.join(path.dirname(root), `${path.basename(root)}-wt-${item.id}`);
  run('git', ['fetch', '--quiet', 'origin', BASE], root);
  if (!existsSync(worktree))
    run('git', ['worktree', 'add', '--detach', worktree, `origin/${BASE}`], root);
  const sessions: Session[] = [];
  let pull: Pull | undefined;
  let reason: string | undefined;
  const started = Date.now();
  // a pull request of an earlier run that changes a protected path, or two open pull requests of the item, are
  // handed over before a session is paid for; a failed read is no verdict here, the guard after the session asks again
  let earlier = tracker.pullOf(item);
  if (earlier) {
    let early = morePulls(earlier) ?? (await guard(earlier));
    let note: string | undefined;
    if (early?.startsWith('changes protected paths')) {
      note = await repairConflict(item, earlier, worktree, sessions);
      if (note) {
        // the repair may have changed the pull request: the guard runs again
        earlier = tracker.pullOf(item) ?? earlier;
        early = morePulls(earlier) ?? (await guard(earlier));
        if (early) early = `${note}, ${early}`;
        else log(`#${item.id} ${note}`, 'info');
      }
    }
    if (
      early &&
      (early.includes('changes protected paths') || early.startsWith(MORE_PULLS)) &&
      !stopped
    ) {
      if (note) log(`#${item.id} ${note}`, note.startsWith('conflict repaired') ? 'info' : 'warn');
      appendFileSync(
        path.join(runDir, 'results.jsonl'),
        `${JSON.stringify({
          issue: item.id,
          title: item.title,
          outcome: 'aside',
          reason: early,
          pr: earlier.url,
          minutes: 0,
          sessions,
        })}\n`,
      );
      log(`#${item.id} worktree kept: ${worktree}`, 'warn');
      return { reason: early, pull: earlier };
    }
    if (stopped) return { reason: 'stopped', pull: earlier };
  }
  const reviews: Review[] = [];
  let fixes = 0;
  let reviewFile: string | undefined;
  let repaired = false;
  let announced = false;
  // a pull request of an earlier run: the label says whether it is built, reviewed or merged next
  let next: Step = earlier ? resumeAt(item, earlier) : { step: 'implement' };
  if (next.step !== 'implement') log(`#${item.id} resumes at the ${next.step}`, 'info');
  else if (next.reviewFile) {
    log(`#${item.id} resumes with the recorded findings`, 'info');
    reviewFile = next.reviewFile;
  }
  for (;;) {
    const from = next;
    next = { step: 'implement' };
    let session: Session | undefined;
    const settle = () => {
      if (reason && session) reason = withBlocked(reason, session);
    };
    reason = undefined;
    if (from.step === 'implement') {
      mark(item, 'session-started');
      const ran = await runSession(item, worktree, sessions, implementer(item, reviewFile));
      reviewFile = undefined;
      // read after every implementer session: the next one (a fix round) starts without this file
      const level = riskOf(item);
      if (level) risks.set(item.id, level);
      session = ran.session;
      pull = tracker.pullOf(item);
      // a stopped or failed session never leads to a merge, also when a pull request is open
      if (stopped) reason = 'stopped';
      else if (ran.reason) reason = ran.reason;
      else if (session.exit !== 0) reason = `session failed (exit ${session.exit})`;
      // the session asked a question: a pull request of an earlier run is not its answer
      else if (tracker.needsDecision(item)) reason = 'needs-decision';
      else if (!pull) {
        // merged by hand while the session ran: delivered, not set aside
        const byOther = tracker.mergedPullOf(item);
        if (byOther) {
          log(`#${item.id} merged by someone else: ${byOther.url}`, 'info');
          pull = byOther;
          break;
        }
        reason = 'session ended without a pull request';
      }
    } else {
      pull = tracker.pullOf(item);
      if (stopped) reason = 'stopped';
      // closed or merged since the labels were read: the implementer looks at it
      else if (!pull) continue;
    }
    if (reason || !pull) {
      settle();
      break;
    }
    // before the review: a session never runs on a pull request the guard hands over, because a checkout of
    // protected paths (agent settings, hooks) would run with the rights of the loop
    reason = morePulls(pull) ?? (await guard(pull));
    if (reason) {
      settle();
      break;
    }
    if (!announced) {
      announced = true;
      emit('step.finished', {
        task,
        step: { name: 'pr', text: 'pull request opened' },
        links: [{ rel: 'pull-request', url: pull.url, title: pull.title }],
      });
    }
    let head: string;
    if (from.step === 'merge') head = from.head;
    else {
      const round = reviews.length + 1;
      log(`#${item.id} pr: ${pull.url}, review round ${round}`, 'info');
      mark(item, 'pull-opened');
      const reviewed = await review(item, pull, round, sessions);
      if ('reason' in reviewed) {
        reason = reviewed.reason;
        if (reason === 'stopped') break;
      }
      const verdict = 'reason' in reviewed ? 'failed' : reviewed.verdict.verdict;
      reviews.push({ round, verdict });
      emit(
        'step.finished',
        { task, step: { name: `review-${round}`, text: `review round ${round}: ${verdict}` } },
        verdict === 'approve' ? 'success' : 'warning',
      );
      log(`#${item.id} review round ${round}: ${verdict}`, verdict === 'approve' ? 'ok' : 'warn');
      if ('reason' in reviewed) {
        settle();
        break;
      }
      // the mark first: a run that dies in between reviews again
      try {
        tracker.recordVerdict(pull, reviewed.verdict, reviewed.head, risks.get(item.id));
      } catch (error) {
        log(`#${item.id} review verdict not recorded: ${firstLine(error)}`, 'warn');
      }
      mark(item, `verdict-${reviewed.verdict.verdict}`);
      note(item, reviewNote(round, reviewed.verdict, reviewed.head, pull.url));
      if (reviewed.verdict.verdict === 'changes') {
        if (fixes >= FIX_ROUNDS) {
          // the findings are text of an agent: it never ends the reason, whose ending decides about a hand-over
          reason = `needs-decision: review still asks for changes after ${FIX_ROUNDS} fix rounds (${findingsLine(reviewed.verdict)})`;
          settle();
          break;
        }
        fixes += 1;
        reviewFile = reviewed.file;
        continue;
      }
      head = reviewed.head;
    }
    log(`#${item.id} approved, waits for checks`, 'info');
    reason = await merge(pull, head);
    // a person closed it or has to look at it: no repair session
    if (!reason || reason === 'stopped' || reason === CLOSED || reason.endsWith(HANDS_OFF)) break;
    log(`#${item.id} ${reason}${repaired ? '' : ', one repair session'}`, 'warn');
    if (repaired) break;
    repaired = true;
    emit('step.finished', { task, step: { name: 'repair', text: reason } }, 'warning');
  }
  if (pull && !reason) {
    // merged: the worktree and the branches can go
    cleanup(item, pull);
  } else {
    // not merged: the worktree may hold work that is not pushed
    log(`#${item.id} worktree kept: ${worktree}`, 'warn');
  }
  const risk = risks.get(item.id);
  appendFileSync(
    path.join(runDir, 'results.jsonl'),
    `${JSON.stringify({
      issue: item.id,
      title: item.title,
      risk,
      outcome: !reason ? 'merged' : reason === 'stopped' ? 'stopped' : 'aside',
      reason,
      pr: pull?.url,
      minutes: Math.round((Date.now() - started) / 60_000),
      reviews,
      sessions,
    })}\n`,
  );
  return { reason, pull };
}

/** The tracker closes an issue a moment after its merge; until then it still blocks its dependents. */
async function untilClosed(id: number): Promise<void> {
  for (let second = 0; second < 120 && !stopped; second += 5) {
    let closed = false;
    try {
      closed = !tracker.items().some((item) => item.id === id);
    } catch {
      // asked again
    }
    if (closed) break;
    for (let wait = 0; wait < 5 && !stopped; wait++) await sleep(1);
  }
}

/**
 * One repair session for a handed-over pull request that conflicts with the base (at most one per pull request and
 * run); the note goes into the hand-over reason and the line of the issue in results.jsonl. The loop never merges it.
 */
async function repairHandedOver(nr: number): Promise<void> {
  const entry = handedPulls.get(nr);
  const reason = handedOver.get(nr);
  if (!entry || stopped || harnessDead || !reason || reason.includes(MORE_PULLS)) return;
  const { item, pull } = entry;
  const worktree = path.join(path.dirname(root), `${path.basename(root)}-wt-${nr}`);
  if (!existsSync(worktree))
    run('git', ['worktree', 'add', '--detach', worktree, `origin/${BASE}`], root);
  const sessions: Session[] = [];
  const repaired = await repairConflict(item, pull, worktree, sessions).catch(
    (error) => `conflict not repaired: ${firstLine(error)}`,
  );
  if (!repaired) return;
  mark(item, 'handed-over');
  log(
    `#${nr} ${repaired}, handed over`,
    repaired.startsWith('conflict repaired') ? 'info' : 'warn',
  );
  handedOver.set(nr, `${repaired}, ${reason}`);
  tell(item, pull, `${repaired}, ${reason}`, true);
  // the line of the issue in results.jsonl says it too
  const file = path.join(runDir, 'results.jsonl');
  const rows = readFileSync(file, 'utf8')
    .split('\n')
    .filter(Boolean)
    .map((line) => {
      const row = JSON.parse(line) as Record<string, unknown>;
      if (row.issue !== nr || typeof row.reason !== 'string') return line;
      const all = [...((row.sessions as Session[]) ?? []), ...sessions];
      return JSON.stringify({ ...row, reason: `${repaired}, ${row.reason}`, sessions: all });
    });
  writeFileSync(file, `${rows.join('\n')}\n`);
}

/**
 * Nothing can start and pull requests are handed over: polls them every 60 s for up to HANDOVER_MINUTES.
 * A merged one counts as merged, a closed one is set aside, a conflicting one gets its repair session.
 * Returns true when one was merged or closed (the loop looks for the next issue), false on time over or stop.
 */
async function awaitHandovers(): Promise<boolean> {
  if (HANDOVER_MINUTES === 0 || handedPulls.size === 0) return false;
  const until = Date.now() + HANDOVER_MINUTES * 60_000 * Math.max(TICK_MS / 1000, 0.0001);
  const names = () =>
    [...handedPulls.values()].map(({ item, pull }) => `#${item.id} ${pull.url}`).join(', ');
  log(`waiting for handed-over pull requests (up to ${HANDOVER_MINUTES} min): ${names()}`, 'warn');
  emit(
    'attention',
    {
      id: 'handover-wait',
      reason: 'handed-over',
      text: `waiting for you to merge: ${names()}`,
      needsYou: true,
      links: [...handedPulls.values()].map(({ pull }) => ({
        rel: 'pull-request',
        url: pull.url,
      })),
    },
    'warning',
  );
  while (!stopped && !harnessDead && Date.now() < until) {
    let changed = false;
    for (const [nr, { item, pull }] of handedPulls) {
      let gate: Gate | undefined;

      try {
        gate = tracker.gate(pull);
      } catch {
        // a failing read is no verdict: asked again
      }
      if (stopped) return false;
      const row = { issue: nr, title: item.title, pr: pull.url, minutes: 0, sessions: [] };
      const file = path.join(runDir, 'results.jsonl');
      if (gate?.state === 'merged') {
        handedOver.delete(nr);
        handedPulls.delete(nr);
        merged.push(`#${nr} ${pull.url}`);
        log(`#${nr} merged by a person: ${pull.url}`, 'ok');
        mark(item, 'merged');
        appendFileSync(file, `${JSON.stringify({ ...row, outcome: 'merged' })}\n`);
        cleanup(item, pull);
        emit('progress', { done: merged.length, total, label: 'PBIs' });
        await untilClosed(nr);
        changed = true;
      } else if (gate?.state === 'closed') {
        handedOver.delete(nr);
        handedPulls.delete(nr);
        aside.set(nr, `closed by a person, ${pull.url}`);
        log(`#${nr} set aside: closed by a person, ${pull.url}`, 'bad');
        tell(item, pull, CLOSED, false);
        appendFileSync(
          file,
          `${JSON.stringify({ ...row, outcome: 'aside', reason: 'closed by a person' })}\n`,
        );
        changed = true;
      } else if (gate?.state === 'conflict') {
        await repairHandedOver(nr);
      }
    }
    if (changed || handedPulls.size === 0) return changed;
    for (let second = 0; second < 60 && !stopped && Date.now() < until; second++) await sleep(1);
  }
  return false;
}

let cutNote = '';
const baseExists = () =>
  attempt('git', ['ls-remote', '--exit-code', '--heads', 'origin', BASE], root) !== undefined;
if (!baseExists()) {
  // after every release the merge deletes release/next: cut it again from the default branch, as github-release cut does
  if (BASE !== 'release/next') fail(`${BASE} is missing on origin`, 3);
  if (opt['dry-run']) {
    console.log(`${BASE} is missing on origin, the loop cuts it from the default branch`);
  } else {
    try {
      const target = tracker.defaultBranch();
      run('git', ['fetch', '--quiet', 'origin', target], root);
      run('git', ['push', '--quiet', 'origin', `origin/${target}:refs/heads/${BASE}`], root);
      cutNote = `${BASE} was missing on origin: cut from ${target}`;
    } catch (error) {
      fail(`${BASE} is missing on origin and could not be cut: ${firstLine(error)}`, 3);
    }
  }
}

const required = tracker.requiredChecks();
if (required.length === 0 && !opt['allow-no-required-checks']) {
  fail(
    `${BASE} requires no checks in ${tracker.repo}, so nothing guards the merge: add a ruleset with required checks, or pass --allow-no-required-checks`,
    3,
  );
}

if (opt['dry-run']) {
  try {
    for (const item of tracker.items()) {
      console.log(`#${item.id} ${tracker.waitReason(item) ?? 'ready'}: ${item.title}`);
    }
  } catch (error) {
    fail(`tracker: ${firstLine(error)}`, 64);
  }
  console.log(
    `harness: ${harnessName} (${harness.command({ prompt: '/deliver <nr> …', model: opt.model }).join(' ')})`,
  );
  console.log(
    `review: ${REVIEW.harnessName} (${REVIEW.harness.command({ prompt: '/deliver-review <nr> …', model: REVIEW.model, effort: REVIEW.effort }).join(' ')})`,
  );
  console.log(`tracker: ${tracker.name}, base: ${BASE}`);
  console.log(`required checks: ${required.join(', ') || 'none'}`);
  console.log(`session token: ${TOKEN_SOURCE ?? 'none, the session uses your login'}`);
  console.log(`protected paths: ${PROTECTED.join(' ')}`);
  process.exit(0);
}

mkdirSync(runDir, { recursive: true });
logFile = path.join(path.dirname(runDir), 'loop.log');
const writeEvent = openEvents(
  process.env.AGENT_EVENTS_DIR ?? path.join(commonDir, '.agents', 'events'),
  runId,
);
// OSC 7501 to the controlling terminal (ADR 0007): the events stay the source for displays
const report = programStatus();
const statusTitle = opt.milestone ?? `issues ${opt.issues}`;
let blockedShown = false;
emit = (type, body, tone) => {
  writeEvent(type, body, tone);
  if (type === 'run.started') {
    report({ state: 'working', title: statusTitle });
  } else if (type === 'attention' && body?.needsYou) {
    blockedShown = true;
    report({
      state: 'blocked',
      kind: 'question',
      title: statusTitle,
      msg: String(body.text ?? ''),
    });
  } else if (type === 'task.started' && blockedShown) {
    blockedShown = false;
    report({ state: 'working', title: statusTitle });
  } else if (type === 'run.finished') {
    report(finishReport(body, statusTitle, needsYou.length));
  }
};
log(`run ${runId} in ${tracker.repo}, results: ${path.join(runDir, 'results.jsonl')}`, 'dim');
if (cutNote) log(cutNote, 'info');
if (!TOKEN_SOURCE) {
  log(
    'the session uses your own login: set DELIVER_APP_ID with DELIVER_APP_KEY_FILE or DELIVER_APP_KEY_COMMAND (GitHub App), or DELIVER_SESSION_TOKEN',
    'warn',
  );
}
emit('run.started', {
  profiles: [],
  host: {
    'jsonl-file.pid': String(process.pid),
    ...(process.env.POIMEN_TERMINAL_ID
      ? { 'poimen.terminal': process.env.POIMEN_TERMINAL_ID }
      : {}),
  },
  agent: { harness: harnessName, model: MODEL },
  subject: tracker.subject(),
});

const aside = new Map<number, string>();
// handed over by rule (the guard): a person merges the pull request, it is no failure
const handedOver = new Map<number, string>();
const handedPulls = new Map<number, { item: Item; pull: Pull }>();
const merged: string[] = [];
let waiting: string[] = [];
let total = 0;
try {
  const created = tracker.syncStageLabels();
  if (created.length) log(`labels created: ${created.join(', ')}`, 'info');
} catch (error) {
  log(`agent:* labels not synced: ${firstLine(error)}`, 'warn');
}
process.on('SIGINT', requestStop);
process.on('SIGBREAK', requestStop);
setInterval(() => {
  if (!stopped && existsSync(path.join(runDir, 'stop'))) requestStop();
}, 500).unref();

// The base branch is checked before every issue: a red base would set every pull request aside.
// A red one is rerun once, then a Bug is filed and delivered first. At most BASE_REPAIRS repairs per run.
const BASE_REPAIRS = 2;
let repairs = 0;
let repairBug: Item | undefined;
let baseBlocked = '';
const readBase = (): BaseState | undefined => {
  try {
    return tracker.baseState();
  } catch (error) {
    // a failing tracker call is no verdict: the issue starts
    log(`${BASE} state unreadable: ${firstLine(error)}`, 'warn');
    return undefined;
  }
};
/** Reads the base until its checks are no longer pending (or the merge time is over). */
async function settleBase(): Promise<BaseState | undefined> {
  const start = Date.now();
  for (;;) {
    const state = readBase();
    if (!state || state.state !== 'pending' || stopped) return state;
    if (Date.now() - start > MERGE_MINUTES * 60_000) return state;
    for (let second = 0; second < 30 && !stopped; second++) await sleep(1);
  }
}
/** The environment of the session token for the calls that change something. A failed mint throws. */
async function actEnv(): Promise<Record<string, string>> {
  const token = tokenSource ? (await tokenSource.current()).token : FIXED_TOKEN;
  return token ? tracker.sessionEnv(token) : {};
}
const names = (failed: BaseState['failed']) => [...new Set(failed.map((f) => f.name))].join(', ');
/** The Bug to deliver first when the base is red, else nothing. Sets `baseBlocked` when the run must end. */
async function checkBase(): Promise<Item | undefined> {
  let state = readBase();
  if (state?.state === 'pending' && repairBug) state = await settleBase();
  if (!state || state.state === 'pending') return undefined;
  if (state.state === 'green') {
    repairBug = undefined;
    return undefined;
  }
  const blocked = (failed: BaseState['failed'], why = '') => {
    baseBlocked = `${BASE} is red (${names(failed)})${why}${repairBug ? `, Bug ${repairBug.url}` : ''}`;
    return undefined;
  };
  // red again after its repair: no second rerun, no second Bug
  if (repairBug) return blocked(state.failed, ' after its repair');
  log(`${BASE} is red (${names(state.failed)}), reruns the failed jobs`, 'warn');
  try {
    tracker.rerunFailed(await actEnv());
    for (let second = 0; second < 30 && !stopped; second++) await sleep(1);
    state = (await settleBase()) ?? state;
  } catch (error) {
    log(`${BASE} rerun failed: ${firstLine(error)}`, 'warn');
  }
  if (state.state !== 'red') {
    if (state.state === 'green') log(`${BASE}: green after rerun (flake)`, 'ok');
    return undefined;
  }
  if (repairs >= BASE_REPAIRS) return blocked(state.failed, `: ${BASE_REPAIRS} repairs used`);
  try {
    repairBug = tracker.reportBaseRed(state.failed, await actEnv());
  } catch (error) {
    return blocked(state.failed, `: no Bug filed (${firstLine(error)})`);
  }
  repairs += 1;
  log(`${BASE} is still red: delivers ${repairBug.url} first`, 'warn');
  return repairBug;
}

try {
  for (let count = 0; !stopped && (MAX === 0 || count < MAX); count++) {
    // an issue closes a moment after its merge: do not pick it again
    const open = tracker
      .items()
      .filter((item) => !merged.some((line) => line.startsWith(`#${item.id} `)));
    if (count === 0) {
      total = open.length;
      emit('progress', { done: 0, total, label: 'PBIs' });
    }
    waiting = [];
    let next: Item | undefined = await checkBase();
    if (baseBlocked) break;
    if (next) {
      // the Bug of a red base goes first, as a normal iteration (it may not be in the selection)
      const repair = next;
      next = open.find((item) => item.id === repair.id) ?? repair;
    }
    for (const item of next ? [] : open) {
      if (aside.has(item.id) || handedOver.has(item.id)) continue;
      const reason = tracker.waitReason(item);
      if (reason) waiting.push(`#${item.id} ${reason}`);
      else if (!next) next = item;
    }
    if (!next) {
      // only handed-over pull requests are left (and what they block): wait for a person to merge them
      if (!(await awaitHandovers())) break;
      count--;
      continue;
    }
    log(`#${next.id} start: ${next.title} (${open.length} open)`, 'info');
    // x: the issues finished or set aside so far, plus this one; the log file is read at a fixed width
    const begun = merged.length + aside.size + handedOver.size + 1;
    log(
      progressLine(begun, total, process.stdout.isTTY ? (process.stdout.columns ?? 80) : 80),
      'info',
    );
    emit('task.started', { task: `issue-${next.id}`, subject: tracker.itemSubject(next) });
    // a failing tracker or git call sets the issue aside, it does not end the run
    const { reason, pull } = await deliver(next).catch(
      (error): { reason?: string; pull?: Pull } => ({ reason: `error: ${firstLine(error)}` }),
    );
    if (!reason) {
      merged.push(
        `#${next.id} ${pull?.url}${risks.has(next.id) ? ` (risk ${risks.get(next.id)})` : ''}`,
      );
      log(`#${next.id} merged: ${pull?.url}`, 'ok');
      mark(next, 'merged');
      if (risks.get(next.id) === 'high') tell(next, pull, 'high risk: merged', false);
      emit(
        'task.finished',
        { task: `issue-${next.id}`, outcome: 'succeeded', text: 'merged' },
        'success',
      );
      await untilClosed(next.id);
    } else if (reason === 'stopped') {
      log(`#${next.id} stopped: not merged${pull ? `, ${pull.url}` : ''}`, 'warn');
      emit('task.finished', { task: `issue-${next.id}`, outcome: 'cancelled', text: 'stopped' });
    } else {
      const planned = reason.endsWith(HANDS_OFF);
      const why = planned ? reason.slice(0, -HANDS_OFF.length) : reason;
      (planned ? handedOver : aside).set(next.id, `${why}${pull ? `, ${pull.url}` : ''}`);
      if (planned && pull) handedPulls.set(next.id, { item: next, pull });
      mark(next, planned ? 'handed-over' : 'set-aside');
      tell(next, pull, why, planned);
      log(
        planned
          ? `#${next.id} handed over: ${why}, merge it yourself`
          : `#${next.id} set aside: ${reason}`,
        planned ? 'warn' : 'bad',
      );
      emit(
        'task.finished',
        { task: `issue-${next.id}`, outcome: 'skipped', text: reason },
        'warning',
      );
      // after the task: an attention of a finished task would close with it
      emit(
        'attention',
        {
          id: `aside-${next.id}`,
          reason: 'set-aside',
          text: `#${next.id} set aside: ${reason}`,
          needsYou: true,
          links: [{ rel: pull ? 'pull-request' : 'issue', url: pull?.url ?? next.url }],
        },
        'warning',
      );
    }
    emit('progress', { done: merged.length, total, label: 'PBIs' });
    if (harnessDead) break;
  }
} catch (error) {
  emit('run.finished', { outcome: 'failed', text: firstLine(error).slice(0, 300) }, 'danger');
  log(`run failed: ${firstLine(error)}`, 'bad');
  process.exit(1);
}

// Before the run ends: a pull request handed over earlier may conflict with the base by now. One repair session
// per pull request and run; the loop never merges it, the person does.
if (!stopped && !harnessDead && !baseBlocked) {
  for (const nr of handedPulls.keys()) await repairHandedOver(nr);
}

// A fully delivered milestone ends with the release pull request, which a person merges.
let needsYou: string[] = [];
let releaseError = '';
const complete =
  Boolean(opt.milestone) &&
  !stopped &&
  aside.size === 0 &&
  handedOver.size === 0 &&
  !baseBlocked &&
  waiting.length === 0;
if (complete) {
  try {
    if (tracker.items().length === 0 && merged.length === 0 && tracker.releaseCommits() === 0) {
      log(
        `milestone ${opt.milestone} has no open issue and ${BASE} has no commit beyond ${tracker.defaultBranch()}, nothing to deliver`,
        'info',
      );
    } else if (tracker.items().length === 0) {
      const release = tracker.ensureReleasePull();
      log(`release pull request: ${release.url} (never merged by the loop)`, 'info');
      emit('step.finished', {
        task: 'release',
        step: { name: 'release-pr', text: 'release pull request' },
        links: [{ rel: 'pull-request', url: release.url, title: release.title }],
      });
      for (const test of tracker.releaseTests()) {
        needsYou.push(`#${test.id} ${test.title}`);
        emit(
          'attention',
          {
            id: `release-test-${test.id}`,
            reason: 'release-test',
            text: `#${test.id} release test is open: a person tests the release candidate`,
            needsYou: true,
            links: [{ rel: 'issue', url: test.url }],
          },
          'warning',
        );
      }
    }
  } catch (error) {
    needsYou = [];
    log(`release pull request: ${firstLine(error)}`, 'bad');
    releaseError = firstLine(error);
  }
}

// an issue of level high is listed first: a person looks at it before the rest
const highRisk = [...risks]
  .filter(([, level]) => level === 'high')
  .map(([nr]) => `#${nr} risk high`);
needsYou = [...highRisk, ...[...handedOver].map(([nr, why]) => `#${nr} ${why}`), ...needsYou];
// the open PBIs that wait for a handed-over pull request: their blockers name its issue
const waitingFor = new Map<number, string[]>();
for (const nr of handedOver.keys()) {
  const dependents = waiting.filter((line) => {
    const blockers = /^#\d+ blocked by (.*)$/.exec(line)?.[1]?.split(', ') ?? [];
    return blockers.some((ref) => ref === `#${nr}` || ref === `${tracker.repo}#${nr}`);
  });
  if (dependents.length) waitingFor.set(nr, dependents);
}
say('');
say(
  `merged (${merged.length}):${merged.map((line) => `\n  ${line}`).join('')}`,
  merged.length ? 'ok' : undefined,
);
say(
  `set aside (${aside.size}):${[...aside].map(([nr, why]) => `\n  #${nr} ${why}`).join('')}`,
  aside.size ? 'bad' : undefined,
);
say(
  `waiting (${waiting.length}):${waiting.map((line) => `\n  ${line}`).join('')}`,
  waiting.length ? 'warn' : undefined,
);
if (releaseError) say(`release pull request failed: ${releaseError}`, 'bad');
say(
  `needs you (${needsYou.length}):${needsYou.map((line) => `\n  ${line}`).join('')}`,
  needsYou.length ? 'warn' : undefined,
);
if (baseBlocked) {
  say(`blocked: ${baseBlocked}`, 'bad');
  emit(
    'attention',
    {
      id: 'base-red',
      reason: 'base-red',
      text: `blocked: ${baseBlocked}`,
      needsYou: true,
      links: repairBug ? [{ rel: 'issue', url: repairBug.url }] : [],
    },
    'warning',
  );
}
for (const [nr, dependents] of waitingFor) {
  say(
    `waiting for the handed-over pull request of #${nr} (${dependents.length}):${dependents.map((line) => `\n  ${line}`).join('')}`,
    'warn',
  );
}
if (handedOver.size) {
  say(
    'next step: merge the pull request above yourself, then run /deliver again or start the loop again with the same arguments',
    'warn',
  );
}
const clean =
  !baseBlocked &&
  !releaseError &&
  aside.size === 0 &&
  handedOver.size === 0 &&
  waiting.length === 0;
// done, blocked (a red base, an issue set aside, an error) or waiting for a handed-over pull request
const status = harnessDead
  ? 'failed'
  : clean
    ? 'done'
    : !baseBlocked && !releaseError && aside.size === 0 && handedOver.size > 0
      ? 'waiting-for-handed-over'
      : 'blocked';
if (!stopped) {
  say(
    status === 'done'
      ? 'done'
      : status === 'failed'
        ? `failed: the harness failed ${harnessDead?.count} times in a row, last exit code ${harnessDead?.exit}`
        : status === 'blocked'
          ? 'blocked'
          : 'waiting for a handed-over PR: merge it, then run /deliver again',
    status === 'done' ? 'ok' : status === 'blocked' || status === 'failed' ? 'bad' : 'warn',
  );
}
appendFileSync(
  path.join(runDir, 'results.jsonl'),
  `${JSON.stringify({
    run: 'finished',
    status,
    handedOver: [...handedOver.keys()],
    waitingFor: Object.fromEntries(waitingFor),
  })}\n`,
);
// An incomplete run (issues set aside or waiting) is no success: the agent-events format has no
// other outcome for it than `failed`; the counts are in the text. OSC 7501 shows it as `done` with
// `tone=warning`; only `reason: 'harness'` (the harness failed) is an `error`.
emit(
  'run.finished',
  {
    outcome: stopped ? 'cancelled' : clean ? 'succeeded' : 'failed',
    ...(stopped || clean ? {} : { reason: status === 'failed' ? 'harness' : 'incomplete' }),
    text: `${status}: merged ${merged.length}, set aside ${aside.size}, waiting ${waiting.length}${needsYou.length ? `, needs you ${needsYou.length}` : ''}`,
  },
  stopped ? 'neutral' : clean ? 'success' : 'warning',
);
process.exit(
  stopped
    ? 130
    : clean
      ? 0
      : status === 'failed'
        ? 5
        : status === 'waiting-for-handed-over'
          ? 3
          : 2,
);
