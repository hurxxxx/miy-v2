import type { DocsSpaceLocation } from './api/docs-api';
import { appendDocPageQuery } from './api/docs-url-state';
import { TRASH_PATH } from '../../platform/runtime/app-location';

export const DOCS_SIDEBAR_SECTION_IDS = ['favorites', 'recentPages'] as const;

export type DocsSidebarSectionId = (typeof DOCS_SIDEBAR_SECTION_IDS)[number];

export function getInitialDocsSidebarExpandedSections(): DocsSidebarSectionId[] {
  return [...DOCS_SIDEBAR_SECTION_IDS];
}

export function isDocsSidebarSectionExpanded(
  expandedSections: DocsSidebarSectionId[],
  sectionId: DocsSidebarSectionId,
): boolean {
  return expandedSections.includes(sectionId);
}

export function toggleDocsSidebarSection(
  expandedSections: DocsSidebarSectionId[],
  sectionId: DocsSidebarSectionId,
): DocsSidebarSectionId[] {
  return isDocsSidebarSectionExpanded(expandedSections, sectionId)
    ? expandedSections.filter(
        (expandedSectionId) => expandedSectionId !== sectionId,
      )
    : [...expandedSections, sectionId];
}

/**
 * miy: a doc's route is `/<docId>` under the app base path, with its space in
 * `?space=` (AI-DO linked `/w/<slug>/docs/<docId>` or the docs app route of
 * the holding space, `buildDocsSpaceDocPath`). The global space needs no
 * parameter. The selected page follows in `?page=`.
 */
export function buildDocsDocPath({
  docId,
  pageId,
  spaceId,
}: {
  docId: string;
  pageId?: string | null;
  spaceId?: string | null;
}): string {
  const base = `/${encodeURIComponent(docId)}`;
  const path =
    spaceId && spaceId !== 'global'
      ? `${base}?space=${encodeURIComponent(spaceId)}`
      : base;
  return appendDocPageQuery(path, pageId);
}

/** Sidebar rows link into the space that holds the doc. */
export function resolveDocsSidebarRowPath({
  docId,
  pageId,
  space,
  currentSpaceId,
}: {
  docId: string;
  pageId?: string | null;
  space?: DocsSpaceLocation | null;
  currentSpaceId?: string | null;
}): string {
  return buildDocsDocPath({
    docId,
    pageId,
    spaceId: space?.key ?? currentSpaceId ?? null,
  });
}

/** The other space's name, or null for the space currently shown. */
export function docsSidebarRowSpaceLabel(
  space: DocsSpaceLocation | null | undefined,
  currentSpaceKey: string | null | undefined,
): string | null {
  return space && space.key !== currentSpaceKey ? space.name : null;
}

/** The space's trash (miy 보강); the global space needs no parameter. */
export function docsTrashPath(spaceId: string | null): string {
  return spaceId && spaceId !== 'global'
    ? `${TRASH_PATH}?space=${encodeURIComponent(spaceId)}`
    : TRASH_PATH;
}
