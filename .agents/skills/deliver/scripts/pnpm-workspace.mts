// The guard of the deliver loop for `pnpm-workspace.yaml`: which top-level parts differ between two versions of the
// file, apart from the catalog entries that are allowed to change. No npm dependency (the installed skill has no
// node_modules): a strict parser of a small YAML subset. Whatever it does not understand on either side throws, and
// the caller then treats the file as a plain protected hit (fail closed).
//
// Understood: comments, block mappings, block sequences of scalars (also at the indent of their key), plain and
// single- or double-quoted single-line scalars (no backslash in a double-quoted one), single-line flow sequences and,
// only as `catalog` or `catalogs.<name>`, a single-line flow mapping. Not understood: anchors, aliases, tags,
// `|`/`>` or multi-line scalars, document markers, tabs, a key defined twice, maps inside a sequence.

type Node = null | string | Node[] | { [key: string]: Node };
type Tree = { [key: string]: Node };
type Line = { indent: number; text: string };

const QUOTED = '\u0000q'; // marks a quoted scalar (`'1'` is not `1`); NUL cannot occur in the text
const isTree = (v: unknown): v is Tree => !!v && typeof v === 'object' && !Array.isArray(v);
const newTree = (): Tree => Object.create(null) as Tree;
const fail = (what: string): never => {
  throw new Error(`pnpm-workspace.yaml: ${what}`);
};

const RESERVED_START = /^[&*!|>%@`#?:,\]}]/;

/** A single-line quoted scalar at the start of `s`: its marked value and the rest of the line. */
function quoted(s: string): { value: string; rest: string } {
  const q = s[0]!;
  let out = '';
  for (let i = 1; i < s.length; i++) {
    const c = s[i]!;
    if (q === "'") {
      if (c === "'") {
        if (s[i + 1] === "'") {
          out += "'";
          i++;
        } else return { value: QUOTED + out, rest: s.slice(i + 1) };
      } else out += c;
    } else {
      if (c === '\\') fail('escape in a double-quoted scalar');
      if (c === '"') return { value: QUOTED + out, rest: s.slice(i + 1) };
      out += c;
    }
  }
  return fail('unterminated quoted scalar');
}
/** The rest of a line after a value: blanks and a comment only. */
function endOfValue(rest: string) {
  const t = rest.trim();
  if (t !== '' && !(t.startsWith('#') && /^\s/.test(rest))) fail('unexpected text after a value');
}
/** A block scalar: quoted, or plain up to a comment. */
function scalar(s: string): string {
  const t = s.trim();
  if (t[0] === '"' || t[0] === "'") {
    const q = quoted(t);
    endOfValue(q.rest);
    return q.value;
  }
  const plain = t.replace(/\s+#.*$/, '').trim();
  if (plain === '' || RESERVED_START.test(plain) || /^-(\s|$)/.test(plain)) fail('bad scalar');
  if (/(:\s|:$)/.test(plain) || /[{}[\]]/.test(plain[0] ?? '')) fail('bad scalar');
  return plain;
}

/** A single-line flow collection, scalars only. */
function flow(s: string, map: boolean): Node {
  const open = map ? '{' : '[';
  const close = map ? '}' : ']';
  const t = s.trim();
  if (!t.startsWith(open)) return fail('bad flow collection');
  let i = 1;
  const items: string[] = [];
  const keys: string[] = [];
  const out = newTree();
  const seen = (k: string) => (k in out ? fail('key defined twice') : undefined);
  const item = (): string => {
    while (t[i] === ' ') i++;
    if (t[i] === '"' || t[i] === "'") {
      const q = quoted(t.slice(i));
      i = t.length - q.rest.length;
      return q.value;
    }
    const m = /^[^\s,{}[\]:#'"&*!|>%@`?-][^,{}[\]:#'"]*|^-[^\s,{}[\]:#'"][^,{}[\]:#'"]*/.exec(
      t.slice(i),
    );
    if (!m) return fail('bad flow scalar');
    i += m[0].length;
    return m[0].trim();
  };
  for (;;) {
    while (t[i] === ' ') i++;
    if (t[i] === close) {
      i++;
      break;
    }
    if (map) {
      const k = item();
      while (t[i] === ' ') i++;
      if (t[i] !== ':') fail('bad flow mapping');
      i++;
      const v = item();
      seen(k);
      keys.push(k);
      out[k] = v;
    } else items.push(item());
    while (t[i] === ' ') i++;
    if (t[i] === ',') i++;
    else if (t[i] !== close) fail('bad flow collection');
  }
  endOfValue(t.slice(i));
  return map ? out : items;
}

class Parser {
  readonly lines: Line[] = [];
  i = 0;
  constructor(text: string) {
    if (/[\u0000﻿]/.test(text) || /\r(?!\n)/.test(text)) fail('control character');
    for (const raw of text.replace(/\r\n/g, '\n').split('\n')) {
      if (raw.trim() === '' || raw.trimStart().startsWith('#')) continue;
      const indent = raw.length - raw.trimStart().length;
      if (/^\s*\t/.test(raw) || /^\s*\t/.test(raw.slice(0, indent + 1))) fail('tab');
      if (/^(---|\.\.\.)(\s|$)/.test(raw)) fail('document marker');
      this.lines.push({ indent, text: raw.slice(indent) });
    }
  }
  parse(): Tree {
    if (this.lines.length === 0) return newTree();
    const first = this.lines[0]!;
    if (first.indent !== 0) fail('bad indent');
    const tree = this.mapping(0, []);
    if (this.i < this.lines.length) fail('bad indent');
    return tree;
  }
  isDash(l: Line) {
    return /^-(\s|$)/.test(l.text);
  }
  sequence(indent: number): Node[] {
    const out: Node[] = [];
    while (this.i < this.lines.length) {
      const l = this.lines[this.i]!;
      if (l.indent !== indent || !this.isDash(l)) break;
      const rest = l.text.slice(1).trim();
      if (rest === '') fail('empty sequence item');
      if (rest[0] === '[') out.push(flow(rest, false));
      else out.push(scalar(rest));
      if (/^[^\s'"][^#]*:(\s|$)/.test(rest)) fail('map in a sequence');
      this.i++;
      const next = this.lines[this.i];
      if (next && next.indent > indent) fail('multi-line item');
    }
    return out;
  }
  mapping(indent: number, path: string[]): Tree {
    const out = newTree();
    while (this.i < this.lines.length) {
      const l = this.lines[this.i]!;
      if (l.indent < indent) break;
      if (l.indent > indent || this.isDash(l)) fail('bad structure');
      let key: string;
      let rest: string;
      if (l.text[0] === '"' || l.text[0] === "'") {
        const q = quoted(l.text);
        key = q.value;
        const m = /^\s*:(\s+(.*))?$/.exec(q.rest);
        if (!m) return fail('bad key');
        rest = m[2] ?? '';
      } else {
        const m = /^(.*?)(?::\s+|:$)(.*)$/.exec(l.text);
        if (!m) return fail('bad key');
        key = m[1]!.trim();
        rest = m[2]!;
        if (key === '' || RESERVED_START.test(key) || /[#{}[\],]/.test(key) || /^-(\s|$)/.test(key))
          fail('bad key');
      }
      if (key in out) fail('key defined twice');
      const here = [...path, key];
      this.i++;
      const next = this.lines[this.i];
      rest = rest.trim();
      const comment = rest.startsWith('#') ? '' : rest;
      if (comment === '') {
        if (next && next.indent > indent) {
          out[key] = this.isDash(next)
            ? this.sequence(next.indent)
            : this.mapping(next.indent, here);
          const after = this.lines[this.i];
          if (after && after.indent > indent) fail('bad indent');
        } else if (next && next.indent === indent && this.isDash(next)) {
          out[key] = this.sequence(indent);
        } else out[key] = null;
        continue;
      }
      if (comment[0] === '{') {
        const catalogPlace =
          (here.length === 1 && here[0] === 'catalog') ||
          (here.length === 2 && here[0] === 'catalogs');
        if (!catalogPlace) fail('flow mapping');
        out[key] = flow(comment, true);
      } else if (comment[0] === '[') out[key] = flow(comment, false);
      else out[key] = scalar(comment);
      if (next && next.indent > indent) fail('multi-line scalar');
    }
    return out;
  }
}

const canonical = (v: unknown): string =>
  JSON.stringify(v, (_k, x) =>
    isTree(x)
      ? Object.fromEntries(Object.entries(x).sort(([a], [b]) => (a < b ? -1 : a > b ? 1 : 0)))
      : x,
  );
const same = (a: unknown, b: unknown) => a === b || canonical(a) === canonical(b);
const union = (a: Tree, b: Tree) => [...new Set([...Object.keys(a), ...Object.keys(b)])];

// One comparator of a version range: `^1.2.3`, `>=1`, `1.x`, `*`.
const COMPARATOR =
  /^(?:[<>]=?|=|\^|~)?v?(?:\d+|[xX*])(?:\.(?:\d+|[xX*])){0,2}(?:-[0-9A-Za-z.-]+)?(?:\+[0-9A-Za-z.-]+)?$/;
/** A plain semver range (or `workspace:`), nothing that points outside the registry. */
export function isPlainRange(value: string): boolean {
  const v = value.startsWith(QUOTED) ? value.slice(QUOTED.length) : value;
  if (/^workspace:(?:[*^~]|.*)$/.test(v)) {
    const rest = v.slice('workspace:'.length);
    return rest === '' || /^[*^~]$/.test(rest) || isPlainRange(rest);
  }
  const sets = v.split('||');
  return sets.every((set) => {
    const tokens = set.trim().split(/\s+/);
    return tokens.every((t, n) =>
      t === '-' ? n > 0 && n < tokens.length - 1 : COMPARATOR.test(t),
    );
  });
}

/** The entries of one catalog that may not change in `after`. */
function catalogHits(
  name: string,
  before: Node | undefined,
  after: Node | undefined,
  out: string[],
) {
  if (same(before, after)) return;
  const empty = (n: Node | undefined) => n === undefined || n === null;
  if ((!empty(after) && !isTree(after)) || (!empty(before) && !isTree(before))) {
    out.push(name);
    return;
  }
  const a = isTree(before) ? before : newTree();
  const b = isTree(after) ? after : newTree();
  for (const entry of union(a, b)) {
    if (same(a[entry], b[entry])) continue;
    const next = b[entry];
    if (next === undefined) continue; // removed
    if (typeof next === 'string' && isPlainRange(next)) continue;
    out.push(`${name}.${entry}`);
  }
}

/**
 * The top-level parts of `pnpm-workspace.yaml` that differ between `before` and `after`, outside the entries of
 * `catalog` and of the named catalogs under `catalogs` (a plain semver range or `workspace:`). Throws when either
 * text is not understood.
 */
export function pnpmWorkspaceKeys(before: string, after: string): string[] {
  const a = new Parser(before).parse();
  const b = new Parser(after).parse();
  const out: string[] = [];
  for (const key of union(a, b)) {
    if (same(a[key], b[key])) continue;
    if (key === 'catalog') catalogHits(key, a[key], b[key], out);
    else if (key === 'catalogs') {
      const x = a[key];
      const y = b[key];
      const empty = (n: Node | undefined) => n === undefined || n === null;
      if ((!empty(x) && !isTree(x)) || (!empty(y) && !isTree(y))) out.push(key);
      else {
        const ta = isTree(x) ? x : newTree();
        const tb = isTree(y) ? y : newTree();
        for (const name of union(ta, tb)) catalogHits(`catalogs.${name}`, ta[name], tb[name], out);
      }
    } else out.push(key);
  }
  return [...new Set(out)].sort();
}
