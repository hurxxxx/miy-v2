import { ArrowLeft, Loader2, Search, X } from 'lucide-react';
import { useCallback, useEffect, useMemo, useState } from 'react';
import { useTranslation } from 'react-i18next';

import { useFeedback } from '@miy/components/ui/lib/feedback/feedback-provider';

import { searchDmUsers } from '../../app-modules/dm/api/dm-api';
import { useDmUserSearch } from '../../app-modules/dm/views/useDmUserSearch';
import { useAuth } from '../auth/auth-provider';
import { UserOptionRow } from '../users/UserSearchMultiSelect';

import { AdminDmSafetySection } from './admin-dm-safety-section';
import {
  claimDmSafetyManager,
  getDmSafetyManagerStatus,
  getDmSafetyTermsSettings,
  grantDmSafetyManager,
  listDmSafetyManagers,
  revokeDmSafetyManager,
  saveDmSafetyTermsSettings,
  type DmSafetyManager,
  type DmSafetyManagerStatus,
  type DmSafetyTermsSettings,
} from './admin-dm-safety-api';

const BUTTON =
  'app-text-body-sm rounded-md border border-app-border px-3 py-1.5 text-app-ink hover:bg-app-surface-hover disabled:opacity-50';
const INPUT =
  'app-text-body-sm w-full rounded-md border border-app-border bg-app-bg px-3 py-1.5 text-app-ink';

function errorText(error: unknown, fallback: string): string {
  return error instanceof Error && error.message ? error.message : fallback;
}

/**
 * miy: AI-DO showed report review in its admin console for platform
 * administrators (admin-dm-safety-section). The messenger shows the same
 * section inside the app for its own manager role, with the role's members
 * and the content rule settings AI-DO kept in server settings.
 */
export function DmSafetyPage({
  onBack,
  reportId,
}: {
  onBack: () => void;
  reportId?: string | null;
}) {
  const { t } = useTranslation('apps');
  const { token } = useAuth();
  const feedback = useFeedback();
  const [status, setStatus] = useState<DmSafetyManagerStatus | null>(null);
  const [claiming, setClaiming] = useState(false);

  const load = useCallback(async () => {
    if (!token) return;
    try {
      setStatus(await getDmSafetyManagerStatus(token));
    } catch (caught) {
      feedback.error(errorText(caught, t('admin.console.dmSafety.loadFailed')));
    }
  }, [feedback, t, token]);

  useEffect(() => {
    void load();
  }, [load]);

  const claim = async () => {
    if (!token || claiming) return;
    setClaiming(true);
    try {
      await claimDmSafetyManager(token);
      feedback.success(t('dm.manager.claimed'));
      await load();
    } catch (caught) {
      feedback.error(errorText(caught, t('dm.manager.managersFailed')));
      await load();
    } finally {
      setClaiming(false);
    }
  };

  return (
    <div className="flex h-full min-h-0 flex-col bg-app-bg text-app-ink">
      <header className="flex h-14 shrink-0 items-center gap-3 border-b border-app-border px-5">
        <button
          aria-label={t('dm.manager.back')}
          className="flex size-9 items-center justify-center rounded-md border border-app-border bg-app-surface"
          onClick={onBack}
          type="button"
        >
          <ArrowLeft size={16} />
        </button>
        <h1 className="app-text-title-md">{t('dm.manager.title')}</h1>
      </header>
      <div className="min-h-0 flex-1 overflow-y-auto px-5 py-4">
        {!status || !token ? (
          <div className="flex justify-center py-10">
            <Loader2 className="animate-spin text-app-ink/45" size={18} />
          </div>
        ) : status.manager ? (
          <div className="mx-auto grid max-w-6xl gap-6">
            <AdminDmSafetySection initialReportId={reportId} token={token} />
            <div className="grid gap-4 lg:grid-cols-2">
              <ManagersSection token={token} />
              <TermsSettingsSection token={token} />
            </div>
          </div>
        ) : status.claimable ? (
          <section className="mx-auto grid max-w-xl gap-3 rounded-md border border-app-border p-5">
            <div className="app-text-title-sm">{t('dm.manager.claimTitle')}</div>
            <p className="app-text-body-sm text-app-ink/70">{t('dm.manager.claimScope')}</p>
            <div>
              <button className={BUTTON} disabled={claiming} onClick={() => void claim()} type="button">
                {t('dm.manager.claim')}
              </button>
            </div>
          </section>
        ) : (
          <p className="app-text-body text-center text-app-ink/55">{t('dm.manager.noAccess')}</p>
        )}
      </div>
    </div>
  );
}

function ManagersSection({ token }: { token: string }) {
  const { t } = useTranslation('apps');
  const { user } = useAuth();
  const feedback = useFeedback();
  const [items, setItems] = useState<DmSafetyManager[]>([]);
  const [query, setQuery] = useState('');
  const [busy, setBusy] = useState(false);
  const managerIds = useMemo(() => new Set(items.map((item) => item.user.id)), [items]);
  const { results, reset, searching } = useDmUserSearch({
    excludeUserIds: managerIds,
    query,
    searchUsers: searchDmUsers,
    token,
  });

  useEffect(() => {
    listDmSafetyManagers(token)
      .then((response) => setItems(response.items))
      .catch((caught: unknown) =>
        feedback.error(errorText(caught, t('dm.manager.managersFailed'))),
      );
  }, [feedback, t, token]);

  const change = async (action: () => Promise<{ items: DmSafetyManager[] }>) => {
    if (busy) return;
    setBusy(true);
    try {
      setItems((await action()).items);
      setQuery('');
      reset();
    } catch (caught) {
      feedback.error(errorText(caught, t('dm.manager.managersFailed')));
    } finally {
      setBusy(false);
    }
  };

  return (
    <section className="grid content-start gap-2 rounded-md border border-app-border p-4">
      <div className="app-text-title-sm">{t('dm.manager.managersTitle')}</div>
      <ul className="grid gap-1">
        {items.map((item) => (
          <li className="flex items-center gap-2" key={item.user.id}>
            <span className="app-text-body-sm min-w-0 flex-1 truncate">
              {item.user.full_name}
              {item.user.primary_org_unit_name ? (
                <span className="text-app-ink/50"> · {item.user.primary_org_unit_name}</span>
              ) : null}
            </span>
            {items.length > 1 ? (
              <button
                aria-label={t('dm.manager.managerRemove', { name: item.user.full_name })}
                className="flex size-7 items-center justify-center rounded-md text-app-ink/50 hover:bg-app-surface-hover"
                disabled={busy}
                onClick={() => void change(() => revokeDmSafetyManager(token, item.user.id))}
                type="button"
              >
                <X size={13} />
              </button>
            ) : null}
          </li>
        ))}
      </ul>
      <div className="flex items-center gap-2 rounded-md border border-app-border bg-app-bg px-3 py-1.5">
        <Search className="shrink-0 text-app-ink/45" size={14} />
        <input
          aria-label={t('dm.manager.managerSearch')}
          className="app-text-body-sm w-full bg-transparent outline-none"
          onChange={(event) => setQuery(event.target.value)}
          placeholder={t('dm.manager.managerSearch')}
          value={query}
        />
        {searching ? <Loader2 className="animate-spin text-app-ink/45" size={13} /> : null}
      </div>
      {query.trim() ? (
        <div className="max-h-44 overflow-y-auto rounded-md border border-app-border">
          {results.length ? (
            results.map((candidate) => (
              <UserOptionRow
                currentUserId={user?.id}
                currentUserLabel={t('pms.taskDetail.me')}
                disabled={busy}
                key={candidate.id}
                onClick={() => void change(() => grantDmSafetyManager(token, candidate.id))}
                user={candidate}
              />
            ))
          ) : (
            <div className="app-text-caption px-3 py-3 text-center text-app-ink/45">
              {searching ? t('dm.searching') : t('dm.noSearchResults')}
            </div>
          )}
        </div>
      ) : null}
    </section>
  );
}

function TermsSettingsSection({ token }: { token: string }) {
  const { t } = useTranslation('apps');
  const feedback = useFeedback();
  const [settings, setSettings] = useState<DmSafetyTermsSettings | null>(null);
  const [saving, setSaving] = useState(false);

  useEffect(() => {
    getDmSafetyTermsSettings(token)
      .then(setSettings)
      .catch((caught: unknown) => feedback.error(errorText(caught, t('dm.manager.termsFailed'))));
  }, [feedback, t, token]);

  if (!settings) return null;
  const update = (patch: Partial<DmSafetyTermsSettings>) =>
    setSettings((current) => (current ? { ...current, ...patch } : current));
  const save = async () => {
    setSaving(true);
    try {
      setSettings(await saveDmSafetyTermsSettings(token, settings));
      feedback.success(t('dm.manager.termsSaved'));
    } catch (caught) {
      feedback.error(errorText(caught, t('dm.manager.termsFailed')));
    } finally {
      setSaving(false);
    }
  };

  return (
    <section className="grid content-start gap-2 rounded-md border border-app-border p-4">
      <div className="app-text-title-sm">{t('dm.manager.termsTitle')}</div>
      <label className="app-text-caption grid gap-1 text-app-ink/60">
        {t('dm.manager.termsVersion')}
        <input
          className={INPUT}
          maxLength={32}
          onChange={(event) => update({ version: event.target.value })}
          value={settings.version}
        />
      </label>
      <label className="app-text-body-sm flex items-center gap-2">
        <input
          checked={settings.required}
          onChange={(event) => update({ required: event.target.checked })}
          type="checkbox"
        />
        {t('dm.manager.termsRequired')}
      </label>
      {(
        [
          ['document_url', 'dm.manager.termsDocumentUrl'],
          ['contact_email', 'dm.manager.termsContactEmail'],
          ['contact_url', 'dm.manager.termsContactUrl'],
        ] as const
      ).map(([field, label]) => (
        <label className="app-text-caption grid gap-1 text-app-ink/60" key={field}>
          {t(label)}
          <input
            className={INPUT}
            onChange={(event) => update({ [field]: event.target.value || null })}
            value={settings[field] ?? ''}
          />
        </label>
      ))}
      <div>
        <button className={BUTTON} disabled={saving || !settings.version.trim()} onClick={() => void save()} type="button">
          {t('dm.manager.termsSave')}
        </button>
      </div>
    </section>
  );
}
