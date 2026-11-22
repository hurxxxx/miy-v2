import { useEffect, useId, useState } from 'react';
import { useTranslation } from 'react-i18next';

import { Dialog } from '@miy/components/ui/lib/primitives/dialog';

import type {
  DmBlockItem,
  DmMessage,
  DmReportCategory,
  DmSafetyCapability,
} from '../api/dm-api';
import { displayDmUserName } from './dm-thread-list-model';

/**
 * DM 신고·1:1 차단 대화 상자(ADR 0025 개정 2026-11-18).
 *
 * 신고는 고른 메시지(와 그 첨부)·메모만 관리자에게 간다는 것을, 차단은 1:1 만 막고 함께 있는 단체 대화는 그대로
 * 보인다는 것을 화면에서 밝힌다.
 */
export function DmReportDialog({
  message,
  onClose,
  onSubmit,
  safety,
}: {
  message: DmMessage | null;
  onClose: () => void;
  onSubmit: (input: {
    message: DmMessage;
    category: DmReportCategory;
    note: string;
    block: boolean;
  }) => Promise<void>;
  safety: DmSafetyCapability | null;
}) {
  const { t } = useTranslation('apps');
  const noteId = useId();
  const [category, setCategory] = useState<DmReportCategory>('harassment');
  const [note, setNote] = useState('');
  const [block, setBlock] = useState(false);
  const [submitting, setSubmitting] = useState(false);

  useEffect(() => {
    if (message) {
      setCategory('harassment');
      setNote('');
      setBlock(false);
      setSubmitting(false);
    }
  }, [message]);

  const categories = (safety?.report_categories ?? []) as DmReportCategory[];
  const noteMax = safety?.report_note_max_length ?? 1000;

  const submit = async () => {
    if (!message || submitting) return;
    setSubmitting(true);
    try {
      await onSubmit({ message, category, note: note.trim(), block });
    } finally {
      setSubmitting(false);
    }
  };

  return (
    <Dialog
      closeLabel={t('dm.safety.cancel')}
      maxWidth="max-w-md"
      onOpenChange={(open) => {
        if (!open) onClose();
      }}
      open={Boolean(message)}
      title={t('dm.safety.reportTitle')}
    >
      <div className="grid gap-4 p-5">
        <p className="app-text-body-sm text-app-ink/70">
          {t('dm.safety.reportScope')}
        </p>
        <fieldset className="grid gap-2">
          <legend className="app-text-body-sm mb-1 font-medium text-app-ink">
            {t('dm.safety.categoryLabel')}
          </legend>
          {categories.map((item) => (
            <label
              className="app-text-body-sm flex items-center gap-2 text-app-ink"
              key={item}
            >
              <input
                checked={category === item}
                name="dm-report-category"
                onChange={() => setCategory(item)}
                type="radio"
                value={item}
              />
              {t(`dm.safety.categories.${item}`, { defaultValue: item })}
            </label>
          ))}
        </fieldset>
        <div className="grid gap-1">
          <label
            className="app-text-body-sm font-medium text-app-ink"
            htmlFor={noteId}
          >
            {t('dm.safety.noteLabel')}
          </label>
          <textarea
            className="app-text-body-sm min-h-20 rounded-md border border-app-border bg-app-bg px-3 py-2 text-app-ink"
            id={noteId}
            maxLength={noteMax}
            onChange={(event) => setNote(event.target.value)}
            value={note}
          />
          <span className="app-text-caption text-app-ink/45">
            {t('dm.safety.noteCount', { count: note.length, max: noteMax })}
          </span>
        </div>
        {safety?.block ? (
          <label className="app-text-body-sm flex items-start gap-2 text-app-ink">
            <input
              checked={block}
              className="mt-1"
              onChange={(event) => setBlock(event.target.checked)}
              type="checkbox"
            />
            <span>
              {t('dm.safety.reportAlsoBlock')}
              <span className="app-text-caption block text-app-ink/55">
                {t('dm.safety.blockScope')}
              </span>
            </span>
          </label>
        ) : null}
        <div className="flex justify-end gap-2">
          <button
            className="app-text-body-sm rounded-md border border-app-border px-3 py-2 text-app-ink"
            onClick={onClose}
            type="button"
          >
            {t('dm.safety.cancel')}
          </button>
          <button
            className="app-text-body-sm rounded-md bg-app-ink px-3 py-2 text-app-bg disabled:opacity-50"
            disabled={submitting || !categories.length}
            onClick={() => void submit()}
            type="button"
          >
            {t('dm.safety.submitReport')}
          </button>
        </div>
      </div>
    </Dialog>
  );
}

export function DmBlockDialog({
  blocked,
  name,
  onClose,
  onConfirm,
  open,
}: {
  blocked: boolean;
  name: string;
  onClose: () => void;
  onConfirm: () => Promise<void>;
  open: boolean;
}) {
  const { t } = useTranslation('apps');
  const [busy, setBusy] = useState(false);

  const confirm = async () => {
    if (busy) return;
    setBusy(true);
    try {
      await onConfirm();
    } finally {
      setBusy(false);
    }
  };

  return (
    <Dialog
      closeLabel={t('dm.safety.cancel')}
      maxWidth="max-w-md"
      onOpenChange={(next) => {
        if (!next) onClose();
      }}
      open={open}
      title={blocked ? t('dm.safety.unblockTitle', { name }) : t('dm.safety.blockTitle', { name })}
    >
      <div className="grid gap-4 p-5">
        <p className="app-text-body-sm text-app-ink/70">
          {blocked ? t('dm.safety.unblockScope') : t('dm.safety.blockScope')}
        </p>
        <div className="flex justify-end gap-2">
          <button
            className="app-text-body-sm rounded-md border border-app-border px-3 py-2 text-app-ink"
            onClick={onClose}
            type="button"
          >
            {t('dm.safety.cancel')}
          </button>
          <button
            className="app-text-body-sm rounded-md bg-app-ink px-3 py-2 text-app-bg disabled:opacity-50"
            disabled={busy}
            onClick={() => void confirm()}
            type="button"
          >
            {blocked ? t('dm.safety.unblock') : t('dm.safety.block')}
          </button>
        </div>
      </div>
    </Dialog>
  );
}

/**
 * 내가 1:1 차단한 사람 목록과 해제. 단체 대화에서 신고하며 차단했고 그 사람과 1:1 대화가 없으면(서버가 차단 관계로
 * 새 1:1 대화를 막는다) 대화 머리의 해제 버튼이 없으므로, 여기서 풀 수 있어야 한다.
 */
export function DmBlockedUsersDialog({
  loadBlocks,
  onClose,
  onUnblock,
  open,
}: {
  loadBlocks: () => Promise<DmBlockItem[]>;
  onClose: () => void;
  /** 풀렸으면 true — 목록에서 뺀다. 실패 알림은 부르는 쪽이 띄운다. */
  onUnblock: (userId: string) => Promise<boolean>;
  open: boolean;
}) {
  const { t } = useTranslation('apps');
  const [items, setItems] = useState<DmBlockItem[] | null>(null);
  const [failed, setFailed] = useState(false);
  const [busyUserId, setBusyUserId] = useState<string | null>(null);

  useEffect(() => {
    if (!open) return undefined;
    let cancelled = false;
    setItems(null);
    setFailed(false);
    loadBlocks()
      .then((next) => {
        if (!cancelled) setItems(next);
      })
      .catch(() => {
        if (!cancelled) setFailed(true);
      });
    return () => {
      cancelled = true;
    };
  }, [loadBlocks, open]);

  const unblock = async (userId: string) => {
    if (busyUserId) return;
    setBusyUserId(userId);
    try {
      if (await onUnblock(userId)) {
        setItems((current) =>
          current ? current.filter((item) => item.user.id !== userId) : current,
        );
      }
    } finally {
      setBusyUserId(null);
    }
  };

  return (
    <Dialog
      closeLabel={t('dm.safety.cancel')}
      maxWidth="max-w-md"
      onOpenChange={(next) => {
        if (!next) onClose();
      }}
      open={open}
      title={t('dm.safety.blockedUsers')}
    >
      <div className="grid gap-3 p-5">
        <p className="app-text-body-sm text-app-ink/70">
          {t('dm.safety.blockedUsersScope')}
        </p>
        {failed ? (
          <p className="app-text-body-sm text-app-danger-text">
            {t('dm.safety.blockedUsersLoadFailed')}
          </p>
        ) : items === null ? null : items.length === 0 ? (
          <p className="app-text-body-sm text-app-ink/50">
            {t('dm.safety.blockedUsersEmpty')}
          </p>
        ) : (
          <ul className="grid gap-2">
            {items.map((item) => {
              const name = displayDmUserName(item.user);
              return (
                <li
                  className="flex items-center justify-between gap-3 rounded-md border border-app-border px-3 py-2"
                  key={item.user.id}
                >
                  <span className="min-w-0">
                    <span className="app-text-body-sm block truncate text-app-ink">
                      {name}
                    </span>
                    {item.user.primary_org_unit_name ? (
                      <span className="app-text-caption block truncate text-app-ink/50">
                        {item.user.primary_org_unit_name}
                      </span>
                    ) : null}
                  </span>
                  <button
                    aria-label={t('dm.safety.unblockUser', { name })}
                    className="app-text-body-sm shrink-0 rounded-md border border-app-border px-3 py-1.5 text-app-ink disabled:opacity-50"
                    disabled={busyUserId !== null}
                    onClick={() => void unblock(item.user.id)}
                    type="button"
                  >
                    {t('dm.safety.unblock')}
                  </button>
                </li>
              );
            })}
          </ul>
        )}
        <div className="flex justify-end">
          <button
            className="app-text-body-sm rounded-md border border-app-border px-3 py-2 text-app-ink"
            onClick={onClose}
            type="button"
          >
            {t('dm.safety.close')}
          </button>
        </div>
      </div>
    </Dialog>
  );
}
