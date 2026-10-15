import { PageHeader } from "../../ui/page-header";
import { Term } from "../../ui/term";
import { ChangeDetectionStrategy, Component, computed, signal } from "@angular/core";
import { analysis } from "../../core/analysis";
import { day, exactTokens, number, tokens } from "../../core/format";
import type { SkillStats, SkillsData } from "../../core/types";
import { HelpLink, termTitle } from "../../ui/help-link";
import { Legend } from "../../ui/legend";
import { type BarRow, StackedBars } from "../../ui/stacked-bars";
import { StatCards } from "../../ui/stat-cards";
import { nullsLast } from "../../ui/sort";
import { Status } from "../../ui/status";
import { HxCell, HxColumnHeader, HxDataTable, type HxColumn } from "@gravionlabs/helix-ui";

const COLORS = { baseline: "var(--c-baseline)", load: "var(--c-load)", carry: "var(--c-carry)" };

export function overhead(s: SkillStats): number {
  return s.baseline.tokens.count + s.load.count + s.carry.tokens.count;
}

function attributedTokens(s: SkillStats): number {
  const a = s.attributed;
  return a.input + a.output + a.cache_read + a.cache_write;
}

/** A skill row with the sortable raw values; `Est` marks values shown with a tilde. */
export interface SkillView {
  name: string;
  invocations: number;
  sessions: number;
  file_uses: number;
  load: number;
  loadEst: boolean;
  carry: number;
  carryEst: boolean;
  baseline: number;
  baselineEst: boolean;
  overhead: number;
  overheadEst: boolean;
  /** `null` when the client attributes no requests to the skill. */
  attributed: number | null;
  follow_on: number;
  last_used: string | null;
}

export function skillView(s: SkillStats): SkillView {
  const f = s.follow_on;
  return {
    name: s.name,
    invocations: s.invocations,
    sessions: s.sessions,
    file_uses: s.file_uses,
    load: s.load.count,
    loadEst: !s.load.measured && s.load.count > 0,
    carry: s.carry.tokens.count,
    carryEst: !s.carry.tokens.measured && s.carry.tokens.count > 0,
    baseline: s.baseline.tokens.count,
    baselineEst: !s.baseline.tokens.measured && s.baseline.tokens.count > 0,
    overhead: overhead(s),
    overheadEst: overhead(s) > 0,
    attributed: s.attributed.requests > 0 ? attributedTokens(s) : null,
    follow_on: f.input + f.output + f.cache_read + f.cache_write,
    last_used: s.last_used,
  };
}

@Component({
  selector: "app-skills",
  imports: [
    PageHeader,
    Term,
    Status,
    StatCards,
    HxDataTable,
    HxCell,
    HxColumnHeader,
    HelpLink,
    StackedBars,
    Legend,
  ],
  changeDetection: ChangeDetectionStrategy.OnPush,
  template: `
    <section class="view">
      <app-page-header
        exportView="skills"
        [exportFormats]="['json', 'html', 'csv']"
        title="Skills"
        summary="Which skills are used and what they cost in tokens."
      >
        <p>
          <span class="est">~</span> marks <app-term id="measured">estimates</app-term>; unmarked
          values were measured by the client.
        </p>
        <p>
          <app-term id="overhead">Overhead</app-term> =
          <app-term id="baseline">baseline</app-term> + <app-term id="load">load</app-term> +
          <app-term id="carry">carry</app-term>.
        </p>
      </app-page-header>
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
      <hx-data-table
        [value]="views()"
        [sortFunction]="sort"
        [columns]="columns()"
        filterDisplay="row"
        sortField="overhead"
        [sortOrder]="-1"
        [rowClass]="rowClass"
        [paginator]="views().length > 100"
        [rows]="100"
        [rowsPerPageOptions]="[100, 'all']"
        allLabel="Show all"
        emptyMessage="No skills match."
        ariaLabel="All skills"
      >
        <ng-template hxColumnHeader="file_uses"
          ><app-help-link term="file-use" label="File uses"
        /></ng-template>
        <ng-template hxColumnHeader="load"><app-help-link term="load" label="Load" /></ng-template>
        <ng-template hxColumnHeader="carry"
          ><app-help-link term="carry" label="Carry"
        /></ng-template>
        <ng-template hxColumnHeader="baseline"
          ><app-help-link term="baseline" label="Baseline"
        /></ng-template>
        <ng-template hxColumnHeader="overhead"
          ><app-help-link term="overhead" label="Overhead"
        /></ng-template>
        <ng-template hxColumnHeader="attributed"
          ><app-help-link term="attributed" label="Attributed"
        /></ng-template>
        <ng-template hxColumnHeader="follow_on"
          ><app-help-link term="follow-on" label="Follow-on"
        /></ng-template>
        <ng-template hxCell="load" let-row>
          <span [class.est]="row.loadEst" [attr.title]="tip(row.load)">{{
            tokens(row.load, row.loadEst)
          }}</span>
        </ng-template>
        <ng-template hxCell="carry" let-row>
          <span [class.est]="row.carryEst" [attr.title]="tip(row.carry)">{{
            tokens(row.carry, row.carryEst)
          }}</span>
        </ng-template>
        <ng-template hxCell="baseline" let-row>
          <span [class.est]="row.baselineEst" [attr.title]="tip(row.baseline)">{{
            tokens(row.baseline, row.baselineEst)
          }}</span>
        </ng-template>
        <ng-template hxCell="overhead" let-row>
          <span [class.est]="row.overheadEst" [attr.title]="tip(row.overhead)">{{
            tokens(row.overhead, row.overheadEst)
          }}</span>
        </ng-template>
        <ng-template hxCell="attributed" let-row>
          <span [attr.title]="tip(row.attributed)">{{
            row.attributed === null ? "-" : tokens(row.attributed)
          }}</span>
        </ng-template>
        <ng-template hxCell="follow_on" let-row>
          <span [attr.title]="tip(row.follow_on)">{{ tokens(row.follow_on) }}</span>
        </ng-template>
        <ng-template hxCell="last_used" let-value="value">{{ day(value) }}</ng-template>
      </hx-data-table>
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
    :host ::ng-deep tr.unused td {
      color: var(--muted);
    }
  `,
})
export class Skills {
  protected readonly res = analysis<SkillsData>("skills");
  protected readonly usedOnly = signal(false);

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
    const totalOverhead = skills.reduce((a, s) => a + overhead(s), 0);
    return [
      {
        value: `${used.length} / ${data?.installed_skills ?? 0}`,
        label: "skills used / installed",
      },
      { value: number(used.reduce((a, s) => a + s.invocations, 0)), label: "invocations" },
      {
        value: tokens(totalOverhead, true),
        title: exactTokens(totalOverhead) ?? undefined,
        label: "total skill overhead (tokens)",
      },
      {
        value: tokens(idleBaseline, true),
        title: exactTokens(idleBaseline) ?? undefined,
        label: "baseline of never-used skills",
      },
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
          {
            label: "carry",
            value: s.carry.tokens.count,
            color: COLORS.carry,
            estimated: !s.carry.tokens.measured,
          },
        ],
      })),
  );

  protected readonly views = computed(() => this.rows().map(skillView));
  protected readonly rowClass = (v: SkillView) => (v.invocations === 0 ? "unused" : "");
  protected readonly sort = nullsLast;
  protected readonly tokens = tokens;
  protected readonly tip = exactTokens;
  protected readonly day = day;

  protected readonly columns = computed<HxColumn[]>(() => {
    const cols: HxColumn[] = [
      { field: "name", header: "Skill", sortable: true, filter: true },
      { field: "invocations", header: "Uses", align: "right", sortable: true },
      { field: "sessions", header: "Sessions", align: "right", sortable: true },
      {
        field: "file_uses",
        header: "File uses",
        title: "Tool calls that read or ran files in the skill's folder without loading it",
        align: "right",
        sortable: true,
      },
      {
        field: "load",
        header: "Load",
        title: "Tokens the skill content added to the context when loaded",
        align: "right",
        sortable: true,
      },
      {
        field: "carry",
        header: "Carry",
        title: "Loaded content sent again with every later request until a compaction",
        align: "right",
        sortable: true,
      },
      {
        field: "baseline",
        header: "Baseline",
        title: "Name and description, sent with every request of the clients that list the skill",
        align: "right",
        sortable: true,
      },
      {
        field: "overhead",
        header: "Overhead",
        title: termTitle("overhead"),
        align: "right",
        sortable: true,
      },
    ];
    if (this.all().some((s) => s.attributed.requests > 0)) {
      cols.push({
        field: "attributed",
        header: "Attributed",
        title: "Tokens of requests the client attributes to the skill (OpenTelemetry); measured",
        align: "right",
        sortable: true,
      });
    }
    cols.push(
      {
        field: "follow_on",
        header: "Follow-on",
        title:
          "Tokens used until the next user turn, skill or compaction (attributed by assumption)",
        align: "right",
        sortable: true,
      },
      { field: "last_used", header: "Last used", sortable: true },
    );
    return cols;
  });
}
