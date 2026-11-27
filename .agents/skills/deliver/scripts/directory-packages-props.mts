// The guard of the deliver loop for `Directory.Packages.props`: which parts differ between two versions of the file,
// apart from the `<PackageVersion Include Version />` items that are allowed to change. No npm dependency (the
// installed skill has no node_modules): a strict tokenizer of the XML subset below. Whatever it does not understand
// on either side throws, and the caller then treats the file as a plain protected hit (fail closed).
//
// Understood: elements with double-quoted attributes, text, comments without `<` or `--` inside, an XML declaration
// at the start. Not understood, so a hit: CDATA, DOCTYPE, processing instructions, single-quoted attributes, a
// comment that holds markup, entities in names, mismatched tags, more than one root, a root other than `Project`.
//
// Allowed to differ: `<PackageVersion Include="<id>" Version="<version>" />` on one line, a direct child of an
// `<ItemGroup>` (no attribute but `Label`) that is a direct child of `<Project>`. The ItemGroup start tags stay
// part of the comparison: a new group is a hit, as is anything else (`Condition`, `VersionOverride`, more attributes).

const TAG = /<(\/?)([A-Za-z_][\w.-]*)((?:\s+[A-Za-z_][\w:.-]*="[^"<]*")*)\s*(\/?)>/y;
const COMMENT = /<!--(?:(?!--)[^<])*-->/y;
const DECLARATION = /^<\?xml\s[^<>?]*\?>/;
const ATTRIBUTE = /([A-Za-z_][\w:.-]*)="([^"<]*)"/g;
const ID = /^[A-Za-z0-9_.-]+$/;
const VERSION = /^[A-Za-z0-9_.+*,\-[\]()$]+$/;

/** The signatures of everything but the allowed items, each with the name it is reported under. */
function parse(input: string): { sig: string; name: string }[] {
  let text = input.startsWith('﻿') ? input.slice(1) : input;
  const declaration = DECLARATION.exec(text);
  if (declaration) text = text.slice(declaration[0].length);
  const out: { sig: string; name: string }[] = [];
  const stack: { name: string; sig: string; plain: boolean }[] = [];
  let roots = 0;
  let at = 0;
  const content = (chunk: string) => {
    if (/[\u0000-\u0008\u000b\u000c\u000e-\u001f\u007f]/.test(chunk))
      throw new Error('Directory.Packages.props: control character');
    const value = chunk.trim().replace(/\s+/g, ' ');
    if (value === '') return;
    const top = stack[stack.length - 1];
    if (!top) throw new Error('Directory.Packages.props: text outside the root');
    out.push({ sig: `${stack.map((e) => e.sig).join('/')}#${value}`, name: top.name });
  };
  while (at < text.length) {
    const lt = text.indexOf('<', at);
    if (lt < 0) {
      content(text.slice(at));
      break;
    }
    content(text.slice(at, lt));
    COMMENT.lastIndex = lt;
    if (COMMENT.test(text)) {
      at = COMMENT.lastIndex;
      continue;
    }
    TAG.lastIndex = lt;
    const m = TAG.exec(text);
    if (!m) throw new Error('Directory.Packages.props: markup not understood');
    at = TAG.lastIndex;
    const [token, closing, name, attributes, selfClosing] = m as unknown as [
      string,
      string,
      string,
      string,
      string,
    ];
    if (closing) {
      if (attributes.trim() !== '' || selfClosing || stack.pop()?.name !== name)
        throw new Error('Directory.Packages.props: mismatched tag');
      continue;
    }
    const attrs = new Map<string, string>();
    for (const a of attributes.matchAll(ATTRIBUTE)) {
      if (attrs.has(a[1]!)) throw new Error('Directory.Packages.props: duplicate attribute');
      attrs.set(a[1]!, a[2]!);
    }
    if (stack.length === 0 && (name !== 'Project' || ++roots > 1))
      throw new Error('Directory.Packages.props: unexpected root');
    const sig = `${name}${[...attrs]
      .map(([k, v]) => ` ${k}=${v}`)
      .sort()
      .join('')}`;
    const parent = stack[stack.length - 1];
    const plainGroup =
      name === 'ItemGroup' &&
      parent?.name === 'Project' &&
      parent.sig === 'Project' &&
      [...attrs.keys()].every((k) => k === 'Label');
    const allowed =
      name === 'PackageVersion' &&
      selfClosing !== '' &&
      !token.includes('\n') &&
      parent?.plain === true &&
      attrs.size === 2 &&
      ID.test(attrs.get('Include') ?? '') &&
      VERSION.test(attrs.get('Version') ?? '');
    if (!allowed) out.push({ sig: `${stack.map((e) => e.sig).join('/')}/${sig}`, name });
    if (!selfClosing) stack.push({ name, sig, plain: plainGroup });
  }
  if (stack.length > 0 || roots !== 1)
    throw new Error('Directory.Packages.props: unclosed element');
  return out;
}

/** The names of the parts (other than the allowed items) in which two versions of the file differ, sorted; throws when unknown. */
export function directoryPackagesPropsKeys(before: string, after: string): string[] {
  const count = (list: { sig: string }[]) => {
    const map = new Map<string, number>();
    for (const { sig } of list) map.set(sig, (map.get(sig) ?? 0) + 1);
    return map;
  };
  const a = parse(before);
  const b = parse(after);
  const ca = count(a);
  const cb = count(b);
  const names = new Set<string>();
  for (const { sig, name } of [...a, ...b])
    if ((ca.get(sig) ?? 0) !== (cb.get(sig) ?? 0)) names.add(name);
  // MSBuild evaluates in document order (the last property wins, imports run in sequence): a reorder or a move of
  // anything but the allowed items is a difference too, named after the entries that changed place.
  if (names.size === 0 && a.length === b.length)
    for (let i = 0; i < a.length; i++)
      if (a[i]!.sig !== b[i]!.sig) {
        names.add(a[i]!.name);
        names.add(b[i]!.name);
      }
  return [...names].sort();
}
