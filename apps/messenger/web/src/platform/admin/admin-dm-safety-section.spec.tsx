import {
  act,
  cleanup,
  fireEvent,
  render,
  screen,
  waitFor,
} from '@testing-library/react';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

import { i18n } from '../i18n';

import {
  getDmSafetyEvidence,
  getDmSafetyReport,
  listDmSafetyFilterTerms,
  listDmSafetyReports,
  type DmSafetyEvidence,
  type DmSafetyReportDetail,
  type DmSafetyReportSummary,
} from './admin-dm-safety-api';
import { AdminDmSafetySection } from './admin-dm-safety-section';

const feedbackMock = vi.hoisted(() => ({
  error: vi.fn(),
  info: vi.fn(),
  success: vi.fn(),
  warning: vi.fn(),
}));

vi.mock('@miy/components/ui/lib/feedback/feedback-provider', async (importOriginal) => ({
  ...(await importOriginal<typeof import('@miy/components/ui/lib/feedback/feedback-provider')>()),
  useFeedback: () => feedbackMock,
}));

vi.mock('./admin-dm-safety-api', async (importOriginal) => {
  const actual = await importOriginal<typeof import('./admin-dm-safety-api')>();
  return {
    ...actual,
    addDmSafetyFilterTerm: vi.fn(),
    downloadDmSafetyEvidenceAttachment: vi.fn(),
    getDmSafetyEvidence: vi.fn(),
    getDmSafetyReport: vi.fn(),
    listDmSafetyFilterTerms: vi.fn(),
    listDmSafetyReports: vi.fn(),
    removeDmSafetyFilterTerm: vi.fn(),
    resolveDmSafetyReport: vi.fn(),
    setDmSafetyMessageVisibility: vi.fn(),
  };
});

vi.mock('../../components/date/UserDateTime', () => ({
  UserDateTime: ({ value }: { value: string }) => <time>{value}</time>,
}));

const listMock = vi.mocked(listDmSafetyReports);
const detailMock = vi.mocked(getDmSafetyReport);
const evidenceMock = vi.mocked(getDmSafetyEvidence);
const filterTermsMock = vi.mocked(listDmSafetyFilterTerms);

function summary(id: string, overrides: Partial<DmSafetyReportSummary> = {}): DmSafetyReportSummary {
  return {
    id,
    status: 'open',
    category: 'harassment',
    source: 'web',
    has_message: true,
    conversation_type: 'direct',
    action: null,
    created_at: '2026-11-18T00:00:00',
    resolved_at: null,
    ...overrides,
  };
}

function detailOf(item: DmSafetyReportSummary): DmSafetyReportDetail {
  return {
    ...item,
    note: null,
    conversation_id: `conv-${item.id}`,
    message_id: `msg-${item.id}`,
    reporter: null,
    reported_user: null,
    resolution_note: null,
    resolved_by: null,
    message_hidden: false,
  };
}

function evidenceOf(reportId: string, body: string): DmSafetyEvidence {
  return {
    report_id: reportId,
    message: {
      id: `msg-${reportId}`,
      conversation_id: `conv-${reportId}`,
      sequence: 1,
      sender: null,
      body,
      created_at: '2026-11-18T00:00:00',
      hidden: false,
      hidden_at: null,
      attachments: [],
    },
  };
}

function deferred<T>() {
  let resolve!: (value: T) => void;
  const promise = new Promise<T>((resolver) => {
    resolve = resolver;
  });
  return { promise, resolve };
}

function listButton(category: string) {
  return screen.getByRole('button', { name: new RegExp(category) });
}

describe('AdminDmSafetySection', () => {
  beforeEach(async () => {
    await i18n.changeLanguage('ko-KR');
    filterTermsMock.mockResolvedValue({ items: [], limit: 100 });
  });

  afterEach(() => {
    cleanup();
    vi.clearAllMocks();
  });

  it('drops evidence that arrives after another report was opened', async () => {
    const first = summary('r1', { category: 'harassment' });
    const second = summary('r2', { category: 'spam', created_at: '2026-11-17T00:00:00' });
    listMock.mockResolvedValue({ items: [first, second], has_more: false });
    detailMock.mockImplementation(async (_token, reportId) =>
      detailOf(reportId === 'r1' ? first : second),
    );
    const lateEvidence = deferred<DmSafetyEvidence>();
    evidenceMock.mockReturnValueOnce(lateEvidence.promise);

    render(<AdminDmSafetySection token="token" />);
    fireEvent.click(await screen.findByRole('button', { name: /괴롭힘/ }));
    fireEvent.click(await screen.findByRole('button', { name: '신고한 메시지 보기' }));
    expect(evidenceMock).toHaveBeenCalledWith('token', 'r1');

    fireEvent.click(listButton('스팸'));
    await waitFor(() => expect(detailMock).toHaveBeenLastCalledWith('token', 'r2'));
    await screen.findByRole('button', { name: '신고한 메시지 보기' });

    await act(async () => {
      lateEvidence.resolve(evidenceOf('r1', '첫 신고의 증거'));
      await lateEvidence.promise;
    });

    expect(screen.queryByText('첫 신고의 증거')).toBeNull();
    expect(screen.getByRole('button', { name: '신고한 메시지 보기' })).toBeTruthy();
  });

  it('keeps the newest detail when an earlier detail request resolves late', async () => {
    const first = summary('r1', { category: 'harassment' });
    const second = summary('r2', { category: 'spam', created_at: '2026-11-17T00:00:00' });
    listMock.mockResolvedValue({ items: [first, second], has_more: false });
    const lateDetail = deferred<DmSafetyReportDetail>();
    detailMock.mockImplementation((_token, reportId) =>
      reportId === 'r1' ? lateDetail.promise : Promise.resolve(detailOf(second)),
    );

    render(<AdminDmSafetySection token="token" />);
    fireEvent.click(await screen.findByRole('button', { name: /괴롭힘/ }));
    fireEvent.click(listButton('스팸'));
    await screen.findByRole('button', { name: '신고한 메시지 보기' });

    await act(async () => {
      lateDetail.resolve({ ...detailOf(first), note: '첫 신고 메모' });
      await lateDetail.promise;
    });

    expect(screen.queryByText('첫 신고 메모')).toBeNull();
    evidenceMock.mockResolvedValueOnce(evidenceOf('r2', '두 번째 증거'));
    fireEvent.click(screen.getByRole('button', { name: '신고한 메시지 보기' }));
    expect(await screen.findByText('두 번째 증거')).toBeTruthy();
    expect(evidenceMock).toHaveBeenLastCalledWith('token', 'r2');
  });

  it('pages older reports of the same status with the server cursor', async () => {
    const firstPage = Array.from({ length: 50 }, (_, index) =>
      summary(`r${index}`, {
        status: 'dismissed',
        created_at: `2026-11-18T00:${String(59 - index).padStart(2, '0')}:00`,
      }),
    );
    const older = summary('r-old', {
      status: 'dismissed',
      category: 'privacy',
      created_at: '2026-11-01T00:00:00',
    });
    listMock.mockImplementation(async (_token, input) => {
      if (input.status !== 'dismissed') return { items: [], has_more: false };
      return input.before
        ? { items: [older], has_more: false }
        : { items: firstPage, has_more: true };
    });

    render(<AdminDmSafetySection token="token" />);
    fireEvent.click(await screen.findByRole('tab', { name: '기각' }));
    const loadMore = await screen.findByRole('button', { name: '이전 신고 더 보기' });
    expect(screen.queryByRole('button', { name: /개인정보 노출/ })).toBeNull();

    fireEvent.click(loadMore);

    expect(await screen.findByRole('button', { name: /개인정보 노출/ })).toBeTruthy();
    expect(listMock).toHaveBeenLastCalledWith('token', {
      status: 'dismissed',
      before: firstPage[49]?.created_at,
    });
    expect(screen.queryByRole('button', { name: '이전 신고 더 보기' })).toBeNull();
  });
});
