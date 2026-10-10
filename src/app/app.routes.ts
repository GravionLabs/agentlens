import { Routes } from "@angular/router";

const placeholder = (title: string) => ({
  loadComponent: () => import("./features/placeholder/placeholder").then((m) => m.Placeholder),
  data: { title },
});

export interface NavItem {
  path: string;
  label: string;
}

export const NAV: NavItem[] = [
  { path: "overview", label: "Sources" },
  { path: "skills", label: "Skills" },
  { path: "context", label: "Context" },
  { path: "unused", label: "Unused" },
  { path: "flow", label: "Flow" },
  { path: "cost", label: "Cost" },
  { path: "efficiency", label: "Efficiency" },
  { path: "compare", label: "Compare" },
  { path: "timeline", label: "Timeline" },
];

export const routes: Routes = [
  { path: "", pathMatch: "full", redirectTo: "overview" },
  {
    path: "overview",
    loadComponent: () => import("./features/overview/overview").then((m) => m.Overview),
  },
  {
    path: "skills",
    loadComponent: () => import("./features/skills/skills").then((m) => m.Skills),
  },
  {
    path: "context",
    loadComponent: () => import("./features/context/context").then((m) => m.Context),
  },
  {
    path: "unused",
    loadComponent: () => import("./features/unused/unused").then((m) => m.Unused),
  },
  ...NAV.filter((n) => !["overview", "skills", "context", "unused"].includes(n.path)).map((n) => ({
    path: n.path,
    ...placeholder(n.label),
  })),
  { path: "**", redirectTo: "overview" },
];
