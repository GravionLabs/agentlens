// GitHub tracker: issues, sub-issue dependencies, pull requests and checks through the gh CLI.
import { execFileSync } from 'node:child_process';
import { existsSync } from 'node:fs';
import { LABEL_PREFIX, labelOf } from '../stage.mts';
import { type ReviewMark, type Verdict, markBody, parseMark } from '../review.mts';
import { syncLabels } from '../sync-labels.mts';
import type { BaseState, Gate, Item, Pull, Subject, Tracker, TrackerContext } from '../tracker.mts';

const HOLD = ['needs-decision', 'needs-refinement', 'interactive', 'release-test'];
// only these can change the text of an issue they wrote; anyone can write a comment
const TRUSTED = ['OWNER', 'MEMBER', 'COLLABORATOR'];

type Issue = {
  number: number;
  title: string;
  state: string;
  html_url: string;
  body: string | null;
  author_association: string;
  labels: { name: string }[];
  pull_request?: unknown;
  user?: { type?: string };
  milestone?: { number: number } | null;
};
type Milestone = { number: number; title: string; html_url: string };
type PullRecord = {
  number: number;
  title: string;
  html_url: string;
  head: { ref: string; repo: { full_name: string } | null };
  merged_at?: string | null;
  body?: string | null;
};
type Check = { name: string; bucket: string };

/**
 * The verdict of the checks of a pull request. The loop decides this itself and does not rely on the
 * ruleset: a login that may bypass the ruleset (an admin) merges through the API although a required
 * check is missing. Every required check must have reported and passed; a required check that has not
 * reported yet is pending, never green. Without required checks every reported check counts.
 */
export function checkVerdict(
  required: string[],
  checks: Check[],
): { state: 'red'; detail: string } | { state: 'pending' | 'green' } {
  const gating = required.length > 0 ? checks.filter((c) => required.includes(c.name)) : checks;
  const red = gating.filter((c) => c.bucket === 'fail');
  if (red.length > 0)
    return { state: 'red', detail: [...new Set(red.map((c) => c.name))].join(', ') };
  const passed = (c: Check) => c.bucket === 'pass' || c.bucket === 'skipping';
  const missing = required.some((name) => !gating.some((c) => c.name === name));
  return { state: missing || !gating.every(passed) ? 'pending' : 'green' };
}

/** The marker of the Bug about a red base branch: an open Bug that has it is reused. */
export const BASE_RED_MARKER = '<!-- deliver-base-red -->';

type CheckRun = {
  name: string;
  status: string;
  conclusion: string | null;
  html_url: string | null;
  details_url: string | null;
};

/** The state of the check runs of a commit, by the same rule as the checks of a pull request. */
export function baseVerdict(required: string[], runs: CheckRun[]): BaseState {
  const bucket = (run: CheckRun): string =>
    run.status !== 'completed'
      ? 'pending'
      : ['success', 'neutral'].includes(String(run.conclusion))
        ? 'pass'
        : run.conclusion === 'skipped'
          ? 'skipping'
          : 'fail';
  const verdict = checkVerdict(
    required,
    runs.map((run) => ({ name: run.name, bucket: bucket(run) })),
  );
  if (verdict.state !== 'red') return { state: verdict.state, failed: [] };
  const names = verdict.detail.split(', ');
  const failed = runs
    .filter(
      (run) => bucket(run) === 'fail' && (required.length === 0 || required.includes(run.name)),
    )
    .filter((run) => names.includes(run.name))
    .map((run) => ({ name: run.name, url: run.details_url ?? run.html_url ?? '' }));
  return { state: 'red', failed };
}

type Comment = {
  body: string | null;
  created_at: string;
  updated_at: string;
  user: { login: string } | null;
};

/**
 * The marks (`<!-- deliver-review: approve <sha> risk=high -->`) of the comments, oldest first. Only a comment of
 * `login` that starts with the mark and was never edited counts: anyone can write a comment.
 */
export function reviewMarks(comments: Comment[], login: string): ReviewMark[] {
  return comments.flatMap((comment) => {
    if (comment.user?.login !== login || comment.created_at !== comment.updated_at) return [];
    return parseMark(comment.body ?? '') ?? [];
  });
}

/** The body of a pull request closes issue `id` with a closing keyword (`Closes #12`, `fixes #12`, `Resolved #12`). */
export function closesIssue(body: string | null | undefined, id: number): boolean {
  return new RegExp(
    `\\b(?:close[sd]?|fix(?:e[sd])?|resolve[sd]?)\\s*:?\\s+#${id}(?!\\d)`,
    'i',
  ).test(body ?? '');
}

/**
 * The merged pull request of issue `id` (head `<type>/<id>-…` of this repository) that was merged at or after
 * `since`, the moment the issue was last opened, and that closes the issue (`closes` decides; by default a closing
 * keyword in its body). A merged pull request that does not close it (an ADR step of the issue) is no delivery.
 * The newest one wins.
 */
export function mergedPullSince(
  records: PullRecord[],
  repo: string,
  id: number,
  since: string,
  closes: (p: PullRecord) => boolean = (p) => closesIssue(p.body, id),
): PullRecord | undefined {
  return records
    .filter(
      (p) =>
        p.merged_at &&
        Date.parse(p.merged_at) >= Date.parse(since) &&
        p.head.repo?.full_name === repo &&
        new RegExp(`^[a-z]+/${id}-`).test(p.head.ref) &&
        closes(p),
    )
    .sort((a, b) => Date.parse(b.merged_at ?? '') - Date.parse(a.merged_at ?? ''))[0];
}

/** The open pull requests of issue `id` (head `<type>/<id>-…` of this repository), the oldest first. */
export function openPullsOf(records: PullRecord[], repo: string, id: number): PullRecord[] {
  return records
    .filter((p) => p.head.repo?.full_name === repo && new RegExp(`^[a-z]+/${id}-`).test(p.head.ref))
    .sort((a, b) => a.number - b.number);
}

/** The default branch takes merge commits (its ruleset allows only those, and the Conventional Commits stay visible); every other base takes squash. */
export function mergeMethodFor(base: string, defaultBranch: string): 'merge' | 'squash' {
  return base === defaultBranch ? 'merge' : 'squash';
}

/** At most this many rate-limit waits in a row (primary and secondary together); the next error is thrown. */
export const RATE_LIMIT_WAITS = 3;
/** Seconds of the consecutive waits of a secondary limit: `gh` does not print `Retry-After` on an error. */
export const SECONDARY_BACKOFF = [60, 120, 300];
/** The longest wait for a primary limit, in seconds. */
const MAX_PRIMARY_WAIT = 3600;

/** A failed `gh` call that says it hit a rate limit (primary or secondary). */
export const isRateLimit = (text: string): boolean =>
  /rate limit|abuse detection|HTTP 429/i.test(text);

/** Seconds until the reset of an exhausted limit in the answer of `gh api rate_limit`; undefined when none is exhausted. */
export function primaryWait(answer: string, nowSeconds: number): number | undefined {
  try {
    const resources = Object.values(JSON.parse(answer).resources ?? {}) as {
      remaining: number;
      reset: number;
    }[];
    const resets = resources.filter((r) => r.remaining === 0).map((r) => r.reset);
    if (resets.length === 0) return undefined;
    return Math.min(MAX_PRIMARY_WAIT, Math.max(1, Math.ceil(Math.max(...resets) - nowSeconds)));
  } catch {
    return undefined;
  }
}

const tickMs = (): number => Number(process.env.DELIVER_TICK_MS ?? 1000);
// a blocking sleep: the tracker is synchronous, so a signal handler does not run during a wait (the `stop` file does)
const pause = (ms: number): void => {
  Atomics.wait(new Int32Array(new SharedArrayBuffer(4)), 0, 0, ms);
};

export function create({
  cwd,
  base,
  selection,
  trustAuthors,
  stopFile,
  onStop,
  onExhausted,
  log,
}: TrackerContext): Tracker {
  const run = (args: string[], env?: Record<string, string>): string =>
    execFileSync('gh', args, {
      cwd,
      ...(env ? { env: { ...process.env, ...env } } : {}),
      encoding: 'utf8',
      stdio: ['ignore', 'pipe', 'pipe'],
      // the default of 1 MB is too small for the lists of a large repository
      maxBuffer: 256 * 1024 * 1024,
    }).trim();
  let waitsInARow = 0;
  let secondaryInARow = 0;
  /** Waits `seconds` (times DELIVER_TICK_MS) in slices of one second; false when the stop request ended it. */
  const wait = (seconds: number): boolean => {
    for (let second = 0; second < seconds; second++) {
      if (stopFile && existsSync(stopFile)) return false;
      pause(tickMs());
    }
    return !(stopFile && existsSync(stopFile));
  };
  // an argument that is an object is an environment added to the process: the token of the session
  const gh = (...input: (string | Record<string, string>)[]): string => {
    const env = input.find((a): a is Record<string, string> => typeof a === 'object');
    const args = input.filter((a): a is string => typeof a === 'string');
    for (;;) {
      try {
        const out = run(args, env);
        waitsInARow = 0;
        secondaryInARow = 0;
        return out;
      } catch (error) {
        const e = error as { stderr?: string; message?: string };
        // stderr only: the message of the error repeats the arguments (an issue title may say "rate limit")
        if (!isRateLimit(e.stderr ?? '')) throw error;
        if (waitsInARow >= RATE_LIMIT_WAITS) {
          onExhausted?.(error as Error);
          throw error;
        }
        waitsInARow++;
        let seconds: number | undefined;
        try {
          seconds = primaryWait(run(['api', 'rate_limit'], env), Date.now() / 1000);
        } catch {
          // unreadable: treated like a secondary limit
        }
        const kind = seconds === undefined ? 'secondary' : 'primary';
        seconds ??= SECONDARY_BACKOFF[Math.min(secondaryInARow++, SECONDARY_BACKOFF.length - 1)];
        log?.(
          `gh hit a ${kind} rate limit: waits ${seconds} s (${waitsInARow} of ${RATE_LIMIT_WAITS})`,
        );
        if (!wait(seconds)) {
          onStop?.();
          throw new Error('stopped during a rate-limit wait');
        }
      }
    }
  };
  const api = <T,>(route: string): T => JSON.parse(gh('api', route));
  const list = <T,>(route: string): T[] =>
    (JSON.parse(gh('api', route, '--paginate', '--slurp')) as T[][]).flat();
  const repo = gh('repo', 'view', '--json', 'nameWithOwner', '-q', '.nameWithOwner');

  let milestoneCache: Milestone | undefined;
  const milestone = (): Milestone => {
    if (milestoneCache) return milestoneCache;
    const found = list<Milestone>(`repos/${repo}/milestones?state=all&per_page=100`).find(
      (m) => m.title === selection.milestone || m.title.startsWith(`${selection.milestone} `),
    );
    if (!found) throw new Error(`milestone ${selection.milestone} not found in ${repo}`);
    milestoneCache = found;
    return found;
  };

  const authors = new Map<number, string>();
  const toItem = (issue: Issue): Item => {
    authors.set(issue.number, issue.author_association);
    return {
      id: issue.number,
      title: issue.title,
      url: issue.html_url,
      labels: issue.labels.map((l) => l.name),
      body: issue.body ?? '',
    };
  };

  const checksOf = (id: number): Check[] => {
    try {
      return JSON.parse(gh('pr', 'checks', String(id), '--repo', repo, '--json', 'name,bucket'));
    } catch (error) {
      const { stdout, stderr } = error as { stdout?: string; stderr?: string };
      // gh exits non-zero for pending or failed checks and still prints the list
      try {
        return JSON.parse(String(stdout));
      } catch {
        if (/no checks reported/i.test(String(stderr))) return [];
        // a failed call is not "no checks": without required checks an empty list would count as green
        throw error;
      }
    }
  };

  let lastFailed: BaseState['failed'] = [];
  let requiredCache: string[] | undefined;
  const required = (): string[] => {
    if (requiredCache) return requiredCache;
    const names: string[] = [];
    try {
      const rules = api<
        { type: string; parameters?: { required_status_checks?: { context: string }[] } }[]
      >(`repos/${repo}/rules/branches/${base}`);
      for (const rule of rules) {
        if (rule.type !== 'required_status_checks') continue;
        names.push(...(rule.parameters?.required_status_checks ?? []).map((c) => c.context));
      }
    } catch {
      // no rulesets readable
    }
    try {
      // classic branch protection
      const route = `repos/${repo}/branches/${encodeURIComponent(base)}/protection/required_status_checks`;
      names.push(...(api<{ contexts?: string[] }>(route).contexts ?? []));
    } catch {
      // none, or not readable with this login
    }
    requiredCache = [...new Set(names)];
    return requiredCache;
  };

  return {
    name: 'github',
    repo,

    subject(): Subject {
      if (selection.milestone) {
        const m = milestone();
        return { kind: 'github.milestone', id: m.title, title: m.title, url: m.html_url };
      }
      return {
        kind: 'github.repository',
        id: repo,
        title: `${repo} ${(selection.issues ?? []).map((nr) => `#${nr}`).join(', ')}`,
        url: `https://github.com/${repo}`,
      };
    },

    items(): Item[] {
      if (selection.issues) {
        return selection.issues
          .map((nr) => api<Issue>(`repos/${repo}/issues/${nr.trim()}`))
          .filter((issue) => issue.state === 'open' && !issue.pull_request)
          .map(toItem);
      }
      const number = milestone().number;
      return list<Issue>(`repos/${repo}/issues?milestone=${number}&state=open&per_page=100`)
        .filter(
          (i) => !i.pull_request && i.labels.some((l) => l.name === 'pbi' || l.name === 'bug'),
        )
        .sort((a, b) => a.number - b.number)
        .map(toItem);
    },

    itemSubject(item: Item): Subject {
      return {
        kind: 'github.issue',
        id: `${repo}#${item.id}`,
        title: `#${item.id} ${item.title}`,
        url: item.url,
      };
    },

    setStage(item: Item, stage: Parameters<Tracker['setStage']>[1]): void {
      const route = `repos/${repo}/issues/${item.id}/labels`;
      const names = list<{ name: string }>(`${route}?per_page=100`).map((l) =>
        l.name.toLowerCase(),
      );
      const target = stage && labelOf(stage);
      for (const name of names) {
        if (!name.startsWith(LABEL_PREFIX) || name === target) continue;
        try {
          gh('api', '-X', 'DELETE', `${route}/${encodeURIComponent(name)}`);
        } catch (error) {
          // removed meanwhile
          if (!/HTTP 404/.test((error as { stderr?: string }).stderr ?? '')) throw error;
        }
      }
      if (target && !names.includes(target))
        gh('api', '-X', 'POST', route, '-f', `labels[]=${target}`);
    },

    syncStageLabels(): string[] {
      return syncLabels((...args) => gh(...args), repo);
    },

    commentOnce(item: Item, marker: string, body: string): void {
      const route = `repos/${repo}/issues/${item.id}/comments`;
      const login = gh('api', 'user', '--jq', '.login');
      // anyone can write the marker: only the comment of the loop itself counts as posted
      const posted = list<Comment>(`${route}?per_page=100`).some(
        (c) => c.user?.login === login && (c.body ?? '').startsWith(marker),
      );
      if (!posted) gh('api', '-X', 'POST', route, '-f', `body=${body}`);
    },

    recordVerdict(pull: Pull, review: Verdict, head: string, risk?: string): void {
      const body = markBody(review, head, risk);
      gh('api', '-X', 'POST', `repos/${repo}/issues/${pull.id}/comments`, '-f', `body=${body}`);
    },

    recordedVerdicts(pull: Pull): ReviewMark[] {
      const login = gh('api', 'user', '--jq', '.login');
      return reviewMarks(
        list<Comment>(`repos/${repo}/issues/${pull.id}/comments?per_page=100`),
        login,
      );
    },

    waitReason(item: Item): string | undefined {
      const held = item.labels.find((name) => HOLD.includes(name));
      if (held) return held;
      const author = authors.get(item.id) ?? 'NONE';
      if (!trustAuthors && !TRUSTED.includes(author)) {
        return `author has no write access (${author.toLowerCase()}), --trust-authors delivers it`;
      }
      const blockers = list<{ number: number; state: string; repository_url: string }>(
        `repos/${repo}/issues/${item.id}/dependencies/blocked_by`,
      ).filter((b) => b.state === 'open');
      if (blockers.length === 0) return undefined;
      return `blocked by ${blockers.map((b) => `${b.repository_url.split('/repos/')[1]}#${b.number}`).join(', ')}`;
    },

    needsDecision(item: Item): boolean {
      return api<Issue>(`repos/${repo}/issues/${item.id}`).labels.some(
        (l) => l.name === 'needs-decision',
      );
    },

    pullOf(item: Item): Pull | undefined {
      const [found, ...others] = openPullsOf(
        list<PullRecord>(`repos/${repo}/pulls?state=open&base=${base}&per_page=100`),
        repo,
        item.id,
      );
      return (
        found && {
          id: found.number,
          title: found.title,
          url: found.html_url,
          branch: found.head.ref,
          ...(others.length > 0 && { others: others.map((p) => p.html_url) }),
        }
      );
    },

    mergedPullOf(item: Item): Pull | undefined {
      // the issue counts as opened at its creation or at its last reopening
      const issue = api<Issue & { created_at: string }>(`repos/${repo}/issues/${item.id}`);
      const reopened = list<{ event: string; created_at: string }>(
        `repos/${repo}/issues/${item.id}/events?per_page=100`,
      )
        .filter((e) => e.event === 'reopened')
        .map((e) => e.created_at)
        .sort()
        .pop();
      // the newest closed pull requests first: one page holds the recent merges
      const closed = api<PullRecord[]>(
        `repos/${repo}/pulls?state=closed&base=${base}&sort=updated&direction=desc&per_page=100`,
      );
      // closing references that the body does not spell out (a linked pull request) are read per candidate
      const found = mergedPullSince(closed, repo, item.id, reopened ?? issue.created_at, (p) => {
        if (closesIssue(p.body, item.id)) return true;
        try {
          const refs = JSON.parse(
            gh('pr', 'view', String(p.number), '--repo', repo, '--json', 'closingIssuesReferences'),
          ) as { closingIssuesReferences?: { number: number }[] };
          return (refs.closingIssuesReferences ?? []).some((r) => r.number === item.id);
        } catch {
          return false;
        }
      });
      return (
        found && {
          id: found.number,
          title: found.title,
          url: found.html_url,
          branch: found.head.ref,
        }
      );
    },

    gate(pull: Pull): Gate {
      const view = JSON.parse(
        gh('pr', 'view', String(pull.id), '--repo', repo, '--json', 'state,mergeable,headRefOid'),
      );
      if (view.state === 'MERGED') return { state: 'merged' };
      if (view.state === 'CLOSED') return { state: 'closed' };
      if (view.mergeable === 'CONFLICTING')
        return { state: 'conflict', detail: `conflict with ${base}` };
      const checks = checksOf(pull.id);
      const verdict = checkVerdict(required(), checks);
      if (verdict.state === 'red') return verdict;
      // mergeable is UNKNOWN while GitHub computes it
      const ready = verdict.state === 'green' && view.mergeable === 'MERGEABLE';
      return {
        state: ready ? 'ready' : 'pending',
        reported: checks.length > 0,
        head: view.headRefOid,
      };
    },

    merge(pull: Pull, head: string): void {
      try {
        const method = mergeMethodFor(base, this.defaultBranch());
        gh(
          'api',
          '-X',
          'PUT',
          `repos/${repo}/pulls/${pull.id}/merge`,
          '-f',
          `merge_method=${method}`,
          '-f',
          `commit_title=${pull.title} (#${pull.id})`,
          // only the commit whose checks were read: a later push is refused
          '-f',
          `sha=${head}`,
        );
      } catch (error) {
        const stderr = String((error as { stderr?: string }).stderr ?? error);
        throw new Error(stderr.trim().split('\n')[0]);
      }
    },

    dropBranch(pull: Pull): void {
      try {
        gh('api', '-X', 'DELETE', `repos/${repo}/git/refs/heads/${pull.branch}`);
      } catch {
        // the repository may delete merged branches itself
      }
    },

    requiredChecks(): string[] {
      return required();
    },

    baseState(): BaseState {
      const pages = JSON.parse(
        gh(
          'api',
          `repos/${repo}/commits/${encodeURIComponent(base)}/check-runs?per_page=100`,
          '--paginate',
          '--slurp',
        ),
      ) as { check_runs: CheckRun[] }[];
      const state = baseVerdict(
        required(),
        pages.flatMap((page) => page.check_runs),
      );
      lastFailed = state.failed;
      return state;
    },

    rerunFailed(env: Record<string, string>): void {
      // one run per URL: several failed jobs of a workflow run are rerun together
      const runs = new Set(
        lastFailed.map((f) => /\/actions\/runs\/(\d+)/.exec(f.url)?.[1]).filter(Boolean),
      );
      if (runs.size === 0) throw new Error('no workflow run to rerun');
      for (const id of runs)
        gh('api', '-X', 'POST', `repos/${repo}/actions/runs/${id}/rerun-failed-jobs`, env);
    },

    reportBaseRed(failed: BaseState['failed'], env: Record<string, string>): Item {
      const open = list<Issue>(`repos/${repo}/issues?labels=bug&state=open&per_page=100`).find(
        (i) =>
          !i.pull_request &&
          (i.body ?? '').includes(BASE_RED_MARKER) &&
          (trustAuthors || TRUSTED.includes(i.author_association) || i.user?.type === 'Bot'),
      );
      if (open) return toItem(open);
      const checks = [...new Set(failed.map((f) => f.name))].join(', ');
      const body = [
        BASE_RED_MARKER,
        '',
        `## Reproduction`,
        '',
        `The required checks of \`${base}\` are red:`,
        '',
        ...failed.map((f) => `- ${f.name}: ${f.url}`),
        '',
        '## Acceptance Criteria',
        '',
        `- [ ] the required checks of \`${base}\` are green`,
        '',
        'Filed by the deliver loop.',
      ].join('\n');
      const number = selection.milestone
        ? milestone().number
        : api<Issue>(`repos/${repo}/issues/${(selection.issues ?? [])[0]?.trim()}`).milestone
            ?.number;
      const fields = [
        '-f',
        `title=[Bug] fix: ${base} is red (${checks})`,
        '-f',
        `body=${body}`,
        '-f',
        'labels[]=bug',
        ...(number ? ['-F', `milestone=${number}`] : []),
      ];
      return toItem(JSON.parse(gh('api', '-X', 'POST', `repos/${repo}/issues`, ...fields, env)));
    },

    changedFiles(pull: Pull): string[] {
      // names only: the full records of a large pull request are many megabytes
      return gh(
        'api',
        `repos/${repo}/pulls/${pull.id}/files?per_page=100`,
        '--paginate',
        '--jq',
        '.[] | .filename, (.previous_filename // empty)',
      )
        .split('\n')
        .filter(Boolean);
    },

    compareRefs(pull: Pull, head?: string): { base: string; head: string } {
      const record = JSON.parse(gh('api', `repos/${repo}/pulls/${pull.id}`)) as {
        base: { ref: string };
        head: { sha: string };
      };
      const sha = head ?? record.head.sha;
      const base = gh(
        'api',
        `repos/${repo}/compare/${encodeURIComponent(record.base.ref)}...${sha}`,
        '--jq',
        '.merge_base_commit.sha',
      );
      if (!base) throw new Error('no merge base');
      return { base, head: sha };
    },

    fileAt(sha: string, file: string): string | undefined {
      try {
        const out = gh(
          'api',
          `repos/${repo}/contents/${file.split('/').map(encodeURIComponent).join('/')}?ref=${sha}`,
          '--jq',
          'if type == "object" and .type == "file" then .content else "" end',
        );
        if (!out) throw new Error(`${file} is not a file`);
        return Buffer.from(out, 'base64').toString('utf8');
      } catch (error) {
        // only a missing file is an answer; the status is on stderr
        if (/HTTP 404/.test((error as { stderr?: string }).stderr ?? '')) return undefined;
        throw error;
      }
    },

    defaultBranch(): string {
      return gh('repo', 'view', '--json', 'defaultBranchRef', '-q', '.defaultBranchRef.name');
    },

    releaseTests(): Item[] {
      if (!selection.milestone) return [];
      return list<Issue>(
        `repos/${repo}/issues?milestone=${milestone().number}&state=open&per_page=100`,
      )
        .filter((i) => !i.pull_request && i.labels.some((l) => l.name === 'release-test'))
        .sort((a, b) => a.number - b.number)
        .map(toItem);
    },

    releaseCommits(): number {
      const target = this.defaultBranch();
      return Number(gh('api', `repos/${repo}/compare/${target}...${base}`, '-q', '.ahead_by'));
    },

    ensureReleasePull(): Pull {
      const target = this.defaultBranch();
      const owner = repo.split('/')[0];
      const find = () =>
        list<PullRecord>(
          `repos/${repo}/pulls?state=open&base=${target}&head=${owner}:${base}&per_page=100`,
        )[0];
      let found = find();
      if (!found) {
        const url = gh(
          'pr',
          'create',
          '--repo',
          repo,
          '--base',
          target,
          '--head',
          base,
          '--title',
          `release: ${base}`,
          '--body',
          `Release pull request of ${selection.milestone ?? base}. Merged by a person, never by the loop.`,
        );
        found = find() ?? {
          number: Number(url.split('/').pop()),
          title: `release: ${base}`,
          html_url: url,
          head: { ref: base, repo: { full_name: repo } },
        };
      }
      return { id: found.number, title: found.title, url: found.html_url, branch: found.head.ref };
    },

    sessionEnv(token: string): Record<string, string> {
      // gh reads GH_TOKEN; git push must use it too, not a stored credential of the user
      const n = Number(process.env.GIT_CONFIG_COUNT ?? 0);
      return {
        GH_TOKEN: token,
        GITHUB_TOKEN: token,
        GIT_CONFIG_COUNT: String(n + 2),
        [`GIT_CONFIG_KEY_${n}`]: 'credential.helper',
        [`GIT_CONFIG_VALUE_${n}`]: '',
        [`GIT_CONFIG_KEY_${n + 1}`]: 'credential.helper',
        [`GIT_CONFIG_VALUE_${n + 1}`]: '!gh auth git-credential',
      };
    },
  };
}
