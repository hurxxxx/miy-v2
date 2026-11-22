import { useCallback, useEffect, useRef, useState } from 'react';
import { useTranslation } from 'react-i18next';

import { useFeedback } from '@miy/components/ui/lib/feedback/feedback-provider';

import { UserDateTime } from '../../components/date/UserDateTime';

import {
  addDmSafetyFilterTerm,
  downloadDmSafetyEvidenceAttachment,
  getDmSafetyEvidence,
  getDmSafetyReport,
  listDmSafetyFilterTerms,
  listDmSafetyReports,
  removeDmSafetyFilterTerm,
  resolveDmSafetyReport,
  setDmSafetyMessageVisibility,
  type DmSafetyEvidence,
  type DmSafetyFilterTerm,
  type DmSafetyPerson,
  type DmSafetyReportDetail,
  type DmSafetyReportStatus,
  type DmSafetyReportSummary,
  type DmSafetyResolveRequest,
} from './admin-dm-safety-api';

const STATUSES: DmSafetyReportStatus[] = ['open', 'actioned', 'dismissed'];
const BUTTON =
  'app-text-body-sm rounded-md border border-app-border px-3 py-1.5 text-app-ink hover:bg-app-surface-hover disabled:opacity-50';

function personLabel(person: DmSafetyPerson | null): string {
  if (!person) return '-';
  const name = person.display_name || person.full_name || person.email;
  return person.primary_org_unit_name ? `${name} (${person.primary_org_unit_name})` : name;
}

function errorText(error: unknown, fallback: string): string {
  return error instanceof Error && error.message ? error.message : fallback;
}

/**
 * DM 신고 검토(ADR 0025 개정 2026-11-18). 목록에는 분류·상태만, 상세에 메모·신원, 증거는 신고한 메시지와 그 첨부뿐이다.
 * 대화를 둘러보는 화면은 없다. 열람·처리는 모두 서버 감사에 남는다.
 */
export function AdminDmSafetySection({
  initialReportId,
  token,
}: {
  /** miy: a report opened from its platform notification (`/safety/reports/{id}`). */
  initialReportId?: string | null;
  token: string;
}) {
  const { t } = useTranslation('apps');
  const feedback = useFeedback();
  const [status, setStatus] = useState<DmSafetyReportStatus>('open');
  const [items, setItems] = useState<DmSafetyReportSummary[]>([]);
  const [hasMore, setHasMore] = useState(false);
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [detail, setDetail] = useState<DmSafetyReportDetail | null>(null);
  const [evidence, setEvidence] = useState<DmSafetyEvidence | null>(null);
  const [resolutionNote, setResolutionNote] = useState('');
  const [busy, setBusy] = useState(false);
  // 마지막으로 연 신고 — 늦게 온 응답(상세·증거·처리 결과)이 다른 신고 화면에 붙지 않게 한다.
  const activeReportIdRef = useRef<string | null>(null);
  // 목록 요청 세대 — 상태 탭을 바꾸거나 새로고침하면 앞선 요청(더 보기 포함)의 응답은 버린다.
  const listSeqRef = useRef(0);
  // 화면의 증거는 언제나 지금 상세의 신고 것만 그린다.
  const shownEvidence = detail && evidence?.report_id === detail.id ? evidence : null;

  const load = useCallback(async () => {
    const seq = ++listSeqRef.current;
    setLoading(true);
    try {
      const response = await listDmSafetyReports(token, { status });
      if (seq !== listSeqRef.current) return;
      setItems(response.items);
      setHasMore(response.has_more);
      setError(null);
    } catch (caught) {
      if (seq !== listSeqRef.current) return;
      setError(errorText(caught, t('admin.console.dmSafety.loadFailed')));
    } finally {
      if (seq === listSeqRef.current) setLoading(false);
    }
  }, [status, t, token]);

  useEffect(() => {
    void load();
  }, [load]);

  useEffect(() => {
    if (initialReportId) void openDetail(initialReportId);
    // eslint-disable-next-line react-hooks/exhaustive-deps -- open once per route
  }, [initialReportId]);

  /** 같은 상태의 더 이전 신고 — 서버가 created_at 내림차순으로 주므로 마지막 항목 시각을 before 로 넘긴다. */
  const loadMore = async () => {
    const last = items[items.length - 1];
    if (!last || loading) return;
    const seq = listSeqRef.current;
    setLoading(true);
    try {
      const response = await listDmSafetyReports(token, { status, before: last.created_at });
      if (seq !== listSeqRef.current) return;
      setItems((current) => {
        const seen = new Set(current.map((item) => item.id));
        return [...current, ...response.items.filter((item) => !seen.has(item.id))];
      });
      setHasMore(response.has_more);
      setError(null);
    } catch (caught) {
      if (seq !== listSeqRef.current) return;
      setError(errorText(caught, t('admin.console.dmSafety.loadFailed')));
    } finally {
      if (seq === listSeqRef.current) setLoading(false);
    }
  };

  const openDetail = async (reportId: string) => {
    activeReportIdRef.current = reportId;
    setDetail(null);
    setEvidence(null);
    setResolutionNote('');
    try {
      const next = await getDmSafetyReport(token, reportId);
      if (activeReportIdRef.current === reportId) setDetail(next);
    } catch (caught) {
      if (activeReportIdRef.current !== reportId) return;
      feedback.error(errorText(caught, t('admin.console.dmSafety.loadFailed')));
    }
  };

  const openEvidence = async () => {
    if (!detail) return;
    const reportId = detail.id;
    try {
      const next = await getDmSafetyEvidence(token, reportId);
      if (activeReportIdRef.current === reportId && next.report_id === reportId) {
        setEvidence(next);
      }
    } catch (caught) {
      if (activeReportIdRef.current !== reportId) return;
      feedback.error(errorText(caught, t('admin.console.dmSafety.loadFailed')));
    }
  };

  const resolve = async (
    outcome: DmSafetyResolveRequest['outcome'],
    action: DmSafetyResolveRequest['action'],
  ) => {
    if (!detail || busy) return;
    const reportId = detail.id;
    setBusy(true);
    try {
      const next = await resolveDmSafetyReport(token, reportId, {
        expected_status: 'open',
        outcome,
        action,
        resolution_note: resolutionNote.trim() || null,
      });
      if (activeReportIdRef.current === reportId) setDetail(next);
      feedback.success(t('admin.console.dmSafety.resolved'));
      void load();
    } catch (caught) {
      feedback.error(errorText(caught, t('admin.console.dmSafety.actionFailed')));
    } finally {
      setBusy(false);
    }
  };

  const toggleHidden = async () => {
    if (!detail || detail.message_hidden == null || busy) return;
    const reportId = detail.id;
    setBusy(true);
    try {
      const result = await setDmSafetyMessageVisibility(token, reportId, !detail.message_hidden);
      if (activeReportIdRef.current !== reportId) return;
      setDetail((current) =>
        current?.id === reportId ? { ...current, message_hidden: result.hidden } : current,
      );
      setEvidence((current) =>
        current?.report_id === reportId && current.message
          ? { ...current, message: { ...current.message, hidden: result.hidden } }
          : current,
      );
    } catch (caught) {
      feedback.error(errorText(caught, t('admin.console.dmSafety.actionFailed')));
    } finally {
      setBusy(false);
    }
  };

  const category = (value: string) =>
    t(`dm.safety.categories.${value}`, { defaultValue: value });

  return (
    <div className="grid gap-4">
      <p className="app-text-body-sm text-app-ink/65">{t('admin.console.dmSafety.scopeNote')}</p>
      <DmSafetyFilterTerms token={token} />
      <div className="flex flex-wrap items-center gap-2" role="tablist">
        {STATUSES.map((item) => (
          <button
            aria-selected={status === item}
            className={`${BUTTON} ${status === item ? 'bg-app-ink text-app-bg' : ''}`}
            key={item}
            onClick={() => {
              activeReportIdRef.current = null;
              setStatus(item);
              setDetail(null);
              setEvidence(null);
            }}
            role="tab"
            type="button"
          >
            {t(`admin.console.dmSafety.status.${item}`)}
          </button>
        ))}
        <button className={BUTTON} disabled={loading} onClick={() => void load()} type="button">
          {t('admin.console.dmSafety.refresh')}
        </button>
      </div>
      {error ? <div className="app-text-body-sm text-app-danger-text">{error}</div> : null}
      <div className="grid gap-4 lg:grid-cols-[minmax(0,1fr)_minmax(0,1.2fr)]">
        <ul className="grid content-start gap-2">
          {items.length === 0 && !loading ? (
            <li className="app-text-body-sm text-app-ink/50">{t('admin.console.dmSafety.empty')}</li>
          ) : null}
          {items.map((item) => (
            <li key={item.id}>
              <button
                className={`w-full rounded-md border px-3 py-2 text-left ${
                  detail?.id === item.id ? 'border-app-ink' : 'border-app-border'
                }`}
                onClick={() => void openDetail(item.id)}
                type="button"
              >
                <div className="app-text-body-sm font-medium text-app-ink">
                  {category(item.category)}
                  <span className="ml-2 text-app-ink/50">
                    {item.has_message
                      ? t('admin.console.dmSafety.messageReport')
                      : t('admin.console.dmSafety.userReport')}
                  </span>
                </div>
                <div className="app-text-caption text-app-ink/50">
                  <UserDateTime value={item.created_at} /> · {item.source}
                </div>
              </button>
            </li>
          ))}
          {hasMore ? (
            <li>
              <button
                className={`${BUTTON} w-full`}
                disabled={loading}
                onClick={() => void loadMore()}
                type="button"
              >
                {t('admin.console.dmSafety.loadMore')}
              </button>
            </li>
          ) : null}
        </ul>
        {detail ? (
          <section className="grid content-start gap-3 rounded-md border border-app-border p-4">
            <div className="app-text-title-sm text-app-ink">{category(detail.category)}</div>
            <dl className="app-text-body-sm grid grid-cols-[auto_1fr] gap-x-3 gap-y-1">
              <dt className="text-app-ink/50">{t('admin.console.dmSafety.reporter')}</dt>
              <dd>{personLabel(detail.reporter)}</dd>
              <dt className="text-app-ink/50">{t('admin.console.dmSafety.reportedUser')}</dt>
              <dd>{personLabel(detail.reported_user)}</dd>
              <dt className="text-app-ink/50">{t('admin.console.dmSafety.note')}</dt>
              <dd className="whitespace-pre-wrap break-words">{detail.note || '-'}</dd>
              <dt className="text-app-ink/50">{t('admin.console.dmSafety.statusLabel')}</dt>
              <dd>
                {t(`admin.console.dmSafety.status.${detail.status}`)}
                {detail.action ? ` · ${t(`admin.console.dmSafety.action.${detail.action}`)}` : ''}
              </dd>
              {detail.resolution_note ? (
                <>
                  <dt className="text-app-ink/50">{t('admin.console.dmSafety.resolutionNote')}</dt>
                  <dd className="whitespace-pre-wrap break-words">{detail.resolution_note}</dd>
                </>
              ) : null}
            </dl>
            {detail.has_message ? (
              <div className="grid gap-2">
                {shownEvidence?.message ? (
                  <div className="rounded-md bg-app-surface p-3">
                    <div className="app-text-caption text-app-ink/50">
                      {personLabel(shownEvidence.message.sender)} ·{' '}
                      <UserDateTime value={shownEvidence.message.created_at} />
                      {shownEvidence.message.hidden ? ` · ${t('admin.console.dmSafety.hiddenBadge')}` : ''}
                      {shownEvidence.message.deleted_at ? ` · ${t('dm.manager.deletedBadge')}` : ''}
                      {shownEvidence.message.edited_at ? ` · ${t('dm.edited')}` : ''}
                    </div>
                    <div className="app-text-body-sm mt-1 whitespace-pre-wrap break-words text-app-ink">
                      {shownEvidence.message.body}
                    </div>
                    {shownEvidence.message.revisions?.length ? (
                      <div className="mt-2 grid gap-1 border-t border-app-border pt-2">
                        <div className="app-text-caption font-medium text-app-ink/60">
                          {t('dm.manager.history')}
                        </div>
                        {shownEvidence.message.revisions.map((revision, index) => (
                          <div className="app-text-caption text-app-ink/70" key={`${revision.created_at}-${index}`}>
                            <UserDateTime value={revision.created_at} /> ·{' '}
                            {revision.action === 'edit'
                              ? t('dm.manager.historyEdit')
                              : t('dm.manager.historyDelete')}
                            :{' '}
                            <span className="whitespace-pre-wrap break-words">
                              {revision.body_before ?? t('dm.manager.historyRemoved')}
                            </span>
                          </div>
                        ))}
                      </div>
                    ) : null}
                    {shownEvidence.message.attachments.map((attachment) => (
                      <button
                        className={`${BUTTON} mt-2 mr-2`}
                        key={attachment.id}
                        onClick={() =>
                          void downloadDmSafetyEvidenceAttachment(token, detail.id, attachment).catch(
                            (caught: unknown) =>
                              feedback.error(errorText(caught, t('admin.console.dmSafety.actionFailed'))),
                          )
                        }
                        type="button"
                      >
                        {attachment.filename}
                      </button>
                    ))}
                  </div>
                ) : (
                  <button className={BUTTON} onClick={() => void openEvidence()} type="button">
                    {t('admin.console.dmSafety.openEvidence')}
                  </button>
                )}
                {detail.message_hidden != null ? (
                  <button className={BUTTON} disabled={busy} onClick={() => void toggleHidden()} type="button">
                    {detail.message_hidden
                      ? t('admin.console.dmSafety.restoreMessage')
                      : t('admin.console.dmSafety.hideMessage')}
                  </button>
                ) : null}
              </div>
            ) : null}
            {detail.status === 'open' ? (
              <div className="grid gap-2">
                <textarea
                  aria-label={t('admin.console.dmSafety.resolutionNote')}
                  className="app-text-body-sm min-h-16 rounded-md border border-app-border bg-app-bg px-3 py-2"
                  maxLength={1000}
                  onChange={(event) => setResolutionNote(event.target.value)}
                  placeholder={t('admin.console.dmSafety.resolutionNote')}
                  value={resolutionNote}
                />
                <div className="flex flex-wrap gap-2">
                  {detail.has_message ? (
                    <button
                      className={BUTTON}
                      disabled={busy}
                      onClick={() => void resolve('actioned', 'hide_message')}
                      type="button"
                    >
                      {t('admin.console.dmSafety.resolveHide')}
                    </button>
                  ) : null}
                  <button
                    className={BUTTON}
                    disabled={busy}
                    onClick={() => void resolve('actioned', 'none')}
                    type="button"
                  >
                    {t('admin.console.dmSafety.resolveNoAction')}
                  </button>
                  <button
                    className={BUTTON}
                    disabled={busy}
                    onClick={() => void resolve('dismissed', 'none')}
                    type="button"
                  >
                    {t('admin.console.dmSafety.dismiss')}
                  </button>
                </div>
              </div>
            ) : null}
          </section>
        ) : null}
      </div>
    </div>
  );
}

/** 관리자 금칙어 — 일치하면 쓰기를 거절한다(AI 판정 없음, 첨부 내용은 보지 않는다). */
function DmSafetyFilterTerms({ token }: { token: string }) {
  const { t } = useTranslation('apps');
  const feedback = useFeedback();
  const [items, setItems] = useState<DmSafetyFilterTerm[]>([]);
  const [limit, setLimit] = useState(0);
  const [term, setTerm] = useState('');
  const [busy, setBusy] = useState(false);

  const load = useCallback(async () => {
    try {
      const response = await listDmSafetyFilterTerms(token);
      setItems(response.items);
      setLimit(response.limit);
    } catch (caught) {
      feedback.error(errorText(caught, t('admin.console.dmSafety.loadFailed')));
    }
  }, [feedback, t, token]);

  useEffect(() => {
    void load();
  }, [load]);

  const add = async () => {
    if (!term.trim() || busy) return;
    setBusy(true);
    try {
      await addDmSafetyFilterTerm(token, term.trim());
      setTerm('');
      await load();
    } catch (caught) {
      feedback.error(errorText(caught, t('admin.console.dmSafety.actionFailed')));
    } finally {
      setBusy(false);
    }
  };

  const remove = async (id: string) => {
    setBusy(true);
    try {
      await removeDmSafetyFilterTerm(token, id);
      await load();
    } catch (caught) {
      feedback.error(errorText(caught, t('admin.console.dmSafety.actionFailed')));
    } finally {
      setBusy(false);
    }
  };

  return (
    <section className="grid gap-2 rounded-md border border-app-border p-4">
      <div className="app-text-title-sm text-app-ink">{t('admin.console.dmSafety.filterTitle')}</div>
      <p className="app-text-caption text-app-ink/60">{t('admin.console.dmSafety.filterScope')}</p>
      <div className="flex flex-wrap gap-2">
        <input
          aria-label={t('admin.console.dmSafety.filterInput')}
          className="app-text-body-sm min-w-48 flex-1 rounded-md border border-app-border bg-app-bg px-3 py-1.5"
          maxLength={50}
          onChange={(event) => setTerm(event.target.value)}
          placeholder={t('admin.console.dmSafety.filterInput')}
          value={term}
        />
        <button className={BUTTON} disabled={busy || !term.trim()} onClick={() => void add()} type="button">
          {t('admin.console.dmSafety.filterAdd')}
        </button>
      </div>
      <div className="app-text-caption text-app-ink/50">
        {t('admin.console.dmSafety.filterCount', { count: items.length, limit })}
      </div>
      <ul className="flex flex-wrap gap-1.5">
        {items.map((item) => (
          <li key={item.id}>
            <button
              aria-label={t('admin.console.dmSafety.filterRemove', { term: item.term })}
              className="app-text-caption rounded-full border border-app-border px-2 py-1 text-app-ink hover:bg-app-surface-hover"
              disabled={busy}
              onClick={() => void remove(item.id)}
              type="button"
            >
              {item.term} ×
            </button>
          </li>
        ))}
      </ul>
    </section>
  );
}
