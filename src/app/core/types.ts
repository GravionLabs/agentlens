// The JSON the Tauri commands return. It mirrors the report types of
// `agentlens-core`; the samples in `samples/` are generated from the Rust side
// and the tests below check that they fit these types.

export type SourceId = "claude-code" | "opencode" | "copilot-cli" | "vscode-copilot" | "otel";

export const SOURCES: { id: SourceId; label: string }[] = [
  { id: "claude-code", label: "Claude Code" },
  { id: "opencode", label: "OpenCode" },
  { id: "copilot-cli", label: "Copilot CLI" },
  { id: "vscode-copilot", label: "VS Code Copilot" },
];

export interface Filters {
  since: string | null;
  /** Upper bound of the period, same forms as `since`; the period is [since, until). */
  until: string | null;
  sources: SourceId[];
  project: string | null;
  /** Only sessions that started a run of one of these roles; empty means all. */
  roles: string[];
}

export interface Answer<T> {
  /** The version of the layout of `data`; raised when a field is renamed, removed or retyped. */
  schema_version: number;
  data: T;
  warnings: number;
  errors: string[];
}

/** A token count and whether the client measured it. */
export interface Tokens {
  count: number;
  measured: boolean;
}

export interface SourceOverview {
  source: SourceId;
  found: boolean;
  roots: string[];
  note: string | null;
  sessions: number;
  events: number;
  warnings: number;
  errors: string[];
}

export interface UsageSum {
  requests: number;
  input: number;
  output: number;
  cache_read: number;
  cache_write: number;
}

export interface SkillStats {
  name: string;
  in_inventory: boolean;
  clients: SourceId[];
  invocations: number;
  /** Tool calls that read or ran files in the skill's folder without loading it. */
  file_uses: number;
  sessions: number;
  projects: string[];
  first_used: string | null;
  last_used: string | null;
  triggers: { model: number; user: number; nested: number };
  by_source: Partial<Record<SourceId, number>>;
  uses_by_day: Record<string, number>;
  load: Tokens;
  unknown_loads: number;
  carry: { requests: number; tokens: Tokens; cache_read_tokens: number; uncached_tokens: number };
  follow_on: UsageSum;
  attributed: UsageSum;
  baseline: { requests: number; tokens: Tokens };
}

export interface SessionContext {
  id: string;
  project: string | null;
  requests: number;
  average_context: number;
}

export interface PeriodTotals {
  requests: number;
  subagent_requests: number;
  tokens: number;
  context_tokens: number;
  large_context_requests: number;
  measured: boolean;
  largest_sessions: SessionContext[];
  /** Sessions in which the client reported its skill listing, and the listing tokens added up. */
  listing_sessions: number;
  listing_tokens: Tokens;
}

export interface SkillReport {
  totals: PeriodTotals;
  sessions: number;
  first_event: string | null;
  last_event: string | null;
  requests_by_source: Partial<Record<SourceId, number>>;
  skills: SkillStats[];
  ignored_commands: { name: string; count: number }[];
}

export type FindingKind =
  | "unused-skills"
  | "listed-but-loaded-by-hand"
  | "costly-descriptions"
  | "used-but-not-installed"
  | "large-bodies"
  | "large-context"
  | "costly-mcp-servers"
  | "context-size";

export interface Reference {
  kind: "agent" | "command" | "skill";
  name: string;
}

export interface AdviceItem {
  /** Unused skills only: the agents, commands and skills whose files name it. */
  referenced_by?: Reference[];
  name: string;
  tokens: Tokens;
  requests: number;
  detail: string;
}

/** How sure the saving of a finding is; `inferred` rests on an assumption. */
export type Confidence = "measured" | "estimated" | "inferred";

/** What the finding's action could have saved in the period. */
export interface Impact {
  saving: Tokens;
  /** Always an estimate (computed from the price table); null without a price or a saving. */
  saving_usd: number | null;
  confidence: Confidence;
}

export interface Finding {
  /** 1 for the largest saving; the findings come in this order. */
  rank: number;
  kind: FindingKind;
  explanation: string;
  action: string;
  count: number;
  total: Tokens | null;
  share_of_baseline: number | null;
  share_of_total_tokens: number | null;
  items: AdviceItem[];
  notes: string[];
  impact: Impact;
}

export interface AdviceReport {
  sessions: number;
  installed_skills: number;
  baseline_total: Tokens;
  total_tokens: Tokens;
  findings: Finding[];
}

export interface SkillsData {
  report: SkillReport;
  installed_skills: number;
  duplicates: { name: string; files: string[]; identical: boolean }[];
}

export type ContextKind =
  | "tool-listing"
  | "tool-definition"
  | "mcp-instructions"
  | "agent-listing"
  | "skill-listing"
  | "system-prompt"
  | "instruction-file";

export interface ContextRow {
  source: SourceId;
  project: string | null;
  kind: ContextKind;
  owner: string;
  items: number;
  bytes: number;
  tokens: Tokens;
  requests: number;
  overhead_tokens: Tokens;
}

export interface ContextReport {
  sessions: number;
  rows: ContextRow[];
  unavailable: Partial<Record<SourceId, number>>;
  first_request: Partial<
    Record<SourceId, { sessions: number; total_tokens: number; max_tokens: number }>
  >;
}

export type UnusedKind = "skill" | "agent" | "mcp-server";

export interface UnusedItem {
  kind: UnusedKind;
  name: string;
  project: string | null;
  clients: SourceId[];
  uses: number;
  requests: number;
  overhead_tokens: Tokens | null;
  /** Unused skills only: the agents, commands and skills whose files name it. */
  referenced_by?: Reference[];
}

export interface UnusedReport {
  sessions: number;
  items: UnusedItem[];
}

export interface TrendWindow {
  since: string;
  until: string;
  sessions: number;
}

export interface TrendWindows {
  current: TrendWindow;
  /** The comparison window. */
  against: TrendWindow;
  /** The windows differ in length: counts are per-day values. */
  normalized: boolean;
}

/** A figure in the comparison window (`before`) and the current one (`after`). */
export interface Delta {
  before: number | null;
  after: number | null;
  /** `null` when either window has no sessions. */
  change: number | null;
  /** In percent of `before`; `null` when `before` is 0. */
  change_pct: number | null;
  /** `false` when either value was estimated. */
  measured: boolean;
}

export interface SkillTrend {
  name: string;
  status: "new" | "gone" | "both";
  loads: Delta;
  overhead_tokens: Delta;
}

export interface TrendReport {
  windows: TrendWindows;
  volume: {
    sessions: Delta;
    requests: Delta;
    subagent_requests: Delta;
    tokens: Delta;
    context_tokens: Delta;
    large_context_requests: Delta;
  };
  cost: { measured_usd: Delta; estimated_usd: Delta; unpriced_requests: Delta };
  efficiency: { cache_hit_rate: Delta; subagent_share: Delta };
  errors: { tool_calls: Delta; failed_calls: Delta; retry_loops: Delta };
  skills: SkillTrend[];
}

export interface SessionGrowth {
  source: SourceId;
  session: string;
  project: string | null;
  model: string | null;
  requests: number;
  first_tokens: number;
  last_tokens: number;
  peak_tokens: number;
  compactions: number[];
  window: number | null;
  window_basis: "documented" | "observed-compaction" | null;
  near_limit: boolean;
}

export interface ToolErrors {
  source: SourceId;
  tool: string;
  calls: number;
  errors: number;
  unknown: number;
  loops: number;
}

export interface RetryLoop {
  source: SourceId;
  session: string;
  tool: string;
  input: string | null;
  calls: number;
  errors: number;
}

export interface HookStats {
  source: SourceId;
  hook: string;
  runs: number;
  failures: number;
  unknown: number;
  total_ms: number;
}

export interface PermissionStats {
  source: SourceId;
  prompts: number;
  answered: number;
  total_wait_ms: number;
}

export interface FlowData {
  growth: { sessions: SessionGrowth[] };
  errors: { tools: ToolErrors[]; loops: RetryLoop[] };
  hooks: {
    hooks: HookStats[];
    permissions: PermissionStats[];
    unavailable: Partial<Record<SourceId, number>>;
  };
}

/** Where the issue number of a session comes from, in the order they are tried. */
export type IssueSource = "branch" | "commit";

export interface CostRow {
  key: string;
  requests: number;
  input: number;
  output: number;
  cache_read: number;
  cache_write: number;
  measured_usd: number;
  estimated_usd: number;
  unpriced_requests: number;
  premium_requests: number;
  /** Only when grouped by issue; the weakest source of the row's sessions. */
  issue_source?: IssueSource;
}

export interface WhatIfRow {
  model: string;
  requests: number;
  actual_usd: number;
  what_if_usd: number;
  excluded_requests: number;
}

export interface SessionCost {
  source: SourceId;
  session: string;
  project: string | null;
  started_at: string | null;
  duration_ms: number | null;
  issue: string | null;
  issue_source: IssueSource | null;
  models: string[];
  requests: number;
  main_requests: number;
  peak_context: number;
  input: number;
  cache_read: number;
  cache_write: number;
  output: number;
  measured_usd: number;
  estimated_usd: number;
  unpriced_requests: number;
  subagent_usd: number;
}

export interface CostData {
  report: {
    rows: CostRow[];
    total: CostRow;
    unpriced_models: Record<string, number>;
    /** Only with `by: "session"`. */
    sessions?: SessionCost[];
  };
  what_if: { target: string; rows: WhatIfRow[]; total: WhatIfRow } | null;
}

export interface CacheRow {
  key: string;
  source: SourceId;
  requests: number;
  input: number;
  cache_read: number;
  cache_write: number;
  hit_rate: number | null;
}

export interface SubagentShare {
  source: SourceId;
  requests: number;
  subagent_requests: number;
  tokens: number;
  subagent_tokens: number;
  usd: number;
  subagent_usd: number;
  unpriced_requests: number;
}

export type RoleKind = "main" | "skill" | "workflow-phase" | "agent-type" | "unknown";

export interface RoleRow {
  role: string;
  kind: RoleKind;
  runs: number;
  requests: number;
  requests_per_run: number;
  input: number;
  cache_read: number;
  cache_write: number;
  output: number;
  usd: number | null;
  usd_per_run: number | null;
  cost_measured: boolean;
  unpriced_requests: number;
  models: string[];
  first_request_context: Tokens;
  uncached_after_first: Tokens;
  hit_rate: number | null;
}

export interface EfficiencyReport {
  cache: CacheRow[];
  subagents: SubagentShare[];
  roles: RoleRow[];
}

export interface SourceStats {
  source: SourceId;
  sessions: number;
  requests: number | null;
  input_tokens: number | null;
  output_tokens: number | null;
  cache_hit_rate: number | null;
  tool_calls: number;
  tool_errors: number;
  tool_error_rate: number | null;
  skill_invocations: number;
  skill_overhead_tokens: Tokens | null;
  total_duration_ms: number | null;
  mean_session_ms: number | null;
}

export interface CompareReport {
  sources: SourceStats[];
}

export interface SessionSummary {
  source: SourceId;
  id: string;
  project: string | null;
  model: string | null;
  started_at: string | null;
  ended_at: string | null;
  requests: number;
  peak_tokens: number;
}

export interface TimelinePoint {
  request: number;
  at: string | null;
  context_tokens: number;
  output_tokens: number;
  cost_usd: number | null;
  cost_estimated: boolean;
}

export type MarkerKind =
  | { type: "skill"; name: string; trigger: "model" | "user" | "nested" }
  | { type: "compaction" }
  | { type: "tool-error"; tool: string }
  | { type: "hook-failed"; hook: string }
  | { type: "permission-prompt"; wait_ms: number | null };

export interface TimelineMarker {
  request: number;
  at: string | null;
  kind: MarkerKind;
}

export interface Timeline {
  session: SessionSummary;
  points: TimelinePoint[];
  markers: TimelineMarker[];
  thinned_from: number | null;
}

/** A newer release than the running application. */
export interface UpdateInfo {
  version: string;
  currentVersion: string;
  notes: string | null;
  date: string | null;
}

export type CliToolMethod = "symlink" | "copy" | "user_path" | "unsupported";
export type CliToolState = "missing" | "installed" | "stale" | "foreign";
export type CliToolAction =
  "created" | "repaired" | "kept" | "removed" | "absent" | "skipped_foreign" | "failed";

/** One command line tool: where its link goes and what is there now. */
export interface CliTool {
  name: string;
  path: string;
  target: string;
  state: CliToolState;
  /** What the call did; null for the status. */
  action: CliToolAction | null;
  error: string | null;
}

/** The line to add to a shell profile so that the tools folder is on PATH. */
export interface ProfileLine {
  shell: string;
  file: string;
  line: string;
}

/** The state of the command line tools (answer of cli_tools_status, _install and _remove). */
export interface CliTools {
  supported: boolean;
  /** Whether the application should show the action (macOS, AppImage and Windows). */
  offered: boolean;
  /** user_path (Windows): dir is the install directory and goes on the user PATH, nothing is linked. */
  method: CliToolMethod;
  dir: string;
  created_dir: boolean;
  /**
   * Whether dir is on the PATH of the application process (a shell's PATH can differ); for user_path
   * whether it is an entry of the user PATH.
   */
  on_path: boolean;
  profile_lines: ProfileLine[];
  tools: CliTool[];
  /** Files in the way that AgentLens did not create; for user_path other folders on PATH with the CLI. */
  foreign: string[];
  note: string | null;
  /** Why the whole call did nothing (for example a temporary location of the application). */
  error: string | null;
}
