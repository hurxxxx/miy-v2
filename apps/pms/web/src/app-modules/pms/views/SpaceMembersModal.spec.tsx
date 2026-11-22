import { fireEvent, screen, waitFor } from '@testing-library/react';
import type { ButtonHTMLAttributes, ReactElement, ReactNode } from 'react';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

import { renderWithRuntime, TEST_USER } from '../../../test-utils/runtime';

import {
  addSpaceMembers,
  listPmsUsers,
  listSpaceMembers,
  removeSpaceMember,
} from '../api/pms-api';
import { SpaceMembersModal } from './SpaceMembersModal';

// miy: frame runtime and `/v1/pms/me` user instead of AI-DO's useAuth().
const render = (ui: ReactElement) =>
  renderWithRuntime(ui, { user: { ...TEST_USER, id: 'owner' } });

vi.mock('@miy/ui', async () => {
  const React = await import('react');
  type ConfirmRequest = {
    cancelLabel: string;
    confirmLabel: string;
    description: string;
    resolve: (value: boolean) => void;
    title: string;
  };

  return {
    Button: ({
      children,
      ...props
    }: ButtonHTMLAttributes<HTMLButtonElement>) => (
      <button {...props}>{children}</button>
    ),
    Dialog: ({
      actions,
      children,
      title,
    }: {
      actions?: ReactNode;
      children: ReactNode;
      title: ReactNode;
    }) => (
      <section role="dialog">
        <h2>{title}</h2>
        {children}
        {actions ? <footer>{actions}</footer> : null}
      </section>
    ),
    Select: ({
      disabled,
      onValueChange,
      options,
      placeholder,
      value,
    }: {
      disabled?: boolean;
      onValueChange: (value: string) => void;
      options: Array<{ label: string; value: string }>;
      placeholder?: string;
      value: string;
    }) => (
      <select
        aria-label={placeholder}
        disabled={disabled}
        onChange={(event) => onValueChange(event.target.value)}
        value={value}
      >
        <option value="">{placeholder}</option>
        {options.map((option) => (
          <option key={option.value} value={option.value}>
            {option.label}
          </option>
        ))}
      </select>
    ),
    useConfirm: () => {
      const [request, setRequest] = React.useState<ConfirmRequest | null>(null);
      const confirm = (options: Omit<ConfirmRequest, 'resolve'>) =>
        new Promise<boolean>((resolve) => {
          setRequest({ ...options, resolve });
        });
      const close = (value: boolean) => {
        const current = request;
        setRequest(null);
        current?.resolve(value);
      };
      const confirmDialog = request ? (
        <section role="dialog">
          <h2>{request.title}</h2>
          <p>{request.description}</p>
          <button onClick={() => close(false)} type="button">
            {request.cancelLabel}
          </button>
          <button onClick={() => close(true)} type="button">
            {request.confirmLabel}
          </button>
        </section>
      ) : null;

      return { confirm, confirmDialog };
    },
  };
});

vi.mock('../api/pms-api', () => ({
  addSpaceMember: vi.fn(),
  addSpaceMembers: vi.fn(),
  listPmsUsers: vi.fn(),
  listSpaceMembers: vi.fn(),
  removeSpaceMember: vi.fn(),
  updateSpaceMemberRole: vi.fn(),
}));

const { translate } = vi.hoisted(() => {
  const translations: Record<string, string> = {
    'common:actions.cancel': '취소',
    'common:actions.close': '닫기',
    'pms.noMatchingUsers': '일치하는 사용자가 없습니다',
    'pms.noUsersToAdd': '추가할 사용자가 없습니다',
    'pms.searchUser': '사용자 검색',
    'pms.settings.role.admin': '관리자',
    'pms.settings.role.member': '멤버',
    'pms.settings.role.owner': '소유자',
    'pms.settings.role.viewer': '뷰어',
    'pms.spaceMembers.changeRole': '역할 변경',
    'pms.spaceMembers.department.add': '{{count}}명 추가',
    'pms.spaceMembers.department.option': '{{name}} ({{count}}명)',
    'pms.spaceMembers.department.select': '부서 선택',
    'pms.spaceMembers.department.title': '부서로 추가',
    'pms.spaceMembers.leave.action': '스페이스 나가기',
    'pms.spaceMembers.leave.confirmAction': '나가기',
    'pms.spaceMembers.leave.confirmTitle': '스페이스에서 나갈까요?',
    'pms.spaceMembers.leave.lastOwner': '마지막 소유자는 나갈 수 없습니다.',
    'pms.spaceMembers.inviteHint': '초대 안내',
    'pms.spaceMembers.inviteTitle': '멤버 초대',
    'pms.spaceMembers.memberActions': '멤버 작업',
    'pms.spaceMembers.memberCount': '멤버 · {{count}}',
    'pms.spaceMembers.noMembers': '아직 멤버가 없습니다.',
    'pms.spaceMembers.removeConfirmAction': '내보내기',
    'pms.spaceMembers.removeConfirmDescription':
      '{{name}}님을 이 스페이스에서 내보냅니다.',
    'pms.spaceMembers.removeConfirmTitle': '멤버를 내보낼까요?',
    'pms.spaceMembers.removeFromSpace': '스페이스에서 내보내기',
    'pms.spaceMembers.roleDescription.admin': '관리 가능',
    'pms.spaceMembers.roleDescription.member': '편집 가능',
    'pms.spaceMembers.roleDescription.owner': '소유 가능',
    'pms.spaceMembers.roleDescription.viewer': '읽기 전용',
    'pms.spaceMembers.titleSuffix': '멤버 관리',
    'pms.taskDetail.me': '나',
  };
  return {
    translate: (key: string, values?: Record<string, string | number>) => {
      let value = translations[key] ?? key;
      for (const [name, replacement] of Object.entries(values ?? {})) {
        value = value.replace(`{{${name}}}`, String(replacement));
      }
      return value;
    },
  };
});

vi.mock('react-i18next', () => ({
  useTranslation: () => ({
    i18n: { language: 'ko-KR', resolvedLanguage: 'ko-KR' },
    t: translate,
  }),
}));

describe('SpaceMembersModal', () => {
  beforeEach(() => {
    vi.clearAllMocks();
    vi.mocked(listSpaceMembers).mockResolvedValue({
      total: 2,
      page: 1,
      page_size: 100,
      items: [
        {
          primary_org_unit_name: null,
          email: 'owner@example.test',
          is_admin: false,
          joined_at: '2026-01-01T00:00:00Z',
          full_name: 'Owner User',
          role: 'owner',
          user_id: 'owner',
        },
        {
          primary_org_unit_name: null,
          email: 'target@example.test',
          is_admin: false,
          joined_at: '2026-01-01T00:00:00Z',
          full_name: 'Target Member',
          role: 'member',
          user_id: 'target',
        },
      ],
    });
    vi.mocked(listPmsUsers).mockResolvedValue([]);
    vi.mocked(removeSpaceMember).mockResolvedValue(undefined);
  });

  afterEach(() => {
    vi.restoreAllMocks();
  });

  it('uses the app confirm dialog before removing a member from a space', async () => {
    const browserConfirm = vi
      .spyOn(window, 'confirm')
      .mockImplementation(() => {
        throw new Error('window.confirm should not be called');
      });

    render(
      <SpaceMembersModal
        canManage
        currentUserRole="owner"
        isOpen
        onClose={() => undefined}
        spaceId="space-1"
        spaceName="스페이스"
        workspaceSlug="workspace"
      />,
    );

    await screen.findByText('Target Member');

    fireEvent.click(screen.getByRole('button', { name: '멤버 작업' }));
    const removeAction = await screen.findByRole('button', {
      name: '스페이스에서 내보내기',
    });
    expect(
      removeAction
        .closest('[data-ui-floating-layer]')
        ?.classList.contains('pointer-events-auto'),
    ).toBe(true);
    fireEvent.click(removeAction);

    expect(browserConfirm).not.toHaveBeenCalled();
    await waitFor(() => {
      expect(screen.getAllByRole('dialog')).toHaveLength(2);
    });

    fireEvent.click(screen.getByRole('button', { name: '내보내기' }));

    await waitFor(() => {
      expect(removeSpaceMember).toHaveBeenCalledWith(
        expect.any(Function),
        'space-1',
        'target',
        'workspace',
      );
    });
  });

  it('adds the people of a chosen department except those unticked', async () => {
    const person = (id: string, name: string, department: string | null) => ({
      email: `${id}@example.test`,
      full_name: name,
      id,
      primary_org_unit_id: department,
      primary_org_unit_name: department === 'cool-1' ? '쿨링 1파트' : '인사팀',
    });
    vi.mocked(listPmsUsers).mockResolvedValue([
      person('target', 'Target Member', 'cool-1'),
      person('kim', '김현수', 'cool-1'),
      person('lee', '이규탁', 'cool-1'),
      person('park', '박인사', 'hr'),
    ]);
    vi.mocked(addSpaceMembers).mockResolvedValue({
      items: [],
      skipped_count: 0,
    });

    render(
      <SpaceMembersModal
        canManage
        currentUserRole="owner"
        isOpen
        onClose={() => undefined}
        spaceId="space-1"
        spaceName="스페이스"
        workspaceSlug="workspace"
      />,
    );

    const select = await screen.findByRole('combobox', { name: '부서 선택' });
    // Target Member is already in the space, so the team offers two people.
    expect(
      screen.getByRole('option', { name: '쿨링 1파트 (2명)' }),
    ).toBeTruthy();
    fireEvent.change(select, { target: { value: 'cool-1' } });
    fireEvent.click(screen.getByRole('checkbox', { name: /이규탁/ }));
    fireEvent.click(screen.getByRole('button', { name: '1명 추가' }));

    await waitFor(() => {
      expect(addSpaceMembers).toHaveBeenCalledWith(
        expect.any(Function),
        'space-1',
        { role: 'member', user_ids: ['kim'] },
        'workspace',
      );
    });
  });

  it('lets a member leave the space after confirming', async () => {
    vi.mocked(listSpaceMembers).mockResolvedValue({
      total: 2,
      page: 1,
      page_size: 100,
      items: [
        {
          primary_org_unit_name: null,
          email: 'boss@example.test',
          is_admin: false,
          joined_at: '2026-01-01T00:00:00Z',
          full_name: 'Boss',
          role: 'owner',
          user_id: 'boss',
        },
        {
          primary_org_unit_name: null,
          email: 'me@example.test',
          is_admin: false,
          joined_at: '2026-01-01T00:00:00Z',
          full_name: 'Me',
          role: 'member',
          user_id: 'owner',
        },
      ],
    });
    const onClose = vi.fn();
    const onLeft = vi.fn();
    const changes: string[] = [];
    const listener = (event: Event) =>
      changes.push((event as CustomEvent<{ type: string }>).detail.type);
    window.addEventListener('pms:space-changed', listener);

    try {
      render(
        <SpaceMembersModal
          canManage={false}
          currentUserRole="member"
          isOpen
          onClose={onClose}
          onLeft={onLeft}
          spaceId="space-1"
          spaceName="스페이스"
          workspaceSlug="workspace"
        />,
      );

      await screen.findByText('Boss');
      fireEvent.click(screen.getByRole('button', { name: '스페이스 나가기' }));
      fireEvent.click(await screen.findByRole('button', { name: '나가기' }));

      await waitFor(() => expect(onLeft).toHaveBeenCalled());
      expect(removeSpaceMember).toHaveBeenCalledWith(
        expect.any(Function),
        'space-1',
        'owner',
        'workspace',
      );
      expect(onClose).toHaveBeenCalled();
      expect(changes).toEqual(['left']);
    } finally {
      window.removeEventListener('pms:space-changed', listener);
    }
  });

  it('keeps the last owner from leaving', async () => {
    render(
      <SpaceMembersModal
        canManage
        currentUserRole="owner"
        isOpen
        onClose={() => undefined}
        spaceId="space-1"
        spaceName="스페이스"
        workspaceSlug="workspace"
      />,
    );

    await screen.findByText('Target Member');
    expect(
      (
        screen.getByRole('button', {
          name: '스페이스 나가기',
        }) as HTMLButtonElement
      ).disabled,
    ).toBe(true);
    expect(screen.getByText('마지막 소유자는 나갈 수 없습니다.')).toBeTruthy();
    expect(removeSpaceMember).not.toHaveBeenCalled();
  });
});
