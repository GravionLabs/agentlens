#!/usr/bin/env node
// Sync-AgentLabels: creates the missing `agent:*` labels of the repository with their color and description.
// Idempotent: an existing label is left as it is. `node sync-labels.mts` in the checkout; the loop calls it at start.
import { execFileSync } from 'node:child_process';
import { realpathSync } from 'node:fs';
import { fileURLToPath } from 'node:url';
import { STAGES, STAGE_LABELS, labelOf } from './stage.mts';

/** Runs `gh` with these arguments and returns its output. */
export type Gh = (...args: string[]) => string;

/** Returns the names of the labels it created. */
export function syncLabels(gh: Gh, repo: string): string[] {
  const list = JSON.parse(
    gh('label', 'list', '--repo', repo, '--search', 'agent:', '--limit', '100', '--json', 'name'),
  ) as { name: string }[];
  const present = new Set(list.map((label) => label.name.toLowerCase()));
  const created: string[] = [];
  for (const stage of STAGES) {
    const name = labelOf(stage);
    if (present.has(name)) continue;
    const { color, description } = STAGE_LABELS[stage];
    try {
      gh('label', 'create', name, '--repo', repo, '--color', color, '--description', description);
      created.push(name);
    } catch (error) {
      // created by another run since the list was read
      if (!/already exists/i.test(String((error as { stderr?: string }).stderr))) throw error;
    }
  }
  return created;
}

if (process.argv[1] && realpathSync(process.argv[1]) === fileURLToPath(import.meta.url)) {
  const gh: Gh = (...args) =>
    execFileSync('gh', args, { encoding: 'utf8', stdio: ['ignore', 'pipe', 'pipe'] }).trim();
  try {
    const repo = gh('repo', 'view', '--json', 'nameWithOwner', '-q', '.nameWithOwner');
    const created = syncLabels(gh, repo);
    console.log(
      created.length
        ? `created in ${repo}: ${created.join(', ')}`
        : `${repo} has all agent:* labels`,
    );
  } catch (error) {
    const e = error as { stderr?: string; message: string };
    console.error(`sync-labels: ${String(e.stderr || e.message).trim()}`);
    process.exit(1);
  }
}
