import { Link } from '../../../platform/router';
import { useTranslation } from 'react-i18next';
import { Layout, List, Plus } from 'lucide-react';
import { ContentState } from '@miy/ui';

import type { PmsSpace, PmsTaskList } from '../api/pms-api';
import { buildPmsSpaceToolPath, buildPmsTaskListToolPath } from './pms-view-route';
import { taskDonePercent } from './space-overview-model';

/**
 * miy: any PMS user may create a space (AI-DO's `can_create_space` for PMS,
 * `space_creation_role="user"`); the server still decides.
 */
const CAN_CREATE_SPACE = true;

/** The global overview keeps every link attached to its server-returned owner. */
export function PmsGlobalOverview({
  spaces,
  taskLists,
  onCreateSpace,
}: {
  spaces: readonly PmsSpace[];
  taskLists: readonly PmsTaskList[];
  onCreateSpace: () => void;
}) {
  const { t } = useTranslation('apps');
  const activeLists = taskLists.filter((item) => !item.archived);
  return (
    <main className="min-h-full bg-app-bg px-4 py-6 text-app-ink sm:px-6">
      <div className="mx-auto max-w-6xl space-y-6">
        <header className="flex flex-wrap items-start justify-between gap-4">
          <div>
            <h1 className="app-text-title-lg">{t('pms.overview')}</h1>
            <p className="app-text-body-sm mt-2 text-app-ink-muted">
              {t('pms.overviewPage.summary', {
                lists: activeLists.length,
                active: activeLists.reduce(
                  (sum, item) => sum + item.open_task_count,
                  0,
                ),
                overdue: activeLists.reduce(
                  (sum, item) => sum + item.overdue_task_count,
                  0,
                ),
              })}
            </p>
          </div>
          {CAN_CREATE_SPACE ? (
            <button
              type="button"
              onClick={onCreateSpace}
              className="app-control-primary gap-2 px-4 py-2"
            >
              <Plus aria-hidden="true" size={16} />
              {t('pms.createSpace')}
            </button>
          ) : null}
        </header>
        {spaces.length === 0 ? (
          <ContentState
            kind="empty"
            title={t('pms.noSpacesTitle')}
            description={
              CAN_CREATE_SPACE
                ? t('pms.noSpacesDescription')
                : t('pms.overviewPage.noAccessibleSpaces')
            }
          />
        ) : (
          <div className="grid gap-4 md:grid-cols-2 xl:grid-cols-3">
            {spaces.map((space) => {
              const lists = activeLists.filter(
                (item) => item.team_id === space.id,
              );
              return (
                <section
                  key={space.id}
                  className="rounded-xl border border-app-border bg-app-surface p-4"
                >
                  <Link
                    to={buildPmsSpaceToolPath(space.id)}
                    className="app-text-title-sm flex items-center gap-2 text-app-ink hover:underline"
                  >
                    <Layout aria-hidden="true" size={18} />
                    {space.name}
                  </Link>
                  {space.description ? (
                    <p className="app-text-body-sm mt-2 line-clamp-2 text-app-ink-muted">
                      {space.description}
                    </p>
                  ) : null}
                  <h2 className="app-text-caption mt-5 text-app-ink-muted">
                    {t('pms.overviewPage.lists')}
                  </h2>
                  {lists.length ? (
                    <ul className="mt-2 space-y-1">
                      {lists.map((list) => (
                        <li key={list.id}>
                          <Link
                            to={buildPmsTaskListToolPath({
                              taskListId: list.id,
                            })}
                            className="flex items-start gap-2 rounded-lg px-2 py-2 hover:bg-app-surface-hover"
                          >
                            <List
                              aria-hidden="true"
                              size={16}
                              className="mt-0.5 shrink-0 text-app-ink-muted"
                            />
                            <div className="min-w-0">
                              <p className="app-text-body-sm truncate">
                                {list.name}
                              </p>
                              <p className="app-text-caption text-app-ink-muted">
                                {t('pms.overviewPage.listProgress', {
                                  count: list.active_task_count,
                                  progress: taskDonePercent(
                                    list.active_task_count,
                                    list.open_task_count,
                                  ),
                                })}
                              </p>
                            </div>
                          </Link>
                        </li>
                      ))}
                    </ul>
                  ) : (
                    <p className="app-text-body-sm mt-2 text-app-ink-muted">
                      {t('pms.overviewPage.noLists')}
                    </p>
                  )}
                </section>
              );
            })}
          </div>
        )}
      </div>
    </main>
  );
}
