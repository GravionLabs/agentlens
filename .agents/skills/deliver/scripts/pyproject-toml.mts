// The guard of the deliver loop for `pyproject.toml`: which parts differ between two versions of the file, apart from
// the requirement lists that are allowed to change. Uses the strict TOML subset parser of `toml-subset.mts`; a file it
// does not understand throws and the caller then treats the file as a plain protected hit (fail closed). Not
// understood, so a hit: `[[array tables]]` (`[[tool.uv.index]]`, `[[tool.mypy.overrides]]`), multi-line strings, dates.

import { Parser, canonical, isTree, newTree, same, union } from './toml-subset.mts';

// The requirement lists whose items may be added, removed or changed. A fixed list: a repository cannot widen it.
const LISTS = [
  ['project', 'dependencies'],
  ['tool', 'uv', 'dev-dependencies'],
];
// Tables whose every child is such a list.
const LIST_TABLES = [['project', 'optional-dependencies'], ['dependency-groups']];

const at = (a: string[], b: string[]) => a.length === b.length && a.every((k, i) => k === b[i]);
const isList = (p: string[]) =>
  LISTS.some((l) => at(l, p)) || LIST_TABLES.some((t) => at(t, p.slice(0, -1)));
// Tables that are walked into to name the part that differs: every strict prefix of an allowed place.
const isWalked = (p: string[]) =>
  LISTS.some((l) => p.length < l.length && at(l.slice(0, p.length), p)) ||
  LIST_TABLES.some((t) => p.length <= t.length && at(t.slice(0, p.length), p));

// A PEP 508 requirement without a direct reference: `name`, extras, a version part of operators and numbers, a marker.
// An `@`, a `:`, a slash or anything else in the part before the marker makes it a hit (`name @ git+https://…`, `./x`).
const REQUIREMENT =
  /^[A-Za-z0-9](?:[A-Za-z0-9._-]*[A-Za-z0-9])?\s*(?:\[[A-Za-z0-9._,\s-]*\])?\s*(?:\(?\s*[<>=!~][A-Za-z0-9.*+!,<>=~()\s_-]*)?$/;
const plainRequirement = (item: unknown): boolean =>
  typeof item === 'string' && REQUIREMENT.test(item.split(';')[0]!.trim());

/** The items of the list at `path` that are new in `after` and not a plain requirement (a list that is no list). */
function listHits(path: string[], before: unknown, after: unknown, out: string[]) {
  if (same(before, after)) return;
  const name = path.join('.');
  if (!Array.isArray(after) || (before !== undefined && !Array.isArray(before))) {
    if (after !== undefined) out.push(name); // a removed list is allowed
    return;
  }
  // an item that was there before stays, whatever it is (`{ include-group = "x" }`); a new one must be a plain string
  const known = new Set(Array.isArray(before) ? before.map((i) => canonical(i)) : []);
  if (after.some((item) => !known.has(canonical(item)) && !plainRequirement(item))) out.push(name);
}

function tableHits(path: string[], before: unknown, after: unknown, out: string[]) {
  if (same(before, after)) return;
  if (isList(path)) return listHits(path, before, after, out);
  if (
    !(path.length === 0 || isWalked(path)) ||
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
 * The parts of `pyproject.toml` that differ between `before` and `after`, outside the requirement lists that may
 * change (`project.dependencies`, `project.optional-dependencies.*`, `dependency-groups.*`, `tool.uv.dev-dependencies`:
 * plain PEP 508 requirements, no direct reference), each named by its dotted path (`build-system`, `tool.pytest`,
 * `project.scripts`). Throws when either text is not understood.
 */
export function pyprojectTomlKeys(before: string, after: string): string[] {
  const out: string[] = [];
  const parse = (text: string) => new Parser(text, () => true, 'pyproject.toml').parse();
  tableHits([], parse(before), parse(after), out);
  return [...new Set(out)].sort();
}
