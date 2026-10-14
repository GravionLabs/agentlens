import { Routes } from "@angular/router";
import { ViewFrame } from "./ui/view-frame";

export type NavSection = "Data" | "Optimize" | "Analyze";

export const NAV_SECTIONS: readonly NavSection[] = ["Data", "Optimize", "Analyze"];

export interface NavItem {
  path: string;
  label: string;
  /** Section of the side navigation; omitted for the final, unlabeled group. */
  group?: NavSection;
}

export const NAV: NavItem[] = [
  { path: "overview", label: "Sources", group: "Data" },
  { path: "timeline", label: "Timeline", group: "Data" },
  { path: "flow", label: "Flow", group: "Data" },
  { path: "skills", label: "Skills", group: "Optimize" },
  { path: "unused", label: "Unused", group: "Optimize" },
  { path: "context", label: "Context", group: "Optimize" },
  { path: "advice", label: "Advice", group: "Optimize" },
  { path: "cost", label: "Cost", group: "Analyze" },
  { path: "efficiency", label: "Efficiency", group: "Analyze" },
  { path: "compare", label: "Compare", group: "Analyze" },
  { path: "glossary", label: "Glossary" },
];

const views: Routes = [
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

/** Every view sits in the frame with the filter toolbar; the shell (topbar, nav, status bar) is in `App`. */
export const routes: Routes = [{ path: "", component: ViewFrame, children: views }];
