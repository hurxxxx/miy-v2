import type { LucideIcon } from 'lucide-react';
import {
  AlertCircle,
  ChevronRight,
  ClipboardList,
  User,
  Users,
} from 'lucide-react';
import { useTranslation } from 'react-i18next';
import { cn } from '../../../lib/utils';
import {
  CALENDAR_SOURCE_COLORS,
  type CalendarEvent,
  type CalendarSourceType,
} from '../../../platform/calendar/calendar-types';
import {
  getPlannerEventDaySpan,
  getPlannerEventStartDate,
  type PlannerBusyDay,
} from './planner-month-insights-model';

const SOURCE_TILES: ReadonlyArray<{
  type: CalendarSourceType;
  labelKey: string;
  icon: LucideIcon;
}> = [
  {
    type: 'pms_due',
    labelKey: 'planner.timeline.sources.pmsDue',
    icon: AlertCircle,
  },
  {
    type: 'pms_block',
    labelKey: 'planner.timeline.sources.pmsBlock',
    icon: ClipboardList,
  },
  {
    type: 'meeting',
    labelKey: 'planner.timeline.sources.meeting',
    icon: Users,
  },
  {
    type: 'planner_event',
    labelKey: 'planner.timeline.sources.plannerEvent',
    icon: User,
  },
];

const TILE_PREVIEW_LIMIT = 6;

function tint(color: string, percent: number): string {
  return `color-mix(in srgb, ${color} ${percent}%, transparent)`;
}

interface PlannerInsightsPanelProps {
  locale: string;
  /**
   * miy: the sources this planner lists. AI-DO always showed the four tiles
   * (PMS due, PMS schedule, meetings, personal events); other apps' sources
   * are not aggregated in this release, so only listed sources get a tile.
   */
  sources?: readonly CalendarSourceType[];
  weekStart: Date;
  weekEnd: Date;
  weekItems: Record<CalendarSourceType, CalendarEvent[]>;
  busyDays: PlannerBusyDay[];
  upcoming: CalendarEvent[];
  onPickDay: (date: Date) => void;
  onPickWeek: (date: Date) => void;
  onOpenEvent: (event: CalendarEvent) => void;
  onViewAll: () => void;
}

export function PlannerInsightsPanel({
  locale,
  weekStart,
  weekEnd,
  weekItems,
  busyDays,
  upcoming,
  onPickDay,
  onPickWeek,
  onOpenEvent,
  onViewAll,
  sources,
}: PlannerInsightsPanelProps) {
  const { t } = useTranslation('apps');
  const shortDate = new Intl.DateTimeFormat(locale, {
    month: 'numeric',
    day: 'numeric',
    weekday: 'short',
  });
  const dayOfMonth = new Intl.DateTimeFormat(locale, {
    day: 'numeric',
    weekday: 'short',
  });
  const longDate = new Intl.DateTimeFormat(locale, {
    month: 'long',
    day: 'numeric',
    weekday: 'short',
  });
  const weekLast = new Date(weekEnd.getTime() - 1);
  const monthDay = new Intl.DateTimeFormat(locale, {
    month: 'numeric',
    day: 'numeric',
  });
  const formatItemSpan = (item: CalendarEvent) => {
    const { first, last } = getPlannerEventDaySpan(item);
    return first.getTime() === last.getTime()
      ? monthDay.format(first)
      : `${monthDay.format(first)} ~ ${monthDay.format(last)}`;
  };

  return (
    <aside className="hidden w-72 shrink-0 flex-col gap-3 overflow-y-auto pl-3 xl:flex">
      <section className="rounded-2xl bg-app-surface p-4">
        <button
          type="button"
          onClick={() => onPickWeek(weekStart)}
          className="mb-3 flex w-full items-center gap-2 text-left"
        >
          <h2 className="app-text-control font-semibold text-app-ink">
            {t('planner.insights.weekSummary')}
          </h2>
          <span className="app-text-caption truncate text-app-ink/55 tabular-nums">
            {shortDate.format(weekStart)} ~ {shortDate.format(weekLast)}
          </span>
          <ChevronRight size={16} className="ml-auto text-app-ink/45" />
        </button>
        <div className="grid grid-cols-2 gap-2">
          {SOURCE_TILES.filter(
            ({ type }) => !sources || sources.includes(type),
          ).map(({ type, labelKey, icon: Icon }, index) => {
            const color = CALENDAR_SOURCE_COLORS[type];
            const items = weekItems[type];
            return (
              <div key={type} className="group relative">
                <div
                  tabIndex={items.length > 0 ? 0 : undefined}
                  className="flex items-center gap-2.5 rounded-xl p-3 outline-none focus-visible:ring-2 focus-visible:ring-app-accent"
                  style={{ backgroundColor: tint(color, 12) }}
                >
                  <span
                    className="flex size-8 shrink-0 items-center justify-center rounded-lg"
                    style={{ backgroundColor: tint(color, 20), color }}
                  >
                    <Icon size={18} />
                  </span>
                  <div className="min-w-0">
                    <div className="truncate text-[11px] text-app-ink/70">
                      {t(labelKey)}
                    </div>
                    <div className="app-text-control font-semibold text-app-ink tabular-nums">
                      {t('planner.insights.count', { count: items.length })}
                    </div>
                  </div>
                </div>
                {items.length > 0 ? (
                  <div
                    className={cn(
                      'invisible absolute top-full z-20 w-[calc(200%+0.5rem)] pt-1 opacity-0 transition-opacity group-focus-within:visible group-focus-within:opacity-100 group-hover:visible group-hover:opacity-100',
                      index % 2 === 0 ? 'left-0' : 'right-0',
                    )}
                  >
                    <div className="rounded-xl border border-app-border bg-app-surface p-2 shadow-xl">
                      <div className="app-text-caption flex items-center gap-1.5 px-2 pb-1 text-app-ink/55">
                        <span
                          className="size-2 rounded-full"
                          style={{ backgroundColor: color }}
                        />
                        {t(labelKey)} ·{' '}
                        {t('planner.insights.count', { count: items.length })}
                      </div>
                      <ul>
                        {items.slice(0, TILE_PREVIEW_LIMIT).map((item) => (
                          <li key={item.id}>
                            <button
                              type="button"
                              onClick={() => onOpenEvent(item)}
                              title={item.title}
                              className="flex w-full items-baseline gap-2 rounded-md px-2 py-1.5 text-left hover:bg-app-surface-hover"
                            >
                              <span className="app-text-control-sm min-w-0 flex-1 truncate text-app-ink">
                                {item.title}
                              </span>
                              <span className="shrink-0 text-[11px] text-app-ink/55 tabular-nums">
                                {formatItemSpan(item)}
                              </span>
                            </button>
                          </li>
                        ))}
                      </ul>
                      {items.length > TILE_PREVIEW_LIMIT ? (
                        <div className="app-text-caption px-2 pt-1 text-app-ink/55">
                          {t('planner.insights.more', {
                            count: items.length - TILE_PREVIEW_LIMIT,
                          })}
                        </div>
                      ) : null}
                    </div>
                  </div>
                ) : null}
              </div>
            );
          })}
        </div>
      </section>

      <section className="rounded-2xl bg-app-surface p-4">
        <h2 className="app-text-control mb-2 font-semibold text-app-ink">
          {t('planner.insights.busyDays')}
        </h2>
        {busyDays.length === 0 ? (
          <p className="app-text-caption py-2 text-app-ink/55">
            {t('planner.insights.empty')}
          </p>
        ) : (
          <ol>
            {busyDays.map((day, index) => (
              <li key={day.date.getTime()}>
                <button
                  type="button"
                  onClick={() => onPickDay(day.date)}
                  className="flex w-full items-center gap-3 rounded-lg px-1 py-2 text-left hover:bg-app-surface-hover"
                >
                  <span className="flex size-7 shrink-0 items-center justify-center rounded-full bg-app-surface-sidebar text-xs text-app-ink/70">
                    {index + 1}
                  </span>
                  <span className="min-w-0 flex-1">
                    <span className="app-text-control-sm block font-semibold text-app-ink">
                      {dayOfMonth.format(day.date)}
                    </span>
                    <span className="block text-[11px] text-app-ink/55">
                      {t('planner.insights.total', { count: day.count })}
                    </span>
                  </span>
                  <span className="flex items-center gap-1">
                    {day.sources.map((source) => (
                      <span
                        key={source}
                        className="size-2.5 rounded-full"
                        style={{
                          backgroundColor: CALENDAR_SOURCE_COLORS[source],
                        }}
                      />
                    ))}
                  </span>
                  <ChevronRight size={16} className="text-app-ink/45" />
                </button>
              </li>
            ))}
          </ol>
        )}
      </section>

      <section className="rounded-2xl bg-app-surface p-4">
        <div className="mb-2 flex items-center justify-between">
          <h2 className="app-text-control font-semibold text-app-ink">
            {t('planner.insights.upcoming')}
          </h2>
          <button
            type="button"
            onClick={onViewAll}
            className="app-text-caption flex items-center text-app-ink/55 hover:text-app-ink"
          >
            {t('planner.insights.viewAll')}
            <ChevronRight size={14} />
          </button>
        </div>
        {upcoming.length === 0 ? (
          <p className="app-text-caption py-2 text-app-ink/55">
            {t('planner.insights.empty')}
          </p>
        ) : (
          <ul className="divide-y divide-app-border">
            {upcoming.map((event) => (
              <li key={event.id}>
                <button
                  type="button"
                  onClick={() => onOpenEvent(event)}
                  className="flex w-full items-start gap-2.5 px-1 py-2.5 text-left hover:bg-app-surface-hover"
                >
                  {event.sourceType === 'pms_due' ? (
                    <AlertCircle
                      size={16}
                      className="mt-0.5 shrink-0"
                      style={{ color: event.color }}
                    />
                  ) : (
                    <span
                      className="mt-1.5 size-2.5 shrink-0 rounded-full"
                      style={{ backgroundColor: event.color }}
                    />
                  )}
                  <span className="min-w-0 flex-1">
                    <span className="block text-[11px] text-app-ink/55">
                      {longDate.format(getPlannerEventStartDate(event))}
                    </span>
                    <span className="app-text-control-sm block truncate text-app-ink">
                      {event.title}
                    </span>
                  </span>
                  <ChevronRight
                    size={16}
                    className="mt-2 shrink-0 text-app-ink/45"
                  />
                </button>
              </li>
            ))}
          </ul>
        )}
      </section>
    </aside>
  );
}
