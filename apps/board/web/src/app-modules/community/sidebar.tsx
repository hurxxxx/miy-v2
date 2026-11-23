import { useCallback, useEffect, useMemo, useState } from 'react';
import { Hash, Megaphone, Settings } from 'lucide-react';
import { useTranslation } from 'react-i18next';

import { cn } from '../../lib/utils';
import { useIsBoardManager } from '../../platform/board/board-context';
import { COMMUNITY_CHANNELS_CHANGED_EVENT } from '../../platform/community/community-channel-events';
import {
  ADMIN_PATH,
  NOTICES_PATH,
  useAppNavigate,
  useAppSearchParams,
  type AppRoute,
} from '../../platform/runtime/app-location';
import { useAppApi } from '../../platform/runtime/runtime-context';

import {
  listCommunityChannels,
  type CommunityChannel,
} from './api/community-api';
import { buildCommunityListUrl } from './community-url';
import { DEFAULT_COMMUNITY_CHANNEL_KEY } from './community-constants';

/**
 * AI-DO app-modules/community/sidebar.tsx (CommunitySidebarChannels). AI-DO
 * drew it in the portal's sub-sidebar; the board frame draws it at its left
 * edge. Links become `runtime.navigate`. miy additions: the notices entry
 * (AI-DO showed notices on the workspace home) and board settings for board
 * managers in place of AI-DO's admin console link.
 */
export function BoardSidebar({ route }: { route: AppRoute }) {
  const { t } = useTranslation('apps');
  const api = useAppApi();
  const navigate = useAppNavigate();
  const [searchParams] = useAppSearchParams();
  const canManageChannels = useIsBoardManager();
  const [channels, setChannels] = useState<CommunityChannel[]>([]);
  const [loading, setLoading] = useState(true);
  const [failed, setFailed] = useState(false);
  const [channelRevision, setChannelRevision] = useState(0);
  const activeChannelKey =
    route.kind === 'community'
      ? searchParams.get('channel') || DEFAULT_COMMUNITY_CHANNEL_KEY
      : null;

  useEffect(() => {
    const refresh = () => setChannelRevision((revision) => revision + 1);
    window.addEventListener(COMMUNITY_CHANNELS_CHANGED_EVENT, refresh);
    return () => window.removeEventListener(COMMUNITY_CHANNELS_CHANGED_EVENT, refresh);
  }, []);

  useEffect(() => {
    let cancelled = false;
    setLoading(true);
    setFailed(false);
    listCommunityChannels(api)
      .then((response) => {
        if (cancelled) return;
        setChannels(response.channels);
      })
      .catch(() => {
        if (cancelled) return;
        setChannels([]);
        setFailed(true);
      })
      .finally(() => {
        if (cancelled) return;
        setLoading(false);
      });
    return () => {
      cancelled = true;
    };
  }, [api, channelRevision]);

  const channelHrefByKey = useMemo(() => {
    const hrefByKey = new Map<string, string>();
    for (const channel of channels) {
      hrefByKey.set(channel.key, buildCommunityListUrl(channel.key));
    }
    return hrefByKey;
  }, [channels]);

  const go = useCallback((path: string) => navigate(path), [navigate]);

  return (
    <nav
      aria-label={t('board.title')}
      className="flex h-full w-56 shrink-0 flex-col gap-5 overflow-y-auto border-r border-app-border bg-app-surface-sidebar px-2 py-3"
    >
      <section className="space-y-1">
        <div className="sidebar-section-label px-3 py-1 text-app-ink/45">
          {t('community.channelsLabel')}
        </div>

        {loading && channels.length === 0 ? (
          <div className="sidebar-submenu-group text-app-ink/45">
            {t('community.loading')}
          </div>
        ) : null}

        {channels.map((channel) => (
          <button
            className={cn(
              'sidebar-submenu-item ml-1 w-[calc(100%-0.25rem)] text-left',
              channel.key === activeChannelKey && 'sidebar-submenu-item-active',
            )}
            key={channel.key}
            onClick={() => go(channelHrefByKey.get(channel.key) ?? '/community')}
            type="button"
          >
            <Hash size={16} className="text-app-ink/55 dark:text-app-ink/65" />
            <span className="sidebar-submenu-label">
              {t(`community.channels.${channel.key}`, {
                defaultValue: channel.name,
              })}
            </span>
          </button>
        ))}

        {failed ? (
          <div className="px-3 py-2 app-text-caption text-app-danger dark:text-app-danger-text">
            {t('community.errors.channelsFailed')}
          </div>
        ) : null}
      </section>

      <section className="space-y-1 border-t border-app-border pt-3">
        <button
          className={cn(
            'sidebar-submenu-item ml-1 w-[calc(100%-0.25rem)] text-left',
            route.kind === 'notices' && 'sidebar-submenu-item-active',
          )}
          onClick={() => go(NOTICES_PATH)}
          type="button"
        >
          <Megaphone size={16} className="text-app-ink/55 dark:text-app-ink/65" />
          <span className="sidebar-submenu-label">{t('board.notices')}</span>
        </button>
        {canManageChannels ? (
          <button
            className={cn(
              'sidebar-submenu-item ml-1 w-[calc(100%-0.25rem)] text-left',
              route.kind === 'admin' && 'sidebar-submenu-item-active',
            )}
            onClick={() => go(ADMIN_PATH)}
            type="button"
          >
            <Settings size={16} className="text-app-ink/55 dark:text-app-ink/65" />
            <span className="sidebar-submenu-label">{t('board.admin')}</span>
          </button>
        ) : null}
      </section>
    </nav>
  );
}
