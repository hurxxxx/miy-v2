import { useCallback, useEffect, useRef, useState } from 'react';
import { useTranslation } from 'react-i18next';
import { CheckCircle2, Loader2, MessageSquare, RotateCcw } from 'lucide-react';
import { useConfirm, useFeedback } from '@miy/ui';

import { cn } from '../../../lib/utils';
import { useAppApi, useRuntime } from '../../../platform/runtime/runtime-context';
import {
  createDocPageComment,
  deleteDocPageComment,
  listDocPageComments,
  listShareableUsers,
  setDocPageCommentResolved,
  updateDocPageComment,
  type DocsComment,
  type DocsCommentMention,
  type ShareableUserItem,
} from '../api/docs-api';
import {
  commentRequestId,
  finishCommentRequest,
  getCommentDraft,
  setCommentDraft,
} from './docs-comment-drafts';
import {
  DOCS_COMMENT_MAX_CHARS,
  activeMentionQuery,
  countVisibleComments,
  insertMention,
  mentionIdsInBody,
  partitionThreads,
  splitCommentBody,
} from './docs-comments-model';
import { timeAgo } from './docs-view-model';

/**
 * Page comments (miy, 보강; AI-DO Docs had none): threads of one level of
 * replies, resolve/reopen, and `@` mentions of directory people. Everyone
 * who reads the page may comment; authors edit their own comments; authors
 * and doc managers delete them. Unsent text is kept per composer for the app
 * session (a failed send, a page switch or a late answer never drops it),
 * and a resend of the same comment reuses its request ID.
 */
export function DocsPageComments({
  docId,
  pageId,
  highlightCommentId,
}: {
  docId: string;
  pageId: string;
  highlightCommentId: string | null;
}) {
  const { t, i18n } = useTranslation(['apps', 'common']);
  const locale = i18n.resolvedLanguage ?? i18n.language;
  const api = useAppApi();
  const runtime = useRuntime();
  const feedback = useFeedback();
  const { confirm, confirmDialog } = useConfirm();
  const [threads, setThreads] = useState<DocsComment[]>([]);
  const [loaded, setLoaded] = useState(false);
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [replyTo, setReplyTo] = useState<string | null>(null);
  const [editing, setEditing] = useState<string | null>(null);
  const [showResolved, setShowResolved] = useState(false);
  const [busyId, setBusyId] = useState<string | null>(null);
  // A late answer for a page the reader left never paints over this one.
  const requested = useRef(pageId);
  const scrolledTo = useRef<string | null>(null);
  // Draft keys of this page's composers (docs-comment-drafts.ts).
  const scope = `${docId}:${pageId}`;

  const load = useCallback(async () => {
    requested.current = pageId;
    setLoading(true);
    try {
      const response = await listDocPageComments(api, pageId);
      if (requested.current !== pageId) return;
      setThreads(response.items);
      setError(null);
      setLoaded(true);
    } catch (caught) {
      if (requested.current !== pageId) return;
      setError(
        caught instanceof Error && caught.message
          ? caught.message
          : t('apps:docsApp.comments.loadFailed'),
      );
    } finally {
      if (requested.current === pageId) setLoading(false);
    }
  }, [api, pageId, t]);

  useEffect(() => {
    setThreads([]);
    setLoaded(false);
    setReplyTo(null);
    setEditing(null);
    void load();
  }, [load]);

  // A notification opens the page with `?comment=`: show that comment.
  useEffect(() => {
    if (!highlightCommentId || scrolledTo.current === highlightCommentId) return;
    const thread = threads.find(
      (item) =>
        item.id === highlightCommentId ||
        (item.replies ?? []).some((reply) => reply.id === highlightCommentId),
    );
    if (!thread) return;
    if (thread.resolved_at) setShowResolved(true);
    scrolledTo.current = highlightCommentId;
    const frame = window.requestAnimationFrame(() => {
      document
        .getElementById(`docs-comment-${highlightCommentId}`)
        ?.scrollIntoView({ block: 'center', behavior: 'smooth' });
    });
    return () => window.cancelAnimationFrame(frame);
  }, [highlightCommentId, threads]);

  const failed = (caught: unknown) =>
    feedback.error(
      caught instanceof Error && caught.message
        ? caught.message
        : t('apps:docsApp.actionFailed'),
    );

  const send = async (
    composerKey: string,
    body: string,
    mentionIds: string[],
    parentId: string | null,
  ): Promise<boolean> => {
    const pageAtStart = pageId;
    const id = commentRequestId(
      composerKey,
      JSON.stringify([pageAtStart, parentId, body, mentionIds]),
    );
    try {
      await createDocPageComment(api, pageAtStart, {
        body,
        parent_id: parentId,
        mention_ids: mentionIds,
        request_id: id,
      });
      finishCommentRequest(composerKey);
      if (requested.current === pageAtStart) {
        if (parentId) setReplyTo(null);
        void load();
      }
      return true;
    } catch (caught) {
      failed(caught);
      return false;
    }
  };

  const saveEdit = async (
    comment: DocsComment,
    body: string,
    mentionIds: string[],
  ): Promise<boolean> => {
    const pageAtStart = pageId;
    try {
      await updateDocPageComment(api, comment.id, { body, mention_ids: mentionIds });
      finishCommentRequest(`${scope}:edit:${comment.id}`);
      if (requested.current === pageAtStart) {
        setEditing(null);
        void load();
      }
      return true;
    } catch (caught) {
      failed(caught);
      return false;
    }
  };

  const remove = async (comment: DocsComment) => {
    const ok = await confirm({
      title: t('apps:docsApp.comments.deleteConfirm'),
      description: t('apps:docsApp.comments.deleteDescription'),
      confirmLabel: t('common:actions.delete'),
      cancelLabel: t('common:actions.cancel'),
      variant: 'danger',
    });
    if (!ok) return;
    setBusyId(comment.id);
    try {
      await deleteDocPageComment(api, comment.id);
      setCommentDraft(`${scope}:edit:${comment.id}`, null);
      if (requested.current === comment.page_id) void load();
    } catch (caught) {
      failed(caught);
    } finally {
      setBusyId(null);
    }
  };

  const toggleResolved = async (thread: DocsComment) => {
    setBusyId(thread.id);
    try {
      await setDocPageCommentResolved(api, thread.id, !thread.resolved_at);
      if (requested.current === thread.page_id) void load();
    } catch (caught) {
      failed(caught);
    } finally {
      setBusyId(null);
    }
  };

  const { open, resolved } = partitionThreads(threads);
  const count = countVisibleComments(threads);

  const renderComment = (comment: DocsComment, thread: DocsComment) => {
    const isEditing = editing === comment.id;
    return (
      <div
        id={`docs-comment-${comment.id}`}
        data-testid="docs-comment"
        className={cn(
          'rounded-md px-3 py-2',
          highlightCommentId === comment.id && 'ring-2 ring-app-accent/50',
        )}
      >
        <div className="app-text-caption flex flex-wrap items-center gap-2 text-app-ink/55">
          <span className="app-text-control text-app-ink">
            {comment.is_deleted ? t('apps:docsApp.comments.deletedAuthor') : comment.author_name}
          </span>
          <span>{timeAgo(comment.created_at, runtime.settings.timeZone, locale)}</span>
          {comment.updated_at !== comment.created_at && !comment.is_deleted ? (
            <span>{t('apps:docsApp.comments.edited')}</span>
          ) : null}
        </div>
        {comment.is_deleted ? (
          <p className="app-text-body-sm italic text-app-ink/45">
            {t('apps:docsApp.comments.deleted')}
          </p>
        ) : isEditing ? (
          <CommentComposer
            draftKey={`${scope}:edit:${comment.id}`}
            initialText={comment.body}
            initialMentions={comment.mentions}
            submitLabel={t('common:actions.save')}
            onSubmit={(body, mentionIds) => saveEdit(comment, body, mentionIds)}
            onCancel={() => {
              setCommentDraft(`${scope}:edit:${comment.id}`, null);
              setEditing(null);
            }}
            autoFocus
          />
        ) : (
          <p className="app-text-body-sm whitespace-pre-wrap break-words text-app-ink">
            {splitCommentBody(comment.body, comment.mentions).map((segment, index) =>
              segment.mention ? (
                <span key={index} className="font-semibold text-app-accent">
                  {segment.text}
                </span>
              ) : (
                <span key={index}>{segment.text}</span>
              ),
            )}
          </p>
        )}
        {!comment.is_deleted && !isEditing ? (
          <div className="app-text-caption mt-1 flex flex-wrap gap-3 text-app-ink/55">
            {comment.parent_id === null && !thread.resolved_at ? (
              <button
                type="button"
                className="hover:text-app-accent"
                onClick={() => setReplyTo(thread.id)}
              >
                {t('apps:docsApp.comments.reply')}
              </button>
            ) : null}
            {comment.can_edit ? (
              <button
                type="button"
                className="hover:text-app-accent"
                onClick={() => setEditing(comment.id)}
              >
                {t('common:actions.edit')}
              </button>
            ) : null}
            {comment.can_delete ? (
              <button
                type="button"
                disabled={busyId === comment.id}
                className="hover:text-app-danger-text disabled:opacity-50"
                onClick={() => void remove(comment)}
              >
                {t('common:actions.delete')}
              </button>
            ) : null}
          </div>
        ) : null}
      </div>
    );
  };

  const renderThread = (thread: DocsComment) => (
    <li
      key={thread.id}
      className={cn(
        'rounded-lg border border-app-border bg-app-bg',
        thread.resolved_at && 'opacity-80',
      )}
    >
      <div className="flex items-start gap-2">
        <div className="min-w-0 flex-1">{renderComment(thread, thread)}</div>
        {thread.can_resolve ? (
          <button
            type="button"
            disabled={busyId === thread.id}
            onClick={() => void toggleResolved(thread)}
            className="app-text-control-sm m-2 inline-flex shrink-0 items-center gap-1 rounded-md px-2 py-1 text-app-ink/70 hover:bg-app-surface-hover disabled:opacity-50"
          >
            {thread.resolved_at ? <RotateCcw size={13} /> : <CheckCircle2 size={13} />}
            {thread.resolved_at
              ? t('apps:docsApp.comments.reopen')
              : t('apps:docsApp.comments.resolve')}
          </button>
        ) : null}
      </div>
      {thread.resolved_at ? (
        <p className="app-text-caption px-3 pb-1 text-app-ink/55">
          {t('apps:docsApp.comments.resolvedBy', { name: thread.resolved_by_name ?? '' })}
        </p>
      ) : null}
      {(thread.replies ?? []).length ? (
        <ul className="ml-4 border-l border-app-border">
          {(thread.replies ?? []).map((reply) => (
            <li key={reply.id}>{renderComment(reply, thread)}</li>
          ))}
        </ul>
      ) : null}
      {replyTo === thread.id && !thread.resolved_at ? (
        <div className="ml-4 border-l border-app-border px-3 pb-3">
          <CommentComposer
            draftKey={`${scope}:reply:${thread.id}`}
            placeholder={t('apps:docsApp.comments.replyPlaceholder')}
            submitLabel={t('apps:docsApp.comments.sendReply')}
            onSubmit={(body, mentionIds) =>
              send(`${scope}:reply:${thread.id}`, body, mentionIds, thread.id)
            }
            onCancel={() => {
              setCommentDraft(`${scope}:reply:${thread.id}`, null);
              setReplyTo(null);
            }}
            autoFocus
          />
        </div>
      ) : null}
    </li>
  );

  return (
    <section aria-labelledby="docs-page-comments-title" className="space-y-3 border-t border-app-border pt-6">
      {confirmDialog}
      <h2
        id="docs-page-comments-title"
        className="app-text-title-sm flex items-center gap-2 text-app-ink"
      >
        <MessageSquare size={16} className="text-app-ink/55" />
        {t('apps:docsApp.comments.title')}
        {count ? <span className="app-text-caption text-app-ink/55">{count}</span> : null}
        {loading ? <Loader2 size={14} className="animate-spin text-app-ink/45" /> : null}
      </h2>
      {error ? (
        <div role="alert" className="app-text-caption flex items-center gap-2 text-app-danger">
          <span>{error}</span>
          <button type="button" className="underline" onClick={() => void load()}>
            {t('apps:docsApp.attachments.retry')}
          </button>
        </div>
      ) : null}
      {loaded && !open.length && !resolved.length ? (
        <p className="app-text-caption text-app-ink/55">{t('apps:docsApp.comments.empty')}</p>
      ) : null}
      {open.length ? <ul className="space-y-2">{open.map(renderThread)}</ul> : null}
      {resolved.length ? (
        <div className="space-y-2">
          <button
            type="button"
            className="app-text-caption text-app-ink/55 hover:text-app-accent"
            onClick={() => setShowResolved((value) => !value)}
          >
            {showResolved
              ? t('apps:docsApp.comments.hideResolved')
              : t('apps:docsApp.comments.showResolved', { count: resolved.length })}
          </button>
          {showResolved ? <ul className="space-y-2">{resolved.map(renderThread)}</ul> : null}
        </div>
      ) : null}
      <CommentComposer
        draftKey={`${scope}:new`}
        placeholder={t('apps:docsApp.comments.placeholder')}
        submitLabel={t('apps:docsApp.comments.send')}
        onSubmit={(body, mentionIds) => send(`${scope}:new`, body, mentionIds, null)}
      />
    </section>
  );
}

/**
 * A comment text box with `@` mention suggestions from the directory. The
 * text is kept in the draft store on every change, so it survives a failed
 * send, a page switch and an unmount.
 */
function CommentComposer({
  draftKey,
  initialText = '',
  initialMentions = [],
  placeholder,
  submitLabel,
  onSubmit,
  onCancel,
  autoFocus = false,
}: {
  draftKey: string;
  initialText?: string;
  initialMentions?: DocsCommentMention[];
  placeholder?: string;
  submitLabel: string;
  onSubmit: (body: string, mentionIds: string[]) => Promise<boolean>;
  onCancel?: () => void;
  autoFocus?: boolean;
}) {
  const { t } = useTranslation(['apps', 'common']);
  const api = useAppApi();
  const stored = getCommentDraft(draftKey);
  const [text, setText] = useState(stored?.text ?? initialText);
  const [mentions, setMentions] = useState<DocsCommentMention[]>(
    stored?.mentions ?? initialMentions,
  );
  const [sending, setSending] = useState(false);
  const [query, setQuery] = useState<{ start: number; query: string } | null>(null);
  const [suggestions, setSuggestions] = useState<ShareableUserItem[]>([]);
  const [highlighted, setHighlighted] = useState(0);
  const inputRef = useRef<HTMLTextAreaElement>(null);

  const remember = (nextText: string, nextMentions: DocsCommentMention[]) => {
    setCommentDraft(
      draftKey,
      nextText === initialText ? null : { text: nextText, mentions: nextMentions },
    );
  };

  // Directory suggestions for the `@query` being typed (late answers for an
  // older query are dropped by the abort).
  useEffect(() => {
    if (!query) {
      setSuggestions([]);
      return;
    }
    const controller = new AbortController();
    const timer = window.setTimeout(() => {
      listShareableUsers(api, query.query || undefined, { signal: controller.signal })
        .then((users) => {
          if (controller.signal.aborted) return;
          setSuggestions(users.slice(0, 8));
          setHighlighted(0);
        })
        .catch(() => {
          if (!controller.signal.aborted) setSuggestions([]);
        });
    }, 200);
    return () => {
      controller.abort();
      window.clearTimeout(timer);
    };
  }, [api, query]);

  const change = (value: string, caret: number) => {
    setText(value);
    remember(value, mentions);
    setQuery(activeMentionQuery(value, caret));
  };

  const pick = (person: ShareableUserItem) => {
    const input = inputRef.current;
    if (!query || !input) return;
    const caret = input.selectionStart ?? text.length;
    const next = insertMention(text, query.start, caret, person.full_name);
    const nextMentions = mentions.some((item) => item.user_id === person.id)
      ? mentions
      : [...mentions, { user_id: person.id, name: person.full_name }];
    setText(next.text);
    setMentions(nextMentions);
    remember(next.text, nextMentions);
    setQuery(null);
    window.requestAnimationFrame(() => {
      input.focus();
      input.setSelectionRange(next.caret, next.caret);
    });
  };

  const submit = async () => {
    const body = text.trim();
    if (!body || sending || body.length > DOCS_COMMENT_MAX_CHARS) return;
    setSending(true);
    const ok = await onSubmit(body, mentionIdsInBody(body, mentions));
    setSending(false);
    if (ok) {
      setText('');
      setMentions([]);
      setQuery(null);
    }
  };

  return (
    <div className="relative mt-2 space-y-2">
      <textarea
        ref={inputRef}
        value={text}
        rows={2}
        maxLength={DOCS_COMMENT_MAX_CHARS}
        placeholder={placeholder}
        aria-label={placeholder ?? submitLabel}
        autoFocus={autoFocus}
        disabled={sending}
        onChange={(event) =>
          change(event.target.value, event.target.selectionStart ?? event.target.value.length)
        }
        onKeyDown={(event) => {
          if (query && suggestions.length) {
            if (event.key === 'ArrowDown') {
              event.preventDefault();
              setHighlighted((index) => (index + 1) % suggestions.length);
              return;
            }
            if (event.key === 'ArrowUp') {
              event.preventDefault();
              setHighlighted((index) => (index - 1 + suggestions.length) % suggestions.length);
              return;
            }
            if ((event.key === 'Enter' || event.key === 'Tab') && !event.nativeEvent.isComposing) {
              event.preventDefault();
              const person = suggestions[highlighted];
              if (person) pick(person);
              return;
            }
            if (event.key === 'Escape') {
              event.preventDefault();
              setQuery(null);
              return;
            }
          }
          if (event.key === 'Enter' && (event.metaKey || event.ctrlKey)) {
            event.preventDefault();
            void submit();
          }
        }}
        className="app-text-body-sm w-full resize-y rounded-md border border-app-border bg-app-bg px-3 py-2 text-app-ink focus:border-app-accent focus:outline-none disabled:opacity-60"
      />
      {query && suggestions.length ? (
        <ul
          role="listbox"
          aria-label={t('apps:docsApp.comments.mentionSuggestions')}
          className="absolute left-0 top-full z-30 mt-1 max-h-56 w-64 overflow-y-auto rounded-md border border-app-border bg-app-surface-sidebar py-1 shadow-lg"
        >
          {suggestions.map((person, index) => (
            <li key={person.id} role="option" aria-selected={index === highlighted}>
              <button
                type="button"
                onMouseDown={(event) => event.preventDefault()}
                onClick={() => pick(person)}
                className={cn(
                  'app-text-control-sm flex w-full flex-col px-3 py-1.5 text-left text-app-ink hover:bg-app-surface-hover',
                  index === highlighted && 'bg-app-surface-hover',
                )}
              >
                <span>{person.full_name}</span>
                {person.email ? (
                  <span className="app-text-caption text-app-ink/55">{person.email}</span>
                ) : null}
              </button>
            </li>
          ))}
        </ul>
      ) : null}
      <div className="flex items-center justify-end gap-2">
        <span className="app-text-caption mr-auto text-app-ink/45">
          {t('apps:docsApp.comments.mentionHint')}
        </span>
        {onCancel ? (
          <button
            type="button"
            onClick={onCancel}
            className="app-text-control-sm rounded-md border border-app-border px-3 py-1.5 text-app-ink hover:bg-app-surface-hover"
          >
            {t('common:actions.cancel')}
          </button>
        ) : null}
        <button
          type="button"
          onClick={() => void submit()}
          disabled={sending || !text.trim()}
          className="app-text-control-sm rounded-md bg-app-accent px-3 py-1.5 text-app-accent-fg hover:opacity-90 disabled:opacity-50"
        >
          {sending ? <Loader2 size={13} className="animate-spin" /> : submitLabel}
        </button>
      </div>
    </div>
  );
}
