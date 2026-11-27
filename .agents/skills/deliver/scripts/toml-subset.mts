// A strict parser of a small TOML subset for the guards of the deliver loop (`Cargo.toml`, `pyproject.toml`). No npm
// dependency (the installed skill has no node_modules). Whatever it does not understand throws, and the caller then
// treats the file as a plain protected hit (fail closed).
//
// Understood: comments, `[a.b."c"]` headers, `key = value` with bare, quoted and dotted keys, single-line basic and
// literal strings, decimal numbers, booleans, single-line inline tables and arrays. A multi-line array is
// understood only where `multiline(path)` says so. Not understood: `[[array tables]]`, multi-line strings, dates,
// hex/octal/binary numbers, newlines inside an inline table, a key or table defined twice.

export type Tree = { [key: string]: unknown };

export const isTree = (v: unknown): v is Tree => !!v && typeof v === 'object' && !Array.isArray(v);
export const newTree = (): Tree => Object.create(null) as Tree;

const ESCAPES: Record<string, string> = {
  b: '\b',
  t: '\t',
  n: '\n',
  f: '\f',
  r: '\r',
  '"': '"',
  '\\': '\\',
};

export class Parser {
  pos = 0;
  newlines = 0;
  readonly text: string;
  readonly multiline: (path: string[]) => boolean;
  readonly label: string;
  constructor(text: string, multiline: (path: string[]) => boolean, label = 'TOML') {
    this.text = text;
    this.multiline = multiline;
    this.label = label;
  }

  fail(what: string): never {
    throw new Error(`${this.label}: ${what} at ${this.pos}`);
  }
  peek(): string | undefined {
    return this.text[this.pos];
  }
  skipBlank() {
    while (this.peek() === ' ' || this.peek() === '\t') this.pos++;
  }
  skipComment() {
    if (this.peek() !== '#') return;
    while (this.pos < this.text.length && this.peek() !== '\n') this.pos++;
  }
  /** The end of a line: blanks, an optional comment, then a newline or the end of the file. */
  endOfLine() {
    this.skipBlank();
    this.skipComment();
    if (this.pos >= this.text.length) return;
    if (this.peek() === '\r' && this.text[this.pos + 1] === '\n') this.pos++;
    if (this.peek() !== '\n') this.fail('unexpected text');
    this.pos++;
  }
  /** Whitespace, newlines and comments between the elements of an array. */
  skipSpace() {
    for (;;) {
      this.skipBlank();
      this.skipComment();
      const c = this.peek();
      if (c === '\n') {
        this.newlines++;
        this.pos++;
      } else if (c === '\r' && this.text[this.pos + 1] === '\n') {
        this.pos++;
      } else return;
    }
  }

  parse(): Tree {
    const root = newTree();
    let table: string[] = [];
    let current = root;
    const defined = new Set<string>();
    for (;;) {
      this.skipBlank();
      if (this.pos >= this.text.length) return root;
      const c = this.peek();
      if (c === '#' || c === '\n' || c === '\r') {
        this.skipComment();
        this.endOfLine();
        continue;
      }
      if (c === '[') {
        this.pos++;
        if (this.peek() === '[') this.fail('array table');
        this.skipBlank();
        table = this.keyPath(']');
        this.skipBlank();
        if (this.peek() !== ']') this.fail('bad table header');
        this.pos++;
        this.endOfLine();
        const id = JSON.stringify(table);
        if (defined.has(id)) this.fail('table defined twice');
        defined.add(id);
        current = this.descend(root, table);
        continue;
      }
      const keys = this.keyPath('=');
      this.skipBlank();
      if (this.peek() !== '=') this.fail('expected =');
      this.pos++;
      this.skipBlank();
      const full = [...table, ...keys];
      const before = this.newlines;
      const value = this.value();
      if (this.newlines !== before && !this.multiline(full)) this.fail('multi-line value');
      this.endOfLine();
      const holder = this.descend(current, keys.slice(0, -1));
      const last = keys[keys.length - 1]!;
      if (last in holder) this.fail('key defined twice');
      holder[last] = value;
    }
  }

  /** The table at `path` below `from`, created when missing. */
  descend(from: Tree, path: string[]): Tree {
    let node = from;
    for (const key of path) {
      const next = node[key];
      if (next === undefined) {
        const created = newTree();
        node[key] = created;
        node = created;
      } else if (isTree(next)) node = next;
      else this.fail('key is not a table');
    }
    return node;
  }

  /** `a.b."c"` up to (not including) the `stop` character. */
  keyPath(stop: string): string[] {
    const keys: string[] = [];
    for (;;) {
      this.skipBlank();
      const c = this.peek();
      if (c === '"') keys.push(this.basicString());
      else if (c === "'") keys.push(this.literalString());
      else {
        const m = /^[A-Za-z0-9_-]+/.exec(this.text.slice(this.pos, this.pos + 256));
        if (!m) this.fail('bad key');
        keys.push(m[0]);
        this.pos += m[0].length;
      }
      this.skipBlank();
      if (this.peek() === '.') {
        this.pos++;
        continue;
      }
      if (this.peek() !== stop) this.fail('bad key');
      return keys;
    }
  }

  basicString(): string {
    if (this.text.startsWith('"""', this.pos)) this.fail('multi-line string');
    this.pos++;
    let out = '';
    for (;;) {
      const c = this.text[this.pos++];
      if (c === undefined || c === '\n' || c === '\r' || c === '\u0000')
        this.fail('unterminated string');
      if (c === '"') return out;
      if (c !== '\\') {
        out += c;
        continue;
      }
      const e = this.text[this.pos++];
      if (e !== undefined && e in ESCAPES) out += ESCAPES[e];
      else if (e === 'u' || e === 'U') {
        const len = e === 'u' ? 4 : 8;
        const hex = this.text.slice(this.pos, this.pos + len);
        if (!new RegExp(`^[0-9A-Fa-f]{${len}}$`).test(hex)) this.fail('bad escape');
        const code = parseInt(hex, 16);
        if (code === 0 || code > 0x10ffff || (code >= 0xd800 && code <= 0xdfff))
          this.fail('bad escape');
        out += String.fromCodePoint(code);
        this.pos += len;
      } else this.fail('bad escape');
    }
  }
  literalString(): string {
    if (this.text.startsWith("'''", this.pos)) this.fail('multi-line string');
    const end = this.text.indexOf("'", this.pos + 1);
    const body = end < 0 ? '' : this.text.slice(this.pos + 1, end);
    if (end < 0 || /[\r\n\u0000]/.test(body)) this.fail('unterminated string');
    this.pos = end + 1;
    return body;
  }

  value(): unknown {
    const c = this.peek();
    if (c === '"') return this.basicString();
    if (c === "'") return this.literalString();
    if (c === '[') return this.array();
    if (c === '{') return this.inlineTable();
    const m = /^[^\s,\]}#]+/.exec(this.text.slice(this.pos, this.pos + 64));
    if (!m) this.fail('bad value');
    const word = m[0];
    this.pos += word.length;
    if (word === 'true') return true;
    if (word === 'false') return false;
    if (/^[+-]?(0|[1-9](_?\d)*)(\.\d(_?\d)*)?([eE][+-]?\d(_?\d)*)?$/.test(word)) {
      // the raw token, never a double: `1e999` and `2e999` must differ (NUL cannot occur in a parsed string)
      return `\u0000${word.replace(/_/g, '')}`;
    }
    return this.fail('unsupported value');
  }

  array(): unknown[] {
    this.pos++;
    const out: unknown[] = [];
    for (;;) {
      this.skipSpace();
      if (this.peek() === ']') {
        this.pos++;
        return out;
      }
      out.push(this.value());
      this.skipSpace();
      if (this.peek() === ',') this.pos++;
      else if (this.peek() !== ']') this.fail('bad array');
    }
  }

  inlineTable(): Tree {
    this.pos++;
    const out = newTree();
    this.skipBlank();
    if (this.peek() === '}') {
      this.pos++;
      return out;
    }
    for (;;) {
      this.skipBlank();
      const keys = this.keyPath('=');
      this.pos++;
      this.skipBlank();
      const value = this.value();
      const holder = this.descend(out, keys.slice(0, -1));
      const last = keys[keys.length - 1]!;
      if (last in holder) this.fail('key defined twice');
      holder[last] = value;
      this.skipBlank();
      const c = this.peek();
      this.pos++;
      if (c === '}') return out;
      if (c !== ',') this.fail('bad inline table');
    }
  }
}

export const canonical = (v: unknown): string =>
  JSON.stringify(v, (_key, x) =>
    isTree(x)
      ? Object.fromEntries(Object.entries(x).sort(([a], [b]) => (a < b ? -1 : a > b ? 1 : 0)))
      : x,
  );
export const same = (a: unknown, b: unknown) => a === b || canonical(a) === canonical(b);
export const union = (a: Tree, b: Tree) => [...new Set([...Object.keys(a), ...Object.keys(b)])];
