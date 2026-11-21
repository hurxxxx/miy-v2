// AI-DO packages/ui/src/lib/data-display/chart-data.spec.ts at 0002af062, moved from vitest to the
// Playwright runner. The cases and expectations are unchanged.
import { expect, test } from "@playwright/test";

import type { ChartSeries, ComposedChartSeries } from "../../src/lib/types";
import {
  toCartesianData,
  toComposedChartData,
  toDonutChartData,
} from "../../src/lib/data-display/chart-data";

const series: ChartSeries[] = [
  {
    key: "planned",
    label: "Planned",
    color: "#2563eb",
    data: [4, 6],
  },
  {
    key: "done",
    label: "Done",
    color: "#16a34a",
    data: [2],
  },
];

test.describe("chart data", () => {
  test("projects category and series values into cartesian rows", () => {
    expect(toCartesianData(["Mon", "Tue"], series)).toEqual([
      {
        category: "Mon",
        planned: 4,
        done: 2,
      },
      {
        category: "Tue",
        planned: 6,
        done: 0,
      },
    ]);
  });

  test("projects donut rows from the first series and category colors", () => {
    expect(toDonutChartData(["Open", "Closed", "Blocked"], series)).toEqual([
      {
        name: "Open",
        value: 4,
        color: "#2563eb",
      },
      {
        name: "Closed",
        value: 6,
        color: "#16a34a",
      },
      {
        name: "Blocked",
        value: 0,
        color: "#2563eb",
      },
    ]);
  });

  test("uses the accent token when donut data has no series", () => {
    expect(toDonutChartData(["Empty"], [])).toEqual([
      {
        name: "Empty",
        value: 0,
        color: "var(--ui-color-accent)",
      },
    ]);
  });

  test("keeps null and missing composed points as gaps instead of zero", () => {
    const composed: ComposedChartSeries[] = [
      {
        key: "count",
        label: "Count",
        color: "#000",
        data: [4, 0, 2],
        kind: "bar",
      },
      {
        key: "rate",
        label: "Rate",
        color: "#111",
        data: [0.5, null],
        kind: "line",
        axis: "right",
      },
    ];
    expect(toComposedChartData(["a", "b", "c"], composed)).toEqual([
      { category: "a", count: 4, rate: 0.5 },
      { category: "b", count: 0, rate: null },
      { category: "c", count: 2, rate: null },
    ]);
  });
});
