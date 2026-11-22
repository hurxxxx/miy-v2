// AI-DO's notification section (apps/web/src/platform/auth/
// NotificationsSettingsSection.tsx at 0002af062) was a placeholder that was not
// connected. miy keeps its layout and connects it: each app the person can
// open can be turned off as a whole or per notification kind received so far.
// The Core applies these when it stores a notification; apps never read them.
import { useEffect, useMemo, useState } from 'react';
import { ContextNote, StatusSlot } from '@miy/ui';

import {
  listNotifications,
  readNotificationSettings,
  saveNotificationSettings,
  type NotificationMute,
} from '../../data/notifications';
import { SettingsSectionHeader } from './SettingsSectionHeader';
import type { SettingsTranslator } from './settings-page-model';

type AppRow = { appId: string; name: string; kinds: string[] };

export function NotificationsSettingsSection({
  apps,
  csrf,
  hasInbox,
  t,
}: {
  /** Apps the person can open in this portal. */
  apps: { appId: string; name: string }[];
  csrf: string;
  hasInbox: boolean;
  t: SettingsTranslator;
}) {
  const [mutes, setMutes] = useState<NotificationMute[] | null>(null);
  const [kinds, setKinds] = useState<Record<string, string[]>>({});
  const [error, setError] = useState<string | null>(null);
  const [saving, setSaving] = useState(false);

  useEffect(() => {
    let current = true;
    readNotificationSettings()
      .then((value) => current && setMutes(value.mutes))
      .catch(() => current && setError(t('portal:notifications.settingsLoadFailed')));
    if (hasInbox)
      listNotifications()
        .then((page) => {
          if (!current) return;
          const seen: Record<string, Set<string>> = {};
          for (const item of page.items) (seen[item.appId] ??= new Set()).add(item.kind);
          setKinds(Object.fromEntries(Object.entries(seen).map(([app, set]) => [app, [...set].sort()])));
        })
        .catch(() => undefined);
    return () => {
      current = false;
    };
  }, [hasInbox, t]);

  const rows: AppRow[] = useMemo(() => {
    const byApp = new Map(apps.map((app) => [app.appId, app.name]));
    for (const mute of mutes ?? []) if (!byApp.has(mute.appId)) byApp.set(mute.appId, mute.appId);
    return [...byApp.entries()].map(([appId, name]) => {
      const fromMutes = (mutes ?? []).filter((mute) => mute.appId === appId && mute.kind !== '*').map((mute) => mute.kind);
      return { appId, name, kinds: [...new Set([...(kinds[appId] ?? []), ...fromMutes])].sort() };
    });
  }, [apps, kinds, mutes]);

  const muted = (appId: string, kind: string) =>
    (mutes ?? []).some((mute) => mute.appId === appId && mute.kind === kind);

  async function toggle(appId: string, kind: string, receive: boolean) {
    if (!mutes) return;
    const next = receive
      ? mutes.filter((mute) => !(mute.appId === appId && mute.kind === kind))
      : [...mutes, { appId, kind }];
    const previous = mutes;
    setMutes(next);
    setSaving(true);
    try {
      setMutes((await saveNotificationSettings(csrf, next)).mutes);
      setError(null);
    } catch {
      setMutes(previous);
      setError(t('auth:settings.saveFailed'));
    } finally {
      setSaving(false);
    }
  }

  return (
    <div>
      <SettingsSectionHeader
        title={t('auth:settings.notifications')}
        description={t('auth:settings.notificationsDescription')}
      />
      <StatusSlot message={error} tone="error" announce="assertive" size="regular" />
      <div className="border-t border-app-border">
        {!hasInbox ? (
          <ContextNote tone="info" className="mt-4">
            {t('portal:notifications.consoleHasNoInbox')}
          </ContextNote>
        ) : null}
        {mutes !== null && rows.length === 0 ? (
          <p className="app-text-body py-4 text-app-ink/55">{t('portal:notifications.noApps')}</p>
        ) : null}
        {mutes !== null &&
          rows.map((row) => {
            const all = !muted(row.appId, '*');
            return (
              <div key={row.appId} className="border-b border-app-border py-4 last:border-b-0">
                <div className="flex items-center justify-between">
                  <div>
                    <div className="app-text-body font-medium text-app-ink">{row.name}</div>
                    <div className="app-text-caption mt-0.5 text-app-ink/55">
                      {t('portal:notifications.appDescription')}
                    </div>
                  </div>
                  <label className="flex cursor-pointer items-center gap-1.5">
                    <input
                      className="size-3.5 rounded border-app-border bg-app-bg accent-app-accent"
                      aria-label={t('portal:notifications.receiveFrom', { app: row.name })}
                      checked={all}
                      disabled={saving}
                      onChange={(event) => void toggle(row.appId, '*', event.target.checked)}
                      type="checkbox"
                    />
                    <span className="app-text-caption text-app-ink/55">{t('portal:notifications.receive')}</span>
                  </label>
                </div>
                {all && row.kinds.length > 0 ? (
                  <div className="mt-2 flex flex-wrap gap-4 pl-1">
                    {row.kinds.map((kind) => (
                      <label key={kind} className="flex cursor-pointer items-center gap-1.5">
                        <input
                          className="size-3.5 rounded border-app-border bg-app-bg accent-app-accent"
                          aria-label={t('portal:notifications.receiveKind', { app: row.name, kind })}
                          checked={!muted(row.appId, kind)}
                          disabled={saving}
                          onChange={(event) => void toggle(row.appId, kind, event.target.checked)}
                          type="checkbox"
                        />
                        <span className="app-text-caption text-app-ink/55">{kind}</span>
                      </label>
                    ))}
                  </div>
                ) : null}
              </div>
            );
          })}
      </div>
    </div>
  );
}
