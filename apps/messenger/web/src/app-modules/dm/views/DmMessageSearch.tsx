import { Loader2, Search } from 'lucide-react';
import { useEffect, useRef, useState, type FormEvent } from 'react';
import { useTranslation } from 'react-i18next';

import { Dialog } from '@miy/components/ui/lib/primitives/dialog';

import { searchDmMessages, type DmSearchHit } from '../api/dm-api';
import { formatDmMessageTime } from './dm-time';

/**
 * miy: message search (COMMON-FEATURES-PLAN §2.2 "보강"; AI-DO had none).
 * Text only, in the open conversation or in all conversations the caller is
 * in now; a hit jumps to the message. Only the newest request's answer is
 * shown (late answers of an earlier query or scope are dropped).
 */
export function DmMessageSearchDialog({
  conversation,
  onClose,
  onSelect,
  open,
  timeZone,
  token,
}: {
  conversation: { id: string; name: string } | null;
  onClose: () => void;
  onSelect: (hit: DmSearchHit) => void;
  open: boolean;
  timeZone?: string | null;
  token: string | null;
}) {
  const { i18n, t } = useTranslation('apps');
  const [query, setQuery] = useState('');
  const [scope, setScope] = useState<'conversation' | 'all'>('conversation');
  const [items, setItems] = useState<DmSearchHit[]>([]);
  const [cursor, setCursor] = useState<string | null>(null);
  const [loading, setLoading] = useState(false);
  const [searched, setSearched] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const seq = useRef(0);
  const effectiveScope = conversation ? scope : 'all';

  useEffect(() => {
    if (!open) {
      seq.current += 1;
      setLoading(false);
    }
  }, [open]);

  const run = async (append: boolean) => {
    const q = query.trim();
    if (!token || !q) return;
    const current = ++seq.current;
    setLoading(true);
    setError(null);
    try {
      const page = await searchDmMessages(token, {
        q,
        conversationId: effectiveScope === 'conversation' ? conversation?.id : null,
        cursor: append ? cursor : null,
        limit: 20,
      });
      if (current !== seq.current) return;
      setItems((existing) => (append ? [...existing, ...page.items] : page.items));
      setCursor(page.next_cursor);
      setSearched(true);
    } catch (caught) {
      if (current !== seq.current) return;
      setError(caught instanceof Error ? caught.message : t('dm.search.failed'));
    } finally {
      if (current === seq.current) setLoading(false);
    }
  };

  const submit = (event: FormEvent) => {
    event.preventDefault();
    void run(false);
  };

  return (
    <Dialog
      closeLabel={t('dm.safety.close')}
      maxWidth="max-w-2xl"
      onOpenChange={(next) => {
        if (!next) onClose();
      }}
      open={open}
      title={t('dm.search.title')}
    >
      <form className="grid gap-3" onSubmit={submit}>
        <div className="flex items-center gap-2 rounded-md border border-app-border bg-app-bg px-3 py-2">
          <Search size={15} className="shrink-0 text-app-ink/45" />
          <input
            aria-label={t('dm.search.placeholder')}
            autoFocus
            className="app-text-body w-full bg-transparent text-app-ink outline-none placeholder:text-app-ink/35"
            maxLength={200}
            onChange={(event) => setQuery(event.target.value)}
            placeholder={t('dm.search.placeholder')}
            value={query}
          />
          {loading ? <Loader2 size={14} className="animate-spin text-app-ink/45" /> : null}
        </div>
        {conversation ? (
          <div className="flex flex-wrap items-center gap-2" role="radiogroup">
            {(['conversation', 'all'] as const).map((value) => (
              <button
                aria-checked={effectiveScope === value}
                className={`app-text-caption rounded-full border px-3 py-1 ${
                  effectiveScope === value
                    ? 'border-app-ink bg-app-ink text-app-bg'
                    : 'border-app-border text-app-ink/70 hover:bg-app-surface-hover'
                }`}
                key={value}
                onClick={() => {
                  setScope(value);
                  setItems([]);
                  setCursor(null);
                  setSearched(false);
                  seq.current += 1;
                  setLoading(false);
                }}
                role="radio"
                type="button"
              >
                {value === 'conversation'
                  ? t('dm.search.scopeConversation', { name: conversation.name })
                  : t('dm.search.scopeAll')}
              </button>
            ))}
          </div>
        ) : null}
      </form>
      {error ? <p className="app-text-body-sm mt-3 text-app-danger-text">{error}</p> : null}
      <ul className="mt-3 grid max-h-[50vh] gap-1 overflow-y-auto">
        {searched && !items.length && !loading ? (
          <li className="app-text-body-sm py-6 text-center text-app-ink/45">{t('dm.search.empty')}</li>
        ) : null}
        {items.map((hit) => (
          <li key={hit.message_id}>
            <button
              className="w-full rounded-md px-3 py-2 text-left hover:bg-app-surface-hover"
              onClick={() => onSelect(hit)}
              type="button"
            >
              <span className="app-text-caption flex items-center justify-between gap-2 text-app-ink/50">
                <span className="truncate">
                  {effectiveScope === 'all' ? `${hit.conversation_name} · ` : ''}
                  {hit.sender_name}
                </span>
                <span className="shrink-0">
                  {formatDmMessageTime(hit.created_at, i18n.language, timeZone)}
                </span>
              </span>
              <span className="app-text-body-sm mt-0.5 block break-words text-app-ink">
                <Highlighted query={query.trim()} text={hit.snippet} />
              </span>
            </button>
          </li>
        ))}
        {cursor ? (
          <li>
            <button
              className="app-text-caption mt-1 w-full rounded-md border border-app-border py-2 text-app-ink/70 hover:bg-app-surface-hover disabled:opacity-50"
              disabled={loading}
              onClick={() => void run(true)}
              type="button"
            >
              {t('dm.search.loadMore')}
            </button>
          </li>
        ) : null}
      </ul>
    </Dialog>
  );
}

/** The query inside a snippet, case-insensitive, as plain text. */
function Highlighted({ query, text }: { query: string; text: string }) {
  if (!query) return <>{text}</>;
  const lower = text.toLocaleLowerCase();
  const needle = query.toLocaleLowerCase();
  const at = lower.indexOf(needle);
  if (at < 0) return <>{text}</>;
  return (
    <>
      {text.slice(0, at)}
      <mark className="rounded bg-app-accent/20 px-0.5 text-app-ink">
        {text.slice(at, at + query.length)}
      </mark>
      {text.slice(at + query.length)}
    </>
  );
}
