import { apiFetchFile, apiFetchJson } from '../api/client';

/**
 * 관리자 DM 신고 검토(ADR 0025 개정 2026-11-18). 증거는 신고한 메시지와 그 첨부뿐이다.
 * miy: AI-DO `/api/v1/admin/dm-safety` for platform administrators → the
 * messenger's own `/v1/dm-safety` for its manager role.
 */
const BASE = '/v1/dm-safety/reports';

export type DmSafetyReportStatus = 'open' | 'actioned' | 'dismissed';

export interface DmSafetyPerson {
  id: string;
  email: string;
  full_name: string;
  display_name?: string | null;
  primary_org_unit_name?: string | null;
}

export interface DmSafetyReportSummary {
  id: string;
  status: DmSafetyReportStatus;
  category: string;
  source: string;
  has_message: boolean;
  conversation_type: string;
  action: string | null;
  created_at: string;
  resolved_at: string | null;
}

export interface DmSafetyReportList {
  items: DmSafetyReportSummary[];
  has_more: boolean;
}

export interface DmSafetyReportDetail extends DmSafetyReportSummary {
  note: string | null;
  conversation_id: string;
  message_id: string | null;
  reporter: DmSafetyPerson | null;
  reported_user: DmSafetyPerson | null;
  resolution_note: string | null;
  resolved_by: DmSafetyPerson | null;
  message_hidden: boolean | null;
  /** miy: the author deleted the reported message. */
  message_deleted?: boolean | null;
}

export interface DmSafetyEvidenceAttachment {
  id: string;
  filename: string;
  content_type: string;
  size_bytes: number;
  is_image: boolean;
}

export interface DmSafetyEvidence {
  report_id: string;
  message: {
    id: string;
    conversation_id: string;
    sequence: number;
    sender: DmSafetyPerson | null;
    body: string;
    created_at: string;
    hidden: boolean;
    hidden_at: string | null;
    attachments: DmSafetyEvidenceAttachment[];
    /** miy: author edits and deletion; texts are kept only while a report is open. */
    edited_at?: string | null;
    deleted_at?: string | null;
    revisions?: { action: 'edit' | 'delete'; body_before: string | null; created_at: string }[];
  } | null;
}

export interface DmSafetyResolveRequest {
  expected_status: 'open';
  outcome: 'actioned' | 'dismissed';
  action: 'none' | 'hide_message';
  resolution_note?: string | null;
}

export interface DmSafetyVisibilityResult {
  report_id: string;
  message_id: string;
  hidden: boolean;
  changed: boolean;
}

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
    { method: 'POST', body: JSON.stringify({ hidden }) },
  );
}

export interface DmSafetyFilterTerm {
  id: string;
  term: string;
  created_at: string;
}

export interface DmSafetyFilterTermList {
  items: DmSafetyFilterTerm[];
  limit: number;
}

const FILTER_TERMS = '/v1/dm-safety/filter-terms';

/** 관리자 금칙어 — DM 본문·단체 이름·첨부 파일 이름·앱 PMS 댓글/업무 제목에 적용(첨부 내용은 보지 않는다). */
export function listDmSafetyFilterTerms(token: string): Promise<DmSafetyFilterTermList> {
  return apiFetchJson<DmSafetyFilterTermList>(FILTER_TERMS, token);
}

export function addDmSafetyFilterTerm(token: string, term: string): Promise<DmSafetyFilterTerm> {
  return apiFetchJson<DmSafetyFilterTerm>(FILTER_TERMS, token, {
    method: 'POST',
    body: JSON.stringify({ term }),
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

export interface DmSafetyManagerStatus {
  manager: boolean;
  claimable: boolean;
}

export interface DmSafetyManager {
  user: DmSafetyPerson;
  granted_by_id: string | null;
  created_at: string;
}

export interface DmSafetyTermsSettings {
  version: string;
  required: boolean;
  document_url: string | null;
  contact_email: string | null;
  contact_url: string | null;
}

const SAFETY = '/v1/dm-safety';

export function getDmSafetyManagerStatus(token: string): Promise<DmSafetyManagerStatus> {
  return apiFetchJson<DmSafetyManagerStatus>(`${SAFETY}/me`, token);
}

export function claimDmSafetyManager(token: string): Promise<{ items: DmSafetyManager[] }> {
  return apiFetchJson(`${SAFETY}/managers/claim`, token, { method: 'POST' });
}

export function listDmSafetyManagers(token: string): Promise<{ items: DmSafetyManager[] }> {
  return apiFetchJson(`${SAFETY}/managers`, token);
}

export function grantDmSafetyManager(
  token: string,
  userId: string,
): Promise<{ items: DmSafetyManager[] }> {
  return apiFetchJson(`${SAFETY}/managers`, token, {
    method: 'POST',
    body: JSON.stringify({ user_id: userId }),
  });
}

export function revokeDmSafetyManager(
  token: string,
  userId: string,
): Promise<{ items: DmSafetyManager[] }> {
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
    body: JSON.stringify(settings),
  });
}
