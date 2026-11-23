import { cleanup, fireEvent, screen } from '@testing-library/react';
import type { AppRuntime } from '@miy/sdk';
import { afterEach, expect, it, vi } from 'vitest';

import { createTestRuntime, renderWithRuntime } from '../../../test-utils/runtime';
import { LinkedResources, linkedResourceUrl } from './LinkedResources';

vi.mock('react-i18next', () => ({ useTranslation: () => ({ t: (key: string) => key }) }));

afterEach(cleanup);

const doc = { id: '19b12a75-7d99-4ebf-b56b-2735caee6d98', title: 'Design notes', space_id: 'engineering' };
const linkId = '1b74fe40-2d53-4bc2-a52d-5d41cc91b2e3';
const portalUrl = () => 'https://dev.example/environments/dev/apps/pms/';

it('opens the target resource in the current portal environment and owning space', () => {
  expect(linkedResourceUrl(portalUrl, 'docs', doc.id, doc.space_id)).toBe(
    `https://dev.example/environments/dev/apps/docs/${doc.id}?space=engineering`,
  );
  expect(linkedResourceUrl(portalUrl, 'whiteboard', '../other', 'global')).toBeNull();
  const previewUrl = () => new URL('/apps/pms/', window.location.origin).href;
  expect(linkedResourceUrl(previewUrl, 'docs', doc.id, 'global')).toBeNull();
});

it('loads links on demand and selects a document from the target space', async () => {
  const ownerPath = '/v1/pms/resource-links/task/task-1';
  const linked = { ...doc, link_id: linkId };
  let items: typeof linked[] = [];
  const api = vi.fn(async (_path: string, init?: RequestInit) => {
    if (init?.method === 'PUT') items = [linked];
    if (init?.method === 'DELETE') items = [];
    return { items, unavailable_links: [] };
  });
  const appApi = vi.fn(async (_target: string, path: string) => {
    if (path === '/v1/spaces') return { spaces: [{ id: doc.space_id, name: 'Engineering' }] };
    return path.includes('space=engineering') ? { items: [doc], total: 1 } : { items: [], total: 0 };
  });
  const runtime = createTestRuntime({
    api: api as AppRuntime['api'], appApi: appApi as AppRuntime['appApi'], portalUrl,
  });
  renderWithRuntime(<LinkedResources scope="task" ownerId="task-1" canEdit />, { runtime });
  expect(api).not.toHaveBeenCalled();
  const disclosure = screen.getByText('pms.resourceLinks.docs').closest('details')!;
  disclosure.open = true;
  fireEvent(disclosure, new Event('toggle'));
  await screen.findByText('pms.resourceLinks.empty');
  fireEvent.click(screen.getByRole('button', { name: 'pms.resourceLinks.pick' }));
  await screen.findByRole('option', { name: 'Engineering' });
  fireEvent.change(screen.getByRole('combobox', { name: 'pms.resourceLinks.space' }), { target: { value: doc.space_id } });
  fireEvent.click(await screen.findByRole('button', { name: 'pms.resourceLinks.attach' }));
  expect(api).toHaveBeenCalledWith(`${ownerPath}/${doc.id}`, { method: 'PUT' });
  expect((await screen.findByRole('link', { name: doc.title })).getAttribute('href')).toBe(
    `https://dev.example/environments/dev/apps/docs/${doc.id}?space=engineering`,
  );
  fireEvent.click(screen.getByRole('button', { name: 'pms.resourceLinks.unlink' }));
  await screen.findByText('pms.resourceLinks.empty');
  expect(api).toHaveBeenCalledWith(`${ownerPath}/links/${linkId}`, { method: 'DELETE' });
});

it('can remove an unavailable link without exposing the target or its title', async () => {
  const api = vi.fn(async (_path: string, init?: RequestInit) => ({
    items: [], unavailable_links: init?.method === 'DELETE' ? [] : [linkId],
  }));
  const runtime = createTestRuntime({ api: api as AppRuntime['api'], portalUrl });
  renderWithRuntime(<LinkedResources scope="task" ownerId="task-1" canEdit />, { runtime });
  const disclosure = screen.getByText('pms.resourceLinks.docs').closest('details')!;
  disclosure.open = true;
  fireEvent(disclosure, new Event('toggle'));
  expect(screen.queryByRole('link')).toBeNull();
  fireEvent.click(await screen.findByRole('button', { name: 'pms.resourceLinks.unlinkUnavailable' }));
  expect(api).toHaveBeenCalledWith(`/v1/pms/resource-links/task/task-1/links/${linkId}`, { method: 'DELETE' });
});
