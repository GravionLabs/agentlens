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
  until?: string;
  sources: SourceId[];
  project: string | null;
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

export interface Finding {
  kind: FindingKind;
  explanation: string;
  action: string;
  count: number;
  total: Tokens | null;
  share_of_baseline: number | null;
  share_of_total_tokens: number | null;
  items: AdviceItem[];
  notes: string[];
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
}

export interface WhatIfRow {
  model: string;
  requests: number;
  actual_usd: number;
  what_if_usd: number;
  excluded_requests: number;
}

export interface CostData {
  report: { rows: CostRow[]; total: CostRow; unpriced_models: Record<string, number> };
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

export interface EfficiencyReport {
  cache: CacheRow[];
  subagents: SubagentShare[];
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

export type CliToolMethod = "symlink" | "copy" | "unsupported";
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
  /** Whether the application should show the action (macOS and AppImage). */
  offered: boolean;
  method: CliToolMethod;
  dir: string;
  created_dir: boolean;
  /** Whether dir is on the PATH of the application process (a shell's PATH can differ). */
  on_path: boolean;
  profile_lines: ProfileLine[];
  tools: CliTool[];
  foreign: string[];
  note: string | null;
  /** Why the whole call did nothing (for example a temporary location of the application). */
  error: string | null;
}
