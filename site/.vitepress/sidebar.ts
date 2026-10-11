import { posix } from "node:path";

export interface SidebarItem {
  text: string;
  link: string;
}

/**
 * The pages of the guide in the order of the numbered list of `docs/guide/README.md`, so that a new
 * page is added in one place: `1. [Getting started](getting-started.md): …`. A link may point to a
 * document next to the guide (`../telemetry.md`).
 */
export function guidePages(readme: string): SidebarItem[] {
  const items: SidebarItem[] = [];
  for (const line of readme.split("\n")) {
    const found = /^\d+\.\s+\[([^\]]+)\]\(([^)#\s]+)\.md(?:#[^)\s]*)?\)/.exec(line);
    if (found) items.push({ text: found[1], link: posix.join("/guide", found[2]) });
  }
  return items;
}

/** The sidebar of the site: the numbered pages of the guide, behind an overview. */
export function sidebarOf(readme: string) {
  return [
    {
      text: "Guide",
      items: [{ text: "Overview", link: "/guide/" }, ...guidePages(readme)],
    },
  ];
}
