import { useCallback, useMemo, useRef, useState, type SetStateAction } from 'react';
import type { TFunction } from 'i18next';

import {
  createTaskComment,
  type PmsComment,
  type PmsTaskListMember,
} from '../api/pms-api';
import {
  userOptionMatchesQuery,
  userOptionNameWithDepartment,
} from '../../../platform/users/user-option-picker-model';
import {
  getTaskDetailMutationErrorMessage,
  notifyTaskDetailUpdated,
} from './task-detail-mutation';
import { type AppApi } from '../../../platform/runtime/runtime-context';

export type TaskCommentMentionToken = {
  displayName: string;
  userId: string;
};

export function getTaskDetailMentionState(value: string): {
  mentionOpen: boolean;
  mentionQuery: string;
} {
  const atIndex = value.lastIndexOf('@');
  if (atIndex < 0) return { mentionOpen: false, mentionQuery: '' };
  const query = value.slice(atIndex + 1);
  if (query.length === 0) return { mentionOpen: true, mentionQuery: '' };
  if (query.includes(' ')) return { mentionOpen: false, mentionQuery: '' };
  return { mentionOpen: true, mentionQuery: query.toLowerCase() };
}

export function getTaskDetailMentionCandidates<
  TMember extends Pick<PmsTaskListMember, 'email' | 'full_name'> & {
    primary_org_unit_name?: string | null;
    user_id?: string;
  },
>(members: TMember[], mentionQuery: string): TMember[] {
  if (!mentionQuery) return members;
  return members.filter((member) =>
    userOptionMatchesQuery(
      {
        id: member.user_id ?? member.email,
        email: member.email,
        full_name: member.full_name,
        primary_org_unit_name: member.primary_org_unit_name,
      },
      mentionQuery,
    ),
  );
}

export function applyTaskDetailMentionPick(
  commentDraft: string,
  displayName: string,
): string {
  const atIndex = commentDraft.lastIndexOf('@');
  if (atIndex < 0) return `${commentDraft}@${displayName} `;
  return `${commentDraft.slice(0, atIndex)}@${displayName} `;
}

function pushTextContent(
  content: Record<string, unknown>[],
  text: string,
): void {
  if (!text) return;
  content.push({ type: 'text', text, styles: {} });
}

export function buildTaskCommentBodyBlocks(
  body: string,
  mentionTokens: readonly TaskCommentMentionToken[],
): Record<string, unknown>[] | null {
  if (!mentionTokens.length) return null;
  const content: Record<string, unknown>[] = [];
  let cursor = 0;

  for (const mentionToken of mentionTokens) {
    const marker = `@${mentionToken.displayName}`;
    const index = body.indexOf(marker, cursor);
    if (index < 0) continue;
    pushTextContent(content, body.slice(cursor, index));
    content.push({
      type: 'mention',
      props: {
        userId: mentionToken.userId,
        displayName: mentionToken.displayName,
      },
    });
    cursor = index + marker.length;
  }

  if (!content.some((item) => item.type === 'mention')) {
    return null;
  }
  pushTextContent(content, body.slice(cursor));
  return [
    {
      type: 'paragraph',
      content,
    },
  ];
}

export function useTaskDetailComments({
  workspaceSlug,
  canEdit,
  members,
  onUpdate,
  setComments,
  setSaveError,
  taskId,
  api,
  t,
}: {
  workspaceSlug?: string | null;
  canEdit: boolean;
  members: PmsTaskListMember[];
  onUpdate?: () => void | Promise<void>;
  setComments: (value: SetStateAction<PmsComment[]>) => void;
  setSaveError: (message: string | null) => void;
  taskId: string;
  api: AppApi | null;
  t: TFunction;
}) {
  const [commentDraft, setCommentDraft] = useState('');
  const [commentSubmitting, setCommentSubmitting] = useState(false);
  const commentInFlight = useRef(false);
  const draftRevision = useRef(0);
  const [mentionOpen, setMentionOpen] = useState(false);
  const [mentionQuery, setMentionQuery] = useState('');
  const [mentionTokens, setMentionTokens] = useState<TaskCommentMentionToken[]>(
    [],
  );
  const mentionCandidates = useMemo(
    () => getTaskDetailMentionCandidates(members, mentionQuery),
    [members, mentionQuery],
  );

  const handleCommentDraftChange = useCallback(
    (value: string) => {
      if (!canEdit) return;
      draftRevision.current += 1;
      setCommentDraft(value);
      const nextMentionState = getTaskDetailMentionState(value);
      setMentionOpen(nextMentionState.mentionOpen);
      setMentionQuery(nextMentionState.mentionQuery);
    },
    [canEdit],
  );

  const closeMention = useCallback(() => {
    setMentionOpen(false);
  }, []);

  const handleMentionPick = useCallback((member: PmsTaskListMember) => {
    draftRevision.current += 1;
    const displayName = userOptionNameWithDepartment({
      id: member.user_id,
      email: member.email,
      full_name: member.full_name,
      primary_org_unit_name: member.primary_org_unit_name,
    });
    setCommentDraft((current) =>
      applyTaskDetailMentionPick(current, displayName),
    );
    setMentionTokens((current) => [
      ...current.filter((mentionToken) => mentionToken.userId !== member.user_id),
      { userId: member.user_id, displayName },
    ]);
    setMentionOpen(false);
  }, []);

  const handleCommentSubmit = useCallback(async () => {
    if (!api || !canEdit || !commentDraft.trim() || commentInFlight.current) return;
    commentInFlight.current = true;
    setCommentSubmitting(true);
    setSaveError(null);
    const submittedRevision = draftRevision.current;
    const body = commentDraft.trim();
    const bodyBlocks = buildTaskCommentBodyBlocks(body, mentionTokens);
    try {
      const newComment = await createTaskComment(api, taskId, body, bodyBlocks, workspaceSlug);
      setComments((prev) => [...prev, newComment]);
      if (draftRevision.current === submittedRevision) {
        setCommentDraft('');
        setMentionTokens([]);
        setMentionOpen(false);
        setMentionQuery('');
      }
      await notifyTaskDetailUpdated(onUpdate);
    } catch (error) {
      setSaveError(
        getTaskDetailMutationErrorMessage(error, t('pms.taskDetail.errors.createCommentFailed')),
      );
    } finally {
      commentInFlight.current = false;
      setCommentSubmitting(false);
    }
  }, [
    canEdit,
    commentDraft,
    mentionTokens,
    onUpdate,
    setComments,
    setSaveError,
    t,
    taskId,
    api,
    workspaceSlug,
  ]);

  return {
    closeMention,
    commentDraft,
    commentSubmitting,
    handleCommentDraftChange,
    handleCommentSubmit,
    handleMentionPick,
    mentionCandidates,
    mentionOpen,
  };
}
