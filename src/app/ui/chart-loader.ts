import type { HxChartModule } from "@gravionlabs/helix-ui/chart";

/**
 * Loads chart.js for helix chart (HX_CHART_LOADER) and registers only the parts bar and line charts need, so
 * the bundler can drop the other chart types, scales and plugins that `chart.js/auto` would pull in.
 * TimeScale is left out on purpose: it needs a date adapter, and the views use category axes.
 */
export async function loadChartJs(): Promise<HxChartModule> {
  const chartjs = await import("chart.js");
  chartjs.Chart.register(
    chartjs.BarController,
    chartjs.BarElement,
    chartjs.LineController,
    chartjs.LineElement,
    chartjs.PointElement,
    chartjs.LinearScale,
    chartjs.CategoryScale,
    chartjs.Filler,
    chartjs.Tooltip,
    chartjs.Legend,
  );
  return { default: chartjs.Chart } as unknown as HxChartModule;
}
