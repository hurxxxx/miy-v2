import type { ChartSeries, ComposedChartSeries } from '../types';

export type CartesianChartData = Array<Record<string, string | number>>;
export type ComposedChartData = Array<Record<string, string | number | null>>;
export type DonutChartData = Array<{
  name: string;
  value: number;
  color: string;
}>;

export function toCartesianData(
  categories: readonly string[],
  series: readonly ChartSeries[],
): CartesianChartData {
  return categories.map((category, index) => {
    return series.reduce<Record<string, string | number>>(
      (row, item) => {
        row[item.key] = item.data[index] ?? 0;
        return row;
      },
      { category },
    );
  });
}

export function toDonutChartData(
  categories: readonly string[],
  series: readonly ChartSeries[],
): DonutChartData {
  return categories.map((category, index) => ({
    name: category,
    value: series[0]?.data[index] ?? 0,
    color: series[index]?.color ?? series[0]?.color ?? 'var(--ui-color-accent)',
  }));
}

/**
 * Like ``toCartesianData`` but keeps ``null`` (and missing) points as ``null`` so the
 * renderer can draw a gap instead of a false zero.
 */
export function toComposedChartData(
  categories: readonly string[],
  series: readonly ComposedChartSeries[],
): ComposedChartData {
  return categories.map((category, index) => {
    return series.reduce<Record<string, string | number | null>>(
      (row, item) => {
        row[item.key] = item.data[index] ?? null;
        return row;
      },
      { category },
    );
  });
}
