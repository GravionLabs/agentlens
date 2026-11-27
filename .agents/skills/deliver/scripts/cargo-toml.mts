// The guard of the deliver loop for `Cargo.toml`: which parts differ between two versions of the file, apart from
// the dependency entries that are allowed to change. No npm dependency (the installed skill has no node_modules):
// a strict parser of a small TOML subset. Whatever it does not understand on either side throws, and the caller
// then treats the file as a plain protected hit (fail closed).
//
// Understood: comments, `[a.b."c"]` headers, `key = value` with bare, quoted and dotted keys, single-line basic and
// literal strings, decimal numbers, booleans, single-line inline tables and arrays. A multi-line array is
// understood only inside a dependency table. Not understood: `[[array tables]]`, multi-line strings, dates,
// hex/octal/binary numbers, newlines inside an inline table, a key or table defined twice.

import { Parser, isTree, newTree, same, union, type Tree } from './toml-subset.mts';

const KINDS = ['dependencies', 'dev-dependencies', 'build-dependencies'];
// An entry with one of these keys points the build at code outside crates.io.
const OUTSIDE = ['path', 'git', 'registry', 'registry-index'];
// Tables whose children are named in a hit: `package.build`, `target.<cfg>.<table>`.
const DESCEND: Record<string, number> = { package: 1, workspace: 1, target: 2 };

/** Is the table at this path a table of dependency entries? */
function isDependencyTable(p: string[]): boolean {
  if (p.length === 1) return KINDS.includes(p[0]!);
  if (p.length === 2) return p[0] === 'workspace' && p[1] === 'dependencies';
  return p.length === 3 && p[0] === 'target' && KINDS.includes(p[2]!);
}
/** Is the path inside or at a dependency table (a place where a multi-line array is understood)? */
function inDependencies(p: string[]): boolean {
  return (
    (p.length >= 1 && KINDS.includes(p[0]!)) ||
    (p.length >= 2 && p[0] === 'workspace' && p[1] === 'dependencies') ||
    (p.length >= 3 && p[0] === 'target' && KINDS.includes(p[2]!))
  );
}

/** The dependency entries of `dependencies` that are not allowed to change in `after` (path, git, registry). */
function dependencyHits(path: string[], before: unknown, after: unknown, out: string[]) {
  if (same(before, after)) return;
  const name = path.join('.');
  if (!isTree(after) || (before !== undefined && !isTree(before))) {
    out.push(name);
    return;
  }
  const a = isTree(before) ? before : newTree();
  for (const entry of union(a, after)) {
    if (same(a[entry], after[entry])) continue;
    const next = after[entry];
    if (next === undefined || typeof next === 'string') continue; // removed, `name = "1.2"`
    if (isTree(next) && !OUTSIDE.some((key) => key in next)) continue;
    out.push(`${name}.${entry}`);
  }
}

/** Compare two tables whose own path is `path`; entries that differ and are not allowed are named in `out`. */
function tableHits(path: string[], before: unknown, after: unknown, out: string[]) {
  if (same(before, after)) return;
  if (isDependencyTable(path)) return dependencyHits(path, before, after, out);
  const depth = DESCEND[path[0] ?? ''] ?? 0;
  const mayDescend = path.length === 0 || path.length <= depth;
  if (
    !mayDescend ||
    (before !== undefined && !isTree(before)) ||
    (after !== undefined && !isTree(after))
  ) {
    out.push(path.join('.'));
    return;
  }
  const a = isTree(before) ? before : newTree();
  const b = isTree(after) ? after : newTree();
  for (const key of union(a, b)) tableHits([...path, key], a[key], b[key], out);
}

/**
 * The parts of `Cargo.toml` that differ between `before` and `after`, outside the dependency entries that may change
 * (`name = "1"` and inline or sub-tables without `path`, `git`, `registry`), each named by its dotted path
 * (`patch`, `package.build`, `dependencies.x`). Throws when either text is not understood.
 */
export function cargoTomlKeys(before: string, after: string): string[] {
  const out: string[] = [];
  tableHits(
    [],
    new Parser(before, inDependencies, 'Cargo.toml').parse(),
    new Parser(after, inDependencies, 'Cargo.toml').parse(),
    out,
  );
  return [...new Set(out)].sort();
}
