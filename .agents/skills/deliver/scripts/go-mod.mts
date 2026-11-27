// The guard of the deliver loop for `go.mod`: which directives differ between two versions of the file, apart from the
// `require` lines that are allowed to change. No npm dependency (the installed skill has no node_modules): a strict
// line parser of the subset below. Whatever it does not understand on either side throws, and the caller then treats
// the file as a plain protected hit (fail closed).
//
// Understood: blank and comment lines, single-line directives, `verb (` blocks with one entry per line, a trailing
// `// comment`. Not understood, so a hit: quotes, backticks (no multi-line or quoted string can hide a line), control
// characters, an unknown verb, a nested or unclosed block, a `require` entry that is not `<path> <version>`.

const VERBS = new Set([
  'module',
  'go',
  'toolchain',
  'godebug',
  'require',
  'replace',
  'exclude',
  'retract',
  'tool',
  'ignore',
]);
const ENTRY = /^[^\s"'`()\\]+\s+v[^\s"'`()\\]*$/;

/** The directives of a file: per verb the normalized entries (comments dropped, blanks collapsed), sorted. */
function parse(text: string): Map<string, string[]> {
  const out = new Map<string, string[]>();
  let block: string | undefined;
  const add = (verb: string, entry: string) => {
    if (verb === 'require' && !ENTRY.test(entry)) throw new Error(`go.mod: bad require "${entry}"`);
    if (verb === 'require') entry = entry.replace(/\s+/g, ' ');
    out.set(verb, [...(out.get(verb) ?? []), entry]);
  };
  for (const raw of text.replace(/\r?\n$/, '').split(/\r?\n/)) {
    if (/[\u0000-\u0008\u000b-\u001f\u007f"'`\\]/.test(raw))
      throw new Error('go.mod: unexpected character');
    const line = raw
      .replace(/\/\/.*$/, '')
      .replace(/\s+/g, ' ')
      .trim();
    if (line === '') continue;
    if (block !== undefined) {
      if (line === ')') block = undefined;
      else if (line.includes('(') || line.includes(')')) throw new Error('go.mod: bad block entry');
      else add(block, line);
      continue;
    }
    const m = /^([a-z]+) ?(.*)$/.exec(line);
    if (!m || !VERBS.has(m[1]!)) throw new Error('go.mod: unknown directive');
    const [, verb, rest] = m as unknown as [string, string, string];
    if (rest === '(') {
      block = verb;
      if (!out.has(verb)) out.set(verb, []);
    } else if (rest === '' || rest.includes('(') || rest.includes(')')) {
      throw new Error('go.mod: bad directive');
    } else add(verb, rest);
  }
  if (block !== undefined) throw new Error('go.mod: unclosed block');
  for (const list of out.values()) list.sort();
  return out;
}

/** The directives (other than `require`) in which two versions of a `go.mod` differ, sorted; throws when unknown. */
export function goModKeys(before: string, after: string): string[] {
  const a = parse(before);
  const b = parse(after);
  return [...new Set([...a.keys(), ...b.keys()])]
    .filter(
      (verb) =>
        verb !== 'require' &&
        JSON.stringify(a.get(verb) ?? []) !== JSON.stringify(b.get(verb) ?? []),
    )
    .sort();
}
