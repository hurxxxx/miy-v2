// AI-DO packages/ui/src/lib/data-display/data-table-model.spec.ts at 0002af062, moved from vitest to the
// Playwright runner. The cases and expectations are unchanged.
import { expect, test } from "@playwright/test";

import {
  getDataTableRowPadding,
  getDataTableSortIndicator,
  isDataTableRowSelected,
  shouldActivateDataTableRowKey,
} from "../../src/lib/data-display/data-table-model";

test.describe("data table model", () => {
  test("projects density into stable row padding classes", () => {
    expect(getDataTableRowPadding("dense")).toContain("px-3");
    expect(getDataTableRowPadding("comfortable")).toContain("px-4");
  });

  test("resolves selected rows with the configured row id accessor", () => {
    const row = { id: "row-1", label: "First" };

    expect(
      isDataTableRowSelected(row, {
        selectedRowId: "row-1",
        getRowId: (item) => item.id,
      }),
    ).toBe(true);
    expect(
      isDataTableRowSelected(row, {
        selectedRowId: "row-2",
        getRowId: (item) => item.id,
      }),
    ).toBe(false);
    expect(isDataTableRowSelected(row, { selectedRowId: "row-1" })).toBe(false);
  });

  test("projects sortable header indicators", () => {
    expect(getDataTableSortIndicator("asc")).toBe(" ↑");
    expect(getDataTableSortIndicator("desc")).toBe(" ↓");
    expect(getDataTableSortIndicator(false)).toBe("");
  });

  test("activates row keyboard actions only for enter and space", () => {
    expect(shouldActivateDataTableRowKey("Enter")).toBe(true);
    expect(shouldActivateDataTableRowKey(" ")).toBe(true);
    expect(shouldActivateDataTableRowKey("Escape")).toBe(false);
  });
});
