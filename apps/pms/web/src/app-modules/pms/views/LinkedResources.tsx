import { useEffect, useState, type FormEvent } from 'react';
import { Button, Input } from '@miy/ui';
import type { AppRuntime } from '@miy/sdk';
import { useTranslation } from 'react-i18next';

import type { components } from '../../../api.generated';
import { useNavigationDirty } from '../../../platform/runtime/navigation-state';
import { useRuntime } from '../../../platform/runtime/runtime-context';

type Scope = components['schemas']['LinkScope'];
type References = components['schemas']['References'];
type Target = 'docs' | 'whiteboard';
type Space = { id: string; name: string };
type Results = { items: { id: string; title: string }[]; total: number };
type Props = { scope: Scope; ownerId: string; canEdit: boolean };

const PAGE_SIZE = 12;
const UUID = /^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$/i;
const EMPTY_LINKS: References = { items: [], unavailable_links: [] };

function pmsPortalUrl(portalUrl: AppRuntime['portalUrl']): URL | null {
  if (!portalUrl) return null;
  try {
    const url = new URL(portalUrl('/'));
    // The CLI preview serves only PMS and has no backend connection route.
    if (url.origin === window.location.origin || !['https:', 'http:'].includes(url.protocol)) return null;
    return /^\/(environments\/(?:dev|prod)\/)?apps\/pms\/?$/.test(url.pathname) ? url : null;
  } catch {
    return null;
  }
}

export function linkedResourceUrl(portalUrl: AppRuntime['portalUrl'], target: Target, id: string, space: string): string | null {
  const url = pmsPortalUrl(portalUrl);
  if (!url || !UUID.test(id)) return null;
  url.pathname = url.pathname.replace(/\/pms\/?$/, `/${target}/${id}`);
  url.search = space === 'global' ? '' : new URLSearchParams({ space }).toString();
  url.hash = '';
  return url.href;
}

export function LinkedResources(props: Props) {
  return <LinkedResourcesDisclosure key={`${props.scope}:${props.ownerId}`} {...props} />;
}

function LinkedResourcesDisclosure(props: Props) {
  const { t } = useTranslation('apps');
  const [opened, setOpened] = useState(false);
  const [open, setOpen] = useState(false);
  return (
    <details className="mb-3 rounded-md border border-app-border bg-app-surface p-3" onToggle={(event) => {
      const next = event.currentTarget.open;
      setOpen(next);
      if (next) setOpened(true);
    }}>
      <summary className="app-text-control-sm cursor-pointer font-semibold">
        {t(props.scope === 'task' ? 'pms.resourceLinks.docs' : 'pms.resourceLinks.boards')}
      </summary>
      {opened && <LinkedResourcesSession {...props} open={open} />}
    </details>
  );
}

function LinkedResourcesSession({ scope, ownerId, canEdit, open }: Props & { open: boolean }) {
  const { t } = useTranslation('apps');
  const { api, appApi, portalUrl } = useRuntime();
  const installed = pmsPortalUrl(portalUrl) !== null;
  const target = scope === 'task' ? 'docs' : 'whiteboard';
  const path = `/v1/pms/resource-links/${scope}/${encodeURIComponent(ownerId)}`;
  const [picking, setPicking] = useState(false);
  const [links, setLinks] = useState<References>(EMPTY_LINKS);
  const [loading, setLoading] = useState(false);
  const [failure, setFailure] = useState<'read' | 'write' | null>(null);
  const [busy, setBusy] = useState(false);
  const [refresh, setRefresh] = useState(0);
  useNavigationDirty(busy);

  useEffect(() => {
    if (!open || !installed) return;
    const controller = new AbortController();
    setLoading(true);
    setLinks(EMPTY_LINKS);
    setFailure(null);
    void api<References>(path, { signal: controller.signal }).then(
      (response) => { if (!controller.signal.aborted) setLinks(response); },
      () => {
        if (controller.signal.aborted) return;
        setLinks(EMPTY_LINKS);
        setFailure('read');
      },
    ).finally(() => { if (!controller.signal.aborted) setLoading(false); });
    return () => controller.abort();
  }, [api, installed, open, path, refresh]);

  async function change(id: string, method: 'PUT' | 'DELETE') {
    if (busy || !canEdit || !UUID.test(id)) return;
    setBusy(true);
    setFailure(null);
    try {
      await api(method === 'PUT' ? `${path}/${id}` : `${path}/links/${id}`, { method });
      if (method === 'PUT') setPicking(false);
      setRefresh((value) => value + 1);
    } catch {
      setFailure('write');
    } finally {
      setBusy(false);
    }
  }

  if (!installed) return <p className="mt-3">{t('pms.resourceLinks.installedOnly')}</p>;

  const hasLinks = links.items.length > 0 || links.unavailable_links.length > 0;
  const pickLabel = scope === 'list' && hasLinks ? 'pms.resourceLinks.replace' : 'pms.resourceLinks.pick';
  return (
    <div className="mt-3 space-y-2">
      {failure && <p role="alert" className="text-app-danger-text">{t(`pms.resourceLinks.${failure}Failed`)}</p>}
      {failure === 'read' && <Button size="dense" onClick={() => setRefresh((value) => value + 1)}>{t('pms.resourceLinks.retry')}</Button>}
      {loading && <p role="status">{t('pms.resourceLinks.loading')}</p>}
      {!loading && !hasLinks && failure !== 'read' && <p>{t('pms.resourceLinks.empty')}</p>}
      {hasLinks && (
        <ul className="space-y-1">
          {links.items.map((item) => {
            const url = linkedResourceUrl(portalUrl, target, item.id, item.space_id);
            return <li key={item.link_id} className="flex items-center justify-between gap-2">
              {url ? <a href={url} target="_blank" rel="noopener noreferrer" className="break-all underline">{item.title}</a> : <span>{item.title}</span>}
              {canEdit && <Button size="dense" variant="ghost" disabled={busy} onClick={() => void change(item.link_id, 'DELETE')}>
                {t('pms.resourceLinks.unlink')}
              </Button>}
            </li>;
          })}
          {canEdit && links.unavailable_links.map((id) => (
            <li key={id} className="flex items-center justify-between gap-2">
              <span>{t('pms.resourceLinks.linkUnavailable')}</span>
              <Button size="dense" variant="ghost" disabled={busy} onClick={() => void change(id, 'DELETE')}>
                {t('pms.resourceLinks.unlinkUnavailable')}
              </Button>
            </li>
          ))}
        </ul>
      )}
      {canEdit && (
        <Button size="dense" disabled={busy || loading} onClick={() => setPicking((value) => !value)}>
          {t(picking ? 'pms.resourceLinks.cancel' : pickLabel)}
        </Button>
      )}
      {open && canEdit && picking && (
        <ResourcePicker appApi={appApi} target={target} busy={busy} linked={links.items}
          onPick={(id) => void change(id, 'PUT')} />
      )}
    </div>
  );
}

function ResourcePicker({ appApi, target, busy, linked, onPick }: {
  appApi: AppRuntime['appApi']; target: Target; busy: boolean; linked: References['items']; onPick: (id: string) => void;
}) {
  const { t } = useTranslation('apps');
  const [spaces, setSpaces] = useState<Space[]>([]);
  const [space, setSpace] = useState('global');
  const [draft, setDraft] = useState('');
  const [query, setQuery] = useState('');
  const [page, setPage] = useState(1);
  const [results, setResults] = useState<Results>();
  const [loadFailed, setLoadFailed] = useState(false);

  useEffect(() => {
    const controller = new AbortController();
    void appApi<{ spaces: Space[] }>(target, '/v1/spaces', { signal: controller.signal }).then(
      (result) => { if (!controller.signal.aborted) setSpaces(result.spaces.filter((item) => item.id !== 'global')); },
      () => undefined,
    );
    return () => controller.abort();
  }, [appApi, target]);

  useEffect(() => {
    const controller = new AbortController();
    const params = new URLSearchParams({ page: String(page), page_size: String(PAGE_SIZE), q: query });
    const base = target === 'docs' || space === 'global' ? `/v1/${target}` : `/v1/workspaces/${encodeURIComponent(space)}/whiteboard`;
    if (target === 'docs' && space !== 'global') params.set('space', space);
    setResults(undefined);
    setLoadFailed(false);
    void appApi<Results>(target, `${base}/hub?${params}`, { signal: controller.signal }).then(
      (result) => { if (!controller.signal.aborted) setResults(result); },
      () => { if (!controller.signal.aborted) setLoadFailed(true); },
    );
    return () => controller.abort();
  }, [appApi, page, query, space, target]);

  function search(event: FormEvent<HTMLFormElement>) {
    event.preventDefault();
    setPage(1);
    setQuery(draft.trim());
  }

  return (
    <div className="space-y-2 border-t border-app-border pt-2">
      <form className="flex flex-wrap gap-2" onSubmit={search}>
        <select
          aria-label={t('pms.resourceLinks.space')}
          className="rounded-md border border-app-border bg-app-surface px-2"
          value={space}
          onChange={(event) => { setSpace(event.target.value); setPage(1); }}
        >
          <option value="global">{t('pms.resourceLinks.global')}</option>
          {spaces.map((item) => <option key={item.id} value={item.id}>{item.name}</option>)}
        </select>
        <Input type="search" aria-label={t('pms.resourceLinks.search')}
          value={draft} maxLength={200} onChange={(event) => setDraft(event.target.value)} />
        <Button type="submit" size="dense">{t('pms.resourceLinks.search')}</Button>
      </form>
      {loadFailed ? <p role="alert">{t('pms.resourceLinks.targetUnavailable')}</p>
        : !results ? <p role="status">{t('pms.resourceLinks.loading')}</p>
        : results.items.length === 0 ? <p>{t('pms.resourceLinks.noMatches')}</p>
        : <ul className="space-y-1">
          {results.items.map((item) => <li key={item.id} className="flex items-center justify-between gap-2">
            <span>{item.title}</span>
            <Button size="dense" disabled={busy || linked.some((entry) => entry.id === item.id)} onClick={() => onPick(item.id)}>
              {t('pms.resourceLinks.attach')}
            </Button>
          </li>)}
        </ul>}
      {results && (page > 1 || results.total > PAGE_SIZE) && <div className="flex gap-2">
        <Button size="dense" disabled={page <= 1} onClick={() => setPage(page - 1)}>{t('pms.resourceLinks.previous')}</Button>
        <Button size="dense" disabled={page * PAGE_SIZE >= results.total} onClick={() => setPage(page + 1)}>{t('pms.resourceLinks.next')}</Button>
      </div>}
    </div>
  );
}
