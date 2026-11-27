import { PageHeader } from "../../ui/page-header";
import { Term } from "../../ui/term";
import { ChangeDetectionStrategy, Component, computed, inject, signal } from "@angular/core";
import { answerOf, analysis, trendAnalysis } from "../../core/analysis";
import { type DeltaView, deltaView, sumDeltas, trendNote } from "../../core/delta";
import { FiltersService } from "../../core/filters.service";
import { WithoutService } from "../../core/without.service";
import { day, exactTokens, number, tokens } from "../../core/format";
import type { SkillStats, SkillsData } from "../../core/types";
import { HelpLink, termTitle } from "../../ui/help-link";
import { Legend } from "../../ui/legend";
import { type BarRow, StackedBars } from "../../ui/stacked-bars";
import { StatCards, trendStat } from "../../ui/stat-cards";
import { TrendCards } from "../../ui/trend-cards";
import { nullsLast } from "../../ui/sort";
import { Status } from "../../ui/status";
import {
  HxButton,
  HxCell,
  HxColumnHeader,
  HxDataTable,
  HxField,
  HxSwitch,
  type HxColumn,
} from "@gravionlabs/helix-ui";

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
  /** The change of the loads against the previous period; `null` without a comparison or a value. */
  loadChange: number | null;
  loadDelta: DeltaView | null;
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
    loadChange: null,
    loadDelta: null,
  };
}

@Component({
  selector: "app-skills",
  imports: [
    PageHeader,
    Term,
    Status,
    StatCards,
    TrendCards,
    HxButton,
    HxDataTable,
    HxCell,
    HxColumnHeader,
    HxField,
    HxSwitch,
    HelpLink,
    StackedBars,
    Legend,
  ],
  changeDetection: ChangeDetectionStrategy.OnPush,
  templateUrl: "./skills.html",
  styles: `
    h3 {
      margin: 20px 0 4px;
      font-size: var(--h-font-size-md);
    }
    .toggle {
      margin: var(--h-space-1) var(--h-space-4) var(--h-space-2) 0;
    }
    .what-if {
      white-space: nowrap;
    }
    .est {
      color: var(--h-tag-warn-color);
    }
    .change {
      font-variant-numeric: tabular-nums;
    }
    .change.good {
      color: var(--h-tag-success-color);
    }
    .change.bad {
      color: var(--h-tag-danger-color);
    }
    :host ::ng-deep tr.unused td {
      color: var(--h-text-muted-color);
    }
  `,
})
export class Skills {
  protected readonly res = analysis<SkillsData>("skills");
  protected readonly answer = computed(() => answerOf(this.res));
  protected readonly usedOnly = signal(false);
  protected readonly without = inject(WithoutService);

  private readonly all = computed(() => answerOf(this.res)?.data.report.skills ?? []);

  protected readonly rows = computed(() =>
    this.usedOnly() ? this.all().filter((s) => s.invocations > 0) : this.all(),
  );

  protected readonly legend = [
    { label: "baseline: name and description in every request", color: COLORS.baseline },
    { label: "load: skill content entering the context", color: COLORS.load },
    { label: "carry: loaded content re-sent in later requests", color: COLORS.carry },
  ];

  protected readonly stats = computed(() => {
    const data = answerOf(this.res)?.data;
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

  protected readonly compare = inject(FiltersService).compare;
  protected readonly trend = trendAnalysis();
  protected readonly note = computed(() => {
    const w = answerOf(this.trend)?.data.windows;
    return w ? trendNote(w) : "";
  });
  protected readonly trendStats = computed(() => {
    const t = answerOf(this.trend)?.data;
    if (!t) return [];
    const perDay = t.windows.normalized;
    return [
      trendStat(
        "Skill loads",
        sumDeltas(
          t.skills.map((s) => s.loads),
          t.windows,
        ),
        "count",
        {
          perDay,
        },
      ),
      trendStat(
        "Skill overhead (tokens)",
        sumDeltas(
          t.skills.map((s) => s.overhead_tokens),
          t.windows,
        ),
        "tokens",
        { good: "down", perDay },
      ),
    ];
  });

  protected readonly views = computed(() => {
    const t = this.compare() ? answerOf(this.trend)?.data : undefined;
    const byName = new Map(t?.skills.map((s) => [s.name, s]));
    return this.rows().map((s) => {
      const view = skillView(s);
      const loads = byName.get(s.name)?.loads;
      if (!loads) return view;
      return {
        ...view,
        loadChange: loads.change,
        loadDelta: deltaView(loads, "count", null, t!.windows.normalized),
      };
    });
  });
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
    if (this.compare()) {
      cols.push({
        field: "loadChange",
        header: "Change",
        title: "Change of the loads against the previous period",
        align: "right",
        sortable: true,
      });
    }
    cols.push({
      field: "action",
      header: "",
      title: "Add the skill to the what-if removal list of the cost view",
    });
    return cols;
  });
}
