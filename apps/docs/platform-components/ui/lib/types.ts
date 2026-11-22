import type { ColumnDef } from '@tanstack/react-table';
import type { ReactNode } from 'react';

export type Density = 'dense' | 'comfortable';

export type SidebarNavItem = {
  id: string;
  label: string;
  hint?: string;
  active?: boolean;
  onSelect?: () => void;
};

export type SidebarNavSection = {
  id: string;
  label: string;
  items: SidebarNavItem[];
};

export type DataTableColumn<TData extends object> = ColumnDef<TData, unknown>;

export type FilterOption = {
  id: string;
  label: string;
  active?: boolean;
  onSelect?: () => void;
};

export type ChartSeries = {
  key: string;
  label: string;
  color: string;
  data: number[];
};

export type ComposedChartSeriesKind = 'bar' | 'line';
export type ComposedChartAxis = 'left' | 'right';

/**
 * One series of a composed (bar + line, dual-axis) chart. ``null`` points are gaps: a
 * line breaks there instead of dropping to zero, a bar is simply absent.
 */
export type ComposedChartSeries = {
  key: string;
  label: string;
  color: string;
  data: Array<number | null>;
  kind: ComposedChartSeriesKind;
  /** Which y-axis the series is scaled against; ``left`` when omitted. */
  axis?: ComposedChartAxis;
  /** Bars sharing a stack id are stacked in one slot instead of drawn side by side. */
  stack?: string;
  /** Dashed outline (bar) or dashed stroke (line): the mark is an estimate, not an observation. */
  dashed?: boolean;
  /** Fill (bar) / stroke (line) opacity 0..1; opaque when omitted. */
  opacity?: number;
};

export type DetailDrawerProps = {
  open: boolean;
  onOpenChange: (open: boolean) => void;
  title: ReactNode;
  description?: ReactNode;
  children: ReactNode;
  actions?: ReactNode;
  closeLabel?: string;
  contentClassName?: string;
  embedded?: boolean;
  side?: 'left' | 'right';
};
