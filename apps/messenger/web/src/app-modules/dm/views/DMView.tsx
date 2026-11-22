import {
  ArrowLeft,
  Ban,
  Bell,
  BellOff,
  ChevronLeft,
  ChevronRight,
  Download,
  ExternalLink,
  FileText,
  Flag,
  Image as ImageIcon,
  Loader2,
  MessageCircle,
  Paperclip,
  Pencil,
  Plus,
  Reply,
  Search,
  Send,
  ShieldAlert,
  SmilePlus,
  Trash2,
  Users,
  Wifi,
  WifiOff,
  X,
} from 'lucide-react';
import type {
  ChangeEvent,
  ClipboardEvent,
  DragEvent,
  MutableRefObject,
  ReactNode,
} from 'react';
import {
  Fragment,
  useCallback,
  useEffect,
  useLayoutEffect,
  useMemo,
  useReducer,
  useRef,
  useState,
} from 'react';
import { useTranslation } from 'react-i18next';

import { DM_REALTIME_EVENT_TYPES, replaceDmMessage } from '../../../contracts/dm';
import { StatusSlot } from '@miy/components/ui/lib/feedback/status-slot';
import { useFeedback } from '@miy/components/ui/lib/feedback/feedback-provider';
import { Dialog } from '@miy/components/ui/lib/primitives/dialog';

import { apiErrorCode } from '../../../platform/api/client';
import { useNavigationDirty } from '../../../platform/runtime/navigation-state';
import { requestId } from '@miy/components/ui/utilities';
import { useAuth } from '../../../platform/auth/auth-provider';
import {
  useRealtime,
  useRealtimeEvent,
  type RealtimeEvent,
} from '../../../platform/realtime/realtime-provider';
import {
  addDmConversationParticipants,
  cancelDmAttachment,
  createGroupDmThread,
  deleteDmMessage,
  editDmMessage,
  loadDmAttachmentObjectUrl,
  setDmConversationMuted,
  setDmReaction,
  type DmSearchHit,
  createDmThread,
  getDmAttachmentDownloadUrl,
  getDmAttachmentPreviewUrl,
  leaveDmConversation,
  listDmMessages,
  acceptDmTerms,
  getDmTerms,
  listDmBlocks,
  listDmThreads,
  markDmThreadRead,
  removeDmConversationParticipant,
  reportDm,
  searchDmUsers,
  sendDmMessage,
  setDmBlock,
  updateDmConversationTitle,
  uploadDmAttachment,
  type DmMessageAttachment,
  type DmMessage,
  type DmReportCategory,
  type DmSafetyCapability,
  type DmThread,
  type DmUser,
} from '../api/dm-api';
import {
  DmBlockDialog,
  DmBlockedUsersDialog,
  DmReportDialog,
} from './DmSafetyDialogs';
import {
  imagePreviewUrlForAttachment,
  type DmImageViewerState,
} from './dm-attachment-url';
import {
  createDmAttachmentActionWorkflow,
  triggerDmAttachmentBrowserDownload,
} from './dm-attachment-action-workflow';
import {
  DM_COMPOSER_ATTACHMENT_INITIAL_STATE,
  applyDmComposerFileDragOver,
  createLocalAttachmentId,
  dataTransferHasFiles,
  dmComposerAttachmentReducer,
  filesFromClipboardData,
  formatAttachmentSize,
  isImageFile,
  readyDmAttachmentIds,
  uploadingDmAttachmentCount,
} from './dm-composer-attachments';
import { uploadDmComposerAttachments } from './dm-composer-attachment-upload';
import {
  DM_MESSAGES_INITIAL_STATE,
  appendDmThreadMessage,
  applyDmConversationRuntimeRealtimeEvent,
  dmMessagesReducer,
  isDmSendActionDisabled,
  resolveDmSendDraftCommand,
  shouldNavigateAfterDmThreadRemoved,
  shouldSubmitDmComposerKey,
  upsertDmThreadList,
  mergeDmMessagePages,
  prependOlderDmMessages,
} from './dm-conversation-runtime-model';
import { DmAttachmentImage } from './DmAttachmentImage';
import { DmMessageSearchDialog } from './DmMessageSearch';
import {
  activeDmMentionQuery,
  dmMentionCandidates,
  dmMentionDraftsFromMessage,
  dmMentionParts,
  insertDmMention,
  type DmActiveMentionQuery,
  type DmMentionDraft,
} from './dm-mention-model';
import {
  applyDmTypingHint,
  liveHintConversationId,
  liveTypingHint,
  pruneDmTyping,
} from './dm-live-model';
import {
  DM_THREAD_NAVIGATION_INITIAL_STATE,
  dmThreadBottomScrollSnapshot,
  dmThreadScrollSnapshot,
  navigateDmThreadHistoryBack,
  navigateDmThreadHistoryForward,
  omitDmThreadRecordValue,
  pruneDmThreadNavigationHistory,
  pruneDmThreadRecordValues,
  selectDmThreadWithHistory,
  setDmThreadRecordValue,
  type DmThreadNavigationHistory,
  type DmThreadScrollSnapshots,
} from './dm-conversation-session-model';
import {
  canManageDmGroupMembers,
  createDmThreadTextDraftOverride,
  directDmGroupParticipantUserIds,
  dmGroupMemberActionTargetId,
  findCurrentDmThreadParticipant,
  isDirectDmGroupCreateActionDisabled,
  isDmConversationCreateActionDisabled,
  isDmGroupMemberPanelVisible,
  isDmGroupMemberRemovable,
  resolveDmConversationCreateCommand,
  resolveDmThreadTextDraft,
  selectedDmGroupUserIdSet,
  selectedDmThreadParticipantIdSet,
  toggleDmGroupSelectedUser,
  type DmThreadTextDraftOverride,
} from './dm-group-thread-model';
import {
  buildDmMessageContent,
  type DmMessageContentSegment,
} from './dm-message-content';
import {
  currentDmReadPresenceFromBrowser,
  markDmThreadReadAndApply,
  markVisibleDmThreadReadAndApply,
} from './dm-read-receipts';
import {
  buildDmThreadListItem,
  displayDmUserName,
  dmInitials,
  dmThreadDisplayName,
} from './dm-thread-list-model';
import { UserOptionRow } from '../../../platform/users/UserSearchMultiSelect';
import {
  dmDateGroupStartIds,
  formatDmDateDivider,
  formatDmMessageTime,
} from './dm-time';
import { useDmUserSearch } from './useDmUserSearch';

const inputClassName =
  'app-text-body w-full rounded-md border border-app-border bg-app-bg px-3 py-2 text-app-ink placeholder:text-app-ink/35 focus:border-app-accent focus:outline-none';

const DM_THREAD_LIST_CACHE_TTL_MS = 30_000;
const DM_MESSAGE_LIST_CACHE_TTL_MS = 120_000;

type DmCacheEntry<T> = {
  fetchedAt: number;
  items: T;
};

const dmThreadListCache = new Map<string, DmCacheEntry<DmThread[]>>();
const dmMessageListCache = new Map<
  string,
  Map<string, DmCacheEntry<DmMessage[]>>
>();
// 신고·차단·이용규칙 상태는 대화 목록 응답에만 실려 온다 — 목록 캐시를 다시 쓸 때 함께 되살린다.
type DmSafetyCacheEntry = {
  safety: DmSafetyCapability | null;
  blockedUserIds: ReadonlySet<string>;
};
const dmSafetyCache = new Map<string, DmSafetyCacheEntry>();
// 실시간 이벤트(가림·새 메시지)는 화면이 떠 있을 때만 받는다. 마지막 화면이 닫히면 캐시를 낡은 것으로 돌려
// 다음에 열 때 서버에서 다시 받게 한다(캐시는 바로 그리고, 받은 것으로 바꾼다).
let mountedDmViewCount = 0;

function expireDmCaches(): void {
  dmThreadListCache.forEach((entry, scopeKey) => {
    dmThreadListCache.set(scopeKey, { ...entry, fetchedAt: 0 });
  });
  dmMessageListCache.forEach((cache) => {
    cache.forEach((entry, threadId) => {
      cache.set(threadId, { ...entry, fetchedAt: 0 });
    });
  });
}

export type DmThreadScrollSnapshotsRef =
  MutableRefObject<DmThreadScrollSnapshots>;

function dmCacheScopeKey(userId?: string | null): string {
  return `personal:${userId ?? 'anonymous'}`;
}

function isFreshDmCacheEntry<T>(
  entry: DmCacheEntry<T>,
  ttlMs: number,
): boolean {
  return Date.now() - entry.fetchedAt < ttlMs;
}

function readCachedDmThreads(
  scopeKey: string,
): DmCacheEntry<DmThread[]> | null {
  return dmThreadListCache.get(scopeKey) ?? null;
}

function writeCachedDmThreads(scopeKey: string, items: DmThread[]): void {
  dmThreadListCache.set(scopeKey, {
    fetchedAt: Date.now(),
    items,
  });
}

function readCachedDmMessages(
  scopeKey: string,
  threadId: string,
): DmCacheEntry<DmMessage[]> | null {
  return dmMessageListCache.get(scopeKey)?.get(threadId) ?? null;
}

function writeCachedDmMessages(
  scopeKey: string,
  threadId: string,
  items: DmMessage[],
): void {
  const cache = dmMessageListCache.get(scopeKey) ?? new Map();
  cache.set(threadId, {
    fetchedAt: Date.now(),
    items,
  });
  dmMessageListCache.set(scopeKey, cache);
}

/** 캐시된 목록만 고친다(받은 시각은 그대로 — 고친다고 새로 받은 것이 되지는 않는다). */
function patchCachedDmMessages(
  scopeKey: string,
  threadId: string,
  update: (items: DmMessage[]) => DmMessage[],
): void {
  const cache = dmMessageListCache.get(scopeKey);
  const entry = cache?.get(threadId);
  if (!cache || !entry) {
    return;
  }
  cache.set(threadId, { ...entry, items: update(entry.items) });
}

function clearCachedDmMessages(scopeKey: string, threadId: string): void {
  const cache = dmMessageListCache.get(scopeKey);
  if (!cache) {
    return;
  }
  cache.delete(threadId);
  if (cache.size === 0) {
    dmMessageListCache.delete(scopeKey);
  }
}

function pruneCachedDmMessages(
  scopeKey: string,
  validThreadIds: ReadonlySet<string>,
): void {
  const cache = dmMessageListCache.get(scopeKey);
  if (!cache) {
    return;
  }
  Array.from(cache.keys()).forEach((threadId) => {
    if (!validThreadIds.has(threadId)) {
      cache.delete(threadId);
    }
  });
  if (cache.size === 0) {
    dmMessageListCache.delete(scopeKey);
  }
}

function messageBodyContent(
  segments: readonly DmMessageContentSegment[],
  own: boolean,
): ReactNode {
  return segments.map((segment, index) => {
    if (segment.kind === 'text') {
      return <Fragment key={`text-${index}`}>{segment.text}</Fragment>;
    }
    return (
      <a
        className={`font-medium underline underline-offset-2 ${
          own
            ? 'text-app-bg decoration-app-bg/50'
            : 'text-app-accent decoration-app-accent/45'
        }`}
        href={segment.href}
        key={`${segment.href}-${index}`}
        rel="noreferrer"
        target="_blank"
      >
        {segment.label}
      </a>
    );
  });
}

function dmReplyPreviewText(
  replyTo: NonNullable<DmMessage['reply_to']>,
  translate: (key: string, options?: Record<string, unknown>) => string,
): string {
  const preview = replyTo.body_preview.trim();
  if (preview) {
    return preview;
  }
  if (replyTo.attachment_count > 0) {
    return translate('dm.replyAttachmentOnly', {
      count: replyTo.attachment_count,
    });
  }
  return translate('dm.replyEmptyPreview');
}

function dmUnreadCountLabel(message: DmMessage): string | null {
  return message.read_state.unread_count > 0
    ? String(message.read_state.unread_count)
    : null;
}

function dmRealtimeEventConversationId(event: RealtimeEvent): string | null {
  const data = event.data;
  if (!data || typeof data !== 'object' || Array.isArray(data)) {
    return null;
  }
  const record = data as { conversation_id?: unknown; thread_id?: unknown };
  const value = record.conversation_id ?? record.thread_id;
  return typeof value === 'string' && value.trim() ? value : null;
}

export function DMEmbeddedView({
  embeddedPersistentList = false,
  focusMessageId = null,
  onOpenSafety,
  onThreadIdChange,
  reloadSeq = 0,
  threadScrollSnapshotsRef,
  threadId,
}: {
  embeddedPersistentList?: boolean;
  /** miy: a message to bring into view (`?message=` of a notification or search). */
  focusMessageId?: string | null;
  /** miy: open safety management for current or first-time managers. */
  onOpenSafety?: () => void;
  onThreadIdChange: (threadId: string) => void;
  reloadSeq?: number;
  threadScrollSnapshotsRef?: DmThreadScrollSnapshotsRef;
  threadId: string;
}) {
  return (
    <>
      {useDMViewElement({
        embeddedPersistentList,
        focusMessageId,
        onOpenSafety,
        onThreadIdChange,
        reloadSeq,
        threadScrollSnapshotsRef,
        threadId,
      })}
    </>
  );
}

/** miy: reactions (fixed set, display order) and message search jumps. */
const DM_REACTIONS: ReadonlyArray<readonly [string, string]> = [
  ['thumbs_up', '👍'],
  ['heart', '❤️'],
  ['laugh', '😂'],
  ['surprised', '😮'],
  ['sad', '😢'],
  ['pray', '🙏'],
  ['check', '✅'],
];
const DM_REACTION_EMOJI = new Map(DM_REACTIONS);
const DM_JUMP_PAGE = 100;
const DM_JUMP_MAX_PAGES = 20;

/** miy (M4): live hint handling. */
const DM_HINT_COALESCE_MS = 120;
const DM_TYPING_SEND_MS = 3000;
const DM_TYPING_TTL_MS = 6000;

/** miy: page size of the message list (AI-DO DEFAULT_DM_MESSAGE_LIMIT). */
const DM_MESSAGE_PAGE = 50;

function useDMViewElement({
  embeddedPersistentList = false,
  focusMessageId = null,
  onOpenSafety,
  onThreadIdChange,
  reloadSeq = 0,
  threadScrollSnapshotsRef,
  threadId,
}: {
  embeddedPersistentList?: boolean;
  focusMessageId?: string | null;
  onOpenSafety?: () => void;
  onThreadIdChange?: (threadId: string) => void;
  reloadSeq?: number;
  threadScrollSnapshotsRef?: DmThreadScrollSnapshotsRef;
  threadId: string;
}): ReactNode {
  const { token, user } = useAuth();
  const { reconnectSeq, status: realtimeState, send: sendLive } = useRealtime();
  const { i18n, t } = useTranslation('apps');
  const timeZone = user?.time_zone;
  const dmCacheKey = useMemo(() => dmCacheScopeKey(user?.id), [user?.id]);
  const [threads, setThreads] = useState<DmThread[]>([]);
  const [threadsLoading, setThreadsLoading] = useState(false);
  const [messageState, dispatchMessages] = useReducer(
    dmMessagesReducer,
    DM_MESSAGES_INITIAL_STATE,
  );
  const [query, setQuery] = useState('');
  const [selectedUsers, setSelectedUsers] = useState<DmUser[]>([]);
  const [directGroupUsers, setDirectGroupUsers] = useState<DmUser[]>([]);
  const [groupTitle, setGroupTitle] = useState('');
  const [creatingGroup, setCreatingGroup] = useState(false);
  const [memberPanelOpen, setMemberPanelOpen] = useState(false);
  const [titleDraftOverride, setTitleDraftOverride] =
    useState<DmThreadTextDraftOverride | null>(null);
  const [savingTitle, setSavingTitle] = useState(false);
  const [memberQueryOverride, setMemberQueryOverride] =
    useState<DmThreadTextDraftOverride | null>(null);
  const [memberActionId, setMemberActionId] = useState<string | null>(null);
  const [draft, setDraft] = useState('');
  const [failedDraftsByThread, setFailedDraftsByThread] = useState<Record<string, string[]>>({});
  const [replyTarget, setReplyTarget] = useState<DmMessage | null>(null);
  // miy: author edit/delete, older pages and mute.
  const [editing, setEditing] = useState<{
    message: DmMessage;
    body: string;
    // miy: people mentioned in the edited text.
    mentions?: DmMentionDraft[];
  } | null>(null);
  // miy: "@" picker, mention drafts per conversation, reactions, search, jumps.
  const composerRef = useRef<HTMLTextAreaElement>(null);
  const [mentionPicker, setMentionPicker] = useState<{
    active: DmActiveMentionQuery;
    caret: number;
    index: number;
  } | null>(null);
  const mentionDraftsByThreadRef = useRef<Record<string, DmMentionDraft[]>>({});
  const [reactionPickerFor, setReactionPickerFor] = useState<string | null>(null);
  const [searchOpen, setSearchOpen] = useState(false);
  const pendingJumpRef = useRef<{ threadId: string; messageId: string } | null>(null);
  const jumpingRef = useRef(false);
  const [jumpSeq, setJumpSeq] = useState(0);
  const editTarget = editing?.message ?? null;
  const [deleteTarget, setDeleteTarget] = useState<DmMessage | null>(null);
  const [deleting, setDeleting] = useState(false);
  const [olderByThreadId, setOlderByThreadId] = useState<
    Record<string, { loading: boolean; exhausted: boolean }>
  >({});
  const olderAnchorRef = useRef<{ scrollHeight: number; scrollTop: number } | null>(null);
  const olderRefreshOffset = useRef(0);
  const [muting, setMuting] = useState(false);
  const [readMarkerByThreadId, setReadMarkerByThreadId] = useState<
    Record<string, string | null>
  >({});
  const [highlightedMessageId, setHighlightedMessageId] = useState<
    string | null
  >(null);
  const [threadNavigationHistory, setThreadNavigationHistory] =
    useState<DmThreadNavigationHistory>(DM_THREAD_NAVIGATION_INITIAL_STATE);
  const [composerAttachmentState, dispatchComposerAttachments] = useReducer(
    dmComposerAttachmentReducer,
    DM_COMPOSER_ATTACHMENT_INITIAL_STATE,
  );
  const [imageViewer, setImageViewer] = useState<DmImageViewerWithSource | null>(
    null,
  );
  // 확대 뷰어로 여는 첨부와 그 원본 메시지 — 원본이 가려지면 그 뷰어만 닫는다.
  const imageViewerSourceRef = useRef<DmImageViewerSource | null>(null);
  const [sending, setSending] = useState(false);
  const [loadError, setLoadError] = useState<string | null>(null);
  // 신고·1:1 차단(ADR 0025 개정 2026-11-18). 서버가 safety 를 주지 않으면(옛 서버) 화면을 숨긴다.
  const [safety, setSafety] = useState<DmSafetyCapability | null>(null);
  const [reportTarget, setReportTarget] = useState<DmMessage | null>(null);
  const [blockDialogOpen, setBlockDialogOpen] = useState(false);
  const [blockedUsersOpen, setBlockedUsersOpen] = useState(false);
  const [acceptingTerms, setAcceptingTerms] = useState(false);
  // safety·blockedUserIds 를 어느 사용자 범위로 받았는지 — 그 범위일 때만 모듈 캐시에 적는다.
  const safetyScopeRef = useRef<string | null>(null);
  // 이용규칙 상태 받기의 순번과, 화면에 반영된 마지막 받기 순번(동의하면 그때까지 시작한 받기 전부로 올린다).
  // 시작한 것만으로는 아무것도 무효로 하지 않는다 — 실제로 반영한 받기·동의보다 먼저 시작한 답만 버린다(리뷰 8029·8040).
  const termsReadSeqRef = useRef(0);
  const termsAppliedSeqRef = useRef(0);
  // 지금 이 화면의 계정 범위 — 요청한 뒤 계정이 바뀌었으면(사용자 전환) 그 이용규칙 답·동의는 이 계정에 반영하지 않는다(리뷰 8067).
  const termsScopeRef = useRef(dmCacheKey);
  termsScopeRef.current = dmCacheKey;
  // 내가 1:1 차단한 사람 — 공유 단체 대화에서 그 사람의 메시지를 접어 보인다(표시만, 서버 기록은 그대로).
  const [blockedUserIds, setBlockedUserIds] = useState<ReadonlySet<string>>(new Set());
  const [expandedBlockedMessageIds, setExpandedBlockedMessageIds] = useState<
    ReadonlySet<string>
  >(new Set());
  const feedback = useFeedback();
  const selectedThreadIdRef = useRef(threadId);
  const pendingThreadNavigation = useRef<{
    from: string | null;
    to: string;
    update: (history: DmThreadNavigationHistory) => DmThreadNavigationHistory;
  } | null>(null);
  const threadsRef = useRef<DmThread[]>([]);
  const messagesRef = useRef<DmMessage[]>([]);
  const draftByThreadIdRef = useRef<Record<string, string>>({});
  const messageCacheByThreadIdRef = useRef<Record<string, DmMessage[]>>({});
  const localScrollByThreadIdRef = useRef<DmThreadScrollSnapshots>({});
  const scrollByThreadIdRef =
    threadScrollSnapshotsRef ?? localScrollByThreadIdRef;
  const messagesScrollRef = useRef<HTMLDivElement>(null);
  const messageElementRefs = useRef<Record<string, HTMLDivElement | null>>({});
  const bottomRef = useRef<HTMLDivElement>(null);
  const fileInputRef = useRef<HTMLInputElement>(null);
  const dragDepthRef = useRef(0);
  const threadRefreshSeqRef = useRef({ reconnectSeq, reloadSeq });
  const messageRefreshSeqRef = useRef({ reconnectSeq, reloadSeq });
  const messages = messageState.items;
  const messagesLoading = messageState.loading;
  const loadedMessageIds = useMemo(
    () => new Set(messages.map((message) => message.id)),
    [messages],
  );
  const dateGroupStartIds = useMemo(
    () => dmDateGroupStartIds(messages, timeZone),
    [messages, timeZone],
  );
  const { attachmentActionId, draggingFiles, pendingAttachments } =
    composerAttachmentState;

  selectedThreadIdRef.current = threadId;
  threadsRef.current = threads;
  messagesRef.current = messages;

  const selectedThread = useMemo(
    () => threads.find((thread) => thread.id === threadId) ?? null,
    [threadId, threads],
  );
  const unresolvedRouteThread =
    Boolean(threadId) && !threadsLoading && !selectedThread;
  const threadListItems = useMemo(
    () =>
      threads.map((thread) =>
        buildDmThreadListItem({
          activeThreadId: threadId,
          currentUserId: user?.id,
          thread,
          translate: t,
        }),
      ),
    [t, threadId, threads, user?.id],
  );
  const selectedThreadName = selectedThread
    ? dmThreadDisplayName(selectedThread, user?.id)
    : '';
  const titleDraft = resolveDmThreadTextDraft({
    fallback: selectedThread?.title,
    override: titleDraftOverride,
    threadId: selectedThread?.id,
  });
  const memberQuery = resolveDmThreadTextDraft({
    fallback: '',
    override: memberQueryOverride,
    threadId: selectedThread?.id,
  });
  const selectedUserIds = useMemo(
    () => selectedDmGroupUserIdSet(selectedUsers),
    [selectedUsers],
  );
  const directGroupUserIds = useMemo(
    () => selectedDmGroupUserIdSet(directGroupUsers),
    [directGroupUsers],
  );
  const selectedParticipantIds = useMemo(
    () => selectedDmThreadParticipantIdSet(selectedThread),
    [selectedThread],
  );
  const directGroupCreatePanelVisible =
    memberPanelOpen && selectedThread?.conversation_type === 'direct';
  const groupMemberPanelVisible = isDmGroupMemberPanelVisible({
    memberPanelOpen,
    selectedThread,
  });
  const {
    results: userResults,
    reset: resetUserSearch,
    searching,
  } = useDmUserSearch({
    query,
    searchUsers: searchDmUsers,
    token,
  });
  const {
    results: memberResults,
    reset: resetMemberSearch,
    searching: memberSearching,
  } = useDmUserSearch({
    enabled: Boolean(
      selectedThread &&
        (selectedThread.conversation_type === 'group' ||
          directGroupCreatePanelVisible),
    ),
    excludeUserIds: selectedParticipantIds,
    query: memberQuery,
    searchUsers: searchDmUsers,
    token,
  });
  const currentParticipant = findCurrentDmThreadParticipant(
    selectedThread,
    user?.id,
  );
  const canManageSelectedThread = canManageDmGroupMembers(currentParticipant);
  const canInviteSelectedThread = Boolean(
    currentParticipant && selectedThread?.conversation_type === 'group',
  );
  const selectedReadMarkerMessageId = threadId
    ? (readMarkerByThreadId[threadId] ?? null)
    : null;
  const visibleReadMarkerMessageId = useMemo(() => {
    if (!selectedReadMarkerMessageId) {
      return null;
    }
    const markerIndex = messages.findIndex(
      (message) => message.id === selectedReadMarkerMessageId,
    );
    return markerIndex >= 0 && markerIndex < messages.length - 1
      ? selectedReadMarkerMessageId
      : null;
  }, [messages, selectedReadMarkerMessageId]);
  const leaveGroupActionId = selectedThread
    ? dmGroupMemberActionTargetId({
        currentUserId: user?.id,
        threadId: selectedThread.id,
      })
    : null;
  const readyAttachmentIds = useMemo(
    () => readyDmAttachmentIds(pendingAttachments),
    [pendingAttachments],
  );
  const uploadingAttachmentCount =
    uploadingDmAttachmentCount(pendingAttachments);
  const openImageViewer = useCallback((viewer: DmImageViewerState) => {
    const source = imageViewerSourceRef.current;
    const sourceMessage = source?.messageId
      ? messagesRef.current.find((item) => item.id === source.messageId)
      : undefined;
    // 미리보기 주소를 받는 사이 원본이 가려졌으면 열지 않는다.
    if (sourceMessage && isDmImageViewerSourceHidden(sourceMessage, source)) {
      return;
    }
    setImageViewer({ ...viewer, source });
  }, []);
  const attachmentActionWorkflow = useMemo(
    () =>
      createDmAttachmentActionWorkflow({
        api: token
          ? {
              getDownloadUrl: (attachmentId) =>
                getDmAttachmentDownloadUrl(token, attachmentId),
              getPreviewUrl: (attachmentId) =>
                getDmAttachmentPreviewUrl(token, attachmentId),
              loadObjectUrl: (path) => loadDmAttachmentObjectUrl(token, path),
              revokeObjectUrl: (url) => URL.revokeObjectURL(url),
            }
          : null,
        browser: {
          download: (spec) => triggerDmAttachmentBrowserDownload(spec),
        },
        busyAttachmentId: attachmentActionId,
        dispatchAttachmentAction: dispatchComposerAttachments,
        messages: {
          downloadFailed: t('dm.errors.attachmentDownloadFailed'),
          previewFailed: t('dm.errors.attachmentPreviewFailed'),
        },
        setError: feedback.error,
        setImageViewer: openImageViewer,
      }),
    [attachmentActionId, feedback, openImageViewer, t, token],
  );
  const captureThreadScroll = useCallback(
    (captureThreadId: string | null) => {
      const element = messagesScrollRef.current;
      if (!captureThreadId || !element) {
        return;
      }
      scrollByThreadIdRef.current = setDmThreadRecordValue(
        scrollByThreadIdRef.current,
        captureThreadId,
        dmThreadScrollSnapshot({
          clientHeight: element.clientHeight,
          scrollHeight: element.scrollHeight,
          scrollTop: element.scrollTop,
        }),
      );
    },
    [scrollByThreadIdRef],
  );

  const scrollThreadToBottom = useCallback(
    (scrollThreadId: string | null) => {
      if (!scrollThreadId) {
        return;
      }
      const element = messagesScrollRef.current;
      scrollByThreadIdRef.current = setDmThreadRecordValue(
        scrollByThreadIdRef.current,
        scrollThreadId,
        dmThreadBottomScrollSnapshot({
          clientHeight: element?.clientHeight ?? 0,
          scrollHeight: element?.scrollHeight ?? 0,
        }),
      );
      if (element && selectedThreadIdRef.current === scrollThreadId) {
        element.scrollTop = element.scrollHeight;
      }
    },
    [scrollByThreadIdRef],
  );

  const commitThreadList = useCallback(
    (nextThreads: DmThread[]) => {
      threadsRef.current = nextThreads;
      writeCachedDmThreads(dmCacheKey, nextThreads);
      return nextThreads;
    },
    [dmCacheKey],
  );

  const selectThreadId = useCallback(
    (
      nextThreadId: string | null,
      options: { recordHistory?: boolean } = {},
    ) => {
      const currentThreadId = selectedThreadIdRef.current || null;
      captureThreadScroll(currentThreadId);
      pendingThreadNavigation.current =
        nextThreadId && nextThreadId !== currentThreadId && options.recordHistory !== false
          ? {
              from: currentThreadId,
              to: nextThreadId,
              update: (history) => selectDmThreadWithHistory(history, { currentThreadId, nextThreadId }),
            }
          : null;
      onThreadIdChange?.(nextThreadId ?? '');
    },
    [captureThreadScroll, onThreadIdChange],
  );

  const navigatePreviousThread = useCallback(() => {
    const currentThreadId = selectedThreadIdRef.current || null;
    captureThreadScroll(currentThreadId);
    const result = navigateDmThreadHistoryBack(
      threadNavigationHistory,
      currentThreadId,
    );
    if (!result.threadId) {
      return;
    }
    pendingThreadNavigation.current = {
      from: currentThreadId,
      to: result.threadId,
      update: (history) => navigateDmThreadHistoryBack(history, currentThreadId).history,
    };
    onThreadIdChange?.(result.threadId);
  }, [captureThreadScroll, onThreadIdChange, threadNavigationHistory]);

  const navigateNextThread = useCallback(() => {
    const currentThreadId = selectedThreadIdRef.current || null;
    captureThreadScroll(currentThreadId);
    const result = navigateDmThreadHistoryForward(
      threadNavigationHistory,
      currentThreadId,
    );
    if (!result.threadId) {
      return;
    }
    pendingThreadNavigation.current = {
      from: currentThreadId,
      to: result.threadId,
      update: (history) => navigateDmThreadHistoryForward(history, currentThreadId).history,
    };
    onThreadIdChange?.(result.threadId);
  }, [captureThreadScroll, onThreadIdChange, threadNavigationHistory]);

  const loadThreads = useCallback(
    async (options: { force?: boolean } = {}) => {
      if (!token) {
        setThreads([]);
        threadsRef.current = [];
        setThreadsLoading(false);
        return;
      }
      const cachedThreads = readCachedDmThreads(dmCacheKey);
      const cachedSafety = dmSafetyCache.get(dmCacheKey) ?? null;
      if (cachedSafety) {
        safetyScopeRef.current = dmCacheKey;
        setSafety(cachedSafety.safety);
        setBlockedUserIds(cachedSafety.blockedUserIds);
      }
      if (cachedThreads) {
        setThreads(cachedThreads.items);
        threadsRef.current = cachedThreads.items;
        setLoadError(null);
        if (
          !options.force &&
          cachedSafety &&
          isFreshDmCacheEntry(cachedThreads, DM_THREAD_LIST_CACHE_TTL_MS)
        ) {
          setThreadsLoading(false);
          return;
        }
      }
      setThreadsLoading(!cachedThreads);
      try {
        const termsSeq = ++termsReadSeqRef.current;
        const response = await listDmThreads(token);
        // 요청한 뒤 계정이 바뀌었으면(사용자 전환) 이 목록·이용규칙은 지금 계정의 것이 아니다 — 아무것도 반영하지 않는다.
        // 섞어 반영하면 다른 계정의 상태가 이 요청 계정의 캐시로 저장될 수 있다(리뷰 8070).
        if (termsScopeRef.current !== dmCacheKey) return;
        safetyScopeRef.current = dmCacheKey;
        const termsFresh = termsSeq > termsAppliedSeqRef.current;
        if (termsFresh) termsAppliedSeqRef.current = termsSeq;
        setSafety((current) => {
          const next = response.safety ?? null;
          // 목록을 받는 사이 동의했거나 더 나중에 시작한 받기가 반영됐으면 이용규칙 상태는 이 화면 값을 지킨다(나머지는 새 값).
          // 늦게 온 답이라도 같은 판의 동의 확인은 받아들인다(되돌리지는 않는다).
          return !termsFresh && next && current?.terms
            ? { ...next, terms: keepConfirmedDmTermsAcceptance(current.terms, next.terms) }
            : next;
        });
        if (response.safety?.block) {
          void listDmBlocks(token)
            .then((blocks) =>
              setBlockedUserIds(new Set(blocks.items.map((item) => item.user.id))),
            )
            .catch(() => undefined);
        }
        setThreads(commitThreadList(response.items));
        setLoadError(null);
      } catch (caughtError) {
        if (termsScopeRef.current !== dmCacheKey) return;
        setLoadError(
          caughtError instanceof Error
            ? caughtError.message
            : t('dm.errors.threadsLoadFailed'),
        );
      } finally {
        setThreadsLoading(false);
      }
    },
    [commitThreadList, dmCacheKey, t, token],
  );

  useEffect(() => {
    if (safetyScopeRef.current === dmCacheKey) {
      dmSafetyCache.set(dmCacheKey, { safety, blockedUserIds });
    }
  }, [blockedUserIds, dmCacheKey, safety]);

  useEffect(() => {
    mountedDmViewCount += 1;
    return () => {
      mountedDmViewCount -= 1;
      if (mountedDmViewCount === 0) {
        expireDmCaches();
      }
    };
  }, []);

  useEffect(() => {
    const force =
      threadRefreshSeqRef.current.reconnectSeq !== reconnectSeq ||
      threadRefreshSeqRef.current.reloadSeq !== reloadSeq;
    threadRefreshSeqRef.current = { reconnectSeq, reloadSeq };
    void loadThreads({ force });
  }, [loadThreads, reconnectSeq, reloadSeq]);

  useEffect(() => {
    // Only a route reported back by the host confirms navigation past its guard.
    const pending = pendingThreadNavigation.current;
    if (pending && threadId !== pending.from) {
      pendingThreadNavigation.current = null;
      if (threadId === pending.to) setThreadNavigationHistory(pending.update);
    }
    setDraft(threadId ? (draftByThreadIdRef.current[threadId] ?? '') : '');
    setReplyTarget(null);
    setEditing(null);
    setHighlightedMessageId(null);
    olderRefreshOffset.current = 0;
  }, [threadId]);

  useEffect(() => {
    const validThreadIds = new Set(threads.map((thread) => thread.id));
    setThreadNavigationHistory((history) =>
      pruneDmThreadNavigationHistory(history, validThreadIds),
    );
    draftByThreadIdRef.current = pruneDmThreadRecordValues(
      draftByThreadIdRef.current,
      validThreadIds,
    );
    setFailedDraftsByThread((current) => pruneDmThreadRecordValues(current, validThreadIds));
    messageCacheByThreadIdRef.current = pruneDmThreadRecordValues(
      messageCacheByThreadIdRef.current,
      validThreadIds,
    );
    pruneCachedDmMessages(dmCacheKey, validThreadIds);
    if (validThreadIds.size > 0) {
      scrollByThreadIdRef.current = pruneDmThreadRecordValues(
        scrollByThreadIdRef.current,
        validThreadIds,
      );
      setReadMarkerByThreadId((markers) =>
        pruneDmThreadRecordValues(markers, validThreadIds),
      );
    }
  }, [dmCacheKey, scrollByThreadIdRef, threads]);

  const applyReadThread = useCallback(
    (thread: DmThread) => {
      setThreads((items) =>
        commitThreadList(upsertDmThreadList(items, thread)),
      );
    },
    [commitThreadList],
  );

  const refreshSelectedThreadMessages = useCallback(
    async (targetThreadId: string) => {
      if (!token) {
        return;
      }
      const response = await listDmMessages(token, targetThreadId);
      if (selectedThreadIdRef.current !== targetThreadId) {
        return;
      }
      const items = mergeDmMessagePages(messagesRef.current, response.items);
      messagesRef.current = items;
      messageCacheByThreadIdRef.current = setDmThreadRecordValue(
        messageCacheByThreadIdRef.current,
        targetThreadId,
        items,
      );
      writeCachedDmMessages(dmCacheKey, targetThreadId, items);
      dispatchMessages({ type: 'replace', items });
    },
    [dmCacheKey, token],
  );

  useEffect(() => {
    const forceRefresh =
      messageRefreshSeqRef.current.reconnectSeq !== reconnectSeq ||
      messageRefreshSeqRef.current.reloadSeq !== reloadSeq;
    messageRefreshSeqRef.current = { reconnectSeq, reloadSeq };
    if (!token || !threadId) {
      messagesRef.current = [];
      dispatchMessages({ type: 'clear' });
      dispatchMessages({ type: 'loading', loading: false });
      return;
    }
    let cancelled = false;
    const cachedEntry = readCachedDmMessages(dmCacheKey, threadId);
    const cachedMessages =
      cachedEntry?.items ?? messageCacheByThreadIdRef.current[threadId] ?? null;
    const threadBeforeRead =
      threadsRef.current.find((thread) => thread.id === threadId) ?? null;
    const readMarkerMessageId =
      threadBeforeRead && threadBeforeRead.unread_count > 0
        ? (threadBeforeRead.last_read_message_id ?? null)
        : null;
    setReadMarkerByThreadId((markers) =>
      setDmThreadRecordValue(markers, threadId, readMarkerMessageId),
    );
    if (cachedMessages) {
      messagesRef.current = cachedMessages;
      messageCacheByThreadIdRef.current = setDmThreadRecordValue(
        messageCacheByThreadIdRef.current,
        threadId,
        cachedMessages,
      );
      dispatchMessages({ type: 'replace', items: cachedMessages });
      dispatchMessages({ type: 'loading', loading: false });
    } else {
      messagesRef.current = [];
      dispatchMessages({ type: 'clear' });
      dispatchMessages({ type: 'loading', loading: true });
    }
    const markVisibleThreadRead = () =>
      markVisibleDmThreadReadAndApply({
        markThreadRead: markDmThreadRead,
        onThreadRead: (thread) => {
          if (!cancelled) applyReadThread(thread);
        },
        presence: currentDmReadPresenceFromBrowser(document),
        selectedThreadId: threadId,
        token,
      });
    if (
      cachedEntry &&
      !forceRefresh &&
      isFreshDmCacheEntry(cachedEntry, DM_MESSAGE_LIST_CACHE_TTL_MS)
    ) {
      void markVisibleThreadRead().catch(() => undefined);
      return () => {
        cancelled = true;
      };
    }
    listDmMessages(token, threadId)
      .then((response) => {
        if (cancelled) return;
        const items = mergeDmMessagePages(messagesRef.current, response.items);
        if (response.items.length < DM_MESSAGE_PAGE) {
          setOlderByThreadId((current) => ({
            ...current,
            [threadId]: { loading: false, exhausted: true },
          }));
        }
        messagesRef.current = items;
        messageCacheByThreadIdRef.current = setDmThreadRecordValue(
          messageCacheByThreadIdRef.current,
          threadId,
          items,
        );
        writeCachedDmMessages(dmCacheKey, threadId, items);
        if (cachedMessages) {
          dispatchMessages({ type: 'replace', items });
        } else {
          dispatchMessages({ type: 'loaded', items });
        }
        const olderCount = items.length - response.items.length;
        if (olderCount > 0) {
          // Refresh one batch of already loaded older messages each poll.
          const offset = olderRefreshOffset.current % olderCount;
          const batch = items.slice(offset, Math.min(offset + DM_MESSAGE_PAGE, olderCount));
          const requested = new Map(batch.map((message) => [message.id, message]));
          olderRefreshOffset.current = offset + batch.length >= olderCount ? 0 : offset + batch.length;
          void listDmMessages(token, threadId, DM_MESSAGE_PAGE, null, [...requested.keys()])
            .then(({ items: updated }) => {
              if (cancelled || selectedThreadIdRef.current !== threadId) return;
              const refreshed = new Map(updated.map((message) => [message.id, message]));
              const next = messagesRef.current.flatMap((message) => {
                // Keep local edits or newly refreshed rows that arrived during this request.
                if (requested.get(message.id) !== message) return [message];
                const updatedMessage = refreshed.get(message.id);
                return updatedMessage ? [updatedMessage] : [];
              });
              messagesRef.current = next;
              messageCacheByThreadIdRef.current = setDmThreadRecordValue(
                messageCacheByThreadIdRef.current,
                threadId,
                next,
              );
              writeCachedDmMessages(dmCacheKey, threadId, next);
              dispatchMessages({ type: 'replace', items: next });
            })
            .catch(() => undefined);
        }
        return markVisibleThreadRead();
      })
      .catch((caughtError) => {
        if (cancelled) return;
        setLoadError(
          caughtError instanceof Error
            ? caughtError.message
            : t('dm.errors.messagesLoadFailed'),
        );
      })
      .finally(() => {
        if (!cancelled) {
          dispatchMessages({ type: 'loading', loading: false });
        }
      });
    return () => {
      cancelled = true;
    };
  }, [
    applyReadThread,
    dmCacheKey,
    reconnectSeq,
    reloadSeq,
    t,
    threadId,
    token,
  ]);

  useLayoutEffect(
    () => () => {
      captureThreadScroll(threadId || null);
    },
    [captureThreadScroll, threadId],
  );

  useLayoutEffect(() => {
    const element = messagesScrollRef.current;
    if (!element || !threadId || messagesLoading) {
      return;
    }
    const olderAnchor = olderAnchorRef.current;
    if (olderAnchor) {
      // miy: an older page went in front; keep the same message in view.
      olderAnchorRef.current = null;
      element.scrollTop =
        element.scrollHeight - olderAnchor.scrollHeight + olderAnchor.scrollTop;
      captureThreadScroll(threadId);
      return;
    }
    const snapshot = scrollByThreadIdRef.current[threadId];
    if (snapshot) {
      if (snapshot.atBottom) {
        element.scrollTop = element.scrollHeight;
        return;
      }
      element.scrollTop = Math.min(snapshot.scrollTop, element.scrollHeight);
      return;
    }
    const readMarkerElement = visibleReadMarkerMessageId
      ? messageElementRefs.current[visibleReadMarkerMessageId]
      : null;
    if (readMarkerElement) {
      element.scrollTop = Math.max(
        0,
        readMarkerElement.offsetTop - element.offsetTop - 16,
      );
      return;
    }
    element.scrollTop = element.scrollHeight;
  }, [
    messages.length,
    messagesLoading,
    scrollByThreadIdRef,
    threadId,
    visibleReadMarkerMessageId,
  ]);

  const resetThreadComposer = useCallback(() => {
    dispatchComposerAttachments({ type: 'reset' });
    dragDepthRef.current = 0;
  }, []);

  const setSelectedThreadDraft = useCallback((nextDraft: string) => {
    setDraft(nextDraft);
    const activeThreadId = selectedThreadIdRef.current;
    if (!activeThreadId) {
      return;
    }
    draftByThreadIdRef.current = setDmThreadRecordValue(
      draftByThreadIdRef.current,
      activeThreadId,
      nextDraft,
    );
  }, []);

  // miy (M4): tell the other participants that this person is typing. At
  // most one hint per DM_TYPING_SEND_MS; an emptied draft or a send ends it.
  const typingSentRef = useRef<{ threadId: string; at: number } | null>(null);
  const stopTyping = useCallback(
    (targetThreadId: string | null | undefined) => {
      const sent = typingSentRef.current;
      if (!sent || !targetThreadId || sent.threadId !== targetThreadId) return;
      typingSentRef.current = null;
      sendLive?.({ type: 'typing', conversationId: targetThreadId, active: false });
    },
    [sendLive],
  );
  const notifyTyping = useCallback(
    (nextDraft: string) => {
      const activeThreadId = selectedThreadIdRef.current;
      if (!activeThreadId) return;
      if (!nextDraft.trim()) {
        stopTyping(activeThreadId);
        return;
      }
      const now = Date.now();
      const sent = typingSentRef.current;
      if (sent && sent.threadId === activeThreadId && now - sent.at < DM_TYPING_SEND_MS) {
        return;
      }
      if (sendLive?.({ type: 'typing', conversationId: activeThreadId, active: true })) {
        typingSentRef.current = { threadId: activeThreadId, at: now };
      }
    },
    [sendLive, stopTyping],
  );
  useEffect(() => () => stopTyping(threadId), [stopTyping, threadId]);

  useEffect(() => {
    resetThreadComposer();
  }, [resetThreadComposer, threadId]);

  useEffect(() => {
    if (directGroupCreatePanelVisible) {
      return;
    }
    setDirectGroupUsers([]);
  }, [directGroupCreatePanelVisible]);

  const handleDmRealtimeEvent = useCallback(
    (event: RealtimeEvent) => {
      const previousSelectedThreadId = selectedThreadIdRef.current || null;
      const readConversationId =
        event.type === DM_REALTIME_EVENT_TYPES.conversationRead
          ? dmRealtimeEventConversationId(event)
          : null;
      const result = applyDmConversationRuntimeRealtimeEvent({
        event,
        conversations: threadsRef.current,
        messages: messagesRef.current,
        selectedConversationId: selectedThreadIdRef.current || null,
        currentUserId: user?.id,
        ...currentDmReadPresenceFromBrowser(document),
      });
      if (!result) {
        return;
      }
      threadsRef.current = result.conversations;
      messagesRef.current = result.messages;
      selectedThreadIdRef.current = result.selectedConversationId ?? '';
      writeCachedDmThreads(dmCacheKey, result.conversations);
      setThreads(result.conversations);
      dispatchMessages({ type: 'replace', items: result.messages });
      if (previousSelectedThreadId) {
        messageCacheByThreadIdRef.current = setDmThreadRecordValue(
          messageCacheByThreadIdRef.current,
          previousSelectedThreadId,
          result.messages,
        );
        writeCachedDmMessages(
          dmCacheKey,
          previousSelectedThreadId,
          result.messages,
        );
      }
      const updatedMessage = result.updatedMessage;
      if (updatedMessage) {
        // 답장 작성 중인 원문이 가려지거나 되돌려지면 작성창 미리보기도 같은 내용으로 바꾼다(가린 원문이 남지 않게).
        setReplyTarget((current) =>
          current && current.id === updatedMessage.id ? updatedMessage : current,
        );
        // 가려진 메시지의 첨부를 확대해 둔 뷰어는 닫는다 — 다른 메시지의 뷰어는 그대로 둔다.
        setImageViewer((current) =>
          current?.source?.messageId === updatedMessage.id &&
          isDmImageViewerSourceHidden(updatedMessage, current.source)
            ? null
            : current,
        );
      }
      if (
        updatedMessage &&
        updatedMessage.conversation_id !== previousSelectedThreadId
      ) {
        // 보고 있지 않은 대화에서 가림·되돌림이 일어났다 — 캐시된 목록(답장 인용 포함)도 고쳐 다시 열 때 원문이 안 보이게.
        const updatedThreadId = updatedMessage.conversation_id;
        patchCachedDmMessages(dmCacheKey, updatedThreadId, (items) =>
          replaceDmMessage(items, updatedMessage),
        );
        const cachedItems = messageCacheByThreadIdRef.current[updatedThreadId];
        if (cachedItems) {
          messageCacheByThreadIdRef.current = setDmThreadRecordValue(
            messageCacheByThreadIdRef.current,
            updatedThreadId,
            replaceDmMessage(cachedItems, updatedMessage),
          );
        }
      }
      if (result.removedConversationId) {
        const removedThreadId = result.removedConversationId;
        clearCachedDmMessages(dmCacheKey, removedThreadId);
        messageCacheByThreadIdRef.current = omitDmThreadRecordValue(
          messageCacheByThreadIdRef.current,
          removedThreadId,
        );
        draftByThreadIdRef.current = omitDmThreadRecordValue(
          draftByThreadIdRef.current,
          removedThreadId,
        );
        scrollByThreadIdRef.current = omitDmThreadRecordValue(
          scrollByThreadIdRef.current,
          removedThreadId,
        );
        setReadMarkerByThreadId((markers) =>
          omitDmThreadRecordValue(markers, removedThreadId),
        );
        setThreadNavigationHistory((history) => ({
          back: history.back.filter((item) => item !== removedThreadId),
          forward: history.forward.filter((item) => item !== removedThreadId),
        }));
      }
      if (
        shouldNavigateAfterDmThreadRemoved({
          activeRouteThreadId: threadId,
          selectedConversationRemoved: result.selectedConversationRemoved,
        })
      ) {
        selectThreadId(null);
      }
      if (token && result.markReadConversationId) {
        const markReadConversationId = result.markReadConversationId;
        void markDmThreadReadAndApply({
          conversationId: markReadConversationId,
          markThreadRead: markDmThreadRead,
          onThreadRead: applyReadThread,
          token,
        })
          .then(() => {
            if (selectedThreadIdRef.current === markReadConversationId) {
              return refreshSelectedThreadMessages(markReadConversationId);
            }
            return undefined;
          })
          .catch(() => undefined);
      }
      if (
        token &&
        readConversationId &&
        previousSelectedThreadId &&
        readConversationId === previousSelectedThreadId
      ) {
        void refreshSelectedThreadMessages(previousSelectedThreadId).catch(
          () => undefined,
        );
      }
    },
    [
      applyReadThread,
      dmCacheKey,
      refreshSelectedThreadMessages,
      selectThreadId,
      scrollByThreadIdRef,
      threadId,
      token,
      user?.id,
    ],
  );

  useRealtimeEvent(
    DM_REALTIME_EVENT_TYPES.messageCreated,
    handleDmRealtimeEvent,
  );
  useRealtimeEvent(
    DM_REALTIME_EVENT_TYPES.messageUpdated,
    handleDmRealtimeEvent,
  );
  useRealtimeEvent(
    DM_REALTIME_EVENT_TYPES.conversationCreated,
    handleDmRealtimeEvent,
  );
  useRealtimeEvent(
    DM_REALTIME_EVENT_TYPES.conversationUpdated,
    handleDmRealtimeEvent,
  );
  useRealtimeEvent(
    DM_REALTIME_EVENT_TYPES.conversationRead,
    handleDmRealtimeEvent,
  );
  useRealtimeEvent(
    DM_REALTIME_EVENT_TYPES.conversationRemoved,
    handleDmRealtimeEvent,
  );

  // miy (M4): the live connection carries hints only. A changed conversation
  // refetches the list and, when it is the one shown, its messages through
  // runtime.api (the late-answer checks of those loaders still apply).
  const hintTimersRef = useRef(new Map<string, number>());
  useEffect(() => {
    const timers = hintTimersRef.current;
    return () => {
      timers.forEach((timer) => window.clearTimeout(timer));
      timers.clear();
    };
  }, []);
  const handleConversationChangedHint = useCallback(
    (event: RealtimeEvent) => {
      const conversationId = liveHintConversationId(event);
      if (!token || !conversationId) return;
      const timers = hintTimersRef.current;
      if (timers.has(conversationId)) return;
      // One refetch per burst (send, read, attach arrive together).
      timers.set(
        conversationId,
        window.setTimeout(() => {
          timers.delete(conversationId);
          void loadThreads({ force: true });
          if (selectedThreadIdRef.current !== conversationId) return;
          void refreshSelectedThreadMessages(conversationId)
            .then(() =>
              markVisibleDmThreadReadAndApply({
                markThreadRead: markDmThreadRead,
                onThreadRead: applyReadThread,
                presence: currentDmReadPresenceFromBrowser(document),
                selectedThreadId: selectedThreadIdRef.current,
                token,
              }),
            )
            .catch(() => undefined);
        }, DM_HINT_COALESCE_MS),
      );
    },
    [applyReadThread, loadThreads, refreshSelectedThreadMessages, token],
  );
  useRealtimeEvent('conversation.changed', handleConversationChangedHint);

  // miy (M4): who is typing in which conversation, until the hint expires.
  const [typingByThreadId, setTypingByThreadId] = useState<
    Record<string, Record<string, number>>
  >({});
  const handleTypingHint = useCallback(
    (event: RealtimeEvent) => {
      const hint = liveTypingHint(event);
      if (!hint || hint.userId === user?.id) return;
      setTypingByThreadId((current) =>
        applyDmTypingHint(current, hint, Date.now() + DM_TYPING_TTL_MS),
      );
    },
    [user?.id],
  );
  useRealtimeEvent('typing', handleTypingHint);
  const typingActive = Object.keys(typingByThreadId).length > 0;
  useEffect(() => {
    if (!typingActive) return undefined;
    const timer = window.setInterval(
      () => setTypingByThreadId((current) => pruneDmTyping(current, Date.now())),
      1000,
    );
    return () => window.clearInterval(timer);
  }, [typingActive]);

  // miy (M4): the server sends typing hints only to frames that show the
  // conversation (rechecking participation); report the shown one after every
  // (re)open.
  useEffect(() => {
    if (realtimeState !== 'live') return;
    sendLive?.({ type: 'open', conversationId: threadId || null });
  }, [realtimeState, reconnectSeq, sendLive, threadId]);

  useEffect(() => {
    if (!token || !threadId) return undefined;
    const markVisibleThreadRead = () => {
      void markVisibleDmThreadReadAndApply({
        markThreadRead: markDmThreadRead,
        onThreadRead: applyReadThread,
        presence: currentDmReadPresenceFromBrowser(document),
        selectedThreadId: selectedThreadIdRef.current,
        token,
      }).catch(() => undefined);
    };
    window.addEventListener('focus', markVisibleThreadRead);
    document.addEventListener('visibilitychange', markVisibleThreadRead);
    return () => {
      window.removeEventListener('focus', markVisibleThreadRead);
      document.removeEventListener('visibilitychange', markVisibleThreadRead);
    };
  }, [applyReadThread, threadId, token]);

  const handleToggleGroupUser = (nextUser: DmUser) => {
    setSelectedUsers((items) => toggleDmGroupSelectedUser(items, nextUser));
  };

  const handleToggleDirectGroupUser = (nextUser: DmUser) => {
    setDirectGroupUsers((items) => toggleDmGroupSelectedUser(items, nextUser));
  };

  const handleCreateConversation = async () => {
    const command = resolveDmConversationCreateCommand({
      authenticated: Boolean(token),
      groupTitle,
      creating: creatingGroup,
      selectedUsers,
    });
    if (!token || !command) return;
    const termsLookedPending = Boolean(
      safety?.terms?.required && !safety.terms.accepted,
    );
    setCreatingGroup(true);
    try {
      const thread =
        command.kind === 'direct'
          ? await createDmThread(token, command.recipientUserId)
          : await createGroupDmThread(
              token,
              command.participantUserIds,
              command.title,
            );
      setThreads((items) => {
        return commitThreadList(upsertDmThreadList(items, thread));
      });
      setQuery('');
      resetUserSearch();
      setSelectedUsers([]);
      setGroupTitle('');
      selectThreadId(thread.id);
      if (termsLookedPending) {
        // 서버가 받아들였다 — 다른 기기에서 동의했을 수 있으니 이 화면의 동의 상태도 새로 받는다.
        void refreshTerms();
      }
    } catch (caughtError) {
      if (dmErrorCode(caughtError) === 'ugc_terms_acceptance_required') {
        // 단체 대화 이름은 이용규칙 동의가 필요하다 — 최신 상태를 받아 만들기 화면에 동의 안내를 보인다(입력한 이름·사람은 그대로).
        void refreshTerms();
      }
      feedback.error(
        caughtError instanceof Error
          ? caughtError.message
          : t('dm.errors.threadCreateFailed'),
      );
    } finally {
      setCreatingGroup(false);
    }
  };

  const handleCreateGroupFromDirect = async () => {
    if (
      !token ||
      !selectedThread ||
      selectedThread.conversation_type !== 'direct'
    ) {
      return;
    }
    const participantUserIds = directDmGroupParticipantUserIds({
      existingRecipientUserId: selectedThread.other_user?.id,
      selectedUsers: directGroupUsers,
    });
    if (participantUserIds.length < 2) {
      return;
    }
    setMemberActionId(selectedThread.id);
    try {
      const thread = await createGroupDmThread(token, participantUserIds, '');
      setThreads((items) => {
        return commitThreadList(upsertDmThreadList(items, thread));
      });
      setDirectGroupUsers([]);
      setMemberPanelOpen(false);
      setMemberQueryOverride(null);
      resetMemberSearch();
      selectThreadId(thread.id);
    } catch (caughtError) {
      feedback.error(
        caughtError instanceof Error
          ? caughtError.message
          : t('dm.errors.threadCreateFailed'),
      );
    } finally {
      setMemberActionId(null);
    }
  };

  const handleSaveTitle = async () => {
    if (!token || !selectedThread || savingTitle) return;
    setSavingTitle(true);
    try {
      const thread = await updateDmConversationTitle(
        token,
        selectedThread.id,
        titleDraft,
      );
      setThreads((items) => {
        return commitThreadList(upsertDmThreadList(items, thread));
      });
    } catch (caughtError) {
      feedback.error(
        caughtError instanceof Error
          ? caughtError.message
          : t('dm.errors.conversationUpdateFailed'),
      );
    } finally {
      setSavingTitle(false);
    }
  };

  const handleAddMember = async (nextUser: DmUser) => {
    if (!token || !selectedThread || memberActionId) return;
    setMemberActionId(nextUser.id);
    try {
      const thread = await addDmConversationParticipants(
        token,
        selectedThread.id,
        [nextUser.id],
      );
      setThreads((items) => {
        return commitThreadList(upsertDmThreadList(items, thread));
      });
      setMemberQueryOverride(null);
      resetMemberSearch();
    } catch (caughtError) {
      feedback.error(
        caughtError instanceof Error
          ? caughtError.message
          : t('dm.errors.memberUpdateFailed'),
      );
    } finally {
      setMemberActionId(null);
    }
  };

  const handleRemoveMember = async (userId: string) => {
    if (!token || !selectedThread || memberActionId) return;
    setMemberActionId(userId);
    try {
      const thread = await removeDmConversationParticipant(
        token,
        selectedThread.id,
        userId,
      );
      setThreads((items) => {
        return commitThreadList(upsertDmThreadList(items, thread));
      });
    } catch (caughtError) {
      feedback.error(
        caughtError instanceof Error
          ? caughtError.message
          : t('dm.errors.memberUpdateFailed'),
      );
    } finally {
      setMemberActionId(null);
    }
  };

  const handleLeaveGroup = async () => {
    if (!token || !selectedThread || memberActionId) return;
    setMemberActionId(
      dmGroupMemberActionTargetId({
        currentUserId: user?.id,
        threadId: selectedThread.id,
      }),
    );
    try {
      await leaveDmConversation(token, selectedThread.id);
      setThreads((items) => {
        clearCachedDmMessages(dmCacheKey, selectedThread.id);
        return commitThreadList(
          items.filter((item) => item.id !== selectedThread.id),
        );
      });
      selectedThreadIdRef.current = '';
      selectThreadId(null);
    } catch (caughtError) {
      feedback.error(
        caughtError instanceof Error
          ? caughtError.message
          : t('dm.errors.memberUpdateFailed'),
      );
    } finally {
      setMemberActionId(null);
    }
  };

  const uploadSelectedFiles = useCallback(
    (selectedFiles: File[]) => {
      if (!selectedFiles.length || sending || editTarget) return;
      void uploadDmComposerAttachments({
        files: selectedFiles,
        token,
        conversationId: threadId,
        uploadAttachment: uploadDmAttachment,
        dispatch: dispatchComposerAttachments,
        createLocalId: createLocalAttachmentId,
        onError: feedback.error,
        fallbackErrorMessage: t('dm.errors.attachmentUploadFailed'),
      });
    },
    [editTarget, feedback, sending, t, threadId, token],
  );

  const handleFileInputChange = (event: ChangeEvent<HTMLInputElement>) => {
    const files = Array.from(event.currentTarget.files ?? []);
    uploadSelectedFiles(files);
    event.currentTarget.value = '';
  };

  const handleComposerPaste = (event: ClipboardEvent<HTMLTextAreaElement>) => {
    const itemFiles = filesFromClipboardData(event.clipboardData);
    if (!itemFiles.length) {
      return;
    }
    event.preventDefault();
    uploadSelectedFiles(itemFiles);
  };

  const handleDragEnter = (event: DragEvent<HTMLElement>) => {
    if (!dataTransferHasFiles(event.dataTransfer)) {
      return;
    }
    event.preventDefault();
    dragDepthRef.current += 1;
    dispatchComposerAttachments({ type: 'dragging', dragging: true });
  };

  const handleDragLeave = (event: DragEvent<HTMLElement>) => {
    if (!dataTransferHasFiles(event.dataTransfer)) {
      return;
    }
    dragDepthRef.current = Math.max(0, dragDepthRef.current - 1);
    if (dragDepthRef.current === 0) {
      dispatchComposerAttachments({ type: 'dragging', dragging: false });
    }
  };

  const handleDrop = (event: DragEvent<HTMLElement>) => {
    if (!dataTransferHasFiles(event.dataTransfer)) {
      return;
    }
    event.preventDefault();
    dragDepthRef.current = 0;
    dispatchComposerAttachments({ type: 'dragging', dragging: false });
    uploadSelectedFiles(Array.from(event.dataTransfer.files));
  };

  const handleRemovePendingAttachment = (localId: string) => {
    const removed = pendingAttachments.find((item) => item.localId === localId);
    dispatchComposerAttachments({ type: 'remove', localId });
    // miy: release the unsent reservation (AI-DO left it unreferenced).
    if (token && removed?.attachment) {
      void cancelDmAttachment(token, removed.attachment.id).catch(() => undefined);
    }
  };

  const handleScrollToReplySource = useCallback((messageId: string) => {
    const element = messageElementRefs.current[messageId];
    if (!element) {
      return;
    }
    element.scrollIntoView({ block: 'center', behavior: 'smooth' });
    setHighlightedMessageId(messageId);
    window.setTimeout(() => {
      setHighlightedMessageId((current) =>
        current === messageId ? null : current,
      );
    }, 1600);
  }, []);

  const handleOpenImageAttachment = async (attachment: DmMessageAttachment) => {
    if (attachmentActionId) return;
    const sourceMessage = messagesRef.current.find((item) =>
      (item.attachments ?? []).some((candidate) => candidate.id === attachment.id),
    );
    imageViewerSourceRef.current = {
      attachmentId: attachment.id,
      messageId: sourceMessage?.id ?? null,
    };
    try {
      await attachmentActionWorkflow.openImageAttachment(attachment);
    } finally {
      imageViewerSourceRef.current = null;
    }
  };

  const handleDownloadAttachment = async (attachment: DmMessageAttachment) => {
    await attachmentActionWorkflow.downloadAttachment(attachment);
  };

  const applyPeerBlocked = (userId: string, blocked: boolean) => {
    setBlockedUserIds((current) => {
      const next = new Set(current);
      if (blocked) next.add(userId);
      else next.delete(userId);
      return next;
    });
    const next = threadsRef.current.map((item) =>
      item.conversation_type === 'direct' && item.other_user?.id === userId
        ? { ...item, peer_blocked: blocked }
        : item,
    );
    threadsRef.current = next;
    writeCachedDmThreads(dmCacheKey, next);
    setThreads(next);
  };

  const handleSubmitReport = async (input: {
    message: DmMessage;
    category: DmReportCategory;
    note: string;
    block: boolean;
  }) => {
    if (!token) return;
    try {
      const result = await reportDm(token, {
        // miy: crypto.randomUUID() needs a secure context; the frame may be on HTTP.
        client_report_id: requestId(),
        conversation_id: input.message.conversation_id,
        message_id: input.message.id,
        category: input.category,
        note: input.note || null,
        block: input.block,
      });
      if (result.blocked) {
        applyPeerBlocked(input.message.sender_id, true);
      }
      setReportTarget(null);
      feedback.success(t('dm.safety.reportReceived'));
    } catch (caughtError) {
      feedback.error(
        caughtError instanceof Error
          ? caughtError.message
          : t('dm.safety.reportFailed'),
      );
    }
  };

  const handleToggleBlock = async () => {
    const peer = selectedThread?.other_user;
    if (!token || !selectedThread || !peer) return;
    const nextBlocked = !selectedThread.peer_blocked;
    try {
      const result = await setDmBlock(token, peer.id, nextBlocked);
      applyPeerBlocked(peer.id, result.blocked);
      setBlockDialogOpen(false);
      feedback.success(
        result.blocked ? t('dm.safety.blocked') : t('dm.safety.unblocked'),
      );
    } catch (caughtError) {
      feedback.error(
        caughtError instanceof Error
          ? caughtError.message
          : t('dm.safety.blockFailed'),
      );
    }
  };

  const loadBlockedUsers = useCallback(async () => {
    if (!token) return [];
    const response = await listDmBlocks(token);
    setBlockedUserIds(new Set(response.items.map((item) => item.user.id)));
    return response.items;
  }, [token]);

  const handleUnblockUser = async (userId: string): Promise<boolean> => {
    if (!token) return false;
    try {
      const result = await setDmBlock(token, userId, false);
      applyPeerBlocked(userId, result.blocked);
      if (!result.blocked) {
        feedback.success(t('dm.safety.unblocked'));
      }
      return !result.blocked;
    } catch (caughtError) {
      feedback.error(
        caughtError instanceof Error
          ? caughtError.message
          : t('dm.safety.blockFailed'),
      );
      return false;
    }
  };

  const handleSaveEdit = async () => {
    if (!token || !editing || sending) return;
    const target = editing.message;
    setSending(true);
    try {
      const updated = await editDmMessage(
        token,
        target.conversation_id,
        target.id,
        editing.body.trim(),
        editing.mentions ?? [],
      );
      applyChangedMessage(updated);
      setEditing((current) => (current === editing ? null : current));
    } catch (caughtError) {
      if (dmErrorCode(caughtError) === 'ugc_terms_acceptance_required') {
        void refreshTerms();
      }
      feedback.error(
        caughtError instanceof Error
          ? caughtError.message
          : t('dm.actionErrors.editFailed'),
      );
    } finally {
      setSending(false);
    }
  };

  const applyChangedMessage = (updated: DmMessage) => {
    const threadMessages =
      selectedThreadIdRef.current === updated.conversation_id
        ? messagesRef.current
        : (messageCacheByThreadIdRef.current[updated.conversation_id] ?? []);
    const nextMessages = replaceDmMessage(threadMessages, updated);
    messageCacheByThreadIdRef.current = setDmThreadRecordValue(
      messageCacheByThreadIdRef.current,
      updated.conversation_id,
      nextMessages,
    );
    writeCachedDmMessages(dmCacheKey, updated.conversation_id, nextMessages);
    if (selectedThreadIdRef.current === updated.conversation_id) {
      messagesRef.current = nextMessages;
      dispatchMessages({ type: 'replace', items: nextMessages });
    }
    void loadThreads({ force: true });
  };

  const startEdit = (message: DmMessage) => {
    if (sending) return;
    setReplyTarget(null);
    setEditing({
      message,
      body: message.body,
      mentions: dmMentionDraftsFromMessage(message.body, message.mentions),
    });
  };

  const cancelEdit = () => {
    setEditing(null);
  };

  const handleConfirmDelete = async () => {
    const target = deleteTarget;
    if (!token || !target || deleting) return;
    setDeleting(true);
    try {
      const updated = await deleteDmMessage(token, target.conversation_id, target.id);
      applyChangedMessage(updated);
      setEditing((current) => (current?.message.id === target.id ? null : current));
      setReplyTarget((current) => (current?.id === target.id ? null : current));
      setDeleteTarget(null);
    } catch (caughtError) {
      feedback.error(
        caughtError instanceof Error
          ? caughtError.message
          : t('dm.actionErrors.deleteFailed'),
      );
    } finally {
      setDeleting(false);
    }
  };

  const handleLoadOlder = async () => {
    const targetThreadId = threadId;
    const oldest = messagesRef.current[0];
    if (!token || !targetThreadId || !oldest) return;
    setOlderByThreadId((current) => ({
      ...current,
      [targetThreadId]: { loading: true, exhausted: false },
    }));
    try {
      const response = await listDmMessages(
        token,
        targetThreadId,
        DM_MESSAGE_PAGE,
        oldest.sequence,
      );
      if (selectedThreadIdRef.current !== targetThreadId) return;
      const element = messagesScrollRef.current;
      if (element) {
        olderAnchorRef.current = {
          scrollHeight: element.scrollHeight,
          scrollTop: element.scrollTop,
        };
      }
      const items = prependOlderDmMessages(messagesRef.current, response.items);
      messagesRef.current = items;
      messageCacheByThreadIdRef.current = setDmThreadRecordValue(
        messageCacheByThreadIdRef.current,
        targetThreadId,
        items,
      );
      writeCachedDmMessages(dmCacheKey, targetThreadId, items);
      dispatchMessages({ type: 'replace', items });
      setOlderByThreadId((current) => ({
        ...current,
        [targetThreadId]: {
          loading: false,
          exhausted: response.items.length < DM_MESSAGE_PAGE,
        },
      }));
    } catch (caughtError) {
      setOlderByThreadId((current) => ({
        ...current,
        [targetThreadId]: { loading: false, exhausted: false },
      }));
      feedback.error(
        caughtError instanceof Error
          ? caughtError.message
          : t('dm.actionErrors.olderLoadFailed'),
      );
    }
  };

  const handleToggleMute = async () => {
    if (!token || !selectedThread || muting) return;
    const nextMuted = !selectedThread.muted_at;
    setMuting(true);
    try {
      const thread = await setDmConversationMuted(token, selectedThread.id, nextMuted);
      setThreads((items) => commitThreadList(upsertDmThreadList(items, thread)));
      feedback.success(nextMuted ? t('dm.muted') : t('dm.unmuted'));
    } catch (caughtError) {
      feedback.error(
        caughtError instanceof Error
          ? caughtError.message
          : t('dm.actionErrors.muteFailed'),
      );
    } finally {
      setMuting(false);
    }
  };

  const handleSend = async () => {
    if (editTarget) {
      await handleSaveEdit();
      return;
    }
    const command = resolveDmSendDraftCommand({
      authenticated: Boolean(token),
      draft,
      readyAttachmentIds,
      replyToMessageId:
        replyTarget?.conversation_id === threadId ? replyTarget.id : null,
      sending,
      threadId,
      uploadingAttachmentCount,
    });
    if (!token || !command) return;
    const submittedDraft = draft;
    stopTyping(command.threadId);
    setSending(true);
    draftByThreadIdRef.current = setDmThreadRecordValue(
      draftByThreadIdRef.current,
      command.threadId,
      '',
    );
    if (selectedThreadIdRef.current === command.threadId) {
      setDraft('');
    }
    try {
      const message = await sendDmMessage(
        token,
        command.threadId,
        command.body,
        command.attachmentIds,
        command.replyToMessageId,
        mentionDraftsByThreadRef.current[command.threadId] ?? [],
      );
      // The mentions went with the body; a later draft starts without them.
      if (!draftByThreadIdRef.current[command.threadId]) {
        delete mentionDraftsByThreadRef.current[command.threadId];
      }
      const activeThreadMatches =
        selectedThreadIdRef.current === command.threadId;
      const currentMessages = activeThreadMatches
        ? messagesRef.current
        : (messageCacheByThreadIdRef.current[command.threadId] ?? []);
      const nextMessages = appendDmThreadMessage(currentMessages, message);
      messageCacheByThreadIdRef.current = setDmThreadRecordValue(
        messageCacheByThreadIdRef.current,
        command.threadId,
        nextMessages,
      );
      writeCachedDmMessages(dmCacheKey, command.threadId, nextMessages);
      if (activeThreadMatches) {
        scrollThreadToBottom(command.threadId);
        messagesRef.current = nextMessages;
        dispatchMessages({ type: 'replace', items: nextMessages });
        dispatchComposerAttachments({ type: 'clearPending' });
        setReplyTarget((current) => (current?.id === command.replyToMessageId ? null : current));
      }
      requestAnimationFrame(() => {
        if (selectedThreadIdRef.current === command.threadId) {
          scrollThreadToBottom(command.threadId);
          bottomRef.current?.scrollIntoView({ block: 'end' });
        }
      });
      void loadThreads({ force: true });
    } catch (caughtError) {
      if (!draftByThreadIdRef.current[command.threadId]) {
        draftByThreadIdRef.current = setDmThreadRecordValue(
          draftByThreadIdRef.current,
          command.threadId,
          submittedDraft,
        );
        if (selectedThreadIdRef.current === command.threadId) {
          setDraft(submittedDraft);
        }
      } else if (submittedDraft.trim()) {
        setFailedDraftsByThread((current) => ({
          ...current,
          [command.threadId]: [...(current[command.threadId] ?? []), submittedDraft],
        }));
      }
      if (dmErrorCode(caughtError) === 'ugc_terms_acceptance_required') {
        // 이용규칙이 바뀌었거나 아직 동의하지 않았다 — 최신 상태를 받아 동의 안내를 보인다.
        void refreshTerms();
      }
      feedback.error(
        caughtError instanceof Error
          ? caughtError.message
          : t('dm.errors.sendFailed'),
      );
    } finally {
      setSending(false);
    }
  };

  const refreshTerms = async () => {
    if (!token) return;
    // 받는 사이 동의했거나 더 나중에 시작한 받기가 이미 반영됐으면 이 (더 오래된) 답은 버린다 — 늦게 온 동의 전 값이
    // 방금 한 동의를 되돌려 보내기를 다시 막지 않게(리뷰 8029). 다른 받기가 시작만 했거나 실패한 것으로는 버리지 않는다
    // (리뷰 8040 — 새 판 이용규칙 안내를 놓치지 않게). 동의 뒤에 시작한 받기는 그대로 반영한다(다른 기기 동의·새 판).
    const seq = ++termsReadSeqRef.current;
    const scope = dmCacheKey;
    try {
      const terms = await getDmTerms(token);
      if (termsScopeRef.current !== scope) return;
      if (seq <= termsAppliedSeqRef.current) {
        // 더 먼저 시작한 답은 화면 값을 되돌리지 못하지만, 같은 판의 동의 확인은 받아들인다(다른 기기 동의).
        setSafety((current) => {
          if (!current?.terms) return current;
          const kept = keepConfirmedDmTermsAcceptance(current.terms, terms);
          return kept === current.terms ? current : { ...current, terms: kept };
        });
        return;
      }
      termsAppliedSeqRef.current = seq;
      setSafety((current) => (current ? { ...current, terms } : current));
    } catch {
      // 다음 목록 갱신에서 다시 받는다.
    }
  };

  const handleAcceptTerms = async () => {
    const terms = safety?.terms;
    if (!token || !terms || acceptingTerms) return;
    setAcceptingTerms(true);
    const scope = dmCacheKey;
    try {
      const result = await acceptDmTerms(token, terms.version);
      if (termsScopeRef.current !== scope) return;
      // 이보다 먼저 시작한 받기(동의 전 값)는 이 동의를 덮어쓰지 못한다.
      termsAppliedSeqRef.current = termsReadSeqRef.current;
      setSafety((current) =>
        // 그사이 새 판 이용규칙을 받았으면 그 판은 동의하지 않은 그대로 둔다.
        current?.terms && current.terms.version === terms.version
          ? {
              ...current,
              terms: { ...current.terms, accepted: true, accepted_at: result.accepted_at },
            }
          : current,
      );
      feedback.success(t('dm.safety.termsAccepted'));
    } catch (caughtError) {
      feedback.error(
        caughtError instanceof Error ? caughtError.message : t('dm.safety.termsAcceptFailed'),
      );
      void refreshTerms();
    } finally {
      setAcceptingTerms(false);
    }
  };

  const termsPending = Boolean(safety?.terms?.required && !safety.terms.accepted);
  // 단체 대화 이름은 서버가 이용규칙 동의를 요구한다 — 첫 대화 전에도 만들기 화면에서 읽고 동의할 수 있게 한다.
  // 만들기 단추는 막지 않는다 — 다른 기기에서 이미 동의했을 수 있어 서버가 판정한다(거절되면 안내를 다시 받는다).
  const groupCreateTermsVisible = termsPending && selectedUsers.length >= 2;
  const visibleImageViewer =
    imageViewer && !isDmImageViewerSourceHiddenIn(messages, imageViewer.source)
      ? imageViewer
      : null;

  // Closing the frame drops the drafts cached for every conversation.
  const failedDrafts = failedDraftsByThread[threadId] ?? [];
  useNavigationDirty(
    Boolean(draft.trim()) ||
      Object.values(draftByThreadIdRef.current).some((body) => Boolean(body.trim())) ||
      Object.values(failedDraftsByThread).some((drafts) => drafts.length > 0) ||
      pendingAttachments.length > 0 ||
      sending ||
      creatingGroup ||
      savingTitle ||
      Boolean(groupTitle.trim()) ||
      Boolean(selectedThread && titleDraft !== (selectedThread.title ?? '')) ||
      Boolean(editTarget),
  );
  const olderState = threadId ? olderByThreadId[threadId] : undefined;
  const canLoadOlder =
    messages.length >= DM_MESSAGE_PAGE && !olderState?.exhausted;
  // miy: "@" picker over the conversation's current participants.
  const mentionCandidates = useMemo(() => {
    if (!mentionPicker || !selectedThread) return [];
    return dmMentionCandidates(
      selectedThread.participants.map((participant) => ({
        id: participant.user.id,
        name: displayDmUserName(participant.user),
      })),
      user?.id,
      mentionPicker.active.query,
    );
  }, [mentionPicker, selectedThread, user?.id]);
  const pickMention = (candidate: { id: string; name: string } | undefined) => {
    const picker = mentionPicker;
    if (!candidate || !picker) return;
    const text = editing?.body ?? draft;
    const inserted = insertDmMention(text, picker.active, picker.caret, candidate.name);
    const mention = { userId: candidate.id, label: inserted.label };
    if (editing) {
      setEditing({
        ...editing,
        body: inserted.text,
        mentions: [...(editing.mentions ?? []), mention],
      });
    } else if (threadId) {
      setSelectedThreadDraft(inserted.text);
      mentionDraftsByThreadRef.current = {
        ...mentionDraftsByThreadRef.current,
        [threadId]: [...(mentionDraftsByThreadRef.current[threadId] ?? []), mention],
      };
    }
    setMentionPicker(null);
    window.requestAnimationFrame(() => {
      const element = composerRef.current;
      if (!element) return;
      element.focus();
      element.setSelectionRange(inserted.caret, inserted.caret);
    });
  };

  // miy: reactions toggle through the API; the answer replaces the message.
  const handleToggleReaction = async (message: DmMessage, reaction: string, on: boolean) => {
    setReactionPickerFor(null);
    if (!token) return;
    try {
      applyChangedMessage(
        await setDmReaction(token, message.conversation_id, message.id, reaction, on),
      );
    } catch (caughtError) {
      feedback.error(
        caughtError instanceof Error ? caughtError.message : t('dm.reactions.failed'),
      );
    }
  };

  // miy: open a search hit or a `?message=` focus: show the conversation, then
  // load older pages (bounded) until the message is there and bring it into
  // view. A later selection or another conversation cancels the earlier one.
  const requestJump = useCallback(
    (conversationId: string, messageId: string) => {
      pendingJumpRef.current = { threadId: conversationId, messageId };
      if (selectedThreadIdRef.current !== conversationId) {
        selectThreadId(conversationId);
      }
      setJumpSeq((value) => value + 1);
    },
    [selectThreadId],
  );
  const handleSelectSearchHit = (hit: DmSearchHit) => {
    setSearchOpen(false);
    requestJump(hit.conversation_id, hit.message_id);
  };
  const lastFocusRef = useRef<string | null>(null);
  useEffect(() => {
    const key = focusMessageId && threadId ? `${threadId}:${focusMessageId}` : null;
    if (!key || key === lastFocusRef.current || !focusMessageId) return;
    lastFocusRef.current = key;
    requestJump(threadId, focusMessageId);
  }, [focusMessageId, requestJump, threadId]);
  useEffect(() => {
    const pending = pendingJumpRef.current;
    if (!pending || !token || pending.threadId !== threadId || messagesLoading) return;
    if (messagesRef.current.some((message) => message.id === pending.messageId)) {
      pendingJumpRef.current = null;
      window.requestAnimationFrame(() => handleScrollToReplySource(pending.messageId));
      return;
    }
    // Wait for the conversation's first page before paging back.
    if (jumpingRef.current || !messagesRef.current.length) return;
    jumpingRef.current = true;
    void (async () => {
      try {
        for (let page = 0; page < DM_JUMP_MAX_PAGES; page += 1) {
          const current = pendingJumpRef.current;
          if (current !== pending || selectedThreadIdRef.current !== pending.threadId) return;
          const oldest = messagesRef.current[0];
          if (!oldest) break;
          const response = await listDmMessages(
            token,
            pending.threadId,
            DM_JUMP_PAGE,
            oldest.sequence,
          );
          // A late answer for another conversation or jump is dropped.
          if (pendingJumpRef.current !== pending || selectedThreadIdRef.current !== pending.threadId) {
            return;
          }
          const items = prependOlderDmMessages(messagesRef.current, response.items);
          messagesRef.current = items;
          messageCacheByThreadIdRef.current = setDmThreadRecordValue(
            messageCacheByThreadIdRef.current,
            pending.threadId,
            items,
          );
          writeCachedDmMessages(dmCacheKey, pending.threadId, items);
          dispatchMessages({ type: 'replace', items });
          if (items.some((message) => message.id === pending.messageId)) {
            pendingJumpRef.current = null;
            window.requestAnimationFrame(() => handleScrollToReplySource(pending.messageId));
            return;
          }
          if (response.items.length < DM_JUMP_PAGE) break;
        }
        if (pendingJumpRef.current === pending) {
          pendingJumpRef.current = null;
          feedback.error(t('dm.search.jumpFailed'));
        }
      } catch (caughtError) {
        if (pendingJumpRef.current === pending) {
          pendingJumpRef.current = null;
          feedback.error(
            caughtError instanceof Error ? caughtError.message : t('dm.search.jumpFailed'),
          );
        }
      } finally {
        jumpingRef.current = false;
      }
    })();
  }, [dmCacheKey, feedback, handleScrollToReplySource, jumpSeq, messages, messagesLoading, t, threadId, token]);

  const typingLabel = useMemo(() => {
    const typing = threadId ? typingByThreadId[threadId] : undefined;
    if (!typing || !selectedThread) return null;
    const names = Object.keys(typing).map((userId) => {
      const participant = selectedThread.participants.find(
        (item) => item.user.id === userId,
      );
      return participant ? displayDmUserName(participant.user) : null;
    });
    const shown = names.filter((name): name is string => Boolean(name));
    if (!shown.length) return null;
    if (shown.length === 1) return t('dm.typingOne', { name: shown[0] });
    if (shown.length === 2) {
      return t('dm.typingTwo', { first: shown[0], second: shown[1] });
    }
    return t('dm.typingMany', { count: shown.length });
  }, [selectedThread, t, threadId, typingByThreadId]);
  const currentUserLabel = t('pms.taskDetail.me');
  const selectedThreadListClass =
    threadId && !unresolvedRouteThread
      ? embeddedPersistentList
        ? 'hidden min-[900px]:flex min-[900px]:w-full'
        : 'hidden'
      : 'max-w-none border-r-0';
  const threadPanelClass =
    threadId && !unresolvedRouteThread
      ? 'flex'
      : embeddedPersistentList
        ? 'hidden min-[900px]:flex'
        : 'hidden';

  return (
    <div
      className={`flex h-full min-h-0 bg-app-bg text-app-ink ${
        embeddedPersistentList
          ? 'min-[900px]:grid min-[900px]:grid-cols-[280px_minmax(0,1fr)]'
          : ''
      }`}
    >
      <aside
        className={`flex min-h-0 w-full shrink-0 flex-col border-r border-app-border bg-app-surface-sidebar ${
          selectedThreadListClass
        }`}
      >
        <header className="border-b border-app-border px-4 py-3">
          {!embeddedPersistentList ? (
            <div className="flex items-center justify-between gap-3">
              <h1 className="app-text-title-md text-app-ink">
                {t('dm.title')}
              </h1>
              <span className="inline-flex size-8 items-center justify-center rounded-md border border-app-border bg-app-bg text-app-ink/65">
                {realtimeState === 'live' ? (
                  <Wifi size={15} />
                ) : (
                  <WifiOff size={15} />
                )}
              </span>
            </div>
          ) : null}
          <div
            className={`flex items-center gap-2 rounded-md border border-app-border bg-app-bg px-3 py-2 ${
              embeddedPersistentList ? '' : 'mt-3'
            }`}
          >
            <Search size={16} className="shrink-0 text-app-ink/45" />
            <input
              aria-label={t('dm.searchPlaceholder')}
              className="app-text-body w-full bg-transparent text-app-ink outline-none placeholder:text-app-ink/35"
              onChange={(event) => setQuery(event.target.value)}
              placeholder={t('dm.searchPlaceholder')}
              value={query}
            />
            {searching ? (
              <Loader2 size={14} className="animate-spin text-app-ink/45" />
            ) : null}
          </div>
          <button
            className="app-text-caption mt-2 mr-3 inline-flex items-center gap-1 text-app-ink/55 hover:text-app-ink"
            onClick={() => setSearchOpen(true)}
            type="button"
          >
            <Search aria-hidden="true" size={12} />
            {t('dm.search.open')}
          </button>
          {safety?.block && blockedUserIds.size > 0 ? (
            <button
              className="app-text-caption mt-2 inline-flex items-center gap-1 text-app-ink/55 hover:text-app-ink"
              onClick={() => setBlockedUsersOpen(true)}
              type="button"
            >
              <Ban aria-hidden="true" size={12} />
              {t('dm.safety.blockedUsers')}
            </button>
          ) : null}
          {(safety?.manager || safety?.manager_claimable) && onOpenSafety ? (
            <button
              className="app-text-caption mt-2 ml-3 inline-flex items-center gap-1 text-app-ink/55 hover:text-app-ink"
              onClick={onOpenSafety}
              type="button"
            >
              <ShieldAlert aria-hidden="true" size={12} />
              {t('dm.safetyReview')}
            </button>
          ) : null}
        </header>

        <div className="border-b border-app-border px-4 py-3">
          {selectedUsers.length >= 2 ? (
            <input
              aria-label={t('dm.groupNamePlaceholder')}
              className={inputClassName}
              onChange={(event) => setGroupTitle(event.target.value)}
              placeholder={t('dm.groupNamePlaceholder')}
              value={groupTitle}
            />
          ) : null}
          {selectedUsers.length ? (
            <div
              className={`custom-scrollbar flex max-h-20 flex-wrap gap-1.5 overflow-y-auto pr-1 ${
                selectedUsers.length >= 2 ? 'mt-2' : ''
              }`}
            >
              {selectedUsers.map((item) => (
                <button
                  className="app-text-caption inline-flex max-w-full items-center gap-1 rounded-full border border-app-border bg-app-bg px-2 py-1 text-app-ink"
                  key={item.id}
                  onClick={() => handleToggleGroupUser(item)}
                  type="button"
                >
                  <span className="truncate">{displayDmUserName(item)}</span>
                  <X size={12} className="shrink-0" />
                </button>
              ))}
            </div>
          ) : null}
          {groupCreateTermsVisible && safety?.terms ? (
            <DmTermsNotice
              accepting={acceptingTerms}
              className="mt-2"
              onAccept={() => void handleAcceptTerms()}
              terms={safety.terms}
            />
          ) : null}
          <div className="mt-2 flex items-center justify-between gap-3">
            <span className="app-text-caption text-app-ink/45">
              {selectedUsers.length
                ? t('dm.selectedRecipients', {
                    count: selectedUsers.length,
                  })
                : t('dm.selectRecipientsHint')}
            </span>
            <button
              className="app-text-caption inline-flex h-8 shrink-0 items-center gap-1 rounded-md bg-app-ink px-3 font-medium text-app-bg disabled:cursor-not-allowed disabled:opacity-50"
              disabled={isDmConversationCreateActionDisabled({
                creating: creatingGroup,
                selectedUsers,
              })}
              onClick={() => void handleCreateConversation()}
              type="button"
            >
              {creatingGroup ? (
                <Loader2 size={13} className="animate-spin" />
              ) : (
                <Plus size={13} />
              )}
              {selectedUsers.length >= 2
                ? t('dm.createGroup')
                : t('dm.startConversation')}
            </button>
          </div>
        </div>

        {query.trim() ? (
          <div className="custom-scrollbar max-h-[min(22rem,45dvh)] shrink-0 overflow-y-auto border-b border-app-border py-2">
            {userResults.length ? (
              userResults.map((item) => {
                const selected = selectedUserIds.has(item.id);
                return (
                  <UserOptionRow
                    currentUserId={user?.id}
                    currentUserLabel={currentUserLabel}
                    key={item.id}
                    onClick={() => handleToggleGroupUser(item)}
                    selected={selected}
                    user={item}
                  />
                );
              })
            ) : (
              <div className="app-text-body px-4 py-6 text-center text-app-ink/45">
                {searching ? t('dm.searching') : t('dm.noSearchResults')}
              </div>
            )}
          </div>
        ) : null}

        <div className="min-h-0 flex-1 overflow-y-auto py-2">
          {threadsLoading ? (
            <div className="flex justify-center py-8">
              <Loader2 size={18} className="animate-spin text-app-ink/45" />
            </div>
          ) : threadListItems.length ? (
            threadListItems.map((item) => (
              <button
                aria-current={item.active ? 'page' : undefined}
                className={`relative flex w-full items-center gap-3 border-l-4 px-4 py-3 text-left transition-colors focus:outline-none focus:ring-2 focus:ring-app-accent/25 ${
                  item.active
                    ? 'border-app-accent bg-app-accent/10 text-app-ink shadow-sm'
                    : 'border-transparent hover:bg-app-surface-hover'
                }`}
                key={item.id}
                onClick={() => selectThreadId(item.id)}
                type="button"
              >
                <span
                  className={`app-text-caption flex size-10 shrink-0 items-center justify-center rounded-full ${
                    item.active
                      ? 'bg-app-accent text-app-accent-fg ring-2 ring-app-accent/25'
                      : 'bg-app-ink text-app-bg'
                  }`}
                >
                  {item.initials}
                </span>
                <span className="min-w-0 flex-1">
                  <span className="app-text-body-sm block truncate font-medium">
                    {item.name}
                  </span>
                  <span
                    className={`app-text-caption block truncate ${
                      item.active ? 'text-app-ink/70' : 'text-app-ink/50'
                    }`}
                  >
                    {item.preview}
                  </span>
                </span>
                {item.unreadBadge ? (
                  <span className="app-text-micro flex h-5 min-w-5 items-center justify-center rounded-full bg-app-danger px-1.5 font-bold text-white">
                    {item.unreadBadge}
                  </span>
                ) : null}
              </button>
            ))
          ) : (
            <div className="app-text-body px-6 py-10 text-center text-app-ink/45">
              {t('dm.emptyThreads')}
            </div>
          )}
        </div>
      </aside>

      <section
        className={`relative min-h-0 min-w-0 flex-1 flex-col overflow-hidden bg-app-bg ${
          threadPanelClass
        }`}
        onDragEnter={selectedThread ? handleDragEnter : undefined}
        onDragLeave={selectedThread ? handleDragLeave : undefined}
        onDragOver={selectedThread ? applyDmComposerFileDragOver : undefined}
        onDrop={selectedThread ? handleDrop : undefined}
      >
        {selectedThread ? (
          <>
            <header className="flex h-16 shrink-0 items-center gap-3 border-b border-app-border px-5">
              <button
                aria-label={t('dm.back')}
                className={`flex size-9 shrink-0 items-center justify-center rounded-md border border-app-border bg-app-surface text-app-ink ${
                  embeddedPersistentList ? 'min-[900px]:hidden' : ''
                }`}
                onClick={() => selectThreadId(null)}
                type="button"
              >
                <ArrowLeft size={16} />
              </button>
              <div className="flex shrink-0 items-center gap-1">
                <button
                  aria-label={t('dm.previousThread')}
                  className="flex size-8 items-center justify-center rounded-md border border-app-border bg-app-surface text-app-ink disabled:cursor-not-allowed disabled:opacity-40"
                  disabled={!threadNavigationHistory.back.length}
                  onClick={navigatePreviousThread}
                  type="button"
                >
                  <ChevronLeft size={15} />
                </button>
                <button
                  aria-label={t('dm.nextThread')}
                  className="flex size-8 items-center justify-center rounded-md border border-app-border bg-app-surface text-app-ink disabled:cursor-not-allowed disabled:opacity-40"
                  disabled={!threadNavigationHistory.forward.length}
                  onClick={navigateNextThread}
                  type="button"
                >
                  <ChevronRight size={15} />
                </button>
              </div>
              <span className="app-text-caption flex size-10 shrink-0 items-center justify-center rounded-full bg-app-ink text-app-bg">
                {dmInitials(selectedThreadName)}
              </span>
              <div className="min-w-0 flex-1">
                <div className="app-text-title-sm truncate text-app-ink">
                  {selectedThreadName}
                </div>
                <div className="app-text-caption truncate text-app-ink/50">
                  {selectedThread.conversation_type === 'group'
                    ? t('dm.memberCount', {
                        count: selectedThread.participant_count,
                      })
                    : selectedThread.other_user?.email ||
                      selectedThread.other_user?.primary_org_unit_name}
                </div>
              </div>
              <button
                aria-label={t('dm.search.open')}
                className="flex size-9 shrink-0 items-center justify-center rounded-md border border-app-border bg-app-surface text-app-ink"
                onClick={() => setSearchOpen(true)}
                title={t('dm.search.open')}
                type="button"
              >
                <Search size={16} />
              </button>
              <button
                aria-label={selectedThread.muted_at ? t('dm.unmute') : t('dm.mute')}
                aria-pressed={Boolean(selectedThread.muted_at)}
                className="flex size-9 shrink-0 items-center justify-center rounded-md border border-app-border bg-app-surface text-app-ink disabled:opacity-50"
                disabled={muting}
                onClick={() => void handleToggleMute()}
                title={selectedThread.muted_at ? t('dm.unmute') : t('dm.mute')}
                type="button"
              >
                {selectedThread.muted_at ? <BellOff size={16} /> : <Bell size={16} />}
              </button>
              <button
                aria-label={
                  selectedThread.conversation_type === 'group'
                    ? t('dm.manageMembers')
                    : t('dm.createGroupFromDirect')
                }
                className="flex size-9 shrink-0 items-center justify-center rounded-md border border-app-border bg-app-surface text-app-ink"
                onClick={() => setMemberPanelOpen((open) => !open)}
                type="button"
              >
                <Users size={16} />
              </button>
              {safety?.block &&
              selectedThread.conversation_type === 'direct' &&
              selectedThread.other_user ? (
                <button
                  aria-label={
                    selectedThread.peer_blocked
                      ? t('dm.safety.unblock')
                      : t('dm.safety.block')
                  }
                  aria-pressed={Boolean(selectedThread.peer_blocked)}
                  className={`flex size-9 shrink-0 items-center justify-center rounded-md border border-app-border bg-app-surface ${
                    selectedThread.peer_blocked ? 'text-app-danger-text' : 'text-app-ink'
                  }`}
                  onClick={() => setBlockDialogOpen(true)}
                  title={
                    selectedThread.peer_blocked
                      ? t('dm.safety.unblock')
                      : t('dm.safety.block')
                  }
                  type="button"
                >
                  <Ban size={16} />
                </button>
              ) : null}
            </header>
            {directGroupCreatePanelVisible ? (
              <div className="shrink-0 border-b border-app-border bg-app-surface px-5 py-4">
                <div className="mx-auto grid max-w-2xl gap-3">
                  <div>
                    <div className="app-text-body-sm font-medium text-app-ink">
                      {t('dm.createGroupFromDirect')}
                    </div>
                    <div className="app-text-caption mt-1 text-app-ink/55">
                      {t('dm.createGroupFromDirectHint', {
                        name: selectedThreadName,
                      })}
                    </div>
                  </div>
                  {directGroupUsers.length ? (
                    <div className="custom-scrollbar flex max-h-20 flex-wrap gap-1.5 overflow-y-auto pr-1">
                      {directGroupUsers.map((item) => (
                        <button
                          className="app-text-caption inline-flex max-w-full items-center gap-1 rounded-full border border-app-border bg-app-bg px-2 py-1 text-app-ink"
                          key={item.id}
                          onClick={() => handleToggleDirectGroupUser(item)}
                          type="button"
                        >
                          <span className="truncate">
                            {displayDmUserName(item)}
                          </span>
                          <X size={12} className="shrink-0" />
                        </button>
                      ))}
                    </div>
                  ) : null}
                  <div className="flex items-center gap-2 rounded-md border border-app-border bg-app-bg px-3 py-2">
                    <Search size={15} className="shrink-0 text-app-ink/45" />
                    <input
                      aria-label={t('dm.addMemberPlaceholder')}
                      className="app-text-body-sm w-full bg-transparent text-app-ink outline-none placeholder:text-app-ink/35"
                      onChange={(event) => {
                        setMemberQueryOverride(
                          createDmThreadTextDraftOverride(
                            selectedThread.id,
                            event.target.value,
                          ),
                        );
                      }}
                      placeholder={t('dm.addMemberPlaceholder')}
                      value={memberQuery}
                    />
                    {memberSearching ? (
                      <Loader2
                        size={13}
                        className="animate-spin text-app-ink/45"
                      />
                    ) : null}
                  </div>
                  {memberQuery.trim() ? (
                    <div className="custom-scrollbar max-h-44 overflow-y-auto rounded-md border border-app-border bg-app-bg">
                      {memberResults.length ? (
                        memberResults.map((item) => {
                          const selected = directGroupUserIds.has(item.id);
                          return (
                            <UserOptionRow
                              currentUserId={user?.id}
                              currentUserLabel={currentUserLabel}
                              key={item.id}
                              onClick={() => handleToggleDirectGroupUser(item)}
                              selected={selected}
                              user={item}
                            />
                          );
                        })
                      ) : (
                        <div className="app-text-caption px-3 py-4 text-center text-app-ink/45">
                          {memberSearching
                            ? t('dm.searching')
                            : t('dm.noSearchResults')}
                        </div>
                      )}
                    </div>
                  ) : null}
                  <div className="flex items-center justify-between gap-3">
                    <span className="app-text-caption text-app-ink/45">
                      {t('dm.directGroupSelectedHint', {
                        count: directDmGroupParticipantUserIds({
                          existingRecipientUserId:
                            selectedThread.other_user?.id,
                          selectedUsers: directGroupUsers,
                        }).length,
                      })}
                    </span>
                    <button
                      className="app-text-caption inline-flex h-9 shrink-0 items-center gap-1 rounded-md bg-app-ink px-3 font-medium text-app-bg disabled:cursor-not-allowed disabled:opacity-50"
                      disabled={isDirectDmGroupCreateActionDisabled({
                        creating: Boolean(memberActionId),
                        existingRecipientUserId: selectedThread.other_user?.id,
                        selectedUsers: directGroupUsers,
                      })}
                      onClick={() => void handleCreateGroupFromDirect()}
                      type="button"
                    >
                      {memberActionId === selectedThread.id ? (
                        <Loader2 size={13} className="animate-spin" />
                      ) : (
                        <Plus size={13} />
                      )}
                      {t('dm.createGroupFromDirectAction')}
                    </button>
                  </div>
                </div>
              </div>
            ) : null}
            {groupMemberPanelVisible ? (
              <div className="shrink-0 border-b border-app-border bg-app-surface px-5 py-4">
                <div className="grid gap-3 lg:grid-cols-[minmax(0,1fr)_minmax(240px,320px)]">
                  <div className="min-w-0">
                    <div className="flex items-center gap-2">
                      <input
                        aria-label={t('dm.groupNamePlaceholder')}
                        className={inputClassName}
                        disabled={!canManageSelectedThread}
                        onChange={(event) => {
                          if (!selectedThread) return;
                          setTitleDraftOverride(
                            createDmThreadTextDraftOverride(
                              selectedThread.id,
                              event.target.value,
                            ),
                          );
                        }}
                        placeholder={t('dm.groupNamePlaceholder')}
                        value={titleDraft}
                      />
                      {canManageSelectedThread ? (
                        <button
                          className="app-text-caption h-10 shrink-0 rounded-md bg-app-ink px-3 font-medium text-app-bg disabled:cursor-not-allowed disabled:opacity-50"
                          disabled={savingTitle}
                          onClick={() => void handleSaveTitle()}
                          type="button"
                        >
                          {savingTitle ? t('dm.saving') : t('dm.save')}
                        </button>
                      ) : null}
                    </div>
                    <div className="mt-3 space-y-1">
                      {selectedThread.participants.map((participant) => {
                        const participantName = displayDmUserName(
                          participant.user,
                        );
                        const removable = isDmGroupMemberRemovable({
                          canManageMembers: canManageSelectedThread,
                          currentUserId: user?.id,
                          participant,
                        });
                        return (
                          <div
                            className="flex items-center gap-3 rounded-md p-2 hover:bg-app-bg"
                            key={participant.user.id}
                          >
                            <span className="app-text-caption flex size-8 shrink-0 items-center justify-center rounded-full bg-app-ink text-app-bg">
                              {dmInitials(participantName)}
                            </span>
                            <span className="min-w-0 flex-1">
                              <span className="app-text-body-sm block truncate text-app-ink">
                                {participantName}
                              </span>
                              <span className="app-text-caption block truncate text-app-ink/45">
                                {participant.role === 'owner'
                                  ? t('dm.owner')
                                  : participant.user.email ||
                                    participant.user.primary_org_unit_name}
                              </span>
                            </span>
                            {removable ? (
                              <button
                                aria-label={t('dm.removeMember')}
                                className="flex size-8 shrink-0 items-center justify-center rounded-md text-app-ink/45 hover:bg-app-bg hover:text-app-ink"
                                disabled={
                                  memberActionId === participant.user.id
                                }
                                onClick={() =>
                                  void handleRemoveMember(participant.user.id)
                                }
                                type="button"
                              >
                                {memberActionId === participant.user.id ? (
                                  <Loader2 size={14} className="animate-spin" />
                                ) : (
                                  <X size={14} />
                                )}
                              </button>
                            ) : null}
                          </div>
                        );
                      })}
                    </div>
                  </div>
                  <div className="min-w-0">
                    <div className="flex items-center gap-2 rounded-md border border-app-border bg-app-bg px-3 py-2">
                      <Search size={15} className="shrink-0 text-app-ink/45" />
                      <input
                        aria-label={t('dm.addMemberPlaceholder')}
                        className="app-text-body-sm w-full bg-transparent text-app-ink outline-none placeholder:text-app-ink/35"
                        disabled={!canInviteSelectedThread}
                        onChange={(event) => {
                          if (!selectedThread) return;
                          setMemberQueryOverride(
                            createDmThreadTextDraftOverride(
                              selectedThread.id,
                              event.target.value,
                            ),
                          );
                        }}
                        placeholder={t('dm.addMemberPlaceholder')}
                        value={memberQuery}
                      />
                      {memberSearching ? (
                        <Loader2
                          size={13}
                          className="animate-spin text-app-ink/45"
                        />
                      ) : null}
                    </div>
                    {memberQuery.trim() && canInviteSelectedThread ? (
                      <div className="mt-2 max-h-44 overflow-y-auto rounded-md border border-app-border bg-app-bg">
                        {memberResults.length ? (
                          memberResults.map((item) => (
                            <UserOptionRow
                              currentUserId={user?.id}
                              currentUserLabel={currentUserLabel}
                              disabled={memberActionId === item.id}
                              key={item.id}
                              onClick={() => void handleAddMember(item)}
                              trailing={
                                memberActionId === item.id ? (
                                  <Loader2
                                    size={13}
                                    className="animate-spin text-app-ink/45"
                                  />
                                ) : (
                                  <Plus size={14} className="text-app-ink/45" />
                                )
                              }
                              user={item}
                            />
                          ))
                        ) : (
                          <div className="app-text-caption px-3 py-4 text-center text-app-ink/45">
                            {memberSearching
                              ? t('dm.searching')
                              : t('dm.noSearchResults')}
                          </div>
                        )}
                      </div>
                    ) : null}
                    <button
                      className="app-text-caption mt-3 h-9 rounded-md border border-app-border px-3 text-app-ink/65 hover:bg-app-bg hover:text-app-ink disabled:cursor-not-allowed disabled:opacity-50"
                      disabled={memberActionId === leaveGroupActionId}
                      onClick={() => void handleLeaveGroup()}
                      type="button"
                    >
                      {t('dm.leaveGroup')}
                    </button>
                  </div>
                </div>
              </div>
            ) : null}
            {draggingFiles ? (
              <div className="pointer-events-none absolute inset-x-5 bottom-24 top-20 z-30 grid place-items-center rounded-lg border-2 border-dashed border-app-accent bg-app-bg/90 text-app-accent shadow-lg">
                <div className="grid place-items-center gap-2 rounded-lg border border-app-border bg-app-surface px-6 py-5 text-center shadow-xl">
                  <Paperclip size={22} />
                  <strong className="app-text-body-sm">
                    {t('dm.dropAttachmentsTitle')}
                  </strong>
                  <span className="app-text-caption text-app-ink/55">
                    {t('dm.dropAttachmentsHint')}
                  </span>
                </div>
              </div>
            ) : null}
            <StatusSlot
              announce="polite"
              className="mx-5 mt-4"
              message={loadError}
              size="compact"
              tone="error"
            />
            <div
              className="min-h-0 flex-1 overflow-y-auto px-5 py-4"
              onScroll={() => captureThreadScroll(threadId || null)}
              ref={messagesScrollRef}
            >
              {messagesLoading ? (
                <div className="flex justify-center py-10">
                  <Loader2 size={18} className="animate-spin text-app-ink/45" />
                </div>
              ) : messages.length ? (
                <div className="space-y-3">
                  {canLoadOlder ? (
                    <div className="flex justify-center">
                      <button
                        className="app-text-caption inline-flex h-8 items-center gap-1 rounded-md border border-app-border bg-app-surface px-3 text-app-ink/65 hover:bg-app-surface-hover hover:text-app-ink disabled:opacity-50"
                        disabled={olderState?.loading}
                        onClick={() => void handleLoadOlder()}
                        type="button"
                      >
                        {olderState?.loading ? (
                          <Loader2 size={13} className="animate-spin" />
                        ) : null}
                        {t('dm.loadOlder')}
                      </button>
                    </div>
                  ) : null}
                  {messages.map((message) => {
                    const own = message.sender_id === user?.id;
                    const dateDividerLabel = dateGroupStartIds.has(message.id)
                      ? formatDmDateDivider(
                          message.created_at,
                          i18n.language,
                          timeZone,
                        )
                      : null;
                    if (
                      selectedThread?.conversation_type === 'group' &&
                      !own &&
                      blockedUserIds.has(message.sender_id) &&
                      !expandedBlockedMessageIds.has(message.id)
                    ) {
                      return (
                        <div className="flex justify-start" key={message.id}>
                          <button
                            className="app-text-caption rounded-md border border-dashed border-app-border px-3 py-1.5 text-app-ink/55 hover:bg-app-surface-hover"
                            onClick={() =>
                              setExpandedBlockedMessageIds(
                                (current) => new Set([...current, message.id]),
                              )
                            }
                            type="button"
                          >
                            {t('dm.safety.blockedSenderCollapsed', {
                              name: message.sender_name,
                            })}
                          </button>
                        </div>
                      );
                    }
                    const content = buildDmMessageContent(message.body);
                    const previews = content.previews;
                    const attachments = message.attachments ?? [];
                    const replyTo = message.reply_to;
                    const replySourceLoaded = replyTo
                      ? loadedMessageIds.has(replyTo.id)
                      : false;
                    const unreadCountLabel = dmUnreadCountLabel(message);
                    const showReadMarker =
                      visibleReadMarkerMessageId === message.id;
                    const messageMeta = (
                      <div
                        className={`mb-0.5 flex shrink-0 flex-col gap-1 ${
                          own ? 'items-end' : 'items-start'
                        }`}
                      >
                        <div className="flex items-center gap-1 opacity-0 transition-opacity group-hover:opacity-100 focus-within:opacity-100">
                          <button
                            aria-label={t('dm.reply')}
                            className="inline-flex size-7 items-center justify-center rounded-md border border-app-border bg-app-bg text-app-ink/55 shadow-sm hover:bg-app-surface-hover hover:text-app-ink"
                            onClick={() => setReplyTarget(message)}
                            title={t('dm.reply')}
                            type="button"
                          >
                            <Reply size={13} />
                          </button>
                          {!message.hidden && !message.deleted ? (
                            <span className="relative">
                              <button
                                aria-expanded={reactionPickerFor === message.id}
                                aria-label={t('dm.reactions.add')}
                                className="inline-flex size-7 items-center justify-center rounded-md border border-app-border bg-app-bg text-app-ink/55 shadow-sm hover:bg-app-surface-hover hover:text-app-ink"
                                onClick={() =>
                                  setReactionPickerFor((current) =>
                                    current === message.id ? null : message.id,
                                  )
                                }
                                title={t('dm.reaction')}
                                type="button"
                              >
                                <SmilePlus size={13} />
                              </button>
                              {reactionPickerFor === message.id ? (
                                <span
                                  className={`absolute bottom-8 z-20 flex gap-0.5 rounded-md border border-app-border bg-app-surface p-1 shadow-lg ${
                                    own ? 'right-0' : 'left-0'
                                  }`}
                                  role="menu"
                                >
                                  {DM_REACTIONS.map(([key, emoji]) => {
                                    const reacted = Boolean(
                                      message.reactions?.find((item) => item.reaction === key)?.reacted,
                                    );
                                    return (
                                      <button
                                        aria-label={t(`dm.reactions.${key}`)}
                                        aria-pressed={reacted}
                                        className={`flex size-7 items-center justify-center rounded text-base hover:bg-app-surface-hover ${
                                          reacted ? 'bg-app-accent/15' : ''
                                        }`}
                                        key={key}
                                        onClick={() => void handleToggleReaction(message, key, !reacted)}
                                        role="menuitem"
                                        title={t(`dm.reactions.${key}`)}
                                        type="button"
                                      >
                                        {emoji}
                                      </button>
                                    );
                                  })}
                                </span>
                              ) : null}
                            </span>
                          ) : null}
                          {own && !message.hidden && !message.deleted ? (
                            <>
                              <button
                                aria-label={t('dm.edit')}
                                className="inline-flex size-7 items-center justify-center rounded-md border border-app-border bg-app-bg text-app-ink/55 shadow-sm hover:bg-app-surface-hover hover:text-app-ink"
                                onClick={() => startEdit(message)}
                                title={t('dm.edit')}
                                type="button"
                              >
                                <Pencil size={13} />
                              </button>
                              <button
                                aria-label={t('dm.delete')}
                                className="inline-flex size-7 items-center justify-center rounded-md border border-app-border bg-app-bg text-app-ink/55 shadow-sm hover:bg-app-surface-hover hover:text-app-ink"
                                onClick={() => setDeleteTarget(message)}
                                title={t('dm.delete')}
                                type="button"
                              >
                                <Trash2 size={13} />
                              </button>
                            </>
                          ) : null}
                          {safety?.report && !own && !message.hidden && !message.deleted ? (
                            <button
                              aria-label={t('dm.safety.report')}
                              className="inline-flex size-7 items-center justify-center rounded-md border border-app-border bg-app-bg text-app-ink/55 shadow-sm hover:bg-app-surface-hover hover:text-app-ink"
                              onClick={() => setReportTarget(message)}
                              title={t('dm.safety.report')}
                              type="button"
                            >
                              <Flag size={13} />
                            </button>
                          ) : null}
                        </div>
                        <div
                          className={`app-text-micro flex flex-col leading-tight ${
                            own ? 'items-end' : 'items-start'
                          }`}
                        >
                          {unreadCountLabel ? (
                            <span className="font-semibold text-app-accent">
                              {unreadCountLabel}
                            </span>
                          ) : null}
                          <span className="text-app-ink/40">
                            {message.edited_at && !message.deleted
                              ? `${t('dm.edited')} · `
                              : ''}
                            {formatDmMessageTime(
                              message.created_at,
                              i18n.language,
                              timeZone,
                            )}
                          </span>
                        </div>
                      </div>
                    );
                    return (
                      <Fragment key={message.id}>
                        {dateDividerLabel ? (
                          <div
                            aria-label={dateDividerLabel}
                            className="flex items-center gap-3 py-2"
                            role="separator"
                          >
                            <span
                              aria-hidden="true"
                              className="h-px flex-1 bg-app-border"
                            />
                            <span className="app-text-caption shrink-0 rounded-full bg-app-surface px-3 py-1 text-app-ink/55">
                              {dateDividerLabel}
                            </span>
                            <span
                              aria-hidden="true"
                              className="h-px flex-1 bg-app-border"
                            />
                          </div>
                        ) : null}
                        <div
                          ref={(element) => {
                            messageElementRefs.current[message.id] = element;
                          }}
                          className={`group flex w-full items-end gap-2 ${
                            own ? 'justify-end' : 'justify-start'
                          }`}
                        >
                          {own ? messageMeta : null}
                          <div
                            className={`max-w-[min(620px,75%)] rounded-lg px-3 py-2 transition-shadow ${
                              highlightedMessageId === message.id
                                ? 'ring-2 ring-app-accent/45'
                                : ''
                            } ${
                              own
                                ? 'bg-app-ink text-app-bg'
                                : 'border border-app-border bg-app-surface text-app-ink'
                            }`}
                          >
                            {selectedThread.conversation_type === 'group' &&
                            !own ? (
                              <div className="app-text-caption mb-1 font-medium text-app-ink/55">
                                {message.sender_name}
                              </div>
                            ) : null}
                            {replyTo ? (
                              <button
                                aria-label={t('dm.jumpToReplySource')}
                                className={`mb-2 block w-full rounded-md border px-2.5 py-2 text-left transition-colors ${
                                  own
                                    ? 'border-app-bg/20 bg-app-bg/10 hover:bg-app-bg/15'
                                    : 'border-app-border bg-app-bg hover:bg-app-surface-hover'
                                } disabled:cursor-default disabled:opacity-80`}
                                disabled={!replySourceLoaded}
                                onClick={() =>
                                  handleScrollToReplySource(replyTo.id)
                                }
                                type="button"
                              >
                                <span
                                  className={`app-text-micro block truncate font-medium ${
                                    own ? 'text-app-bg/70' : 'text-app-ink/50'
                                  }`}
                                >
                                  {t('dm.replyingTo', {
                                    name: replyTo.sender_name,
                                  })}
                                </span>
                                <span
                                  className={`app-text-caption mt-0.5 line-clamp-2 block whitespace-pre-wrap break-words ${
                                    own ? 'text-app-bg/85' : 'text-app-ink/70'
                                  }`}
                                >
                                  {dmReplyPreviewText(replyTo, t)}
                                </span>
                              </button>
                            ) : null}
                            {message.hidden ? (
                              <div className="app-text-body italic opacity-60">
                                {message.body || t('dm.safety.hiddenMessage')}
                              </div>
                            ) : message.deleted ? (
                              <div className="app-text-body italic opacity-60">
                                {t('dm.deletedMessage')}
                              </div>
                            ) : message.body ? (
                              <div className="app-text-body whitespace-pre-wrap break-words">
                                {message.mentions?.length
                                  ? dmMentionParts(message.body, message.mentions).map(
                                      (part, index) =>
                                        part.kind === 'mention' ? (
                                          <span
                                            className={`rounded px-0.5 font-semibold ${
                                              part.userId === user?.id
                                                ? 'bg-app-accent text-app-accent-fg'
                                                : own
                                                  ? 'bg-app-bg/20'
                                                  : 'bg-app-accent/10 text-app-accent'
                                            }`}
                                            data-mention={part.userId}
                                            key={`mention-${index}`}
                                          >
                                            {part.text}
                                          </span>
                                        ) : (
                                          <Fragment key={`part-${index}`}>
                                            {messageBodyContent(
                                              buildDmMessageContent(part.text).segments,
                                              own,
                                            )}
                                          </Fragment>
                                        ),
                                    )
                                  : messageBodyContent(content.segments, own)}
                              </div>
                            ) : null}
                            {attachments.length ? (
                              <div
                                className={
                                  message.body ? 'mt-2 space-y-2' : 'space-y-2'
                                }
                              >
                                {attachments.map((attachment) => {
                                  const loading =
                                    attachmentActionId === attachment.id;
                                  const previewUrl =
                                    imagePreviewUrlForAttachment(attachment);
                                  return (
                                    <div
                                      className={`overflow-hidden rounded-md border ${
                                        own
                                          ? 'border-app-bg/20 bg-app-bg/10 text-app-bg'
                                          : 'border-app-border bg-app-bg text-app-ink'
                                      }`}
                                      key={attachment.id}
                                    >
                                      {previewUrl ? (
                                        <button
                                          aria-label={t(
                                            'dm.openImageAttachment',
                                            { filename: attachment.filename },
                                          )}
                                          className="block w-full bg-black/5 text-left"
                                          disabled={loading}
                                          onClick={() =>
                                            void handleOpenImageAttachment(
                                              attachment,
                                            )
                                          }
                                          type="button"
                                        >
                                          <DmAttachmentImage
                                            alt={attachment.filename}
                                            className="max-h-72 w-full object-contain"
                                            path={previewUrl}
                                          />
                                        </button>
                                      ) : null}
                                      <div className="flex min-w-0 items-center gap-2 px-3 py-2">
                                        <span
                                          className={`flex size-8 shrink-0 items-center justify-center rounded-md ${
                                            own
                                              ? 'bg-app-bg/15 text-app-bg'
                                              : 'bg-app-accent/10 text-app-accent'
                                          }`}
                                        >
                                          {attachment.is_image ? (
                                            <ImageIcon size={15} />
                                          ) : (
                                            <FileText size={15} />
                                          )}
                                        </span>
                                        <span className="min-w-0 flex-1">
                                          <span className="app-text-body-sm block truncate font-medium">
                                            {attachment.filename}
                                          </span>
                                          <span
                                            className={`app-text-micro block ${
                                              own
                                                ? 'text-app-bg/55'
                                                : 'text-app-ink/45'
                                            }`}
                                          >
                                            {formatAttachmentSize(
                                              attachment.size_bytes,
                                              i18n.language,
                                            )}
                                          </span>
                                        </span>
                                        <button
                                          aria-label={t(
                                            'dm.downloadAttachment',
                                            {
                                              filename: attachment.filename,
                                            },
                                          )}
                                          className={`flex size-8 shrink-0 items-center justify-center rounded-md ${
                                            own
                                              ? 'text-app-bg/70 hover:bg-app-bg/15 hover:text-app-bg'
                                              : 'text-app-ink/55 hover:bg-app-surface-hover hover:text-app-ink'
                                          } disabled:cursor-not-allowed disabled:opacity-50`}
                                          disabled={loading}
                                          onClick={() =>
                                            void handleDownloadAttachment(
                                              attachment,
                                            )
                                          }
                                          type="button"
                                        >
                                          {loading ? (
                                            <Loader2
                                              size={14}
                                              className="animate-spin"
                                            />
                                          ) : (
                                            <Download size={14} />
                                          )}
                                        </button>
                                      </div>
                                    </div>
                                  );
                                })}
                              </div>
                            ) : null}
                            {previews.length ? (
                              <div className="mt-2 space-y-2">
                                {previews.map((preview) => (
                                  <a
                                    aria-label={t('dm.openLinkPreview', {
                                      host: preview.host,
                                    })}
                                    className={`block rounded-md border p-3 transition-colors ${
                                      own
                                        ? 'border-app-bg/20 bg-app-bg/10 text-app-bg hover:bg-app-bg/15'
                                        : 'border-app-border bg-app-bg text-app-ink hover:bg-app-surface-hover'
                                    }`}
                                    href={preview.href}
                                    key={preview.key}
                                    rel="noreferrer"
                                    target="_blank"
                                  >
                                    <span className="flex items-start gap-3">
                                      <span
                                        className={`mt-0.5 flex size-8 shrink-0 items-center justify-center rounded-md ${
                                          own
                                            ? 'bg-app-bg/15 text-app-bg'
                                            : 'bg-app-accent/10 text-app-accent'
                                        }`}
                                      >
                                        <ExternalLink size={15} />
                                      </span>
                                      <span className="min-w-0 flex-1">
                                        <span className="app-text-body-sm block truncate font-medium">
                                          {preview.host}
                                        </span>
                                        <span
                                          className={`app-text-caption block truncate ${
                                            own
                                              ? 'text-app-bg/65'
                                              : 'text-app-ink/50'
                                          }`}
                                        >
                                          {preview.path}
                                        </span>
                                        <span
                                          className={`app-text-micro mt-1 block ${
                                            own
                                              ? 'text-app-bg/50'
                                              : 'text-app-ink/40'
                                          }`}
                                        >
                                          {t('dm.openLink')}
                                        </span>
                                      </span>
                                    </span>
                                  </a>
                                ))}
                              </div>
                            ) : null}
                            {message.reactions?.length && !message.hidden && !message.deleted ? (
                              <div className="mt-1.5 flex flex-wrap gap-1">
                                {message.reactions.map((reaction) => (
                                  <button
                                    aria-label={`${t(`dm.reactions.${reaction.reaction}`, { defaultValue: reaction.reaction })} ${reaction.count}`}
                                    aria-pressed={reaction.reacted}
                                    className={`app-text-caption inline-flex items-center gap-1 rounded-full border px-1.5 py-0.5 ${
                                      reaction.reacted
                                        ? 'border-app-accent bg-app-accent/15 text-app-ink'
                                        : 'border-app-border bg-app-bg text-app-ink/70'
                                    } hover:bg-app-surface-hover`}
                                    key={reaction.reaction}
                                    onClick={() =>
                                      void handleToggleReaction(message, reaction.reaction, !reaction.reacted)
                                    }
                                    title={reaction.users.map((person) => displayDmUserName(person)).join(', ')}
                                    type="button"
                                  >
                                    <span aria-hidden="true">
                                      {DM_REACTION_EMOJI.get(reaction.reaction) ?? reaction.reaction}
                                    </span>
                                    {reaction.count}
                                  </button>
                                ))}
                              </div>
                            ) : null}
                          </div>
                          {own ? null : messageMeta}
                        </div>
                        {showReadMarker ? (
                          <div
                            aria-label={t('dm.lastReadMarker')}
                            className="flex items-center gap-3 py-1"
                          >
                            <span className="h-px flex-1 bg-app-border" />
                            <span className="app-text-micro rounded-full border border-app-border bg-app-surface px-2 py-0.5 font-medium text-app-ink/50">
                              {t('dm.lastReadMarker')}
                            </span>
                            <span className="h-px flex-1 bg-app-border" />
                          </div>
                        ) : null}
                      </Fragment>
                    );
                  })}
                  <div ref={bottomRef} />
                </div>
              ) : (
                <div className="flex h-full items-center justify-center text-app-ink/45">
                  <div className="text-center">
                    <MessageCircle size={28} className="mx-auto mb-2" />
                    <div className="app-text-body">{t('dm.noMessages')}</div>
                  </div>
                </div>
              )}
            </div>
            <footer className="shrink-0 border-t border-app-border p-4">
              {typingLabel ? (
                <div
                  aria-live="polite"
                  className="app-text-caption -mt-1 mb-2 flex items-center gap-1.5 text-app-ink/55"
                >
                  <span aria-hidden="true" className="inline-flex gap-0.5">
                    <span className="size-1 animate-pulse rounded-full bg-app-ink/45" />
                    <span className="size-1 animate-pulse rounded-full bg-app-ink/45 [animation-delay:150ms]" />
                    <span className="size-1 animate-pulse rounded-full bg-app-ink/45 [animation-delay:300ms]" />
                  </span>
                  {typingLabel}
                </div>
              ) : null}
              {termsPending && safety?.terms ? (
                <DmTermsNotice
                  accepting={acceptingTerms}
                  className="mb-3"
                  onAccept={() => void handleAcceptTerms()}
                  terms={safety.terms}
                />
              ) : null}
              {safety?.block &&
              selectedThread.conversation_type === 'direct' &&
              selectedThread.peer_blocked ? (
                <div className="app-text-body-sm mb-3 rounded-md border border-app-border bg-app-surface px-3 py-2 text-app-ink/70">
                  {t('dm.safety.composerBlocked')}
                </div>
              ) : null}
              {editTarget ? (
                <div className="mb-3 flex items-start gap-2 rounded-md border border-app-accent/40 bg-app-surface px-3 py-2">
                  <Pencil size={15} className="mt-0.5 shrink-0 text-app-accent" />
                  <div className="app-text-caption min-w-0 flex-1 truncate font-medium text-app-ink">
                    {t('dm.editing')}
                  </div>
                  <button
                    aria-label={t('dm.cancelEdit')}
                    className="flex size-7 shrink-0 items-center justify-center rounded-md text-app-ink/45 hover:bg-app-bg hover:text-app-ink"
                    onClick={cancelEdit}
                    type="button"
                  >
                    <X size={13} />
                  </button>
                </div>
              ) : null}
              {failedDrafts.length > 0 ? (
                <button
                  className="app-text-body-sm mb-3 rounded-md border border-app-border px-3 py-1.5 text-app-ink disabled:opacity-50"
                  disabled={Boolean(draft || editTarget) || sending}
                  onClick={() => {
                    setSelectedThreadDraft(failedDrafts[0]);
                    setFailedDraftsByThread((current) => ({
                      ...current,
                      [threadId]: failedDrafts.slice(1),
                    }));
                  }}
                  type="button"
                >
                  {t('dm.restoreFailedDraft')}
                </button>
              ) : null}
              {replyTarget ? (
                <div className="mb-3 flex items-start gap-2 rounded-md border border-app-border bg-app-surface px-3 py-2">
                  <Reply
                    size={15}
                    className="mt-0.5 shrink-0 text-app-accent"
                  />
                  <div className="min-w-0 flex-1">
                    <div className="app-text-caption truncate font-medium text-app-ink">
                      {t('dm.replyingTo', { name: replyTarget.sender_name })}
                    </div>
                    <div className="app-text-caption line-clamp-2 whitespace-pre-wrap break-words text-app-ink/60">
                      {replyTarget.body.trim() ||
                        (replyTarget.attachments?.length
                          ? t('dm.replyAttachmentOnly', {
                              count: replyTarget.attachments.length,
                            })
                          : t('dm.replyEmptyPreview'))}
                    </div>
                  </div>
                  <button
                    aria-label={t('dm.cancelReply')}
                    className="flex size-7 shrink-0 items-center justify-center rounded-md text-app-ink/45 hover:bg-app-bg hover:text-app-ink"
                    onClick={() => setReplyTarget(null)}
                    type="button"
                  >
                    <X size={13} />
                  </button>
                </div>
              ) : null}
              {pendingAttachments.length ? (
                <div className="mb-3 flex flex-wrap gap-2">
                  {pendingAttachments.map((item) => {
                    const failed = item.status === 'failed';
                    const uploading = item.status === 'uploading';
                    return (
                      <div
                        className={`flex max-w-full items-center gap-2 rounded-md border px-2 py-1.5 ${
                          failed
                            ? 'border-app-danger-border bg-app-danger-bg text-app-danger-text'
                            : 'border-app-border bg-app-surface text-app-ink'
                        }`}
                        key={item.localId}
                      >
                        <span className="flex size-7 shrink-0 items-center justify-center rounded-md bg-app-bg text-app-ink/60">
                          {uploading ? (
                            <Loader2 size={14} className="animate-spin" />
                          ) : isImageFile(item.file) ? (
                            <ImageIcon size={14} />
                          ) : (
                            <FileText size={14} />
                          )}
                        </span>
                        <span className="min-w-0">
                          <span className="app-text-caption block max-w-[220px] truncate font-medium">
                            {item.attachment?.filename ||
                              item.file.name ||
                              t('dm.unnamedAttachment')}
                          </span>
                          <span className="app-text-micro block text-app-ink/45">
                            {failed
                              ? item.error
                              : uploading
                                ? t('dm.uploadingAttachment')
                                : formatAttachmentSize(
                                    item.attachment?.size_bytes ??
                                      item.file.size,
                                    i18n.language,
                                  )}
                          </span>
                        </span>
                        <button
                          aria-label={t('dm.removeAttachment')}
                          className="flex size-7 shrink-0 items-center justify-center rounded-md text-app-ink/45 hover:bg-app-bg hover:text-app-ink"
                          disabled={sending}
                          onClick={() =>
                            handleRemovePendingAttachment(item.localId)
                          }
                          type="button"
                        >
                          <X size={13} />
                        </button>
                      </div>
                    );
                  })}
                </div>
              ) : null}
              {mentionPicker && mentionCandidates.length ? (
                <div
                  aria-label={t('dm.mentionPicker')}
                  className="mb-2 max-h-48 overflow-y-auto rounded-md border border-app-border bg-app-surface py-1 shadow-sm"
                  role="listbox"
                >
                  {mentionCandidates.map((candidate, index) => (
                    <button
                      aria-selected={index === mentionPicker.index}
                      className={`app-text-body-sm block w-full px-3 py-1.5 text-left ${
                        index === mentionPicker.index
                          ? 'bg-app-accent/10 text-app-ink'
                          : 'text-app-ink/80 hover:bg-app-surface-hover'
                      }`}
                      key={candidate.id}
                      onMouseDown={(event) => {
                        event.preventDefault();
                        pickMention(candidate);
                      }}
                      role="option"
                      type="button"
                    >
                      @{candidate.name}
                    </button>
                  ))}
                </div>
              ) : null}
              <div className="flex items-end gap-2">
                <input
                  aria-label={t('dm.attachFile')}
                  className="hidden"
                  multiple
                  onChange={handleFileInputChange}
                  ref={fileInputRef}
                  type="file"
                />
                <button
                  aria-label={t('dm.attachFile')}
                  className="flex size-11 shrink-0 items-center justify-center rounded-md border border-app-border bg-app-surface text-app-ink transition-colors hover:bg-app-surface-hover disabled:cursor-not-allowed disabled:opacity-50"
                  disabled={sending || Boolean(editTarget)}
                  onClick={() => fileInputRef.current?.click()}
                  type="button"
                >
                  <Paperclip size={17} />
                </button>
                <textarea
                  aria-label={t('dm.messagePlaceholder')}
                  className={`${inputClassName} min-h-11 resize-none`}
                  onChange={(event) => {
                    const body = event.currentTarget.value;
                    if (editing) {
                      setEditing({ ...editing, body });
                    } else {
                      setSelectedThreadDraft(body);
                      notifyTyping(body);
                    }
                    const caret = event.currentTarget.selectionStart ?? body.length;
                    const active = activeDmMentionQuery(body, caret);
                    setMentionPicker(active ? { active, caret, index: 0 } : null);
                  }}
                  onBlur={() => window.setTimeout(() => setMentionPicker(null), 150)}
                  onKeyDown={(event) => {
                    const composing =
                      event.nativeEvent.isComposing || event.nativeEvent.keyCode === 229;
                    if (mentionPicker && mentionCandidates.length && !composing) {
                      if (event.key === 'ArrowDown' || event.key === 'ArrowUp') {
                        event.preventDefault();
                        const step = event.key === 'ArrowDown' ? 1 : -1;
                        setMentionPicker({
                          ...mentionPicker,
                          index:
                            (mentionPicker.index + step + mentionCandidates.length) %
                            mentionCandidates.length,
                        });
                        return;
                      }
                      if (event.key === 'Enter' || event.key === 'Tab') {
                        event.preventDefault();
                        pickMention(mentionCandidates[mentionPicker.index] ?? mentionCandidates[0]);
                        return;
                      }
                      if (event.key === 'Escape') {
                        event.preventDefault();
                        setMentionPicker(null);
                        return;
                      }
                    }
                    if (
                      shouldSubmitDmComposerKey({
                        key: event.key,
                        shiftKey: event.shiftKey,
                        isComposing: event.nativeEvent.isComposing || event.nativeEvent.keyCode === 229,
                      })
                    ) {
                      event.preventDefault();
                      void handleSend();
                    }
                  }}
                  onPaste={handleComposerPaste}
                  placeholder={t('dm.messagePlaceholder')}
                  rows={1}
                  readOnly={Boolean(editing) && sending}
                  ref={composerRef}
                  value={editing?.body ?? draft}
                />
                <button
                  aria-label={editTarget ? t('dm.saveEdit') : t('dm.send')}
                  className="flex size-11 shrink-0 items-center justify-center rounded-md bg-app-ink text-app-bg transition-colors hover:bg-app-ink/90 disabled:cursor-not-allowed disabled:opacity-50"
                  disabled={
                    termsPending ||
                    Boolean(
                      selectedThread?.conversation_type === 'direct' &&
                        selectedThread.peer_blocked,
                    ) ||
                    (editTarget
                      ? sending ||
                        (!editing?.body.trim() && !(editTarget.attachments ?? []).length)
                      : isDmSendActionDisabled({
                          draft,
                          readyAttachmentIds,
                          sending,
                          uploadingAttachmentCount,
                        }))
                  }
                  onClick={() => void handleSend()}
                  type="button"
                >
                  {sending ? (
                    <Loader2 size={17} className="animate-spin" />
                  ) : (
                    <Send size={17} />
                  )}
                </button>
              </div>
            </footer>
          </>
        ) : (
          <div className="flex h-full items-center justify-center text-app-ink/45">
            <div className="text-center">
              <MessageCircle size={32} className="mx-auto mb-2" />
              <div className="app-text-body">{t('dm.selectThread')}</div>
            </div>
          </div>
        )}
      </section>
      <DmMessageSearchDialog
        conversation={
          selectedThread ? { id: selectedThread.id, name: selectedThreadName } : null
        }
        onClose={() => setSearchOpen(false)}
        onSelect={handleSelectSearchHit}
        open={searchOpen}
        timeZone={timeZone}
        token={token}
      />
      <DmReportDialog
        message={reportTarget}
        onClose={() => setReportTarget(null)}
        onSubmit={handleSubmitReport}
        safety={safety}
      />
      <DmBlockedUsersDialog
        loadBlocks={loadBlockedUsers}
        onClose={() => setBlockedUsersOpen(false)}
        onUnblock={handleUnblockUser}
        open={blockedUsersOpen}
      />
      {selectedThread ? (
        <DmBlockDialog
          blocked={Boolean(selectedThread.peer_blocked)}
          name={selectedThreadName}
          onClose={() => setBlockDialogOpen(false)}
          onConfirm={handleToggleBlock}
          open={blockDialogOpen}
        />
      ) : null}
      <Dialog
        closeLabel={t('dm.closeImageViewer')}
        maxWidth="max-w-5xl"
        onOpenChange={(open) => {
          if (!open) {
            // miy: the viewer shows a local object URL; release it.
            if (imageViewer?.url.startsWith('blob:')) {
              URL.revokeObjectURL(imageViewer.url);
            }
            setImageViewer(null);
          }
        }}
        open={Boolean(visibleImageViewer)}
        title={visibleImageViewer?.filename ?? t('dm.imageViewerTitle')}
      >
        {visibleImageViewer ? (
          <div className="flex min-h-0 justify-center overflow-hidden bg-slate-950">
            <img
              alt={visibleImageViewer.filename}
              className="max-h-[70vh] max-w-full object-contain"
              src={visibleImageViewer.url}
            />
          </div>
        ) : null}
      </Dialog>
      <Dialog
        actions={
          <div className="flex justify-end gap-2">
            <button
              className="app-text-body-sm h-9 rounded-md border border-app-border px-3 text-app-ink"
              onClick={() => setDeleteTarget(null)}
              type="button"
            >
              {t('dm.safety.cancel')}
            </button>
            <button
              className="app-text-body-sm inline-flex h-9 items-center gap-1 rounded-md bg-app-danger px-3 font-medium text-white disabled:opacity-50"
              disabled={deleting}
              onClick={() => void handleConfirmDelete()}
              type="button"
            >
              {deleting ? <Loader2 size={13} className="animate-spin" /> : null}
              {t('dm.delete')}
            </button>
          </div>
        }
        closeLabel={t('dm.safety.close')}
        onOpenChange={(open) => {
          if (!open && !deleting) setDeleteTarget(null);
        }}
        open={Boolean(deleteTarget)}
        title={t('dm.deleteTitle')}
      >
        <p className="app-text-body-sm text-app-ink/70">{t('dm.deleteScope')}</p>
      </Dialog>
    </div>
  );
}

type DmImageViewerSource = {
  attachmentId: string;
  messageId: string | null;
};

type DmImageViewerWithSource = DmImageViewerState & {
  source: DmImageViewerSource | null;
};

/** 원본 메시지가 가려졌거나(관리자 가림) 그 첨부가 더는 메시지에 없으면 확대 뷰어에 남기지 않는다. */
/**
 * 더 먼저 시작해 늦게 온 이용규칙 답은 화면 값을 되돌리지 못한다 — 다만 같은 판의 동의 확인은 받아들인다.
 * 동의는 되돌아가지 않으므로, 서버가 확인한 동의가 더 나중에 시작한 미동의 답에 가려지지 않게 한다. 다른 판의 답은 무시한다.
 */
function keepConfirmedDmTermsAcceptance(
  current: NonNullable<DmSafetyCapability['terms']>,
  late: DmSafetyCapability['terms'] | undefined,
): NonNullable<DmSafetyCapability['terms']> {
  if (late?.accepted && !current.accepted && late.version === current.version) {
    return { ...current, accepted: true, accepted_at: late.accepted_at };
  }
  return current;
}

function isDmImageViewerSourceHidden(
  message: Pick<DmMessage, 'attachments' | 'hidden'>,
  source: DmImageViewerSource | null,
): boolean {
  if (message.hidden) return true;
  return Boolean(
    source &&
      !(message.attachments ?? []).some((item) => item.id === source.attachmentId),
  );
}

function isDmImageViewerSourceHiddenIn(
  messages: readonly DmMessage[],
  source: DmImageViewerSource | null,
): boolean {
  if (!source?.messageId) return false;
  const message = messages.find((item) => item.id === source.messageId);
  return Boolean(message && isDmImageViewerSourceHidden(message, source));
}

/** 이용규칙 동의 안내 — 작성창과 단체 대화 만들기 화면이 같은 문구·동작을 쓴다(수락은 서버가 판정한다). */
function DmTermsNotice({
  accepting,
  className,
  onAccept,
  terms,
}: {
  accepting: boolean;
  className?: string;
  onAccept: () => void;
  terms: NonNullable<DmSafetyCapability['terms']>;
}) {
  const { t } = useTranslation('apps');
  return (
    <div
      className={`app-text-body-sm grid gap-2 rounded-md border border-app-border bg-app-surface px-3 py-2 text-app-ink/80 ${className ?? ''}`}
    >
      <div>{t('dm.safety.termsRequired')}</div>
      <div className="flex flex-wrap items-center gap-3">
        {terms.document_url ? (
          <a
            className="underline"
            href={terms.document_url}
            rel="noreferrer"
            target="_blank"
          >
            {t('dm.safety.termsDocument')}
          </a>
        ) : null}
        {terms.contact?.support_url ? (
          <a
            className="underline"
            href={terms.contact.support_url}
            rel="noreferrer"
            target="_blank"
          >
            {t('dm.safety.termsContact')}
          </a>
        ) : null}
        <button
          className="inline-flex items-center gap-1 rounded-md bg-app-ink px-3 py-1.5 text-app-bg disabled:cursor-not-allowed disabled:opacity-50"
          disabled={accepting}
          onClick={onAccept}
          type="button"
        >
          {accepting ? <Loader2 size={13} className="animate-spin" /> : null}
          {t('dm.safety.termsAccept')}
        </button>
      </div>
    </div>
  );
}

function dmErrorCode(error: unknown): string | null {
  return apiErrorCode(error);
}
