import type { components } from '../../api.generated';
import { apiFetchFile, apiFetchJson } from '../api/client';

/**
 * 관리자 DM 신고 검토(ADR 0025 개정 2026-11-18). 증거는 신고한 메시지와 그 첨부뿐이다.
 * miy: AI-DO `/api/v1/admin/dm-safety` for platform administrators → the
 * messenger's own `/v1/dm-safety` for its manager role.
 */
const BASE = '/v1/dm-safety/reports';

type WireSchema = components['schemas'];

export type DmSafetyReportStatus = WireSchema['ReportStatus'];
export type DmSafetyPerson = Pick<WireSchema['DmUserItem'], 'id' | 'email' | 'full_name'> &
  Partial<Pick<WireSchema['DmUserItem'], 'display_name' | 'primary_org_unit_name'>>;
export type DmSafetyReportSummary = WireSchema['ReportSummary'];
export type DmSafetyReportList = WireSchema['ReportsResponse'];

export type DmSafetyReportDetail = Omit<WireSchema['ReportDetail'],
  'reporter' | 'reported_user' | 'resolved_by' | 'message_deleted'> & {
    reporter: DmSafetyPerson | null;
    reported_user: DmSafetyPerson | null;
    resolved_by: DmSafetyPerson | null;
    message_deleted?: WireSchema['ReportDetail']['message_deleted'];
  };

export type DmSafetyEvidenceAttachment = WireSchema['EvidenceAttachment'];
type EvidenceMessage = Omit<WireSchema['EvidenceMessage'],
  'sender' | 'edited_at' | 'deleted_at' | 'revisions'> & {
    sender: DmSafetyPerson | null;
  } & Partial<Pick<WireSchema['EvidenceMessage'], 'edited_at' | 'deleted_at' | 'revisions'>>;
export type DmSafetyEvidence = Omit<WireSchema['EvidenceResponse'], 'message'> & {
  message: EvidenceMessage | null;
};
export type DmSafetyResolveRequest = WireSchema['ResolveRequest'];
export type DmSafetyVisibilityResult = WireSchema['VisibilityResponse'];

function reportPath(reportId: string, suffix = ''): string {
  return `${BASE}/${encodeURIComponent(reportId)}${suffix}`;
}

export function listDmSafetyReports(
  token: string,
  input: { status: DmSafetyReportStatus; before?: string | null; limit?: number },
): Promise<DmSafetyReportList> {
  const params = new URLSearchParams({
    status: input.status,
    limit: String(input.limit ?? 50),
  });
  if (input.before) params.set('before', input.before);
  return apiFetchJson<DmSafetyReportList>(`${BASE}?${params.toString()}`, token);
}

export function getDmSafetyReport(token: string, reportId: string): Promise<DmSafetyReportDetail> {
  return apiFetchJson<DmSafetyReportDetail>(reportPath(reportId), token);
}

export function getDmSafetyEvidence(token: string, reportId: string): Promise<DmSafetyEvidence> {
  return apiFetchJson<DmSafetyEvidence>(reportPath(reportId, '/evidence'), token);
}

export function resolveDmSafetyReport(
  token: string,
  reportId: string,
  request: DmSafetyResolveRequest,
): Promise<DmSafetyReportDetail> {
  return apiFetchJson<DmSafetyReportDetail>(reportPath(reportId, '/resolve'), token, {
    method: 'POST',
    body: JSON.stringify(request),
  });
}

export function setDmSafetyMessageVisibility(
  token: string,
  reportId: string,
  hidden: boolean,
): Promise<DmSafetyVisibilityResult> {
  return apiFetchJson<DmSafetyVisibilityResult>(
    reportPath(reportId, '/message-visibility'),
    token,
    { method: 'POST', body: JSON.stringify({ hidden } satisfies WireSchema['VisibilityRequest']) },
  );
}

export type DmSafetyFilterTerm = WireSchema['FilterTermItem'];
export type DmSafetyFilterTermList = WireSchema['FilterTermsResponse'];

const FILTER_TERMS = '/v1/dm-safety/filter-terms';

/** 관리자 금칙어 — DM 본문·단체 이름·첨부 파일 이름·앱 PMS 댓글/업무 제목에 적용(첨부 내용은 보지 않는다). */
export function listDmSafetyFilterTerms(token: string): Promise<DmSafetyFilterTermList> {
  return apiFetchJson<DmSafetyFilterTermList>(FILTER_TERMS, token);
}

export function addDmSafetyFilterTerm(token: string, term: string): Promise<DmSafetyFilterTerm> {
  return apiFetchJson<DmSafetyFilterTerm>(FILTER_TERMS, token, {
    method: 'POST',
    body: JSON.stringify({ term } satisfies WireSchema['FilterTermRequest']),
  });
}

export function removeDmSafetyFilterTerm(token: string, termId: string): Promise<void> {
  return apiFetchJson<void>(`${FILTER_TERMS}/${encodeURIComponent(termId)}`, token, {
    method: 'DELETE',
  });
}

/**
 * 증거 첨부는 인증된 경로로만 받는다(서명 주소 없음). 받은 파일은 브라우저 내려받기로 넘긴다.
 * miy: the bytes come through the file gateway (`runtime.fileApi`).
 */
export async function downloadDmSafetyEvidenceAttachment(
  token: string,
  reportId: string,
  attachment: DmSafetyEvidenceAttachment,
): Promise<void> {
  const response = await apiFetchFile(
    `/dm-safety/reports/${encodeURIComponent(reportId)}/evidence/attachments/${encodeURIComponent(attachment.id)}/content`,
    token,
    { cache: 'no-store' },
  );
  const url = URL.createObjectURL(await response.blob());
  try {
    const link = document.createElement('a');
    link.href = url;
    link.download = attachment.filename || 'attachment';
    document.body.appendChild(link);
    link.click();
    link.remove();
  } finally {
    setTimeout(() => URL.revokeObjectURL(url), 1000);
  }
}

// --- miy: the messenger manager role and content rule settings ----------------

export type DmSafetyManagerStatus = WireSchema['ManagerStatus'];
export type DmSafetyManager = Omit<WireSchema['ManagerItem'], 'user'> & { user: DmSafetyPerson };
export type DmSafetyTermsSettings = WireSchema['TermsSettingsItem'];
type DmSafetyManagers = Omit<WireSchema['ManagersResponse'], 'items'> & { items: DmSafetyManager[] };

const SAFETY = '/v1/dm-safety';

export function getDmSafetyManagerStatus(token: string): Promise<DmSafetyManagerStatus> {
  return apiFetchJson<DmSafetyManagerStatus>(`${SAFETY}/me`, token);
}

export function claimDmSafetyManager(token: string): Promise<DmSafetyManagers> {
  return apiFetchJson(`${SAFETY}/managers/claim`, token, { method: 'POST' });
}

export function listDmSafetyManagers(token: string): Promise<DmSafetyManagers> {
  return apiFetchJson(`${SAFETY}/managers`, token);
}

export function grantDmSafetyManager(
  token: string,
  userId: string,
): Promise<DmSafetyManagers> {
  return apiFetchJson(`${SAFETY}/managers`, token, {
    method: 'POST',
    body: JSON.stringify({ user_id: userId } satisfies WireSchema['ManagerGrantRequest']),
  });
}

export function revokeDmSafetyManager(
  token: string,
  userId: string,
): Promise<DmSafetyManagers> {
  return apiFetchJson(`${SAFETY}/managers/${encodeURIComponent(userId)}`, token, {
    method: 'DELETE',
  });
}

export function getDmSafetyTermsSettings(token: string): Promise<DmSafetyTermsSettings> {
  return apiFetchJson(`${SAFETY}/terms-settings`, token);
}

export function saveDmSafetyTermsSettings(
  token: string,
  settings: DmSafetyTermsSettings,
): Promise<DmSafetyTermsSettings> {
  return apiFetchJson(`${SAFETY}/terms-settings`, token, {
    method: 'PUT',
    body: JSON.stringify(settings satisfies WireSchema['TermsSettingsRequest']),
  });
}
