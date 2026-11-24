import { useCallback, useEffect, useMemo, useRef, useState, type ReactNode } from 'react';
import { useTranslation } from 'react-i18next';
import {
  Copy,
  ExternalLink,
  FileText,
  Globe,
  History,
  LayoutTemplate,
  Link2,
  Loader2,
  Lock,
  Maximize2,
  MoreHorizontal,
  Pencil,
  Plus,
  Printer,
  Search,
  Share2,
  Star,
  Trash2,
  X,
} from 'lucide-react';
import { DropdownMenu, useConfirm, useFeedback, usePrompt } from '@miy/ui';
import {
  DndContext,
  DragOverlay,
  KeyboardSensor,
  PointerSensor,
  closestCenter,
  useSensor,
  useSensors,
  type DragEndEvent,
  type DragOverEvent,
  type DragStartEvent,
} from '@dnd-kit/core';
import {
  SortableContext,
  sortableKeyboardCoordinates,
  verticalListSortingStrategy,
} from '@dnd-kit/sortable';

import {
  blockContentToMarkdown,
  markdownToBlockContent,
  type BlockContent,
} from '../../../components/editor';
import { cn } from '../../../lib/utils';
import { useDocsApp } from '../../../platform/docs/docs-context';
import { localizeResourceLocationLabel } from '../../../platform/i18n/resource-location-label';
import { useMediaUpload } from '../../../platform/media/use-media-upload';
import {
  useAppNavigate,
  useAppSearchParams,
} from '../../../platform/runtime/app-location';
import { useNavigationDirty } from '../../../platform/runtime/navigation-state';
import { useRuntime } from '../../../platform/runtime/runtime-context';
import {
  createDocPage,
  createDocsTemplate,
  createNativeDoc,
  deleteDocPage,
  deleteDocTarget,
  deleteDocUserShare,
  deleteDocsItem,
  duplicateDocsItem,
  getDocPage,
  getDocSharing,
  getDocsItem,
  isDocsRevisionConflict,
  listDocPages,
  listShareableUsers,
  recordDocView,
  resolveDocsContentFormat,
  restoreDocPageVersion,
  resolveDocsHubContentFormat,
  toggleDocFavorite,
  updateDocPage,
  updateDocTarget,
  updateDocsItem,
  upsertDocUserShare,
  type DocsHubItem,
  type DocsPageItem,
  type DocsPageVersion,
  type NativeDocSharingResponse,
  type ShareableUserItem,
} from '../api/docs-api';
import {
  applyReorder,
  collectDescendantIds,
  computeDropTarget,
  flattenVisibleTree,
  resolveDropZone,
  type DropZone,
} from '../api/docs-page-reorder';
import {
  consumeDocsCreateSearchParam,
  getDocPageIdFromSearchParams,
  getDocsFilterQueryString,
  getExpandedDocPageNodeIds,
  removeLegacyDocsSearchParams,
  withDocPageSearchParam,
} from '../api/docs-url-state';
import { buildDocsDocPath } from '../docs-sidebar-model';
import { docsListPath, refreshDocsSidebar } from '../sidebar';
import { DocsBlockContentSurface } from './DocsBlockContentSurface';
import { DocsBlockMarkdownActions } from './docs-content-renderers';
import { DocsFullscreenReadModal } from './DocsFullscreenReadModal';
import { copyText, portalUrlFor } from './docs-link';
import { downloadMarkdownFile } from './docs-markdown-file';
import { DocsPageAttachments } from './DocsPageAttachments';
import { DocsPageComments } from './DocsPageComments';
import { clearCommentDrafts } from './docs-comment-drafts';
import { DocsPageVersionsModal } from './DocsPageVersionsModal';
import { DocsTemplatePicker } from './DocsTemplatePicker';
import {
  CATEGORY_LABEL_KEYS,
  DocsContentFormatBadge,
  DocsPageTreeNode,
  LocationPicker,
  VIEW_LABEL_KEYS,
  applyUpdatedDocsPage,
  buildDocsPagesById,
  bumpDocsContentEditorVersion,
  countDocsPageChildren,
  docsAuthorInitials,
  isDocsTextUploadTooLarge,
  isDocsViewCategory,
  removeDocsPageSubtree,
  resolveDefaultDocsCreateLocation,
  resolveDocsCreateLocationValue,
  resolveDocsCreatePrimaryTarget,
  resolveDocsPageAuthorName,
  resolveDocsPageSelection,
  sharingLabel,
  summarizeDocsPageCollection,
  timeAgo,
  updateDocsPageContentBlocks,
  type DocsViewCategory,
  type LocationOption,
} from './DocsViewParts';
import { useDocsHubController } from './useDocsHubController';
import { useDocsPageContentSaveController } from './useDocsPageContentSaveController';
import { useDocsViewState } from './useDocsViewState';

/*
 * AI-DO apps/web/src/app-modules/docs/views/DocsView.tsx for miy. The list
 * (hub) and the doc editor keep AI-DO's markup, handlers and strings. What
 * miy changed, all recorded in docs/PORTING.md:
 *
 * - routing: `runtime.location`/`runtime.navigate` instead of react-router;
 *   the space is `?space=` (AI-DO's workspace slug), the page `?page=`;
 * - API: `runtime.api`/`runtime.fileApi` instead of the bearer token;
 * - single-editor saving with revisions instead of the Yjs collaborative
 *   editor and its realtime page-tree events (conflict notice in
 *   DocsBlockContentSurface);
 * - excluded: HTML pages, link shares, PMS task links, the AI button;
 * - links: the portal URL (docs-link.ts) and a clipboard fallback, because
 *   the frame has its own origin and no clipboard delegation;
 * - unsaved drafts and in-flight saves are reported with
 *   `runtime.setNavigationState`; a late answer for another doc or page never
 *   replaces the one shown.
 */

type DocsVisibleTreeNode = ReturnType<typeof flattenVisibleTree>[number];
type NativeDocShareUser = NativeDocSharingResponse['users'][number];

const useCloseOnOutsidePointer = (
  ref: React.RefObject<HTMLElement | null>,
  enabled: boolean,
  onClose: () => void,
) => {
  useEffect(() => {
    if (!enabled) return;
    function onClick(e: MouseEvent) {
      if (ref.current && !ref.current.contains(e.target as Node)) {
        onClose();
      }
    }
    document.addEventListener('mousedown', onClick);
    return () => document.removeEventListener('mousedown', onClick);
  }, [enabled, onClose, ref]);
};

function handleBlockMarkdownExport(page: DocsPageItem) {
  const markdown = blockContentToMarkdown(
    (page.content_blocks ?? []) as BlockContent,
  ).trimEnd();
  downloadMarkdownFile(page.title, markdown ? `${markdown}\n` : '');
}

// eslint-disable-next-line @typescript-eslint/no-explicit-any
const renderDocsEditor = (props: any) => {
  const {
    activeContentFormat,
    activeDragId,
    activePage,
    activePageAuthorInitials,
    activePageAuthorName,
    activePageUploadFile,
    childCounts,
    contentEditorVersions,
    copiedDocId,
    dndSensors,
    docMenuOpen,
    docMenuRef,
    dropIndicator,
    editorLoading,
    expandedNodes,
    handleAddPage,
    handleBack,
    handleBlockMarkdownImportFile,
    handleCopyLink,
    handleDeleteDoc,
    handleDeletePage,
    handleDragEnd,
    handleDragOver,
    handleDragPointerMove,
    handleDragStart,
    handleDuplicateDoc,
    handleEditorChange,
    handleLivePageChange,
    handleKeepMine,
    handleOpenInNewTab,
    handleOpenVersions,
    handlePageTitleSave,
    handlePrintDoc,
    handleReloadConflict,
    handleRenameDoc,
    handleRetrySave,
    handleSaveAsTemplate,
    handleToggleFavorite,
    highlightCommentId,
    loadError,
    locale,
    openShareModal,
    pageBodyPlaceholder,
    pages,
    pagesById,
    resetDragState,
    resolveFileUrl,
    saveState,
    selectDocPage,
    selectedDoc,
    selectedPageId,
    setDocMenuOpen,
    setReadModeOpen,
    t,
    timeZone,
    toggleExpand,
    visibleTree,
  } = props;
  if (editorLoading && !selectedDoc) {
    return (
      <div className="flex-1 flex items-center justify-center bg-app-bg">
        <Loader2 size={28} className="animate-spin text-app-accent" />
      </div>
    );
  }

  if (!selectedDoc) {
    return (
      <div className="flex-1 flex items-center justify-center bg-app-bg text-app-ink/55">
        <div className="text-center">
          <FileText size={48} className="mx-auto mb-4 opacity-20" />
          <h2 className="app-text-title-md text-app-ink">
            {loadError ?? t('apps:docs.documentNotFound')}
          </h2>
          <button
            type="button"
            onClick={handleBack}
            className="app-text-control mt-4 text-app-accent hover:underline"
          >
            {t('apps:docs.goBack')}
          </button>
        </div>
      </div>
    );
  }

  return (
    <div className="h-full flex flex-col bg-app-bg overflow-hidden">
      <div className="h-12 border-b border-app-border flex items-center justify-between px-4 bg-app-surface-sidebar">
        <div className="app-text-caption flex items-center gap-2 min-w-0">
          <button
            type="button"
            className="text-app-ink/55 hover:text-app-ink"
            onClick={handleBack}
          >
            {t('apps:docs.docs')}
          </button>
          <span className="text-app-ink/70">/</span>
          <span className="truncate text-app-ink">{selectedDoc.title}</span>
          <DocsContentFormatBadge
            format={resolveDocsHubContentFormat(selectedDoc)}
            label={t(
              `apps:docs.contentFormat.${resolveDocsHubContentFormat(selectedDoc)}`,
            )}
            compact
          />
          <Star
            size={12}
            className={cn(
              'text-app-ink/70 cursor-pointer',
              selectedDoc.is_favorite && 'text-yellow-500 fill-yellow-500',
            )}
            onClick={(event) => void handleToggleFavorite(event, selectedDoc)}
          />
        </div>

        <div className="flex items-center gap-2">
          {selectedDoc.can_share ? (
            <button
              type="button"
              onClick={() => void openShareModal()}
              className="app-text-control-sm flex items-center gap-1.5 rounded-md px-3 py-1.5 text-app-ink transition-colors hover:bg-app-surface-hover"
            >
              <Share2 size={14} />
              <span>{t('common:actions.share')}</span>
            </button>
          ) : null}
          <button
            type="button"
            onClick={() => setReadModeOpen(true)}
            disabled={!activePage}
            className="app-text-control-sm flex items-center gap-1.5 rounded-md px-3 py-1.5 text-app-ink transition-colors hover:bg-app-surface-hover disabled:cursor-not-allowed disabled:opacity-40"
          >
            <Maximize2 size={14} />
            <span>{t('apps:docs.readMode.open')}</span>
          </button>
          <div ref={docMenuRef} className="relative">
            <button
              type="button"
              onClick={() => setDocMenuOpen((o: boolean) => !o)}
              className="rounded p-1.5 text-app-ink/55 transition-colors hover:bg-app-surface-hover hover:text-app-ink"
              title={t('apps:docs.moreActions')}
            >
              <MoreHorizontal size={18} />
            </button>
            {docMenuOpen ? (
              <div className="absolute right-0 top-full mt-1 z-30 w-52 rounded-lg border border-app-border bg-app-surface-sidebar py-1 shadow-xl">
                <button
                  type="button"
                  onClick={() => void handleCopyLink(selectedDoc)}
                  className="app-text-control-sm flex w-full items-center gap-2 px-3 py-2 text-left text-app-ink hover:bg-app-surface-hover"
                >
                  <Link2 size={14} />
                  <span>
                    {copiedDocId === selectedDoc.id
                      ? t('apps:docs.copied')
                      : t('apps:docs.copyLink')}
                  </span>
                </button>
                <button
                  type="button"
                  onClick={() => handleOpenInNewTab(selectedDoc)}
                  className="app-text-control-sm flex w-full items-center gap-2 px-3 py-2 text-left text-app-ink hover:bg-app-surface-hover"
                >
                  <ExternalLink size={14} />
                  <span>{t('apps:docs.openInNewTab')}</span>
                </button>
                <button
                  type="button"
                  onClick={() => void handleDuplicateDoc(selectedDoc)}
                  className="app-text-control-sm flex w-full items-center gap-2 px-3 py-2 text-left text-app-ink hover:bg-app-surface-hover"
                >
                  <Copy size={14} />
                  <span>{t('apps:docs.duplicate')}</span>
                </button>
                <button
                  type="button"
                  onClick={handlePrintDoc}
                  disabled={Boolean(pageBodyPlaceholder)}
                  className="app-text-control-sm flex w-full items-center gap-2 px-3 py-2 text-left text-app-ink hover:bg-app-surface-hover disabled:opacity-50"
                >
                  <Printer size={14} />
                  <span>{t('apps:docs.print')}</span>
                </button>
                {activePage ? (
                  <>
                    <div className="my-1 h-px bg-app-border" />
                    <button
                      type="button"
                      onClick={() => {
                        setDocMenuOpen(false);
                        handleOpenVersions(activePage);
                      }}
                      className="app-text-control-sm flex w-full items-center gap-2 px-3 py-2 text-left text-app-ink hover:bg-app-surface-hover"
                    >
                      <History size={14} />
                      <span>{t('apps:docsApp.versions.open')}</span>
                    </button>
                    <button
                      type="button"
                      onClick={() => {
                        setDocMenuOpen(false);
                        void handleSaveAsTemplate(activePage);
                      }}
                      className="app-text-control-sm flex w-full items-center gap-2 px-3 py-2 text-left text-app-ink hover:bg-app-surface-hover"
                    >
                      <LayoutTemplate size={14} />
                      <span>{t('apps:docsApp.templates.saveAsTemplate')}</span>
                    </button>
                  </>
                ) : null}
                {selectedDoc.can_manage ? (
                  <>
                    <div className="my-1 h-px bg-app-border" />
                    <button
                      type="button"
                      onClick={() => {
                        setDocMenuOpen(false);
                        void handleRenameDoc(selectedDoc);
                      }}
                      className="app-text-control-sm flex w-full items-center gap-2 px-3 py-2 text-left text-app-ink hover:bg-app-surface-hover"
                    >
                      <Pencil size={14} />
                      <span>{t('common:actions.rename')}</span>
                    </button>
                    <button
                      type="button"
                      onClick={() => {
                        setDocMenuOpen(false);
                        void handleDeleteDoc(selectedDoc);
                      }}
                      className="app-text-control-sm flex w-full items-center gap-2 px-3 py-2 text-left text-app-danger-text hover:bg-app-surface-hover"
                    >
                      <Trash2 size={14} />
                      <span>{t('common:actions.delete')}</span>
                    </button>
                  </>
                ) : null}
              </div>
            ) : null}
          </div>
          <button
            type="button"
            onClick={handleBack}
            aria-label={t('apps:docs.goBack')}
            className="p-1.5 rounded text-app-ink/55 transition-colors hover:bg-app-surface-hover hover:text-app-danger"
          >
            <X size={20} />
          </button>
        </div>
      </div>

      <div className="flex-1 flex overflow-hidden">
        <div className="w-64 border-r border-app-border bg-app-surface-sidebar flex flex-col overflow-hidden">
          <div className="p-4 space-y-4">
            <div className="space-y-1">
              <h2 className="app-text-title-sm text-app-ink truncate">
                {selectedDoc.title}
              </h2>
              <div className="flex flex-wrap items-center gap-2">
                <DocsContentFormatBadge
                  format={resolveDocsHubContentFormat(selectedDoc)}
                  label={t(
                    `apps:docs.contentFormat.${resolveDocsHubContentFormat(selectedDoc)}`,
                  )}
                  compact
                />
                <p className="app-text-caption min-w-0 truncate text-app-ink/55">
                  {localizeResourceLocationLabel(selectedDoc.location_label, t)}
                </p>
              </div>
            </div>

            <div>
              <div className="flex items-center justify-between px-2 mb-2">
                <span className="app-text-overline text-app-ink/55">
                  {t('apps:docs.pages')}
                </span>
              </div>
              <DndContext
                sensors={dndSensors}
                collisionDetection={closestCenter}
                onDragStart={handleDragStart}
                onDragOver={handleDragOver}
                onDragEnd={handleDragEnd}
                onDragCancel={resetDragState}
              >
                <SortableContext
                  items={visibleTree.map(
                    (node: DocsVisibleTreeNode) => node.id,
                  )}
                  strategy={verticalListSortingStrategy}
                >
                  <div
                    className="space-y-0.5 overflow-y-auto custom-scrollbar max-h-[calc(100vh-250px)]"
                    onPointerMove={handleDragPointerMove}
                  >
                    {visibleTree.map((node: DocsVisibleTreeNode) => {
                      const page = pagesById.get(node.id);
                      if (!page) return null;
                      const zone =
                        dropIndicator && dropIndicator.overId === node.id
                          ? dropIndicator.zone
                          : null;
                      return (
                        <DocsPageTreeNode
                          key={node.id}
                          page={page}
                          contentFormat={resolveDocsContentFormat(page)}
                          depth={node.depth}
                          hasChildren={(childCounts.get(node.id) ?? 0) > 0}
                          isExpanded={expandedNodes.has(node.id)}
                          isSelected={selectedPageId === node.id}
                          dropZone={zone}
                          canDrag={Boolean(
                            selectedDoc.can_edit && page.can_edit,
                          )}
                          canDelete={Boolean(page.can_edit)}
                          onSelect={selectDocPage}
                          onToggleExpand={toggleExpand}
                          onDelete={handleDeletePage}
                        />
                      );
                    })}
                    {selectedDoc.can_edit ? (
                      <button
                        type="button"
                        onClick={() => void handleAddPage(null)}
                        className="app-text-body-sm flex w-full items-center gap-2 rounded px-3 py-1.5 text-app-ink/55 transition-all hover:bg-app-surface-hover hover:text-app-accent"
                      >
                        <Plus size={14} />
                        <span>{t('apps:docs.addPage')}</span>
                      </button>
                    ) : null}
                  </div>
                </SortableContext>
                <DragOverlay dropAnimation={null}>
                  {activeDragId ? (
                    <div className="app-text-body-sm flex items-center gap-1 rounded bg-app-surface-sidebar px-2 py-1.5 text-app-ink shadow-lg ring-1 ring-app-accent/40">
                      <FileText size={14} className="text-app-accent" />
                      <span className="truncate">
                        {pagesById.get(activeDragId)?.title ?? ''}
                      </span>
                    </div>
                  ) : null}
                </DragOverlay>
              </DndContext>
            </div>
          </div>

          <div className="mt-auto p-4 border-t border-app-border">
            <div className="app-text-body-sm flex items-center gap-2 rounded px-3 py-1.5 text-app-ink/55">
              {selectedDoc.is_private ? <Lock size={14} /> : <Globe size={14} />}
              <span>{sharingLabel(selectedDoc, t)}</span>
            </div>
          </div>
        </div>

        <div className="docs-print-area flex-1 flex flex-col min-w-0 bg-white dark:bg-app-surface overflow-y-auto custom-scrollbar">
          <div
            data-testid="docs-content-canvas"
            className="w-full px-8 py-12 2xl:px-12 max-[760px]:px-5 max-[760px]:py-6"
          >
            {activePage ? (
              <div className="space-y-6">
                <div className="space-y-4">
                  {selectedDoc.can_edit && activePage.can_edit ? (
                    <input
                      key={activePage.id}
                      type="text"
                      aria-label={t('planner.title')}
                      defaultValue={activePage.title}
                      placeholder={t('apps:docs.untitled')}
                      maxLength={200}
                      className="app-text-title-xl w-full bg-transparent text-app-ink placeholder:text-app-ink/45 focus:outline-none"
                      onBlur={(event) =>
                        void handlePageTitleSave(
                          activePage.id,
                          event.target.value,
                        )
                      }
                      onKeyDown={(event) => {
                        if (event.nativeEvent.isComposing) return;
                        if (event.key === 'Enter') {
                          event.preventDefault();
                          event.currentTarget.blur();
                        }
                        if (event.key === 'Escape') {
                          event.currentTarget.value = activePage.title;
                          event.currentTarget.blur();
                        }
                      }}
                    />
                  ) : (
                    <h1 className="app-text-title-xl text-app-ink">
                      {activePage.title}
                    </h1>
                  )}
                  <div className="app-text-caption flex items-center gap-3 text-app-ink/55">
                    <div className="flex items-center gap-1.5">
                      <div className="app-text-micro flex size-5 items-center justify-center rounded-full bg-app-accent font-bold text-app-accent-fg">
                        {activePageAuthorInitials}
                      </div>
                      <span className="app-text-control text-app-ink">
                        {activePageAuthorName}
                      </span>
                    </div>
                    <span>·</span>
                    <span>
                      {localizeResourceLocationLabel(
                        selectedDoc.location_label,
                        t,
                      )}
                    </span>
                    <span>·</span>
                    <DocsContentFormatBadge
                      format={activeContentFormat}
                      label={t(
                        `apps:docs.contentFormat.${activeContentFormat}`,
                      )}
                      compact
                    />
                    <span>·</span>
                    <span>
                      {t('apps:docs.updated', {
                        date: timeAgo(activePage.updated_at, timeZone, locale),
                      })}
                    </span>
                  </div>
                </div>
                {pageBodyPlaceholder ?? (
                  <div className="prose dark:prose-invert max-w-none pt-4">
                    <DocsBlockMarkdownActions
                      canImport={selectedDoc.can_edit && activePage.can_edit}
                      exportLabel={t('apps:docs.blockMarkdown.export')}
                      importLabel={t('apps:docs.blockMarkdown.import')}
                      onExport={() => handleBlockMarkdownExport(activePage)}
                      onImportFile={(file) =>
                        void handleBlockMarkdownImportFile(activePage, file)
                      }
                    />
                    <DocsBlockContentSurface
                      page={activePage}
                      canEdit={selectedDoc.can_edit && activePage.can_edit}
                      contentEditorVersion={
                        contentEditorVersions[activePage.id] ?? 0
                      }
                      uploadFile={activePageUploadFile}
                      resolveFileUrl={resolveFileUrl}
                      saveState={saveState}
                      onBlocksChange={handleEditorChange}
                      onLiveChange={handleLivePageChange}
                      onReloadConflict={handleReloadConflict}
                      onKeepMine={handleKeepMine}
                      onRetrySave={handleRetrySave}
                    />
                  </div>
                )}
                <DocsPageAttachments
                  key={activePage.source_page_id}
                  canEdit={selectedDoc.can_edit && activePage.can_edit}
                  pageId={activePage.source_page_id}
                />
                <DocsPageComments
                  key={`comments:${activePage.source_page_id}`}
                  docId={selectedDoc.id}
                  pageId={activePage.source_page_id}
                  highlightCommentId={highlightCommentId}
                />
              </div>
            ) : (
              <div className="text-center py-20">
                <FileText
                  size={48}
                  className="mx-auto mb-4 text-app-ink/70 opacity-20"
                />
                <p className="app-text-body text-app-ink/55 mb-4">
                  {t('apps:docs.noPages')}
                </p>
                {selectedDoc.can_edit ? (
                  <button
                    type="button"
                    onClick={() => void handleAddPage(null)}
                    className="app-text-control rounded-md bg-app-accent px-4 py-2 text-app-accent-fg hover:opacity-90"
                  >
                    {t('apps:docs.addFirstPage')}
                  </button>
                ) : null}
              </div>
            )}
          </div>
        </div>
      </div>
    </div>
  );
};

// eslint-disable-next-line @typescript-eslint/no-explicit-any
const renderDocsShell = (props: any) => {
  const {
    activeCategoryLabel,
    changingLocation,
    confirmDialog,
    copyDirectLink,
    creating,
    creatingPage,
    docUrlFor,
    docs,
    editorView,
    fetchDocs,
    handleChangeLocation,
    handleChangeUserAccess,
    handleCloseVersions,
    handleCreateDoc,
    handleCreatePage,
    handleRestoreVersion,
    handleDeleteDoc,
    handleInviteUser,
    handleRemoveUserShare,
    handleRenameDoc,
    handleSearchChange,
    handleToggleFavorite,
    hasActiveRefinement,
    inviteFocused,
    inviteQuery,
    inviteSuggestions,
    isListView,
    linkCopied,
    listLoadFailed,
    loadingList,
    locale,
    locationOptions,
    menuOpenId,
    newDocLocation,
    newDocTemplateId,
    newDocTitle,
    newPageTemplateId,
    newPageTitle,
    openCreateModal,
    openDoc,
    page,
    pageBodyPlaceholder,
    pageCount,
    pages,
    promptDialog,
    readModeOpen,
    resetDocsFilters,
    resolveFileUrl,
    resolveLocationValueFromTarget,
    searchOpen,
    searchQuery,
    selectDocPage,
    selectedDoc,
    selectedPageId,
    setInviteFocused,
    setInviteQuery,
    setMenuOpenId,
    setNewDocLocation,
    setNewDocTemplateId,
    setNewDocTitle,
    setNewPageTemplateId,
    setNewPageTitle,
    setPage,
    setReadModeOpen,
    setSearchOpen,
    setShareAccessLevel,
    setShowCreateModal,
    setShowCreatePageModal,
    setShowShareModal,
    setSortBy,
    setSortDir,
    shareAccessLevel,
    shareLoading,
    sharingState,
    showCreateModal,
    showCreatePageModal,
    showShareModal,
    sortBy,
    sortDir,
    t,
    templateSpaceId,
    docTemplateSpaceId,
    timeZone,
    total,
    versionsPage,
  } = props;
  return (
    <div className="h-full w-full flex flex-col min-w-0 bg-app-bg overflow-hidden relative">
      {confirmDialog}
      {promptDialog}
      {readModeOpen && selectedDoc ? (
        <DocsFullscreenReadModal
          doc={selectedDoc}
          pages={pages}
          selectedPageId={selectedPageId}
          pageBodyPlaceholder={pageBodyPlaceholder}
          onSelectPage={selectDocPage}
          onClose={() => setReadModeOpen(false)}
          resolveFileUrl={resolveFileUrl}
        />
      ) : null}

      {versionsPage && selectedDoc ? (
        <DocsPageVersionsModal
          key={versionsPage.id}
          page={versionsPage}
          canRestore={Boolean(selectedDoc.can_edit && versionsPage.can_edit)}
          resolveFileUrl={resolveFileUrl}
          onRestore={(version: DocsPageVersion) =>
            handleRestoreVersion(versionsPage, version)
          }
          onClose={handleCloseVersions}
        />
      ) : null}

      {showCreateModal ? (
        <div className="fixed inset-0 z-[100] flex items-center justify-center">
          <button
            type="button"
            aria-label={t('common:actions.cancel')}
            className="absolute inset-0 bg-black/50"
            onClick={() => setShowCreateModal(false)}
          />
          <div className="relative w-full max-w-2xl rounded-lg border border-app-border bg-app-surface-sidebar p-6 space-y-4">
            <h2 className="app-text-title-md text-app-ink">
              {t('docs.newDoc')}
            </h2>
            <div>
              <label className="app-text-caption text-app-ink/55 mb-1 block">
                {t('planner.title')}
              </label>
              <input
                type="text"
                aria-label={t('planner.title')}
                value={newDocTitle}
                maxLength={200}
                onChange={(event) => setNewDocTitle(event.target.value)}
                placeholder={t('docs.documentTitlePlaceholder')}
                className="app-text-body w-full rounded-md border border-app-border bg-app-bg px-3 py-2 text-app-ink focus:border-app-accent focus:outline-none"
                onKeyDown={(event) => {
                  if (event.key === 'Enter' && !event.nativeEvent.isComposing) {
                    void handleCreateDoc();
                  }
                }}
              />
            </div>
            <div>
              <label className="app-text-caption text-app-ink/55 mb-1.5 block">
                {t('ai.search.metadataLocation')}
              </label>
              <LocationPicker
                value={newDocLocation}
                onChange={setNewDocLocation}
                options={locationOptions}
              />
            </div>
            <DocsTemplatePicker
              spaceId={templateSpaceId}
              value={newDocTemplateId}
              onChange={setNewDocTemplateId}
            />
            <div className="flex justify-end gap-2 pt-2">
              <button
                type="button"
                onClick={() => setShowCreateModal(false)}
                className="app-text-control rounded-md border border-app-border px-4 py-2 text-app-ink hover:bg-app-surface-hover"
              >
                {t('common:actions.cancel')}
              </button>
              <button
                type="button"
                onClick={() => void handleCreateDoc()}
                disabled={creating || !newDocTitle.trim()}
                className="app-text-control rounded-md bg-app-accent px-4 py-2 text-app-accent-fg hover:opacity-90 disabled:opacity-50"
              >
                {creating ? t('pms.creating') : t('common:actions.create')}
              </button>
            </div>
          </div>
        </div>
      ) : null}

      {showCreatePageModal && selectedDoc ? (
        <div className="fixed inset-0 z-[105] flex items-center justify-center">
          <button
            type="button"
            aria-label={t('common:actions.cancel')}
            className="absolute inset-0 bg-black/50"
            onClick={() => setShowCreatePageModal(false)}
          />
          <div className="relative w-full max-w-lg rounded-lg border border-app-border bg-app-surface-sidebar p-6 space-y-4">
            <h2 className="app-text-title-md text-app-ink">
              {t('docs.newPage')}
            </h2>
            <div>
              <label className="app-text-caption text-app-ink/55 mb-1 block">
                {t('planner.title')}
              </label>
              <input
                type="text"
                aria-label={t('planner.title')}
                value={newPageTitle}
                maxLength={200}
                onChange={(event) => setNewPageTitle(event.target.value)}
                placeholder={t('docs.untitled')}
                className="app-text-body w-full rounded-md border border-app-border bg-app-bg px-3 py-2 text-app-ink focus:border-app-accent focus:outline-none"
                onKeyDown={(event) => {
                  if (event.key === 'Enter' && !event.nativeEvent.isComposing) {
                    void handleCreatePage();
                  }
                }}
              />
            </div>
            <DocsTemplatePicker
              spaceId={docTemplateSpaceId}
              value={newPageTemplateId}
              onChange={setNewPageTemplateId}
            />
            <div className="flex justify-end gap-2 pt-2">
              <button
                type="button"
                onClick={() => setShowCreatePageModal(false)}
                className="app-text-control rounded-md border border-app-border px-4 py-2 text-app-ink hover:bg-app-surface-hover"
              >
                {t('common:actions.cancel')}
              </button>
              <button
                type="button"
                onClick={() => void handleCreatePage()}
                disabled={creatingPage || !newPageTitle.trim()}
                className="app-text-control rounded-md bg-app-accent px-4 py-2 text-app-accent-fg hover:opacity-90 disabled:opacity-50"
              >
                {creatingPage ? t('pms.creating') : t('common:actions.create')}
              </button>
            </div>
          </div>
        </div>
      ) : null}

      {showShareModal && selectedDoc ? (
        <div className="fixed inset-0 z-[110] flex items-center justify-center">
          <button
            type="button"
            aria-label={t('common:actions.cancel')}
            className="absolute inset-0 bg-black/50"
            onClick={() => setShowShareModal(false)}
          />
          <div className="relative w-full max-w-2xl rounded-lg border border-app-border bg-app-surface-sidebar p-6 space-y-5">
            <div className="flex items-start justify-between gap-4">
              <div>
                <h2 className="app-text-title-md text-app-ink">
                  {t('docs.share.title')}
                </h2>
                <p className="app-text-caption text-app-ink/55">
                  {selectedDoc.title}
                </p>
              </div>
              <button
                type="button"
                onClick={() => setShowShareModal(false)}
                aria-label={t('common:actions.close')}
                className="rounded p-1.5 text-app-ink/55 hover:bg-app-surface-hover"
              >
                <X size={18} />
              </button>
            </div>

            {shareLoading && !sharingState ? (
              <div className="py-10 flex items-center justify-center">
                <Loader2 size={24} className="animate-spin text-app-accent" />
              </div>
            ) : (
              <>
                {/* 1. Invite by name (top, most common action) */}
                <div className="space-y-3">
                  <div className="relative flex items-center gap-2">
                    <div className="relative flex-1">
                      <Search
                        className="absolute left-3 top-1/2 -translate-y-1/2 text-app-ink/55"
                        size={14}
                      />
                      <input
                        type="text"
                        aria-label={t('docs.share.invitePlaceholder')}
                        value={inviteQuery}
                        onChange={(event) => setInviteQuery(event.target.value)}
                        onFocus={() => setInviteFocused(true)}
                        onBlur={() =>
                          window.setTimeout(() => setInviteFocused(false), 150)
                        }
                        placeholder={t('docs.share.invitePlaceholder')}
                        className="app-text-body w-full rounded-md border border-app-border bg-app-bg py-2 pl-9 pr-3 text-app-ink focus:border-app-accent focus:outline-none"
                      />
                      {inviteFocused && inviteSuggestions.length > 0 ? (
                        <div className="absolute left-0 right-0 top-full z-10 mt-1 max-h-60 overflow-y-auto rounded-md border border-app-border bg-app-surface-sidebar shadow-lg">
                          {inviteSuggestions.map((user: ShareableUserItem) => (
                            <button
                              key={user.id}
                              type="button"
                              disabled={shareLoading}
                              onMouseDown={(event) => {
                                event.preventDefault();
                                void handleInviteUser(user.id);
                              }}
                              className="flex w-full items-center gap-2 px-3 py-2 text-left hover:bg-app-surface-hover"
                            >
                              <div className="app-text-micro flex size-7 items-center justify-center rounded-full bg-app-accent/20 font-bold text-app-accent">
                                {docsAuthorInitials(user.full_name)}
                              </div>
                              <div className="flex-1 min-w-0">
                                <div className="app-text-control text-app-ink truncate">
                                  {user.full_name}
                                </div>
                                {user.email ? (
                                  <div className="app-text-micro text-app-ink/55 truncate">
                                    {user.email}
                                  </div>
                                ) : null}
                              </div>
                            </button>
                          ))}
                        </div>
                      ) : null}
                    </div>
                    <select
                      aria-label={t('docs.share.access.label')}
                      value={shareAccessLevel}
                      onChange={(event) =>
                        setShareAccessLevel(
                          event.target.value as 'read' | 'edit',
                        )
                      }
                      className="app-field-input w-auto"
                    >
                      <option value="read">
                        {t('docs.share.access.read')}
                      </option>
                      <option value="edit">
                        {t('docs.share.access.edit')}
                      </option>
                    </select>
                  </div>

                  {sharingState?.users.length ? (
                    <div className="space-y-1.5">
                      <div className="app-text-overline text-app-ink/55">
                        {t('docs.share.peopleWithAccess')}
                      </div>
                      {sharingState.users.map((user: NativeDocShareUser) => (
                        <div
                          key={user.user_id}
                          className="flex items-center justify-between rounded-md border border-app-border bg-app-bg px-3 py-2"
                        >
                          <div className="flex items-center gap-2 min-w-0">
                            <div className="app-text-micro flex size-7 items-center justify-center rounded-full bg-app-accent/20 font-bold text-app-accent shrink-0">
                              {docsAuthorInitials(user.full_name)}
                            </div>
                            <div className="min-w-0">
                              <div className="app-text-control text-app-ink truncate">
                                {user.full_name}
                              </div>
                              {user.email ? (
                                <div className="app-text-micro text-app-ink/55 truncate">
                                  {user.email}
                                </div>
                              ) : null}
                            </div>
                          </div>
                          <div className="flex items-center gap-2 shrink-0">
                            <select
                              aria-label={t('docs.share.access.label')}
                              value={user.access_level}
                              disabled={shareLoading}
                              onChange={(event) =>
                                void handleChangeUserAccess(
                                  user.user_id,
                                  event.target.value as 'read' | 'edit',
                                )
                              }
                              className="app-field-input-sm w-auto"
                            >
                              <option value="read">
                                {t('docs.share.access.read')}
                              </option>
                              <option value="edit">
                                {t('docs.share.access.edit')}
                              </option>
                            </select>
                            <button
                              type="button"
                              disabled={shareLoading}
                              onClick={() =>
                                void handleRemoveUserShare(user.user_id)
                              }
                              className="app-text-caption rounded border border-app-border px-2 py-1 text-app-ink hover:bg-app-surface-hover disabled:opacity-50"
                            >
                              {t('pms.bulk.remove')}
                            </button>
                          </div>
                        </div>
                      ))}
                    </div>
                  ) : null}
                </div>

                {/* 2. Who can access — target visibility */}
                <div className="rounded-lg border border-app-border bg-app-bg p-4 space-y-3">
                  <div>
                    <div className="app-text-control text-app-ink">
                      {t('docs.share.whoCanAccess')}
                    </div>
                    <div className="app-text-caption text-app-ink/55">
                      {t('docs.share.visibilityDescription')}
                    </div>
                  </div>
                  <LocationPicker
                    value={resolveLocationValueFromTarget(
                      selectedDoc.primary_target,
                    )}
                    onChange={(value) => void handleChangeLocation(value)}
                    options={locationOptions}
                    busy={changingLocation || !selectedDoc.can_manage}
                  />
                  {!selectedDoc.can_manage ? (
                    <div className="app-text-micro text-app-ink/55">
                      {t('docs.share.manageOnlyNotice')}
                    </div>
                  ) : null}
                </div>

                {/* 3. Copy link — always visible direct doc URL */}
                <div className="flex items-center gap-2 rounded-lg border border-app-border bg-app-bg px-3 py-2">
                  <Link2 size={16} className="text-app-ink/55 shrink-0" />
                  <input
                    type="text"
                    aria-label={t('docs.copyLink')}
                    readOnly
                    value={docUrlFor(selectedDoc)}
                    onFocus={(event) => event.currentTarget.select()}
                    className="app-text-body-sm flex-1 min-w-0 bg-transparent text-app-ink focus:outline-none"
                  />
                  <button
                    type="button"
                    onClick={() => void copyDirectLink()}
                    className="app-text-control-sm flex items-center gap-1 rounded-md border border-app-border px-2.5 py-1 text-app-ink hover:bg-app-surface-hover"
                  >
                    <Copy size={12} />
                    <span>
                      {linkCopied ? t('docs.copied') : t('docs.copyLink')}
                    </span>
                  </button>
                </div>
              </>
            )}
          </div>
        </div>
      ) : null}

      {isListView ? (
        <div className="flex-1 flex flex-col min-h-0 overflow-hidden">
          <div className="p-8 space-y-5 overflow-y-auto h-full custom-scrollbar">
            <div className="flex items-center justify-between">
              <div>
                <h1 className="app-text-title-lg text-app-ink">
                  {activeCategoryLabel}
                </h1>
                <p className="app-text-caption text-app-ink/55">
                  {t('docs.documentCount', { count: total })}
                </p>
              </div>
              <button
                type="button"
                onClick={() => openCreateModal()}
                className="app-text-control flex items-center gap-2 rounded-md bg-app-accent px-4 py-2 text-app-accent-fg shadow-sm transition-opacity hover:opacity-90"
              >
                <Plus size={16} />
                <span>{t('home.actionNewDoc')}</span>
              </button>
            </div>

            <div className="flex flex-wrap items-center gap-3 border-b border-app-border pb-2">
              {hasActiveRefinement ? (
                <button
                  type="button"
                  onClick={resetDocsFilters}
                  className="app-text-control-sm rounded px-2 py-1 text-app-ink/55 hover:bg-app-surface-hover hover:text-app-ink"
                >
                  {t('docs.filters.reset')}
                </button>
              ) : null}
              <div className="app-text-caption ml-auto flex items-center gap-2 text-app-ink/55">
                {searchOpen ? (
                  <div className="relative">
                    <Search
                      className="absolute left-3 top-1/2 -translate-y-1/2 text-app-ink/55"
                      size={14}
                    />
                    <input
                      type="text"
                      aria-label={t('docs.searchPlaceholder')}
                      defaultValue={searchQuery}
                      placeholder={t('docs.searchPlaceholder')}
                      onChange={(event) =>
                        handleSearchChange(event.target.value)
                      }
                      className="app-text-body-sm w-64 rounded-md border border-app-border bg-app-surface-sidebar py-1.5 pl-9 pr-4 text-app-ink focus:border-app-accent focus:outline-none"
                      onBlur={(event) => {
                        if (!event.target.value) {
                          setSearchOpen(false);
                        }
                      }}
                    />
                  </div>
                ) : (
                  <button
                    type="button"
                    onClick={() => setSearchOpen(true)}
                    aria-label={t('docs.searchPlaceholder')}
                    className="p-1.5 rounded text-app-ink/55 hover:bg-app-surface-hover hover:text-app-ink"
                  >
                    <Search size={16} />
                  </button>
                )}
              </div>
            </div>

            {loadingList && docs.length === 0 ? (
              <div className="flex items-center justify-center py-20">
                <Loader2 size={32} className="animate-spin text-app-accent" />
              </div>
            ) : listLoadFailed ? (
              <div role="alert" className="flex flex-col items-center gap-3 py-20 text-app-ink">
                <p>{t('apps:docsApp.loadFailed')}</p>
                <button
                  type="button"
                  className="app-text-control text-app-accent hover:underline"
                  onClick={() => void fetchDocs()}
                >
                  {t('common:actions.retry')}
                </button>
              </div>
            ) : docs.length === 0 ? (
              <div className="flex-1 flex flex-col items-center justify-center py-20 text-center">
                <div className="size-24 bg-app-surface-sidebar rounded-full flex items-center justify-center mb-6">
                  <FileText size={48} className="text-app-ink/70 opacity-20" />
                </div>
                <h2 className="app-text-title-md mb-2 text-app-ink">
                  {t('docs.noDocsFound')}
                </h2>
                <p className="app-text-body mb-8 max-w-xs mx-auto text-app-ink/55">
                  {t('docs.noDocsDescription')}
                </p>
                <button
                  type="button"
                  onClick={() => openCreateModal()}
                  className="app-text-control rounded-md bg-app-accent px-6 py-2 font-bold text-app-accent-fg transition-opacity hover:opacity-90"
                >
                  {t('home.actionNewDoc')}
                </button>
              </div>
            ) : (
              <div className="border border-app-border rounded-lg overflow-hidden">
                <table aria-busy={loadingList} className="w-full">
                  <thead>
                    <tr className="border-b border-app-border bg-app-surface-sidebar">
                      <th
                        className="app-text-overline text-left px-4 py-2.5 text-app-ink/55 cursor-pointer hover:text-app-ink"
                        aria-sort={
                          sortBy === 'title'
                            ? sortDir === 'asc'
                              ? 'ascending'
                              : 'descending'
                            : 'none'
                        }
                        onClick={() => {
                          if (sortBy === 'title')
                            setSortDir((current: 'asc' | 'desc') =>
                              current === 'asc' ? 'desc' : 'asc',
                            );
                          else {
                            setSortBy('title');
                            setSortDir('asc');
                          }
                        }}
                      >
                        {t('pms.name')}
                      </th>
                      <th className="app-text-overline text-left px-4 py-2.5 text-app-ink/55">
                        {t('ai.search.metadataLocation')}
                      </th>
                      <th className="app-text-overline text-left px-4 py-2.5 text-app-ink/55">
                        {t('docs.share.sharing')}
                      </th>
                      <th
                        className="app-text-overline text-left px-4 py-2.5 text-app-ink/55 cursor-pointer hover:text-app-ink"
                        aria-sort={
                          sortBy === 'updated_at'
                            ? sortDir === 'asc'
                              ? 'ascending'
                              : 'descending'
                            : 'none'
                        }
                        onClick={() => {
                          if (sortBy === 'updated_at')
                            setSortDir((current: 'asc' | 'desc') =>
                              current === 'asc' ? 'desc' : 'asc',
                            );
                          else {
                            setSortBy('updated_at');
                            setSortDir('desc');
                          }
                        }}
                      >
                        {t('docs.updatedHeader')}
                      </th>
                      <th
                        className="w-12"
                        aria-label={t('apps:docs.moreActions')}
                      />
                    </tr>
                  </thead>
                  <tbody>
                    {docs.map((item: DocsHubItem) => {
                      const itemContentFormat =
                        resolveDocsHubContentFormat(item);
                      return (
                        <tr
                          key={item.id}
                          onClick={() => openDoc(item.id)}
                          className="cursor-pointer border-b border-app-border last:border-b-0 hover:bg-app-surface-sidebar/60"
                        >
                          <td className="px-4 py-3">
                            <div className="flex items-center gap-3 min-w-0">
                              <FileText
                                size={16}
                                className="text-app-accent shrink-0"
                              />
                              <div className="min-w-0">
                                <button
                                  type="button"
                                  onClick={(event) => {
                                    event.stopPropagation();
                                    openDoc(item.id);
                                  }}
                                  className="app-text-control block max-w-full truncate text-left text-app-ink hover:text-app-accent"
                                >
                                  {item.title}
                                </button>
                                <div className="app-text-caption flex flex-wrap items-center gap-x-2 gap-y-1 text-app-ink/55">
                                  <DocsContentFormatBadge
                                    format={itemContentFormat}
                                    label={t(
                                      `docs.contentFormat.${itemContentFormat}`,
                                    )}
                                    compact
                                  />
                                  <span>
                                    {t('docs.pageCount', {
                                      count: item.page_count,
                                    })}
                                  </span>
                                </div>
                              </div>
                              <Star
                                size={14}
                                className={cn(
                                  'shrink-0 text-app-ink/70',
                                  item.is_favorite &&
                                    'text-yellow-500 fill-yellow-500',
                                )}
                                onClick={(event) =>
                                  void handleToggleFavorite(event, item)
                                }
                              />
                            </div>
                          </td>
                          <td className="px-4 py-3 app-text-body-sm text-app-ink/45">
                            {localizeResourceLocationLabel(
                              item.location_label,
                              t,
                            )}
                          </td>
                          <td className="px-4 py-3 app-text-body-sm text-app-ink/45">
                            <div className="flex items-center gap-2">
                              {item.is_private ? (
                                <Lock size={14} />
                              ) : (
                                <Globe size={14} />
                              )}
                              <span>{sharingLabel(item, t)}</span>
                            </div>
                          </td>
                          <td className="px-4 py-3 app-text-body-sm text-app-ink/45">
                            {timeAgo(item.updated_at, timeZone, locale)}
                          </td>
                          <td className="px-4 py-3">
                            <DropdownMenu
                              open={menuOpenId === item.id}
                              onOpenChange={(open) =>
                                setMenuOpenId(open ? item.id : null)
                              }
                              side="bottom"
                              sideOffset={6}
                              contentClassName="w-48 min-w-[12rem] max-w-[12rem] rounded-lg border-app-border bg-app-surface-sidebar p-0 py-1 text-app-ink"
                              trigger={
                                <button
                                  type="button"
                                  aria-label={t('apps:docs.moreActions')}
                                  title={t('apps:docs.moreActions')}
                                  onClick={(event) => event.stopPropagation()}
                                  className="rounded p-1.5 text-app-ink/55 hover:bg-app-surface-hover"
                                >
                                  <MoreHorizontal size={16} />
                                </button>
                              }
                              items={[
                                {
                                  id: 'rename',
                                  disabled: !item.can_manage,
                                  onSelect: () => void handleRenameDoc(item),
                                  label: (
                                    <span className="flex items-center gap-2">
                                      <Pencil size={14} />
                                      <span className="app-text-control-sm">
                                        {t('common:actions.rename')}
                                      </span>
                                    </span>
                                  ),
                                },
                                {
                                  id: 'delete',
                                  disabled: !item.can_manage,
                                  tone: 'danger',
                                  onSelect: () => void handleDeleteDoc(item),
                                  label: (
                                    <span className="flex items-center gap-2">
                                      <Trash2 size={14} />
                                      <span className="app-text-control-sm">
                                        {t('common:actions.delete')}
                                      </span>
                                    </span>
                                  ),
                                },
                              ]}
                            />
                          </td>
                        </tr>
                      );
                    })}
                  </tbody>
                </table>
                {pageCount > 1 ? (
                  <div className="flex items-center justify-end gap-3 border-t border-app-border px-4 py-2">
                    <button
                      type="button"
                      disabled={loadingList || page <= 1}
                      onClick={() => setPage(page - 1)}
                      className="app-text-control-sm rounded-md px-2 py-1 text-app-ink hover:bg-app-surface-hover disabled:opacity-50"
                    >
                      {t('common:actions.previous')}
                    </button>
                    <span aria-live="polite" className="app-text-caption text-app-ink/55">
                      {t('apps:docsApp.pagePosition', { page, total: pageCount })}
                    </span>
                    <button
                      type="button"
                      disabled={loadingList || page >= pageCount}
                      onClick={() => setPage(page + 1)}
                      className="app-text-control-sm rounded-md px-2 py-1 text-app-ink hover:bg-app-surface-hover disabled:opacity-50"
                    >
                      {t('common:actions.next')}
                    </button>
                  </div>
                ) : null}
              </div>
            )}
          </div>
        </div>
      ) : (
        editorView
      )}
    </div>
  );
};

export const DocsView = ({ docId }: { docId: string | null }) => {
  const { t, i18n } = useTranslation(['apps', 'common']);
  const locale = i18n.resolvedLanguage ?? i18n.language;
  const runtime = useRuntime();
  const api = runtime.api;
  const timeZone = runtime.settings.timeZone;
  const navigate = useAppNavigate();
  const feedback = useFeedback();
  const { spaces } = useDocsApp();
  const [searchParams, setSearchParams] = useAppSearchParams();
  const spaceId = searchParams.get('space')?.trim() || 'global';
  const requestedPageId = getDocPageIdFromSearchParams(searchParams);
  const { uploadFile, resolveFileUrl, linkUploadsToPage } = useMediaUpload();
  const { confirm, confirmDialog } = useConfirm();
  const { prompt, promptDialog } = usePrompt();
  const [loadError, setLoadError] = useState<string | null>(null);
  const [failedPageId, setFailedPageId] = useState<string | null>(null);
  const [pageLoadAttempt, setPageLoadAttempt] = useState(0);
  // miy 보강: version history, templates for new docs and pages, and the
  // comment a notification points at (`?comment=`).
  const [versionsPageId, setVersionsPageId] = useState<string | null>(null);
  const [newDocTemplateId, setNewDocTemplateId] = useState<string | null>(null);
  const [newPageTemplateId, setNewPageTemplateId] = useState<string | null>(null);
  const highlightCommentId = searchParams.get('comment');

  const docsViewState = useDocsViewState();
  const {
    activeDragId,
    changingLocation,
    contentEditorVersions,
    copiedDocId,
    creating,
    creatingPage,
    docMenuOpen,
    dropIndicator,
    editorLoading,
    inviteFocused,
    inviteQuery,
    linkCopied,
    manualExpandedNodes,
    menuOpenId,
    newDocLocation,
    newDocTitle,
    newPageTitle,
    pages,
    readModeOpen,
    searchOpen,
    selectedDoc,
    shareAccessLevel,
    shareLoading,
    shareableUsers,
    sharingState,
    showCreateModal,
    showCreatePageModal,
    showShareModal,
  } = docsViewState;
  const {
    setActiveDragId,
    setChangingLocation,
    setContentEditorVersions,
    setCopiedDocId,
    setCreating,
    setCreatingPage,
    setDocMenuOpen,
    setDropIndicator,
    setEditorLoading,
    setInviteFocused,
    setInviteQuery,
    setLinkCopied,
    setManualExpandedNodes,
    setMenuOpenId,
    setNewDocContentFormat,
    setNewDocLocation,
    setNewDocTitle,
    setNewPageContentFormat,
    setNewPageTitle,
    setPages,
    setReadModeOpen,
    setSearchOpen,
    setSelectedDoc,
    setShareAccessLevel,
    setShareLoading,
    setShareableUsers,
    setSharingState,
    setShowCreateModal,
    setShowCreatePageModal,
    setShowShareModal,
  } = docsViewState;
  const docMenuRef = useRef<HTMLDivElement>(null);
  const closeDocMenu = useCallback(
    () => setDocMenuOpen(false),
    [setDocMenuOpen],
  );
  useCloseOnOutsidePointer(docMenuRef, docMenuOpen, closeDocMenu);

  const newPageParentIdRef = useRef<string | null>(null);
  // The doc shown now; a late answer for another doc is dropped.
  const activeDocRef = useRef<string | null>(docId);
  activeDocRef.current = docId;

  const requestedCategory = searchParams.get('view');
  const activeCategory: DocsViewCategory = isDocsViewCategory(requestedCategory)
    ? requestedCategory
    : 'all';
  const activeCategoryLabel = t(
    VIEW_LABEL_KEYS[activeCategory] || CATEGORY_LABEL_KEYS['docs-all'],
  );
  const activeSourceApp = searchParams.get('source_app') ?? undefined;
  const activeSourceKind = searchParams.get('source_kind') ?? undefined;
  const activeFilterQuery = useMemo(() => {
    const query = new URLSearchParams(getDocsFilterQueryString(searchParams));
    query.delete('create');
    return query.toString();
  }, [searchParams]);
  const activeDocId = docId ?? null;
  const selectedPageId = resolveDocsPageSelection(pages, requestedPageId);
  const selectedPageExpandedNodes = useMemo(
    () => getExpandedDocPageNodeIds(pages, selectedPageId),
    [pages, selectedPageId],
  );
  const expandedNodes = useMemo(() => {
    if (selectedPageExpandedNodes.size === 0) {
      return manualExpandedNodes;
    }
    const next = new Set(manualExpandedNodes);
    for (const pageId of selectedPageExpandedNodes) {
      next.add(pageId);
    }
    return next;
  }, [manualExpandedNodes, selectedPageExpandedNodes]);
  const visibleTree = useMemo(
    () => flattenVisibleTree(pages, expandedNodes),
    [pages, expandedNodes],
  );
  const pagesById = useMemo(() => buildDocsPagesById(pages), [pages]);
  const childCounts = useMemo(() => countDocsPageChildren(pages), [pages]);
  const activePage =
    pages.find((page) => page.id === selectedPageId) ?? pages[0] ?? null;
  const activePageId = activePage?.id ?? null;
  const activePageRevision = activePage?.revision;
  const needsPageBody = Boolean(activePage && activePage.content_blocks === null);
  const activeContentFormat = resolveDocsContentFormat(activePage);
  const activePageAuthorName = resolveDocsPageAuthorName(
    activePage,
    selectedDoc,
  );
  const activePageAuthorInitials = docsAuthorInitials(activePageAuthorName);
  const activePageSourceId = activePage?.source_page_id ?? null;
  const activePageUploadFile = useMemo(() => {
    if (!activePageSourceId) return uploadFile;
    // AI-DO createLinkedUploadFile: link the upload to the page at once.
    return async (file: File) => {
      const url = await uploadFile(file);
      void linkUploadsToPage([url], activePageSourceId);
      return url;
    };
  }, [activePageSourceId, linkUploadsToPage, uploadFile]);
  const isListView = !activeDocId;
  const docsHubController = useDocsHubController({
    api,
    spaceId,
    listEnabled: isListView,
    activeCategory,
    activeSourceApp,
    activeSourceKind,
  });
  const {
    docs, loadFailed: listLoadFailed, loadingList, page, pageCount, searchQuery, sortBy, sortDir, total,
  } = docsHubController.state;
  const {
    fetchDocs, handleSearchChange, setDocs, setPage, setSortBy, setSortDir,
  } = docsHubController.actions;
  const docSpaceId = selectedDoc?.space_id ?? spaceId;
  const docSpace = useMemo(
    () => spaces.find((space) => space.id === docSpaceId) ?? null,
    [docSpaceId, spaces],
  );
  const listSpace = useMemo(
    () => spaces.find((space) => space.id === spaceId) ?? null,
    [spaceId, spaces],
  );

  const docPathFor = useCallback(
    (itemId: string, pageId?: string | null, itemSpaceId?: string | null) =>
      buildDocsDocPath({
        docId: itemId,
        pageId,
        spaceId: itemSpaceId ?? spaceId,
      }),
    [spaceId],
  );

  const selectDocPage = useCallback(
    (pageId: string | null) => {
      const next = withDocPageSearchParam(searchParams, pageId);
      // The comment a notification pointed at belongs to the page left.
      if (pageId !== requestedPageId) next.delete('comment');
      setSearchParams(next, {
        replace: true,
      });
    },
    [requestedPageId, searchParams, setSearchParams],
  );

  // ── page content saving (miy: single editor, revisions) ─────────────
  const {
    baseRevision: contentBaseRevision,
    cancelQueuedSave,
    dirty: contentDirty,
    flush: flushContentSave,
    hasDraft,
    isSettled: isContentSettled,
    queueBlockSave,
    resolveConflictKeepMine,
    resolveConflictReload,
    retry: retryContentSave,
    saveState,
    setBaseRevision,
  } = useDocsPageContentSaveController({
    api,
    savePage: updateDocPage,
    onSavedPage: (updated, hasNewerDraft) => {
      setPages((current) =>
        current.map((page) =>
          page.id === updated.id
            ? hasNewerDraft
              ? // Keep the newer local draft; take the server's metadata.
                { ...updated, content_blocks: page.content_blocks }
              : updated
            : page,
        ),
      );
    },
  });
  useNavigationDirty(contentDirty);

  useEffect(() => {
    for (const page of pages) {
      if (
        page.content_blocks !== null &&
        (!hasDraft(page.id) || contentBaseRevision(page.id) === undefined)
      ) {
        setBaseRevision(page.id, page.revision);
      }
    }
  }, [contentBaseRevision, hasDraft, pages, setBaseRevision]);

  const loadDoc = useCallback(
    async (itemId: string) => {
      setEditorLoading(true);
      setLoadError(null);
      try {
        const [item, pageResponse] = await Promise.all([
          getDocsItem(api, itemId),
          listDocPages(api, itemId),
        ]);
        if (activeDocRef.current !== itemId) return;
        setSelectedDoc(item);
        setPages((current) => {
          const localPages = buildDocsPagesById(current);
          return pageResponse.items.map((page) => {
            const local = localPages.get(page.id);
            // Keep the draft paired with the revision it was edited from.
            if (local && (hasDraft(page.id) || local.revision >= page.revision)) {
              return {
                ...page,
                revision: local.revision,
                content_blocks: local.content_blocks,
              };
            }
            return page;
          });
        });
        setManualExpandedNodes(new Set());
      } catch (error) {
        if (activeDocRef.current !== itemId) return;
        setSelectedDoc(null);
        setPages([]);
        setLoadError(
          error instanceof Error && error.message ? error.message : null,
        );
      } finally {
        if (activeDocRef.current === itemId) setEditorLoading(false);
      }
    },
    [
      api,
      hasDraft,
      setEditorLoading,
      setManualExpandedNodes,
      setPages,
      setSelectedDoc,
    ],
  );

  useEffect(() => {
    const next = removeLegacyDocsSearchParams(searchParams);
    if (!next) return;
    setSearchParams(next, { replace: true });
  }, [searchParams, setSearchParams]);

  useEffect(() => {
    if (!docId) {
      setSelectedDoc(null);
      setPages([]);
      return;
    }
    // A different doc: start from a clean editor state.
    setSelectedDoc((current) => (current?.id === docId ? current : null));
    setPages((current) =>
      current.length && current[0]?.doc_id !== docId ? [] : current,
    );
    void loadDoc(docId);
  }, [docId, loadDoc, setPages, setSelectedDoc]);

  useEffect(() => {
    if (!activePageId || !needsPageBody || selectedDoc?.id !== activeDocId) return;
    const request = new AbortController();
    setFailedPageId(null);
    void getDocPage(api, activePageId, { signal: request.signal })
      .then((loaded) => {
        if (request.signal.aborted) return;
        setPages((current) => current.map((page) =>
          page.id === loaded.id && page.content_blocks === null
            ? {
                ...page,
                can_edit: loaded.can_edit,
                revision: loaded.revision,
                updated_at: loaded.updated_at > page.updated_at ? loaded.updated_at : page.updated_at,
                content_blocks: loaded.content_blocks ?? [],
              }
            : page,
        ));
      })
      .catch(() => {
        if (!request.signal.aborted) setFailedPageId(activePageId);
      });
    return () => request.abort();
  }, [
    activeDocId, activePageId, activePageRevision, api, needsPageBody, pageLoadAttempt, selectedDoc?.id, setPages,
  ]);

  useEffect(() => {
    if (!activeDocId || !activePageId) return;
    let cancelled = false;
    void recordDocView(api, activeDocId, activePageId)
      .then(() => {
        if (!cancelled) refreshDocsSidebar();
      })
      .catch(() => undefined);
    return () => {
      cancelled = true;
    };
  }, [activeDocId, activePageId, api]);

  const listPath = useCallback(
    (targetSpaceId?: string | null) => {
      const base = docsListPath(targetSpaceId ?? spaceId, null);
      const query = new URLSearchParams(activeFilterQuery);
      query.delete('space');
      const extra = query.toString();
      if (!extra) return base;
      return base.includes('?') ? `${base}&${extra}` : `${base}?${extra}`;
    },
    [activeFilterQuery, spaceId],
  );

  const docNavigationPathFor = (itemId: string) => {
    const base = docPathFor(itemId);
    const query = new URLSearchParams(activeFilterQuery);
    query.delete('space');
    const extra = query.toString();
    if (!extra) return base;
    return base.includes('?') ? `${base}&${extra}` : `${base}?${extra}`;
  };

  const openDoc = (itemId: string) => {
    navigate(docNavigationPathFor(itemId));
  };

  const handleBack = () => {
    void flushContentSave();
    navigate(listPath(spaces.some((space) => space.id === docSpaceId) ? docSpaceId : null));
  };

  const openCreateModal = useCallback(
    (templateTitle?: string) => {
      const defaultLocation = resolveDefaultDocsCreateLocation(activeCategory);
      setNewDocTitle(templateTitle ?? '');
      setNewDocTemplateId(null);
      setNewDocLocation(defaultLocation);
      setNewDocContentFormat('block');
      setShowCreateModal(true);
    },
    [
      activeCategory,
      setNewDocContentFormat,
      setNewDocLocation,
      setNewDocTitle,
      setShowCreateModal,
    ],
  );

  useEffect(() => {
    const createState = consumeDocsCreateSearchParam(searchParams);
    if (!createState.shouldOpen || !createState.searchParams) return;
    openCreateModal();
    setSearchParams(createState.searchParams, { replace: true });
  }, [openCreateModal, searchParams, setSearchParams]);

  const resetDocsFilters = useCallback(() => {
    const next = new URLSearchParams(searchParams);
    for (const key of ['page', 'collection_id', 'doc_type', 'source_app', 'source_kind', 'space_id']) {
      next.delete(key);
    }
    setSearchParams(next, { replace: true });
  }, [searchParams, setSearchParams]);

  const hasActiveRefinement = Boolean(activeSourceApp || activeSourceKind);

  // Unsaved input in the create/share dialogs is reported to the portal.
  useNavigationDirty(
    (showCreateModal && newDocTitle.trim() !== '') ||
      creating ||
      creatingPage ||
      (showCreatePageModal && newPageTitle.trim() !== ''),
  );

  const locationSpace = isListView ? listSpace : docSpace;
  const locationSpaceId = isListView ? spaceId : docSpaceId;
  const locationOptions = useMemo<LocationOption[]>(
    () => [
      {
        value: 'workspace',
        icon: Globe,
        title: locationSpace
          ? t('docs.location.workspaceNamed', {
              name:
                locationSpace.kind === 'app_global'
                  ? t('docsApp.spaces.global')
                  : locationSpace.name,
            })
          : t('docs.location.workspace'),
        desc: t('docs.location.workspaceDesc'),
      },
      {
        value: 'private',
        icon: Lock,
        title: t('docs.category.private'),
        desc: t('docs.location.privateDesc'),
      },
    ],
    [locationSpace, t],
  );

  const resolveTargetFromLocationValue = useCallback(
    (locationValue: string) => {
      return resolveDocsCreatePrimaryTarget(locationValue, locationSpaceId);
    },
    [locationSpaceId],
  );

  const resolveLocationValueFromTarget = useCallback(
    (target: { app: string; type: string; id: string } | null) => {
      return resolveDocsCreateLocationValue(target);
    },
    [],
  );

  const handleCreateDoc = async () => {
    if (creating || !newDocTitle.trim()) return;
    setCreating(true);
    try {
      const item = await createNativeDoc(
        api,
        {
          title: newDocTitle.trim(),
          content_format: 'block',
          primary_target: resolveTargetFromLocationValue(newDocLocation),
          template_id: newDocTemplateId,
        },
        spaceId,
      );
      setShowCreateModal(false);
      setNewDocTitle('');
      setNewDocContentFormat('block');
      openDoc(item.id);
    } catch (error) {
      feedback.error(
        error instanceof Error && error.message
          ? error.message
          : t('docsApp.actionFailed'),
      );
    } finally {
      setCreating(false);
    }
  };

  const handleToggleFavorite = async (
    event: React.MouseEvent,
    item: DocsHubItem,
  ) => {
    event.stopPropagation();
    try {
      const response = await toggleDocFavorite(api, item.id);
      setDocs((current) =>
        current.map((doc) =>
          doc.id === item.id
            ? { ...doc, is_favorite: response.is_favorite }
            : doc,
        ),
      );
      setSelectedDoc((current) =>
        current?.id === item.id
          ? { ...current, is_favorite: response.is_favorite }
          : current,
      );
      refreshDocsSidebar();
    } catch {
      // no-op
    }
  };

  const handleRenameDoc = async (item: DocsHubItem) => {
    setMenuOpenId(null);
    if (!item.can_manage) return;
    const nextTitle = await prompt({
      title: t('docs.renameDocument'),
      defaultValue: item.title,
      submitLabel: t('common:actions.save'),
      cancelLabel: t('common:actions.cancel'),
    });
    if (!nextTitle || nextTitle.trim() === item.title) return;
    try {
      const updated = await updateDocsItem(api, item.id, {
        title: nextTitle.trim(),
      });
      setDocs((current) =>
        current.map((doc) => (doc.id === updated.id ? updated : doc)),
      );
      setSelectedDoc((current) => (current?.id === updated.id ? updated : current));
      refreshDocsSidebar();
    } catch (error) {
      feedback.error(
        error instanceof Error && error.message
          ? error.message
          : t('docsApp.actionFailed'),
      );
    }
  };

  const handleDeleteDoc = async (item: DocsHubItem) => {
    setMenuOpenId(null);
    if (!item.can_manage) return;
    const ok = await confirm({
      title: t('docs.deleteDocumentConfirm', { title: item.title }),
      description: t('docs.deleteDocDescription'),
      confirmLabel: t('common:actions.delete'),
      cancelLabel: t('common:actions.cancel'),
      variant: 'danger',
    });
    if (!ok) return;
    try {
      await deleteDocsItem(api, item.id);
      clearCommentDrafts(item.id);
      refreshDocsSidebar();
      if (activeDocRef.current === item.id) {
        cancelQueuedSave();
        navigate(listPath());
      } else {
        void fetchDocs();
      }
    } catch (error) {
      feedback.error(
        error instanceof Error && error.message
          ? error.message
          : t('docsApp.actionFailed'),
      );
    }
  };

  const handleDuplicateDoc = async (item: DocsHubItem) => {
    setMenuOpenId(null);
    setDocMenuOpen(false);
    try {
      await flushContentSave();
      const duplicate = await duplicateDocsItem(api, item.id);
      void fetchDocs();
      navigate(docPathFor(duplicate.id, null, duplicate.space_id));
    } catch (error) {
      feedback.error(
        error instanceof Error && error.message
          ? error.message
          : t('docsApp.actionFailed'),
      );
    }
  };

  const docUrlFor = (item: DocsHubItem): string => {
    const pageId = item.id === activeDocId ? activePageId : null;
    const path = docPathFor(item.id, pageId, item.space_id);
    return runtime.portalUrl?.(path) ?? portalUrlFor(runtime.basePath, path);
  };

  const handleCopyLink = async (item: DocsHubItem) => {
    setDocMenuOpen(false);
    if (await copyText(docUrlFor(item))) {
      setCopiedDocId(item.id);
      window.setTimeout(
        () =>
          setCopiedDocId((current) => (current === item.id ? null : current)),
        1500,
      );
    } else {
      feedback.error(t('docsApp.copyFailed'));
    }
  };

  const handleOpenInNewTab = (item: DocsHubItem) => {
    setDocMenuOpen(false);
    void flushContentSave();
    window.open(docUrlFor(item), '_blank', 'noopener,noreferrer');
  };

  const handlePrintDoc = () => {
    setDocMenuOpen(false);
    window.print();
  };

  const handleAddPage = (parentId?: string | null) => {
    if (!selectedDoc || !selectedDoc.can_edit) return;
    setNewPageTitle(t('apps:docs.untitled'));
    setNewPageTemplateId(null);
    newPageParentIdRef.current = parentId ?? null;
    setNewPageContentFormat('block');
    setShowCreatePageModal(true);
  };

  const handleCreatePage = async () => {
    if (creatingPage || !selectedDoc || !selectedDoc.can_edit || !newPageTitle.trim())
      return;
    const docIdAtStart = selectedDoc.id;
    setCreatingPage(true);
    try {
      const parentId = newPageParentIdRef.current;
      const page = await createDocPage(
        api,
        docIdAtStart,
        newPageTemplateId
          ? {
              title: newPageTitle.trim(),
              parent_id: parentId,
              content_format: 'block',
              template_id: newPageTemplateId,
            }
          : {
              title: newPageTitle.trim(),
              parent_id: parentId,
              content_format: 'block',
              content_blocks: [],
            },
      );
      setShowCreatePageModal(false);
      setNewPageTitle('');
      if (activeDocRef.current !== docIdAtStart) return;
      setPages((current) => {
        const nextPages = [
          ...current,
          { ...page, content_blocks: page.content_blocks ?? [] },
        ];
        const nextDocSummary = summarizeDocsPageCollection(nextPages);
        setSelectedDoc((currentDoc) =>
          currentDoc ? { ...currentDoc, ...nextDocSummary } : currentDoc,
        );
        setDocs((docsNow) =>
          docsNow.map((doc) =>
            doc.id === docIdAtStart ? { ...doc, ...nextDocSummary } : doc,
          ),
        );
        return nextPages;
      });
      selectDocPage(page.id);
      if (parentId) {
        setManualExpandedNodes((current) => new Set(current).add(parentId));
      }
    } catch (error) {
      feedback.error(
        error instanceof Error && error.message
          ? error.message
          : t('docsApp.actionFailed'),
      );
    } finally {
      setCreatingPage(false);
    }
  };

  const handleDeletePage = async (page: DocsPageItem) => {
    if (!page.can_edit) return;
    const ok = await confirm({
      title: t('docs.deletePageConfirm', { title: page.title }),
      description: t('docs.deletePageDescription'),
      confirmLabel: t('common:actions.delete'),
      cancelLabel: t('common:actions.cancel'),
      variant: 'danger',
    });
    if (!ok) return;
    try {
      await deleteDocPage(api, page.id);
      for (const id of removeDocsPageSubtree(pages, page.id).removedIds) {
        clearCommentDrafts(`${page.doc_id}:${id}`);
      }
      if (activeDocRef.current !== page.doc_id) return;
      let removed = new Set<string>();
      setPages((current) => {
        const result = removeDocsPageSubtree(current, page.id);
        removed = result.removedIds;
        setSelectedDoc((currentDoc) =>
          currentDoc ? { ...currentDoc, ...result.summary } : currentDoc,
        );
        setDocs((docsNow) =>
          docsNow.map((doc) =>
            doc.id === page.doc_id ? { ...doc, ...result.summary } : doc,
          ),
        );
        if (selectedPageId && result.removedIds.has(selectedPageId)) {
          selectDocPage(result.pages[0]?.id ?? null);
        }
        return result.pages;
      });
      for (const id of removed) cancelQueuedSave(id);
    } catch (error) {
      feedback.error(
        error instanceof Error && error.message
          ? error.message
          : t('docsApp.actionFailed'),
      );
    }
  };

  const handleLivePageChange = useCallback((pageId: string, update: Partial<Pick<DocsPageItem, 'content_blocks' | 'revision'>>) => {
    setPages(current => current.map(item => item.id === pageId ? { ...item, ...update } : item));
  }, [setPages]);

  const handleEditorChange = (
    pageId: string,
    blocks: Record<string, unknown>[],
  ) => {
    const target = pagesById.get(pageId);
    if (!target?.can_edit) return;
    setPages((current) => updateDocsPageContentBlocks(current, pageId, blocks));
    queueBlockSave(pageId, blocks);
  };

  const bumpContentEditorVersion = (pageId: string) => {
    setContentEditorVersions((current) =>
      bumpDocsContentEditorVersion(current, pageId),
    );
  };

  const handleReloadConflict = async (page: DocsPageItem) => {
    try {
      const latest = await getDocPage(api, page.id);
      resolveConflictReload(latest);
      setPages((current) => applyUpdatedDocsPage(current, {
        ...latest,
        content_blocks: latest.content_blocks ?? [],
      }));
      bumpContentEditorVersion(page.id);
    } catch (error) {
      feedback.error(
        error instanceof Error && error.message
          ? error.message
          : t('docsApp.actionFailed'),
      );
    }
  };

  const handleKeepMine = async (page: DocsPageItem) => {
    try {
      const latest = await getDocPage(api, page.id);
      await resolveConflictKeepMine(page.id, latest.revision);
    } catch (error) {
      feedback.error(
        error instanceof Error && error.message
          ? error.message
          : t('docsApp.actionFailed'),
      );
    }
  };

  const handleBlockMarkdownImportFile = async (
    page: DocsPageItem,
    file: File | null,
  ) => {
    if (!page.can_edit || !file) return;
    if (isDocsTextUploadTooLarge(file.size)) {
      window.alert(t('apps:docs.uploadTooLarge'));
      return;
    }
    const ok = await confirm({
      title: t('apps:docs.blockMarkdown.importConfirmTitle'),
      description: t('apps:docs.blockMarkdown.importConfirmDescription'),
      confirmLabel: t('apps:docs.blockMarkdown.import'),
      cancelLabel: t('common:actions.cancel'),
    });
    if (!ok) return;
    try {
      const markdown = await file.text();
      const blocks = markdownToBlockContent(markdown) as unknown as Record<
        string,
        unknown
      >[];
      // The import replaces the page; a queued draft of it is dropped.
      cancelQueuedSave(page.id);
      const current = pagesById.get(page.id) ?? page;
      const updated = await updateDocPage(api, page.id, {
        content_blocks: blocks,
        expected_revision: current.revision,
      });
      setBaseRevision(updated.id, updated.revision);
      setPages((pagesNow) => applyUpdatedDocsPage(pagesNow, updated));
      bumpContentEditorVersion(page.id);
    } catch (error) {
      window.alert(
        isDocsRevisionConflict(error)
          ? t('apps:docsApp.conflict.message')
          : t('apps:docs.blockMarkdown.importFailed'),
      );
    }
  };

  // ── version history (miy 보강) ────────────────────────────────────────
  const versionsPage = versionsPageId
    ? (pages.find((page) => page.id === versionsPageId) ?? null)
    : null;

  const handleOpenVersions = (page: DocsPageItem) => {
    // Show the history with the latest saved revision in it.
    void flushContentSave();
    setVersionsPageId(page.id);
  };

  const handleCloseVersions = () => setVersionsPageId(null);

  /**
   * Restore a version as a new revision. The page's draft is saved first;
   * while it cannot be (a failed save or an open conflict) nothing is
   * restored, so the draft is never lost.
   */
  const handleRestoreVersion = async (
    page: DocsPageItem,
    version: DocsPageVersion,
  ): Promise<boolean> => {
    const docIdAtStart = activeDocRef.current;
    await flushContentSave();
    if (!isContentSettled(page.id)) {
      feedback.error(t('apps:docsApp.versions.saveFirst'));
      return false;
    }
    try {
      const updated = await restoreDocPageVersion(
        api,
        page.id,
        version.id,
        contentBaseRevision(page.id) ?? page.revision,
      );
      setBaseRevision(updated.id, updated.revision);
      if (activeDocRef.current === docIdAtStart) {
        setPages((current) => applyUpdatedDocsPage(current, {
          ...updated,
          content_blocks: updated.content_blocks ?? [],
        }));
        bumpContentEditorVersion(updated.id);
      }
      feedback.success(
        t('apps:docsApp.versions.restored', { revision: version.revision }),
      );
      return true;
    } catch (error) {
      feedback.error(
        isDocsRevisionConflict(error)
          ? t('apps:docsApp.versions.conflict')
          : error instanceof Error && error.message
            ? error.message
            : t('docsApp.actionFailed'),
      );
      return false;
    }
  };

  // ── templates (miy 보강) ──────────────────────────────────────────────
  const handleSaveAsTemplate = async (page: DocsPageItem) => {
    await flushContentSave();
    const title = await prompt({
      title: t('apps:docsApp.templates.saveTitle'),
      description: t('apps:docsApp.templates.saveDescription'),
      defaultValue: page.title,
      submitLabel: t('common:actions.save'),
      cancelLabel: t('common:actions.cancel'),
    });
    if (!title?.trim()) return;
    if (!isContentSettled(page.id)) {
      feedback.error(t('apps:docsApp.versions.saveFirst'));
      return;
    }
    try {
      const template = await createDocsTemplate(api, {
        page_id: page.id,
        title: title.trim(),
      });
      feedback.success(
        t('apps:docsApp.templates.saved', { title: template.title }),
      );
    } catch (error) {
      feedback.error(
        error instanceof Error && error.message
          ? error.message
          : t('docsApp.actionFailed'),
      );
    }
  };

  const handlePageTitleSave = async (pageId: string, nextTitle: string) => {
    const trimmed = nextTitle.trim();
    if (!trimmed) return;
    const target = pages.find((page) => page.id === pageId);
    if (!target || target.title === trimmed) return;
    try {
      const updated = await updateDocPage(api, pageId, { title: trimmed });
      setPages((current) =>
        current.map((page) =>
          page.id === updated.id
            ? // A title save never replaces the body being edited.
              { ...page, title: updated.title, updated_at: updated.updated_at }
            : page,
        ),
      );
    } catch (error) {
      feedback.error(
        error instanceof Error && error.message
          ? error.message
          : t('docsApp.actionFailed'),
      );
    }
  };

  const toggleExpand = (nodeId: string) => {
    setManualExpandedNodes((current) => {
      const next = new Set(current);
      if (next.has(nodeId)) next.delete(nodeId);
      else next.add(nodeId);
      return next;
    });
  };

  const refreshSharing = useCallback(
    async (itemId: string) => {
      const sharing = await getDocSharing(api, itemId);
      if (activeDocRef.current !== itemId) return;
      setSharingState(sharing);
    },
    [api, setSharingState],
  );

  const openShareModal = async () => {
    if (!selectedDoc?.can_share) return;
    setSharingState(null);
    setShareableUsers([]);
    setInviteQuery('');
    setShareLoading(true);
    setShowShareModal(true);
    try {
      await refreshSharing(selectedDoc.id);
    } catch (error) {
      feedback.error(
        error instanceof Error && error.message
          ? error.message
          : t('docsApp.actionFailed'),
      );
    } finally {
      setShareLoading(false);
    }
  };

  // miy: the person search asks the platform directory for each query
  // (AI-DO loaded the first 30 workspace members once and filtered them).
  useEffect(() => {
    if (!showShareModal) return;
    const query = inviteQuery.trim();
    if (!query) {
      setShareableUsers([]);
      return;
    }
    const controller = new AbortController();
    const timer = window.setTimeout(() => {
      listShareableUsers(api, query, { signal: controller.signal })
        .then((users) => setShareableUsers(users))
        .catch(() => undefined);
    }, 200);
    return () => {
      controller.abort();
      window.clearTimeout(timer);
    };
  }, [api, inviteQuery, setShareableUsers, showShareModal]);

  const handleChangeLocation = async (nextLocation: string) => {
    if (!selectedDoc || !selectedDoc.can_manage) return;
    const current = resolveLocationValueFromTarget(selectedDoc.primary_target);
    if (current === nextLocation) return;
    const itemId = selectedDoc.id;
    setChangingLocation(true);
    try {
      const nextTarget = resolveTargetFromLocationValue(nextLocation);
      const updated =
        nextTarget === null
          ? await deleteDocTarget(api, itemId)
          : await updateDocTarget(api, itemId, nextTarget);
      setSelectedDoc((currentDoc) => (currentDoc?.id === itemId ? updated : currentDoc));
      setDocs((docsNow) =>
        docsNow.map((doc) => (doc.id === updated.id ? updated : doc)),
      );
    } catch (error) {
      feedback.error(
        error instanceof Error && error.message
          ? error.message
          : t('docsApp.actionFailed'),
      );
    } finally {
      setChangingLocation(false);
    }
  };

  const runShareChange = async (
    change: (itemId: string) => Promise<NativeDocSharingResponse>,
  ) => {
    if (!selectedDoc) return;
    const itemId = selectedDoc.id;
    setShareLoading(true);
    try {
      const updated = await change(itemId);
      if (activeDocRef.current !== itemId) return;
      setSharingState(updated);
      setSelectedDoc((current) =>
        current?.id === itemId && current.sharing_summary
          ? {
              ...current,
              sharing_summary: {
                ...current.sharing_summary,
                user_share_count: updated.users.length,
              },
            }
          : current,
      );
    } catch (error) {
      feedback.error(
        error instanceof Error && error.message
          ? error.message
          : t('docsApp.actionFailed'),
      );
    } finally {
      setShareLoading(false);
    }
  };

  const handleRemoveUserShare = (userId: string) =>
    runShareChange((itemId) => deleteDocUserShare(api, itemId, userId));

  const copyDirectLink = async () => {
    if (!selectedDoc) return;
    if (await copyText(docUrlFor(selectedDoc))) {
      setLinkCopied(true);
      window.setTimeout(() => setLinkCopied(false), 1500);
    } else {
      feedback.error(t('docsApp.copyFailed'));
    }
  };

  const invitedUserIds = useMemo(
    () => new Set(sharingState?.users.map((user) => user.user_id) ?? []),
    [sharingState?.users],
  );

  const inviteSuggestions = useMemo(() => {
    const query = inviteQuery.trim();
    if (!query) return [];
    return shareableUsers
      .filter((user) => !invitedUserIds.has(user.id))
      .slice(0, 6);
  }, [inviteQuery, invitedUserIds, shareableUsers]);

  const handleInviteUser = async (userId: string) => {
    await runShareChange((itemId) =>
      upsertDocUserShare(api, itemId, userId, shareAccessLevel),
    );
    setInviteQuery('');
  };

  const handleChangeUserAccess = (userId: string, level: 'read' | 'edit') =>
    runShareChange((itemId) => upsertDocUserShare(api, itemId, userId, level));

  const dragDescendantsRef = useRef<Set<string> | null>(null);
  const dragPointerYRef = useRef<number>(0);

  const dndSensors = useSensors(
    useSensor(PointerSensor, { activationConstraint: { distance: 4 } }),
    useSensor(KeyboardSensor, {
      coordinateGetter: sortableKeyboardCoordinates,
    }),
  );

  const handleDragPointerMove = useCallback(
    (event: React.PointerEvent<HTMLDivElement>) => {
      dragPointerYRef.current = event.clientY;
    },
    [],
  );

  const handleDragStart = useCallback(
    (event: DragStartEvent) => {
      const activeId = String(event.active.id);
      dragDescendantsRef.current = collectDescendantIds(pages, activeId);
      setActiveDragId(activeId);
      setDropIndicator(null);
    },
    [pages, setActiveDragId, setDropIndicator],
  );

  const handleDragOver = useCallback(
    (event: DragOverEvent) => {
      const { active, over } = event;
      if (!over || !active) {
        setDropIndicator(null);
        return;
      }
      const overId = String(over.id);
      if (overId === String(active.id)) {
        setDropIndicator(null);
        return;
      }
      const descendants = dragDescendantsRef.current;
      if (descendants && descendants.has(overId)) {
        setDropIndicator(null);
        return;
      }
      const rect = over.rect;
      if (!rect) return;
      const pointerY = dragPointerYRef.current;
      const pointerWithinRow =
        pointerY >= rect.top && pointerY <= rect.top + rect.height;
      let zone: DropZone;
      if (pointerWithinRow) {
        zone = resolveDropZone(pointerY, {
          top: rect.top,
          height: rect.height,
        });
      } else {
        // Keyboard / out-of-row fallback: compare over vs active position.
        const activeRect =
          active.rect?.current?.translated ??
          active.rect?.current?.initial ??
          null;
        if (activeRect && rect.top < activeRect.top) {
          zone = 'before';
        } else {
          zone = 'after';
        }
      }
      setDropIndicator((current) =>
        current && current.overId === overId && current.zone === zone
          ? current
          : { overId, zone },
      );
    },
    [setDropIndicator],
  );

  const resetDragState = useCallback(() => {
    setActiveDragId(null);
    setDropIndicator(null);
    dragDescendantsRef.current = null;
  }, [setActiveDragId, setDropIndicator]);

  const handleDragEnd = useCallback(
    async (event: DragEndEvent) => {
      const indicator = dropIndicator;
      const activeId = String(event.active.id);
      resetDragState();
      if (!indicator) return;
      const target = computeDropTarget(pages, indicator.overId, indicator.zone);
      if (!target) return;
      const result = applyReorder(pages, activeId, target);
      if (!result) return;
      const snapshot = pages;
      setPages(result.nextPages);
      if (target.parentId && !expandedNodes.has(target.parentId)) {
        setManualExpandedNodes((current) => {
          const next = new Set(current);
          next.add(target.parentId as string);
          return next;
        });
      }
      try {
        const updated = await Promise.all(
          result.patches.map((patch) =>
            updateDocPage(api, patch.id, {
              parent_id: patch.parent_id,
              sort_order: patch.sort_order,
            }),
          ),
        );
        setPages((current) => {
          const byId = new Map(updated.map((page) => [page.id, page]));
          // Move metadata only; bodies being edited stay as they are.
          return current.map((page) => {
            const moved = byId.get(page.id);
            return moved
              ? {
                  ...page,
                  parent_id: moved.parent_id,
                  sort_order: moved.sort_order,
                  updated_at: moved.updated_at,
                }
              : page;
          });
        });
      } catch {
        setPages((current) =>
          current.map((page) => {
            const before = snapshot.find((item) => item.id === page.id);
            return before
              ? { ...page, parent_id: before.parent_id, sort_order: before.sort_order }
              : page;
          }),
        );
      }
    },
    [
      api,
      dropIndicator,
      expandedNodes,
      pages,
      resetDragState,
      setManualExpandedNodes,
      setPages,
    ],
  );

  let pageBodyPlaceholder: ReactNode = null;
  if (needsPageBody) {
    pageBodyPlaceholder = failedPageId === activePageId ? (
      <div role="alert" className="space-y-2 py-4 text-app-ink">
        <p>{t('apps:docsApp.actionFailed')}</p>
        <button
          type="button"
          className="app-text-control text-app-accent hover:underline"
          onClick={() => setPageLoadAttempt((attempt) => attempt + 1)}
        >
          {t('common:actions.retry')}
        </button>
      </div>
    ) : (
      <div role="status" aria-label={t('apps:docsApp.loading')} className="py-6">
        <Loader2 size={24} className="animate-spin text-app-accent" />
      </div>
    );
  }
  const editorView = renderDocsEditor({
    activeContentFormat,
    activeDragId,
    activePage,
    activePageAuthorInitials,
    activePageAuthorName,
    activePageUploadFile,
    childCounts,
    contentEditorVersions,
    copiedDocId,
    dndSensors,
    docMenuOpen,
    docMenuRef,
    dropIndicator,
    editorLoading,
    expandedNodes,
    handleAddPage,
    handleBack,
    handleBlockMarkdownImportFile,
    handleCopyLink,
    handleDeleteDoc,
    handleDeletePage,
    handleDragEnd,
    handleDragOver,
    handleDragPointerMove,
    handleDragStart,
    handleDuplicateDoc,
    handleEditorChange,
    handleLivePageChange,
    handleKeepMine,
    handleOpenInNewTab,
    handleOpenVersions,
    handlePageTitleSave,
    handlePrintDoc,
    handleReloadConflict,
    handleRenameDoc,
    handleRetrySave: () => void retryContentSave(),
    handleSaveAsTemplate,
    handleToggleFavorite,
    highlightCommentId,
    loadError,
    locale,
    openShareModal,
    pageBodyPlaceholder,
    pages,
    pagesById,
    resetDragState,
    resolveFileUrl,
    saveState,
    selectDocPage,
    selectedDoc,
    selectedPageId,
    setDocMenuOpen,
    setReadModeOpen,
    t,
    timeZone,
    toggleExpand,
    visibleTree,
  });

  return renderDocsShell({
    activeCategoryLabel,
    changingLocation,
    confirmDialog,
    copyDirectLink,
    creating,
    creatingPage,
    docUrlFor,
    docs,
    editorView,
    fetchDocs,
    handleChangeLocation,
    handleChangeUserAccess,
    handleCloseVersions,
    handleCreateDoc,
    handleCreatePage,
    handleRestoreVersion,
    handleDeleteDoc,
    handleInviteUser,
    handleRemoveUserShare,
    handleRenameDoc,
    handleSearchChange,
    handleToggleFavorite,
    hasActiveRefinement,
    inviteFocused,
    inviteQuery,
    inviteSuggestions,
    isListView,
    linkCopied,
    listLoadFailed,
    loadingList,
    locale,
    locationOptions,
    menuOpenId,
    newDocLocation,
    newDocTemplateId,
    newDocTitle,
    newPageTemplateId,
    newPageTitle,
    openCreateModal,
    openDoc,
    page,
    pageBodyPlaceholder,
    pageCount,
    pages,
    promptDialog,
    readModeOpen,
    resetDocsFilters,
    resolveFileUrl,
    resolveLocationValueFromTarget,
    searchOpen,
    searchQuery,
    selectDocPage,
    selectedDoc,
    selectedPageId,
    setInviteFocused,
    setInviteQuery,
    setMenuOpenId,
    setNewDocLocation,
    setNewDocTemplateId,
    setNewDocTitle,
    setNewPageTemplateId,
    setNewPageTitle,
    setPage,
    setReadModeOpen,
    setSearchOpen,
    setShareAccessLevel,
    setShowCreateModal,
    setShowCreatePageModal,
    setShowShareModal,
    setSortBy,
    setSortDir,
    shareAccessLevel,
    shareLoading,
    sharingState,
    showCreateModal,
    showCreatePageModal,
    showShareModal,
    sortBy,
    sortDir,
    t,
    templateSpaceId: spaceId,
    docTemplateSpaceId: docSpaceId,
    timeZone,
    total,
    versionsPage,
  });
};

