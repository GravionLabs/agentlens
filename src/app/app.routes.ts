import { Routes } from "@angular/router";

export interface NavItem {
  path: string;
  label: string;
}

export const NAV: NavItem[] = [
  { path: "overview", label: "Sources" },
  { path: "skills", label: "Skills" },
  { path: "advice", label: "Advice" },
  { path: "context", label: "Context" },
  { path: "unused", label: "Unused" },
  { path: "flow", label: "Flow" },
  { path: "cost", label: "Cost" },
  { path: "efficiency", label: "Efficiency" },
  { path: "compare", label: "Compare" },
  { path: "timeline", label: "Timeline" },
  { path: "glossary", label: "Glossary" },
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
    path: "advice",
    loadComponent: () => import("./features/advice/advice").then((m) => m.Advice),
  },
  {
    path: "context",
    loadComponent: () => import("./features/context/context").then((m) => m.Context),
  },
  {
    path: "unused",
    loadComponent: () => import("./features/unused/unused").then((m) => m.Unused),
  },
  {
    path: "flow",
    loadComponent: () => import("./features/flow/flow").then((m) => m.Flow),
  },
  {
    path: "cost",
    loadComponent: () => import("./features/cost/cost").then((m) => m.Cost),
  },
  {
    path: "efficiency",
    loadComponent: () => import("./features/efficiency/efficiency").then((m) => m.Efficiency),
  },
  {
    path: "compare",
    loadComponent: () => import("./features/compare/compare").then((m) => m.Compare),
  },
  {
    path: "timeline",
    loadComponent: () => import("./features/timeline/timeline").then((m) => m.Timeline),
  },
  {
    path: "glossary",
    loadComponent: () => import("./features/glossary/glossary").then((m) => m.Glossary),
  },
  { path: "**", redirectTo: "overview" },
];
