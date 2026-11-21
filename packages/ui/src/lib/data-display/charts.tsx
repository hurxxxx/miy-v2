import { lazy, Suspense, type CSSProperties, type ReactNode } from 'react';

import type { ChartSeries, ComposedChartSeries } from '../types';
import {
  toCartesianData,
  toComposedChartData,
  toDonutChartData,
  type CartesianChartData,
  type ComposedChartData,
  type DonutChartData,
} from './chart-data';

type ChartStatus = 'ready' | 'loading' | 'empty' | 'error';

const chartAxisTick = {
  fill: 'var(--ui-color-ink-muted)',
  fontSize: 12,
};

const chartTooltipContentStyle: CSSProperties = {
  backgroundColor: 'var(--ui-color-surface-raised)',
  border: '1px solid var(--ui-color-border)',
  borderRadius: 'var(--ui-radius-sm)',
  boxShadow: 'var(--ui-shadow-lg)',
  color: 'var(--ui-color-ink)',
};

const chartTooltipLabelStyle: CSSProperties = {
  color: 'var(--ui-color-ink)',
};

const chartTooltipItemStyle: CSSProperties = {
  color: 'var(--ui-color-ink-muted)',
};

export interface ChartFrameProps {
  title: ReactNode;
  description?: ReactNode;
  status?: ChartStatus;
  emptyState?: ReactNode;
  children?: ReactNode;
  /** Height class of the plot area; ``h-48`` when omitted (the shared card height). */
  bodyClassName?: string;
}

export type ComposedChartAxisFormatter = (value: number) => string;

export interface ComposedChartAxisOptions {
  /** Tick label formatter (e.g. percentages on the right axis). */
  tickFormatter?: ComposedChartAxisFormatter;
  /** Axis gutter width in px; wide enough for ``tickFormatter`` output. Default 40. */
  width?: number;
}

const LineChartRenderer = lazy(async () => {
  const { Line, LineChart, ResponsiveContainer, Tooltip, XAxis, YAxis } =
    await import('recharts');

  return {
    default: function LineChartRenderer({
      data,
      series,
    }: {
      data: CartesianChartData;
      series: ChartSeries[];
    }) {
      return (
        <ResponsiveContainer
          width="100%"
          height="100%"
          initialDimension={{ width: 640, height: 192 }}
          minWidth={0}
          minHeight={192}
        >
          <LineChart data={data}>
            <XAxis
              axisLine={false}
              dataKey="category"
              tick={chartAxisTick}
              tickLine={false}
              tickMargin={8}
            />
            <YAxis
              axisLine={false}
              tick={chartAxisTick}
              tickLine={false}
              tickMargin={8}
              width={32}
            />
            <Tooltip
              contentStyle={chartTooltipContentStyle}
              itemStyle={chartTooltipItemStyle}
              labelStyle={chartTooltipLabelStyle}
            />
            {series.map((item) => (
              <Line
                key={item.key}
                type="monotone"
                dataKey={item.key}
                name={item.label}
                stroke={item.color}
                strokeWidth={2}
                dot={false}
              />
            ))}
          </LineChart>
        </ResponsiveContainer>
      );
    },
  };
});

const BarChartRenderer = lazy(async () => {
  const { Bar, BarChart, ResponsiveContainer, Tooltip, XAxis, YAxis } =
    await import('recharts');

  return {
    default: function BarChartRenderer({
      data,
      series,
    }: {
      data: CartesianChartData;
      series: ChartSeries[];
    }) {
      return (
        <ResponsiveContainer
          width="100%"
          height="100%"
          initialDimension={{ width: 640, height: 192 }}
          minWidth={0}
          minHeight={192}
        >
          <BarChart data={data}>
            <XAxis
              axisLine={false}
              dataKey="category"
              tick={chartAxisTick}
              tickLine={false}
              tickMargin={8}
            />
            <YAxis
              axisLine={false}
              tick={chartAxisTick}
              tickLine={false}
              tickMargin={8}
              width={32}
            />
            <Tooltip
              contentStyle={chartTooltipContentStyle}
              itemStyle={chartTooltipItemStyle}
              labelStyle={chartTooltipLabelStyle}
            />
            {series.map((item) => (
              <Bar
                key={item.key}
                dataKey={item.key}
                name={item.label}
                fill={item.color}
                radius={[4, 4, 0, 0]}
              />
            ))}
          </BarChart>
        </ResponsiveContainer>
      );
    },
  };
});

const ComposedChartRenderer = lazy(async () => {
  const {
    Bar,
    ComposedChart,
    Legend,
    Line,
    ResponsiveContainer,
    Tooltip,
    XAxis,
    YAxis,
  } = await import('recharts');

  return {
    default: function ComposedChartRenderer({
      data,
      leftAxis,
      rightAxis,
      series,
      showLegend,
      legendPosition,
      valueFormatter,
    }: {
      data: ComposedChartData;
      leftAxis?: ComposedChartAxisOptions;
      rightAxis?: ComposedChartAxisOptions;
      series: ComposedChartSeries[];
      showLegend: boolean;
      legendPosition: 'top' | 'bottom';
      valueFormatter?: (value: number | null, seriesKey: string) => string;
    }) {
      const hasRightAxis = series.some((item) => item.axis === 'right');
      const formatValue = (value: unknown, seriesKey: string) => {
        const numeric = typeof value === 'number' ? value : null;
        if (valueFormatter) return valueFormatter(numeric, seriesKey);
        return numeric === null ? '' : String(numeric);
      };
      return (
        <ResponsiveContainer
          width="100%"
          height="100%"
          initialDimension={{ width: 640, height: 192 }}
          minWidth={0}
          minHeight={192}
        >
          <ComposedChart data={data}>
            <XAxis
              axisLine={false}
              dataKey="category"
              interval="preserveStartEnd"
              minTickGap={12}
              tick={chartAxisTick}
              tickLine={false}
              tickMargin={8}
            />
            <YAxis
              axisLine={false}
              tick={chartAxisTick}
              tickFormatter={leftAxis?.tickFormatter}
              tickLine={false}
              tickMargin={8}
              width={leftAxis?.width ?? 40}
              yAxisId="left"
            />
            {hasRightAxis ? (
              <YAxis
                axisLine={false}
                orientation="right"
                tick={chartAxisTick}
                tickFormatter={rightAxis?.tickFormatter}
                tickLine={false}
                tickMargin={8}
                width={rightAxis?.width ?? 40}
                yAxisId="right"
              />
            ) : null}
            <Tooltip
              contentStyle={chartTooltipContentStyle}
              formatter={(value, _name, entry) =>
                formatValue(value, String(entry?.dataKey ?? ''))
              }
              itemStyle={chartTooltipItemStyle}
              labelStyle={chartTooltipLabelStyle}
            />
            {showLegend ? (
              <Legend
                iconSize={10}
                verticalAlign={legendPosition}
                wrapperStyle={{
                  color: 'var(--ui-color-ink-muted)',
                  fontSize: 12,
                  ...(legendPosition === 'top' ? { paddingBottom: 8 } : {}),
                }}
              />
            ) : null}
            {series.map((item) =>
              item.kind === 'bar' ? (
                <Bar
                  key={item.key}
                  dataKey={item.key}
                  fill={item.color}
                  fillOpacity={item.opacity}
                  name={item.label}
                  // Stacked segments meet flush; a rounded top would show under the next one.
                  radius={item.stack ? 0 : [4, 4, 0, 0]}
                  stackId={item.stack}
                  stroke={item.dashed ? item.color : undefined}
                  strokeDasharray={item.dashed ? '4 2' : undefined}
                  yAxisId={item.axis ?? 'left'}
                />
              ) : (
                <Line
                  key={item.key}
                  connectNulls={false}
                  dataKey={item.key}
                  dot={{ r: 2.5, strokeWidth: 0, fill: item.color }}
                  name={item.label}
                  stroke={item.color}
                  strokeDasharray={item.dashed ? '6 3' : undefined}
                  strokeOpacity={item.opacity}
                  strokeWidth={2}
                  type="monotone"
                  yAxisId={item.axis ?? 'left'}
                />
              ),
            )}
          </ComposedChart>
        </ResponsiveContainer>
      );
    },
  };
});

const DonutChartRenderer = lazy(async () => {
  const { Cell, Pie, PieChart, ResponsiveContainer, Tooltip } = await import(
    'recharts'
  );

  return {
    default: function DonutChartRenderer({ data }: { data: DonutChartData }) {
      return (
        <ResponsiveContainer
          width="100%"
          height="100%"
          initialDimension={{ width: 320, height: 192 }}
          minWidth={0}
          minHeight={192}
        >
          <PieChart>
            <Tooltip
              contentStyle={chartTooltipContentStyle}
              itemStyle={chartTooltipItemStyle}
              labelStyle={chartTooltipLabelStyle}
            />
            <Pie
              data={data}
              dataKey="value"
              nameKey="name"
              innerRadius={52}
              outerRadius={74}
            >
              {data.map((item) => (
                <Cell key={item.name} fill={item.color} />
              ))}
            </Pie>
          </PieChart>
        </ResponsiveContainer>
      );
    },
  };
});

export function ChartFrame({
  title,
  description,
  status = 'ready',
  emptyState,
  children,
  bodyClassName = 'h-48',
}: ChartFrameProps) {
  const body =
    status === 'empty' || status === 'error' ? (emptyState ?? null) : children;

  return (
    <section className="rounded-[var(--ui-radius-lg)] border border-[var(--ui-color-border)] bg-ui-surface p-5 shadow-[var(--ui-shadow-sm)]">
      <div className="mb-4 grid gap-1">
        <h3 className="m-0 text-[length:var(--ui-text-h3)] font-semibold text-[var(--ui-color-ink)]">
          {title}
        </h3>
        {description ? (
          <p className="m-0 text-[length:var(--ui-text-body)] text-[var(--ui-color-ink-muted)]">
            {description}
          </p>
        ) : null}
      </div>
      <div className={bodyClassName}>{body}</div>
    </section>
  );
}

export interface LineChartCardProps {
  title: ReactNode;
  categories: string[];
  series: ChartSeries[];
  status?: ChartStatus;
  emptyState?: ReactNode;
}

export function LineChartCard({
  title,
  categories,
  series,
  status = 'ready',
  emptyState,
}: LineChartCardProps) {
  const data = toCartesianData(categories, series);

  return (
    <ChartFrame title={title} status={status} emptyState={emptyState}>
      <Suspense fallback={null}>
        <LineChartRenderer data={data} series={series} />
      </Suspense>
    </ChartFrame>
  );
}

export type BarChartCardProps = LineChartCardProps;

export function BarChartCard({
  title,
  categories,
  series,
  status = 'ready',
  emptyState,
}: BarChartCardProps) {
  const data = toCartesianData(categories, series);

  return (
    <ChartFrame title={title} status={status} emptyState={emptyState}>
      <Suspense fallback={null}>
        <BarChartRenderer data={data} series={series} />
      </Suspense>
    </ChartFrame>
  );
}

export interface ComposedChartCardProps {
  title: ReactNode;
  description?: ReactNode;
  categories: string[];
  series: ComposedChartSeries[];
  status?: ChartStatus;
  emptyState?: ReactNode;
  leftAxis?: ComposedChartAxisOptions;
  /** Read when any series declares ``axis: 'right'``; the axis is not drawn otherwise. */
  rightAxis?: ComposedChartAxisOptions;
  /** Tooltip value text per series; ``null`` marks a gap (no data for that category). */
  valueFormatter?: (value: number | null, seriesKey: string) => string;
  /** Legend on by default: a mixed bar/line chart is not readable without one. */
  showLegend?: boolean;
  /** Where the legend sits; ``bottom`` (default) or ``top`` above the plot. */
  legendPosition?: 'top' | 'bottom';
  /** Plot height class; defaults to a taller ``h-64`` so the legend does not crowd the plot. */
  bodyClassName?: string;
}

/**
 * Bars and lines in one plot with an optional right-hand y-axis, for series of different
 * scale (counts against a percentage). Null points render as gaps, never as zero.
 */
export function ComposedChartCard({
  title,
  description,
  categories,
  series,
  status = 'ready',
  emptyState,
  leftAxis,
  rightAxis,
  valueFormatter,
  showLegend = true,
  legendPosition = 'bottom',
  bodyClassName = 'h-64',
}: ComposedChartCardProps) {
  const data = toComposedChartData(categories, series);

  return (
    <ChartFrame
      bodyClassName={bodyClassName}
      description={description}
      emptyState={emptyState}
      status={status}
      title={title}
    >
      <Suspense fallback={null}>
        <ComposedChartRenderer
          data={data}
          leftAxis={leftAxis}
          rightAxis={rightAxis}
          series={series}
          showLegend={showLegend}
          legendPosition={legendPosition}
          valueFormatter={valueFormatter}
        />
      </Suspense>
    </ChartFrame>
  );
}

export interface DonutChartCardProps {
  title: ReactNode;
  categories: string[];
  series: ChartSeries[];
  status?: ChartStatus;
  emptyState?: ReactNode;
}

export function DonutChartCard({
  title,
  categories,
  series,
  status = 'ready',
  emptyState,
}: DonutChartCardProps) {
  const data = toDonutChartData(categories, series);

  return (
    <ChartFrame title={title} status={status} emptyState={emptyState}>
      <Suspense fallback={null}>
        <DonutChartRenderer data={data} />
      </Suspense>
    </ChartFrame>
  );
}
