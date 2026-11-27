// The contract between the deliver loop and an issue tracker. The loop knows nothing else about
// GitHub, Azure DevOps or any other system: it asks for the next work item, finds its pull request,
// reads the gate of the merge and merges. A tracker is the file tracker/<name>.mts, chosen by
// `deliver.tracker` in .agents/config.json (default `github`), and exports `create(context)`.

import path from 'node:path';
import { pathToFileURL } from 'node:url';
import type { Stage } from './stage.mts';
import type { ReviewMark, Verdict } from './review.mts';

/** What a reader of the run sees: a milestone, an issue, a repository. */
export type Subject = { kind: string; id: string; title: string; url?: string };

/** `body` is the text the session works from: the loop hands it over, the session does not fetch it. */
export type Item = { id: number; title: string; url: string; labels: string[]; body: string };

/** `others`: the URLs of further open pull requests of the same item (`pullOf` only); the loop merges none of them. */
export type Pull = { id: number; title: string; url: string; branch: string; others?: string[] };

/**
 * Where a pull request stands on its way to the merge.
 * `ready`: every required check has reported and is green, and nothing conflicts; `head` is the commit
 * that was checked. The tracker decides this from the checks themselves, never from whether the merge
 * would be accepted. `reported` tells whether any check has reported yet, because checks register a
 * moment after the pull request opens.
 */
export type Gate =
  | { state: 'merged' | 'closed' }
  | { state: 'conflict' | 'red'; detail: string }
  | { state: 'pending'; reported: boolean; head?: string }
  | { state: 'ready'; reported: boolean; head: string };

/** The check runs of the head of the base branch. `failed` names the red checks with the URL of their run. */
export type BaseState = {
  state: 'green' | 'pending' | 'red';
  failed: { name: string; url: string }[];
};

export type Selection = { milestone?: string; issues?: string[] };

export type TrackerContext = {
  /** The checkout the loop runs in. */
  cwd: string;
  /** The branch the pull requests go into. */
  base: string;
  selection: Selection;
  /** Deliver items of any author. Default: only authors with write access, whose text nobody else can change. */
  trustAuthors: boolean;
  /** The `stop` file of the run folder: a rate-limit wait ends when it exists. */
  stopFile?: string;
  /** Called when a wait saw the stop request (the tracker call blocks, so the loop cannot see it itself). */
  onStop?: () => void;
  /** Called when the cap of rate-limit waits is used up: callers of the tracker may swallow the error, the run still ends `failed`. */
  onExhausted?: (error: Error) => void;
  /** Progress lines of a rate-limit wait. */
  log?: (text: string) => void;
};

export interface Tracker {
  /** `github`, `ado`, … Names the subject kinds (`github.issue`). */
  readonly name: string;
  /** The repository as the tracker names it, for example `GravionLabs/poimen`. */
  readonly repo: string;
  /** What the whole run delivers: the milestone, or the repository for a list of issues. */
  subject(): Subject;
  /** The open work items of the selection in delivery order. */
  items(): Item[];
  itemSubject(item: Item): Subject;
  /** Sets the `agent:*` label of the stage and removes every other `agent:*` label; `undefined` removes all. */
  setStage(item: Item, stage: Stage | undefined): void;
  /** Creates the missing `agent:*` labels and returns their names. */
  syncStageLabels(): string[];
  /** Comments `body` on the item unless a comment of the login of the loop already starts with `marker`. */
  commentOnce(item: Item, marker: string, body: string): void;
  /** Marks on the pull request the verdict the loop accepted for the commit `head` (`markBody`), with the risk level. */
  recordVerdict(pull: Pull, review: Verdict, head: string, risk?: string): void;
  /** The marks the login of the loop recorded on the pull request, oldest first (`parseMark`); anyone else's are ignored. */
  recordedVerdicts(pull: Pull): ReviewMark[];
  /** Why the item cannot start now (a hold label, an open blocker, an author without write access), else nothing. */
  waitReason(item: Item): string | undefined;
  /** The session asked a question instead of delivering. */
  needsDecision(item: Item): boolean;
  /** The open pull request of the item, found by its branch `<type>/<id>-…`; more than one: the oldest, the rest in `others`. */
  pullOf(item: Item): Pull | undefined;
  /**
   * A pull request of the item that is already merged into the base branch, merged after the item was last
   * opened (a reopened item with an old merged pull request is still work), and that closes the item (a closing
   * keyword or a closing reference; a merged pull request that does not, such as an ADR step, is no delivery). The item is delivered.
   */
  mergedPullOf(item: Item): Pull | undefined;
  gate(pull: Pull): Gate;
  /** Merges exactly the commit `head`: a merge commit into the default branch, a squash into every other base. Throws an Error whose message is the reason when the tracker refuses. */
  merge(pull: Pull, head: string): void;
  /** Removes the branch of a merged pull request on the remote. */
  dropBranch(pull: Pull): void;
  /** The checks the base branch requires before a merge. None: the tracker does not guard the merge. */
  requiredChecks(): string[];
  /** The check runs of the head of the base branch (the required checks, all runs when none can be read). */
  baseState(): BaseState;
  /** Reruns the failed jobs of the last red `baseState()`. `env` is the environment of the session token. */
  rerunFailed(env: Record<string, string>): void;
  /** The open Bug about a red base (marker `<!-- deliver-base-red -->`), reused, else filed in the milestone of the run. */
  reportBaseRed(failed: BaseState['failed'], env: Record<string, string>): Item;
  /** The paths the pull request changes, a rename with both names. */
  changedFiles(pull: Pull): string[];
  /** The merge base of the pull request and the commit that is judged: `head` when given, else the current head of the pull request. */
  compareRefs(pull: Pull, head?: string): { base: string; head: string };
  /** The content of the file at the commit, `undefined` when the file does not exist there. Any other failure throws. */
  fileAt(sha: string, file: string): string | undefined;
  /** The branch the release goes into (`main`), where a missing base branch is cut from. */
  defaultBranch(): string;
  /** The open `release-test` issues of the selection: a person tests the release candidate, the loop never starts them. */
  releaseTests(): Item[];
  /** The number of commits the base branch has that the default branch lacks; 0: a release pull request cannot be opened. */
  releaseCommits(): number;
  /** The open release pull request from the base branch into the default branch; opens it when missing. Never merges it. */
  ensureReleasePull(): Pull;
  /** The environment that makes a session act with `token` instead of the login of the user. */
  sessionEnv(token: string): Record<string, string>;
}

export async function loadTracker(name: string, context: TrackerContext): Promise<Tracker> {
  if (!/^[a-z][a-z0-9-]*$/.test(name)) throw new Error(`invalid tracker name: ${name}`);
  // Tests only: a directory from the environment of the user that holds the tracker file.
  const dir = process.env.DELIVER_TRACKER_DIR;
  const file = dir
    ? pathToFileURL(path.join(dir, `${name}.mts`))
    : new URL(`./tracker/${name}.mts`, import.meta.url);
  const module = await import(file.href);
  return module.create(context);
}
