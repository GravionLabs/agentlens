import { ChangeDetectionStrategy, Component, computed, signal } from "@angular/core";
import { analysis } from "../../core/analysis";
import { day, number, tokens } from "../../core/format";
import type { SkillStats, SkillsData } from "../../core/types";
import { type Column, DataTable } from "../../ui/data-table";
import { Legend } from "../../ui/legend";
import { type BarRow, StackedBars } from "../../ui/stacked-bars";
import { StatCards } from "../../ui/stat-cards";
import { Status } from "../../ui/status";

const COLORS = { baseline: "var(--c-baseline)", load: "var(--c-load)", carry: "var(--c-carry)" };

export function overhead(s: SkillStats): number {
  return s.baseline.tokens.count + s.load.count + s.carry.tokens;
}

function attributedTokens(s: SkillStats): number {
  const a = s.attributed;
  return a.input + a.output + a.cache_read + a.cache_write;
}

@Component({
  selector: "app-skills",
  imports: [Status, StatCards, DataTable, StackedBars, Legend],
  changeDetection: ChangeDetectionStrategy.OnPush,
  template: `
    <section class="view">
      <h2>Skills</h2>
      <p class="muted">
        Which skills are used and what they cost. <span class="est">~</span> marks estimated values;
        unmarked values were measured by the client. Overhead = baseline + load + carry.
      </p>
      <app-status
        [loading]="res.isLoading()"
        [error]="res.error()"
        [warnings]="res.value()?.warnings ?? 0"
        [errors]="res.value()?.errors ?? []"
      />
      <app-stat-cards [stats]="stats()" />

      <h3>Cost split</h3>
      <p class="muted">
        Top 15 skills by overhead. Hatched segments are estimates, solid ones were measured.
      </p>
      <app-legend [items]="legend" />
      <app-stacked-bars
        [rows]="bars()"
        label="Skill overhead split into baseline, load and carry"
      />

      <h3>All skills</h3>
      <label class="check">
        <input type="checkbox" [checked]="usedOnly()" (change)="usedOnly.set(!usedOnly())" />
        Used skills only
      </label>
      <app-data-table
        [columns]="columns()"
        [data]="rows()"
        [initialSort]="{ key: 'overhead', descending: true }"
        [filterable]="true"
        placeholder="Filter skills"
        [dim]="unused"
        empty="No skills match."
      />
    </section>
  `,
  styles: `
    h3 {
      margin: 20px 0 4px;
      font-size: 1.05rem;
    }
    .check {
      display: inline-flex;
      gap: 6px;
      margin: 4px 16px 8px 0;
      color: var(--muted);
      align-items: center;
    }
    .est {
      color: var(--warn);
    }
  `,
})
export class Skills {
  protected readonly res = analysis<SkillsData>("skills");
  protected readonly usedOnly = signal(false);
  protected readonly unused = (s: SkillStats) => s.invocations === 0;

  private readonly all = computed(() => this.res.value()?.data.report.skills ?? []);

  protected readonly rows = computed(() =>
    this.usedOnly() ? this.all().filter((s) => s.invocations > 0) : this.all(),
  );

  protected readonly legend = [
    { label: "baseline: name and description in every request", color: COLORS.baseline },
    { label: "load: skill content entering the context", color: COLORS.load },
    { label: "carry: loaded content re-sent in later requests", color: COLORS.carry },
  ];

  protected readonly stats = computed(() => {
    const data = this.res.value()?.data;
    const skills = this.all();
    const used = skills.filter((s) => s.invocations > 0);
    const idleBaseline = skills
      .filter((s) => s.invocations === 0)
      .reduce((a, s) => a + s.baseline.tokens.count, 0);
    return [
      {
        value: `${used.length} / ${data?.installed_skills ?? 0}`,
        label: "skills used / installed",
      },
      { value: number(used.reduce((a, s) => a + s.invocations, 0)), label: "invocations" },
      {
        value: tokens(
          skills.reduce((a, s) => a + overhead(s), 0),
          true,
        ),
        label: "total skill overhead (tokens)",
      },
      { value: tokens(idleBaseline, true), label: "baseline of never-used skills" },
    ];
  });

  protected readonly bars = computed<BarRow[]>(() =>
    [...this.all()]
      .filter((s) => overhead(s) > 0)
      .sort((a, b) => overhead(b) - overhead(a))
      .slice(0, 15)
      .map((s) => ({
        label: s.name,
        total: tokens(overhead(s), true),
        segments: [
          {
            label: "baseline",
            value: s.baseline.tokens.count,
            color: COLORS.baseline,
            estimated: !s.baseline.tokens.measured,
          },
          { label: "load", value: s.load.count, color: COLORS.load, estimated: !s.load.measured },
          { label: "carry", value: s.carry.tokens, color: COLORS.carry, estimated: true },
        ],
      })),
  );

  protected readonly columns = computed<Column<SkillStats>[]>(() => {
    const cols: Column<SkillStats>[] = [
      { key: "name", label: "Skill", value: (s) => s.name },
      { key: "uses", label: "Uses", value: (s) => s.invocations, numeric: true },
      { key: "sessions", label: "Sessions", value: (s) => s.sessions, numeric: true },
      {
        key: "load",
        label: "Load",
        value: (s) => s.load.count,
        text: (s) => tokens(s.load.count, !s.load.measured && s.load.count > 0),
        estimated: (s) => !s.load.measured && s.load.count > 0,
        numeric: true,
        title: "Tokens the skill content added to the context when loaded",
      },
      {
        key: "carry",
        label: "Carry",
        value: (s) => s.carry.tokens,
        text: (s) => tokens(s.carry.tokens, s.carry.tokens > 0),
        estimated: (s) => s.carry.tokens > 0,
        numeric: true,
        title: "Loaded content sent again with every later request until a compaction",
      },
      {
        key: "baseline",
        label: "Baseline",
        value: (s) => s.baseline.tokens.count,
        text: (s) =>
          tokens(
            s.baseline.tokens.count,
            !s.baseline.tokens.measured && s.baseline.tokens.count > 0,
          ),
        estimated: (s) => !s.baseline.tokens.measured && s.baseline.tokens.count > 0,
        numeric: true,
        title: "Name and description, sent with every request of the clients that list the skill",
      },
      {
        key: "overhead",
        label: "Overhead",
        value: (s) => overhead(s),
        text: (s) => tokens(overhead(s), overhead(s) > 0),
        estimated: (s) => overhead(s) > 0,
        numeric: true,
      },
    ];
    if (this.all().some((s) => s.attributed.requests > 0)) {
      cols.push({
        key: "attributed",
        label: "Attributed",
        value: (s) => (s.attributed.requests > 0 ? attributedTokens(s) : null),
        text: (s) => (s.attributed.requests > 0 ? tokens(attributedTokens(s)) : "-"),
        numeric: true,
        title: "Tokens of requests the client attributes to the skill (OpenTelemetry); measured",
      });
    }
    cols.push(
      {
        key: "follow",
        label: "Follow-on",
        value: (s) =>
          s.follow_on.input + s.follow_on.output + s.follow_on.cache_read + s.follow_on.cache_write,
        text: (s) =>
          number(
            s.follow_on.input +
              s.follow_on.output +
              s.follow_on.cache_read +
              s.follow_on.cache_write,
          ),
        numeric: true,
        title:
          "Tokens used until the next user turn, skill or compaction (attributed by assumption)",
      },
      { key: "last", label: "Last used", value: (s) => s.last_used, text: (s) => day(s.last_used) },
    );
    return cols;
  });
}
