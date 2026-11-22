import { describe, expect, it } from 'vitest';

import type { PmsSpaceMember, PmsUserSummary } from '../api/pms-api';
import {
  SPACE_MEMBERS_INITIAL_STATE,
  canMutateSpaceMember,
  chunkSpaceMemberIds,
  inviteCandidates,
  roleOptionConfigForUser,
  spaceDepartmentCandidates,
  spaceDepartmentOptions,
  spaceLeaveState,
  spaceMemberRowMenuPosition,
  spaceMemberIds,
  spaceMembersModalReducer,
  visibleSpaceMembers,
} from './space-members-modal-model';

function user(id: string, fullName = id, email = `${id}@example.test`) {
  return {
    id,
    full_name: fullName,
    email,
  } as PmsUserSummary;
}

function member(
  userId: string,
  role: PmsSpaceMember['role'],
  fullName = userId,
  email = `${userId}@example.test`,
) {
  return {
    user_id: userId,
    role,
    full_name: fullName,
    email,
  } as PmsSpaceMember;
}

describe('space members modal model', () => {
  it('keeps load and row-menu state transitions in the reducer', () => {
    const loading = spaceMembersModalReducer(SPACE_MEMBERS_INITIAL_STATE, {
      type: 'loadStart',
    });
    expect(loading).toMatchObject({ loading: true, error: null });

    const loaded = spaceMembersModalReducer(loading, {
      type: 'loadSuccess',
      members: [member('owner', 'owner')],
      users: [user('candidate')],
    });
    expect(loaded.loading).toBe(false);
    expect(loaded.members).toHaveLength(1);
    expect(loaded.allUsers).toHaveLength(1);

    expect(
      spaceMembersModalReducer(loaded, {
        type: 'toggleRowMenu',
        userId: 'owner',
      }).openRowMenu,
    ).toBe('owner');
    expect(
      spaceMembersModalReducer(
        { ...loaded, openRowMenu: 'owner' },
        { type: 'toggleRowMenu', userId: 'owner' },
      ).openRowMenu,
    ).toBeNull();
  });

  it('derives role options from admin-management permission', () => {
    expect(
      roleOptionConfigForUser(false).map((option) => option.value),
    ).toEqual(['admin', 'member', 'viewer']);
    expect(roleOptionConfigForUser(true).map((option) => option.value)).toEqual(
      ['owner', 'admin', 'member', 'viewer'],
    );
  });

  it('filters invite candidates by existing members, query, and limit', () => {
    const candidates = [
      user('member', 'Existing Member'),
      user('ada', 'Ada Lovelace'),
      user('grace', 'Grace Hopper'),
      user('alan', 'Alan Turing'),
    ];

    expect(
      inviteCandidates({
        allUsers: candidates,
        memberIds: spaceMemberIds([member('member', 'member')]),
        query: 'a',
        limit: 2,
      }).map((candidate) => candidate.id),
    ).toEqual(['ada', 'grace']);
  });

  it('pins the current user first when they are eligible for invitation', () => {
    const candidates = [
      user('ada', 'Ada Lovelace'),
      user('self', 'Self User'),
      user('grace', 'Grace Hopper'),
    ];

    expect(
      inviteCandidates({
        allUsers: candidates,
        currentUserId: 'self',
        memberIds: spaceMemberIds([]),
        query: 'a',
      }).map((candidate) => candidate.id),
    ).toEqual(['self', 'ada', 'grace']);
  });

  it('sorts visible members by role rank, then locale name, and filters search', () => {
    const members = [
      member('viewer', 'viewer', 'Victor'),
      member('admin', 'admin', 'Admin'),
      member('owner', 'owner', 'Owner'),
      member('member', 'member', 'Member'),
    ];

    expect(
      visibleSpaceMembers({ locale: 'en', members, query: '' }).map(
        (item) => item.user_id,
      ),
    ).toEqual(['owner', 'admin', 'member', 'viewer']);
    expect(
      visibleSpaceMembers({ locale: 'en', members, query: 'victor' }).map(
        (item) => item.user_id,
      ),
    ).toEqual(['viewer']);
  });

  it('guards member mutation for self and protected manager roles', () => {
    expect(
      canMutateSpaceMember({
        canManage: false,
        canManageAdmins: true,
        currentUserId: 'other',
        member: member('target', 'member'),
      }),
    ).toBe(false);
    expect(
      canMutateSpaceMember({
        canManage: true,
        canManageAdmins: true,
        currentUserId: 'self',
        member: member('self', 'owner'),
      }),
    ).toBe(false);
    expect(
      canMutateSpaceMember({
        canManage: true,
        canManageAdmins: false,
        currentUserId: 'other',
        member: member('target', 'admin'),
      }),
    ).toBe(false);
    expect(
      canMutateSpaceMember({
        canManage: true,
        canManageAdmins: false,
        currentUserId: 'other',
        member: member('target', 'viewer'),
      }),
    ).toBe(true);
  });

  it('keeps the row action menu inside the viewport', () => {
    expect(
      spaceMemberRowMenuPosition({
        anchorRect: { bottom: 204, right: 640, top: 180 },
        menuHeight: 320,
        viewportHeight: 900,
        viewportWidth: 1200,
      }),
    ).toEqual({ left: 440, top: 208 });

    const nearBottom = spaceMemberRowMenuPosition({
      anchorRect: { bottom: 724, right: 640, top: 700 },
      menuHeight: 320,
      viewportHeight: 760,
      viewportWidth: 1200,
    });
    expect(nearBottom).toEqual({ left: 440, top: 376 });
    expect(nearBottom.top + 320).toBeLessThanOrEqual(752);

    expect(
      spaceMemberRowMenuPosition({
        anchorRect: { bottom: 184, right: 140, top: 160 },
        menuHeight: 320,
        viewportHeight: 260,
        viewportWidth: 180,
      }),
    ).toEqual({ left: 8, top: 8 });
  });

  it('offers departments by name with only the people still to add', () => {
    const inDepartment = (id: string, name: string, department: string) =>
      ({
        ...user(id, name),
        primary_org_unit_id: department,
        primary_org_unit_name: department === 'b' ? '냉각' : '구매',
      }) as PmsUserSummary;
    const allUsers = [
      inDepartment('kim', '김', 'b'),
      inDepartment('lee', '이', 'b'),
      inDepartment('choi', '최', 'a'),
      user('no-department'),
    ];
    const memberIds = new Set(['lee', 'choi']);

    expect(
      spaceDepartmentOptions({ allUsers, locale: 'ko-KR', memberIds }),
    ).toEqual([{ candidateCount: 1, id: 'b', name: '냉각' }]);
    expect(
      spaceDepartmentCandidates({
        allUsers,
        departmentId: 'b',
        locale: 'ko-KR',
        memberIds,
      }).map((candidate) => candidate.id),
    ).toEqual(['kim']);
  });

  it('lets anyone but the last owner leave', () => {
    expect(
      spaceLeaveState({
        currentUserId: 'me',
        members: [member('me', 'viewer'), member('boss', 'owner')],
      }),
    ).toBe('allowed');
    expect(
      spaceLeaveState({
        currentUserId: 'me',
        members: [member('me', 'owner'), member('other', 'member')],
      }),
    ).toBe('last-owner');
    expect(
      spaceLeaveState({
        currentUserId: 'me',
        members: [member('me', 'owner'), member('co', 'owner')],
      }),
    ).toBe('allowed');
    expect(
      spaceLeaveState({ currentUserId: 'me', members: [member('x', 'owner')] }),
    ).toBe('not-member');
  });
  it('splits a large department into server-sized batches', () => {
    const ids = Array.from({ length: 205 }, (_, index) => `u${index}`);

    const chunks = chunkSpaceMemberIds(ids);

    expect(chunks.map((chunk) => chunk.length)).toEqual([100, 100, 5]);
    expect(chunks.flat()).toEqual(ids);
  });
});
