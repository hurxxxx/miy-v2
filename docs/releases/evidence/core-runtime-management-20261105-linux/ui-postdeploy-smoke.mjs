#!/usr/bin/env node
// Post-deployment browser check for 7b43789 (administrator runtime management list).
// Session login/logout only; all domain mutations are blocked in the browser.
// Run only after deployment: node <this-file> --run-after-deploy --output <new-directory>
import { chromium, expect } from '/projects/miy-v2/node_modules/@playwright/test/index.mjs';
import { createHash } from 'node:crypto';
import { mkdir, readFile, writeFile } from 'node:fs/promises';
import { resolve, join } from 'node:path';

process.umask(0o077);
const root = '/projects/miy-v2';
const release = join(root, '.local/release-7b43789/ui');
const origin = 'https://v2.dwdcc.kr';
const args = process.argv.slice(2);
if (args.length !== 3 || args[0] !== '--run-after-deploy' || args[1] !== '--output') {
  console.error('Required: --run-after-deploy --output <new-directory>');
  process.exit(2);
}
const output = resolve(args[2]);
if (!output.startsWith(release + '/')) throw new Error('output_must_be_in_isolated_ui_release');
await mkdir(output, { mode: 0o700 });
const manifest = JSON.parse(await readFile(join(release, 'manifest.json'), 'utf8'));
const result = { sourceCommit: manifest.sourceCommit, origin, result: 'FAIL', checks: [], cleanupErrors: [], pageErrors: 0, blockedMutations: 0 };
let step = 'start';
let browser;
let adminApps = [];
const requireThat = (value, code) => { if (!value) throw Object.assign(new Error(code), { safeCode: code }); };
const passed = (name, details = {}) => result.checks.push({ name, result: 'PASS', ...details });
const notRun = (name, reason) => result.checks.push({ name, result: 'NOT_RUN', reason });
const ids = (items) => items.map((item) => item.id).sort();
const sameIds = (a, b) => JSON.stringify(ids(a)) === JSON.stringify(ids(b));
const panel = (page, name) => page.locator('.ui-panel').filter({ has: page.getByRole('heading', { name, exact: true }) });
const isManagedPath = (response) => new URL(response.url()).pathname === '/api/v1/runtime/installations';

async function get(context, path, status = 200) {
  const response = await context.request.get(origin + path, { timeout: 20000, maxRedirects: 0 });
  requireThat(response.status() === status, 'unexpected_read_status');
  return status === 200 ? response.json() : null;
}

async function pages(context, name) {
  const rows = [];
  let cursor;
  for (let count = 0; count < 100; count++) {
    const query = new URLSearchParams({ limit: '200' });
    if (cursor) query.set('cursor', cursor);
    const page = await get(context, `/api/v1/${name}?${query}`);
    requireThat(Array.isArray(page[name]), 'invalid_page_shape');
    rows.push(...page[name]);
    if (!page.nextCursor) return rows;
    requireThat(page.nextCursor !== cursor, 'cursor_did_not_advance');
    cursor = page.nextCursor;
  }
  throw Object.assign(new Error('page_limit'), { safeCode: 'page_limit' });
}

// The company switch blocks use, not runtime control: the managed list keeps disabled apps and
// reports the server's current open decision; everything the administrator may open is in it.
function checkManaged(managed, normal) {
  requireThat(Array.isArray(managed), 'managed_installations_shape');
  for (const item of managed) {
    requireThat(typeof item.appDisabled === 'boolean' && typeof item.canOpen === 'boolean', 'managed_flags_missing');
    requireThat(item.canManage === true, 'managed_item_not_manageable');
    if (item.appDisabled && item.environment === 'prod') requireThat(item.canOpen === false, 'disabled_prod_offered_for_opening');
  }
  requireThat(new Set(ids(managed)).size === managed.length, 'managed_duplicate_ids');
  requireThat(sameIds(managed.filter((item) => item.canOpen), normal), 'managed_open_set_differs_from_use_list');
  return { managedCount: managed.length, disabledCount: managed.filter((item) => item.appDisabled).length };
}

async function session(label, admin) {
  const role = admin ? 'administrator' : 'regular_user';
  const context = await browser.newContext({ baseURL: origin });
  context.setDefaultTimeout(20000);
  let loggedIn = false;
  await context.route('**/*', async (route) => {
    const request = route.request();
    const url = new URL(request.url());
    const read = ['GET', 'HEAD', 'OPTIONS'].includes(request.method());
    const sessionWrite = request.method() === 'POST' && url.origin === origin && ['/auth/development', '/auth/logout'].includes(url.pathname);
    if (!read && !sessionWrite) {
      result.blockedMutations++;
      return route.abort('blockedbyclient');
    }
    return route.continue();
  });
  const page = await context.newPage();
  page.on('pageerror', () => { result.pageErrors++; });
  try {
    step = role + ':login';
    await page.goto('/');
    await page.getByRole('button', { name: label }).click();
    await expect(page.getByRole('button', { name: '로그아웃', exact: true })).toBeVisible();
    loggedIn = true;
    const me = await get(context, '/api/v1/me');
    requireThat(me.admin === admin, 'unexpected_session_role');
    await expect(page.getByRole('heading', { name: '테스트 앱', exact: true })).toBeVisible();
    passed(role + '_home');

    step = role + ':installation_consistency';
    const normal = await get(context, '/api/v1/installations');
    requireThat(Array.isArray(normal), 'default_installations_shape');
    const paged = await pages(context, 'installations');
    requireThat(sameIds(normal, paged), 'default_and_paged_installations_differ');
    const apps = await pages(context, 'apps');
    for (const app of apps) {
      const detail = await get(context, '/api/v1/apps/' + encodeURIComponent(app.id));
      requireThat(Array.isArray(detail.installations) && sameIds(detail.installations, normal.filter((item) => item.appId === app.id)), 'app_detail_installations_differ');
    }
    passed(role + '_default_paged_and_app_detail_installations', { installationCount: normal.length, appCount: apps.length });

    if (admin) {
      adminApps = apps;
      step = 'administrator:managed_installations_api';
      const managed = await get(context, '/api/v1/runtime/installations');
      const counts = checkManaged(managed, normal);
      passed('administrator_managed_runtime_installations_api', counts);

      step = 'administrator:existing_admin_routes';
      const groups = await get(context, '/api/v1/groups');
      const grants = await get(context, '/api/v1/group-grants');
      const audit = await get(context, '/api/v1/audit?limit=2');
      requireThat(Array.isArray(groups) && Array.isArray(grants) && Array.isArray(audit.entries), 'admin_api_shape');
      passed('administrator_groups_group_grants_and_audit_get', { groupCount: groups.length, groupGrantCount: grants.length });

      step = 'administrator:management_screen';
      const screenList = page.waitForResponse(isManagedPath);
      await page.goto('/admin');
      await expect(page.getByRole('heading', { name: '플랫폼 관리', exact: true })).toBeVisible();
      const screenResponse = await screenList;
      requireThat(screenResponse.status() === 200, 'admin_screen_managed_list_failed');
      const shown = await screenResponse.json();
      checkManaged(shown, normal);
      await expect(panel(page, '그룹')).toBeVisible();
      await expect(panel(page, '앱 접근')).toBeVisible();
      const auditPanel = panel(page, '감사 기록');
      await expect(auditPanel).toBeVisible();
      await auditPanel.getByLabel('동작').selectOption('grant.');
      const auditResponse = page.waitForResponse((response) => {
        const url = new URL(response.url());
        return url.pathname === '/api/v1/audit' && url.searchParams.get('action') === 'grant.';
      });
      await auditPanel.getByRole('button', { name: '조회', exact: true }).click();
      requireThat((await auditResponse).status() === 200, 'audit_filter_failed');
      passed('administrator_management_sections_and_personal_grant_audit_filter');

      step = 'administrator:runtime_panel';
      await expect(page.getByRole('heading', { name: '앱 실행 상태', exact: true })).toBeVisible();
      const dockerApps = shown.filter((item) => item.kind === 'docker_app');
      const titles = new Map();
      for (const item of dockerApps) titles.set(`${item.name} · ${item.environment}`, [...(titles.get(`${item.name} · ${item.environment}`) || []), item]);
      for (const [title, items] of titles) {
        const panels = panel(page, title);
        await expect(panels).toHaveCount(items.length);
        await expect(panels.getByRole('link', { name: '앱 열기', exact: true })).toHaveCount(items.filter((item) => item.canOpen).length);
        await expect(panels.getByText('회사 사용 중지', { exact: true })).toHaveCount(items.filter((item) => item.appDisabled).length);
      }
      passed('administrator_runtime_panel_uses_managed_list', { runtimePanels: dockerApps.length, openLinks: dockerApps.filter((item) => item.canOpen).length, disabledBadges: dockerApps.filter((item) => item.appDisabled).length });

      step = 'administrator:app_access';
      const appPanel = panel(page, '앱 접근');
      for (const app of apps.slice(0, 2)) {
        const accessResponse = page.waitForResponse((response) => new URL(response.url()).pathname === `/api/v1/apps/${app.id}/access`);
        await appPanel.getByRole('combobox', { name: '앱', exact: true }).selectOption(app.id);
        const response = await accessResponse;
        requireThat(response.status() === 200, 'app_access_read_failed');
        const access = await response.json();
        requireThat(access.appId === app.id, 'app_access_identity_mismatch');
        await expect(appPanel.getByLabel('운영 입장 대상')).toHaveValue(access.admission.prod);
        await expect(appPanel.getByLabel('테스트 입장 대상')).toHaveValue(access.admission.dev);
        await expect(appPanel.getByRole('combobox', { name: '그룹', exact: true }).locator('option')).toHaveCount(groups.filter((group) => group.active).length);
      }
      if (apps.length >= 2) passed('administrator_switches_two_app_access_details');
      else notRun('administrator_switches_two_app_access_details', 'fewer_than_two_registered_apps');
      if (groups.length) {
        const membersResponse = page.waitForResponse((response) => /\/api\/v1\/groups\/[^/]+\/members$/.test(new URL(response.url()).pathname));
        await panel(page, '그룹').getByRole('button', { name: '구성원', exact: true }).first().click();
        requireThat((await membersResponse).status() === 200, 'group_members_read_failed');
        await expect(panel(page, '그룹').getByRole('region')).toBeVisible();
        passed('administrator_group_members_read_only');
      } else notRun('administrator_group_members_read_only', 'no_existing_groups');
    } else {
      step = 'regular_user:admin_denial';
      for (const path of ['/api/v1/runtime/installations', '/api/v1/groups', '/api/v1/group-grants', '/api/v1/audit?limit=1']) await get(context, path, 403);
      if (adminApps.length) await get(context, `/api/v1/apps/${encodeURIComponent(adminApps[0].id)}/access`, 403);
      await page.goto('/admin');
      await expect(page.getByRole('heading', { name: '관리자 권한이 필요합니다', exact: true })).toBeVisible();
      passed('regular_user_admin_api_and_screen_denied', { deniedRoutes: adminApps.length ? 5 : 4 });
    }

    step = role + ':existing_app_render';
    const app = normal.find((item) => item.kind === 'docker_app' && item.runtimeStatus === 'running');
    if (app) {
      await page.goto(`/environments/${app.environment}/apps/${encodeURIComponent(app.appId)}`);
      const container = page.locator('.miy-app-root');
      await expect(container).toBeVisible();
      await expect.poll(() => container.evaluate((element) => element.children.length > 0 || !!element.textContent.trim())).toBe(true);
      await expect(page.getByRole('heading', { name: '앱을 열 수 없습니다', exact: true })).toHaveCount(0);
      passed(role + '_existing_app_render');
    } else notRun(role + '_existing_app_render', 'no_visible_running_app');
  } finally {
    try {
      const response = await context.request.get(origin + '/api/v1/me', { timeout: 15000, maxRedirects: 0 });
      if (response.status() === 200) {
        const me = await response.json();
        requireThat(typeof me.csrf === 'string' && me.csrf.length > 0, 'cleanup_csrf_missing');
        const loggedOut = await context.request.post(origin + '/auth/logout', { timeout: 15000, maxRedirects: 0, headers: { origin, 'x-miy-csrf': me.csrf } });
        requireThat(loggedOut.status() === 200, 'session_logout_failed');
        requireThat((await context.request.get(origin + '/api/v1/me')).status() === 401, 'session_logout_unconfirmed');
        passed(role + '_owned_session_logout');
      } else requireThat(response.status() === 401 && !loggedIn, 'session_cleanup_status');
    } catch (error) { result.cleanupErrors.push({ role, code: error.safeCode || 'session_cleanup_failed' }); }
    await context.close();
  }
}

try {
  browser = await chromium.launch({ headless: true });
  step = 'deployed_index';
  const context = await browser.newContext();
  try {
    const response = await context.request.get(origin + '/', { timeout: 20000 });
    requireThat(response.status() === 200, 'public_index_unavailable');
    const hash = createHash('sha256').update(await response.body()).digest('hex');
    requireThat(hash === manifest.candidateIndexSha256, 'public_index_is_not_candidate');
    passed('public_index_matches_candidate', { indexSha256: hash });
  } finally { await context.close(); }
  await session(/관리자로 시작/, true);
  await session(/사용자 A/, false);
  requireThat(result.pageErrors === 0, 'browser_page_errors');
  requireThat(result.blockedMutations === 0, 'unexpected_domain_mutation_attempt');
  requireThat(result.cleanupErrors.length === 0, 'session_cleanup_failed');
  result.result = 'PASS';
} catch (error) {
  result.failure = { step, code: error.safeCode || 'browser_check_failed' };
  process.exitCode = 1;
} finally {
  await browser?.close();
  await writeFile(join(output, 'result.json'), JSON.stringify(result, null, 2) + '\n', { mode: 0o600 });
  console.log(JSON.stringify(result));
}
