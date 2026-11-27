// The gate `check:changed` (references/gate.md): copy to `scripts/check-changed.mts` and wire it as
// the package script `"check:changed": "node scripts/check-changed.mts"` (Node >= 22.18, no build).
// Steps on the changed files (diff against the merge-base with the base, plus untracked files):
// format check, lint, typecheck, related tests, then every `checks` entry of `.agents/config.json`.
// Success prints one line; a failed step prints its full output (only that step) and the exit is 1.
// A pass is cached under `<git-common-dir>/check-changed/` (key: content of the changed files, base commit,
// step list), so worktrees and agents share it; only passes are cached. A hit prints `cached pass`.
// Usage: node scripts/check-changed.mts [--base=<ref>] [--no-cache]   (GATE_MAX_WORKERS limits the test workers)
// Base: --base, else `base` of `.agents/config.json`, else origin/HEAD, else main/master.
import { spawnSync } from 'node:child_process';
import { createHash } from 'node:crypto';
import { existsSync, mkdirSync, readdirSync, readFileSync, writeFileSync } from 'node:fs';
import { availableParallelism } from 'node:os';
import * as path from 'node:path';

interface Step {
  name: string;
  run: () => { status: number | null; output: string } | undefined; // undefined: step not applicable
}

const git = (...args: string[]) => {
  const r = spawnSync('git', args, { encoding: 'utf8' });
  return { ok: r.status === 0, out: (r.stdout ?? '').trim(), err: (r.stderr ?? '').trim() };
};
const lines = (s: string) => s.split('\n').filter(Boolean);

function configBase(): string | undefined {
  const file = path.resolve('.agents/config.json');
  if (!existsSync(file)) return undefined;
  try {
    const base = (JSON.parse(readFileSync(file, 'utf8')) as { base?: unknown }).base;
    return typeof base === 'string' && base ? base : undefined;
  } catch {
    return undefined; // a broken config is reported by configChecks
  }
}

function baseRef(argv: string[]): string {
  const flag = argv.find((a) => a.startsWith('--base='));
  if (flag) return flag.slice('--base='.length);
  const configured = configBase();
  if (configured) return configured;
  for (const ref of ['origin/HEAD', 'origin/main', 'origin/master', 'main', 'master'])
    if (git('rev-parse', '--verify', '--quiet', ref).ok) return ref;
  throw new Error('no base branch found; pass --base=<ref>');
}

export function changedFiles(base: string): string[] {
  const mb = git('merge-base', base, 'HEAD');
  if (!mb.ok) throw new Error(`git merge-base ${base} HEAD failed: ${mb.err}`);
  const diff = git('diff', '--name-only', '--diff-filter=d', mb.out);
  const untracked = git('ls-files', '--others', '--exclude-standard');
  if (!diff.ok || !untracked.ok) throw new Error(`git failed: ${diff.err || untracked.err}`);
  return [...new Set([...lines(diff.out), ...lines(untracked.out)])].filter((f) => existsSync(f));
}

export function workers(env = process.env): number {
  const n = Number(env.GATE_MAX_WORKERS);
  if (Number.isInteger(n) && n >= 1) return n;
  return Math.max(1, Math.min(4, Math.floor(availableParallelism() / 2)));
}

const bin = (tool: string) => {
  const dir = path.resolve('node_modules/.bin');
  return ['', '.cmd'].some((e) => existsSync(path.join(dir, tool + e))) ? tool : undefined;
};
const tool = (...names: string[]) => names.find((n) => bin(n));

function exec(cmd: string, args: string[]) {
  const r = spawnSync(cmd, args, {
    encoding: 'utf8',
    shell: process.platform === 'win32',
    env: { ...process.env, PATH: `${path.resolve('node_modules/.bin')}${path.delimiter}${process.env.PATH}` },
  });
  const output = `${r.stdout ?? ''}${r.stderr ?? ''}${r.error ? `${r.error.message}\n` : ''}`;
  return { status: r.error ? 1 : r.status, output };
}

const ext = (files: string[], re: RegExp) => files.filter((f) => re.test(f));
const FORMAT = /\.(m?[jt]sx?|cjs|cts|json|jsonc|css|scss|html|md|ya?ml)$/;
const LINT = /\.(m?[jt]sx?|cjs|cts)$/;
const STYLES = /\.(css|scss)$/;
const hasStylelint = () =>
  readdirSync('.').some((f) => /^stylelint\.config\.|^\.stylelintrc/.test(f)) && bin('stylelint');

export function steps(files: string[]): Step[] {
  const out: Step[] = [];
  const fmtFiles = ext(files, FORMAT);
  const lintFiles = ext(files, LINT);
  out.push({
    name: 'format',
    run: () => {
      if (!fmtFiles.length) return undefined;
      const t = tool('oxfmt', 'biome', 'prettier');
      if (t === 'oxfmt') return exec(t, ['--check', '--no-error-on-unmatched-pattern', ...fmtFiles]);
      if (t === 'biome') return exec(t, ['format', '--no-errors-on-unmatched', ...fmtFiles]);
      if (t === 'prettier') return exec(t, ['--check', '--ignore-unknown', ...fmtFiles]);
      return undefined;
    },
  });
  out.push({
    name: 'lint',
    run: () => {
      if (!lintFiles.length) return undefined;
      const t = tool('oxlint', 'biome', 'eslint');
      if (t === 'oxlint') return exec(t, ['--no-error-on-unmatched-pattern', ...lintFiles]);
      if (t === 'biome') return exec(t, ['lint', '--no-errors-on-unmatched', ...lintFiles]);
      if (t === 'eslint') return exec(t, ['--no-warn-ignored', ...lintFiles]);
      return undefined;
    },
  });
  const styleFiles = ext(files, STYLES);
  out.push({
    name: 'stylelint',
    run: () => (styleFiles.length && hasStylelint() ? exec('stylelint', styleFiles) : undefined),
  });
  out.push({
    name: 'typecheck',
    run: () => {
      if (!lintFiles.length || !existsSync('tsconfig.json') || !bin('tsc')) return undefined;
      return exec('tsc', ['--noEmit']);
    },
  });
  out.push({
    name: 'tests',
    run: () => {
      if (!lintFiles.length) return undefined;
      const n = String(workers());
      if (bin('vitest')) return exec('vitest', ['related', '--run', `--maxWorkers=${n}`, ...lintFiles]);
      if (bin('jest'))
        return exec('jest', ['--findRelatedTests', `--maxWorkers=${n}`, '--passWithNoTests', ...lintFiles]);
      return undefined;
    },
  });
  return out;
}

export function configChecks(): { name: string; command: string[] }[] {
  const file = path.resolve('.agents/config.json');
  if (!existsSync(file)) return [];
  const cfg = JSON.parse(readFileSync(file, 'utf8')) as {
    checks?: { name?: string; command: string | string[] }[];
  };
  return (cfg.checks ?? []).map((c) => ({
    name: c.name ?? String(Array.isArray(c.command) ? c.command[0] : c.command),
    command: Array.isArray(c.command) ? c.command : [c.command],
  }));
}

// Cache key: content of the changed files (name + bytes), commit of the base, names of the steps.
export function cacheKey(files: string[], base: string, stepList: string[]): string {
  const h = createHash('sha256');
  const baseSha = git('rev-parse', '--verify', base);
  h.update(`base\0${baseSha.ok ? baseSha.out : base}\0steps\0${stepList.join('\n')}\0`);
  for (const f of [...files].sort()) h.update(`file\0${f}\0`).update(readFileSync(f)).update('\0');
  return h.digest('hex');
}

function cacheFile(key: string): string | undefined {
  const dir = git('rev-parse', '--git-common-dir');
  return dir.ok ? path.join(path.resolve(dir.out), 'check-changed', key) : undefined;
}

function main(): number {
  const argv = process.argv.slice(2);
  let files: string[];
  let base: string;
  try {
    base = baseRef(argv);
    files = changedFiles(base);
  } catch (e) {
    console.error(`check-changed: ${(e as Error).message}`);
    return 2;
  }
  let checks: ReturnType<typeof configChecks>;
  try {
    checks = configChecks();
  } catch (e) {
    console.error(`check-changed: .agents/config.json: ${(e as Error).message}`);
    return 2;
  }
  const stepList = [
    ...steps(files).map((s) => s.name),
    ...checks.map((c) => `check ${c.name}: ${c.command.join(' && ')}`),
  ];
  const cache = argv.includes('--no-cache') ? undefined : cacheFile(cacheKey(files, base, stepList));
  if (cache && existsSync(cache)) {
    console.log('cached pass');
    return 0;
  }
  const failed: string[] = [];
  const report = (name: string, r: { status: number | null; output: string } | undefined) => {
    if (!r || r.status === 0) return;
    failed.push(name);
    console.log(`=== check:changed ${name}: FAILED (exit ${r.status}) ===\n${r.output.replace(/\s+$/, '')}`);
  };
  for (const s of steps(files)) report(s.name, s.run());
  for (const c of checks) {
    for (const cmd of c.command) {
      const r = spawnSync(cmd, { shell: true, encoding: 'utf8' });
      report(`check ${c.name}`, { status: r.status ?? 1, output: `${r.stdout ?? ''}${r.stderr ?? ''}` });
    }
  }
  if (failed.length) {
    console.log(`check:changed failed: ${failed.join(', ')}`);
    return 1;
  }
  if (cache) {
    try {
      mkdirSync(path.dirname(cache), { recursive: true });
      writeFileSync(cache, `${new Date().toISOString()}\n`);
    } catch {
      // an unwritable cache only costs the next run
    }
  }
  console.log(`check:changed: ok (${files.length} changed file${files.length === 1 ? '' : 's'})`);
  return 0;
}

process.exitCode = main();
