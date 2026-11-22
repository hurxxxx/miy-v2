// Regenerate web/src/platform/i18n/resources.ts from AI-DO.
//
// Copies these parts of AI-DO apps/web/src/platform/i18n/resources.ts
// (commit 0002af062, Korean and English) verbatim and adds the miy keys
// below (MIY):
//
// - common (actions, feedback, resourceLocation labels);
// - shell.nav.whiteboard-* (the hub view titles);
// - apps.whiteboard (every whiteboard screen);
// - the few keys the whiteboard screens borrow from other apps:
//   apps.docs.share.access.{read,edit}, apps.pms.searchUser,
//   apps.pms.bulk.remove, apps.pms.actions.favorite,
//   apps.ai.search.metadataPeople.
//
// AI-DO's file is a plain object literal (`export const resources = {…} as
// const`), so it is evaluated as JavaScript and the parts are picked by key;
// a missing key stops the script.
//
//   node scripts/gen-i18n-resources.mjs \
//     ../../../ai-do/prod/apps/web/src/platform/i18n/resources.ts \
//     web/src/platform/i18n/resources.ts
import { readFileSync, writeFileSync } from 'node:fs';
import vm from 'node:vm';

const [source, target] = process.argv.slice(2);
const text = readFileSync(source, 'utf8')
  .replace(/^export const resources =/m, 'globalThis.resources =')
  .replace(/\}\s*as const;\s*$/, '};');
const context = {};
vm.runInNewContext(text, context);
const aiDo = context.resources;

function pick(locale, path) {
  let node = aiDo[locale];
  for (const key of path) {
    if (!node || !(key in node)) throw new Error(`${locale}: missing ${path.join('.')}`);
    node = node[key];
  }
  return JSON.parse(JSON.stringify(node));
}

function set(tree, path, value) {
  let node = tree;
  for (const key of path.slice(0, -1)) node = node[key] ??= {};
  node[path[path.length - 1]] = value;
}

const NAV_KEYS = [
  'whiteboard-all',
  'whiteboard-my',
  'whiteboard-recent',
  'whiteboard-favorites',
  'whiteboard-archived',
];
const BORROWED = [
  ['docs', 'share', 'access', 'read'],
  ['docs', 'share', 'access', 'edit'],
  ['pms', 'searchUser'],
  ['pms', 'bulk', 'remove'],
  ['pms', 'actions', 'favorite'],
  ['ai', 'search', 'metadataPeople'],
];

const MIY = {
  'ko-KR': {
    whiteboardApp: {
      title: '화이트보드',
      unsavedChanges: '저장되지 않은 화이트보드 변경이 있습니다. 이동하시겠습니까?',
      loading: '불러오는 중...',
      libraries: '라이브러리',
      conflict: {
        title: '다른 사람이 이 화이트보드를 먼저 저장했습니다.',
        description:
          '최신 내용을 다시 불러오면 내 변경은 사라집니다. 내 변경 유지를 누르면 최신 저장본을 내 화면 내용으로 덮어씁니다.',
        reload: '최신 내용 불러오기',
        keep: '내 변경 유지',
      },
      imageUploadFailed: '이미지를 저장하지 못했습니다. 다시 저장하면 이어서 올립니다.',
      imageUnsupported: 'PNG, JPEG, GIF, WebP 이미지만 저장할 수 있습니다. SVG는 PNG로 바꿔 저장합니다.',
      imageTooLarge: '10MB 이하의 이미지만 저장할 수 있습니다.',
      imageLoadFailed: '일부 이미지를 불러오지 못했습니다.',
      spaces: {
        label: '공간',
        global: '전사 공간',
        select: '공간 선택',
        create: '공간 만들기',
        createTitle: '새 공간',
        namePlaceholder: '공간 이름',
        descriptionPlaceholder: '공간 설명 (선택)',
        created: '"{{name}}" 공간을 만들었습니다.',
        createFailed: '공간을 만들지 못했습니다.',
        members: '멤버 관리',
        membersTitle: '{{name}} 멤버와 권한',
        delete: '공간 삭제',
        deleteConfirm: '"{{name}}" 공간을 삭제할까요?',
        deleteFailed: '공간을 삭제하지 못했습니다.',
        close: '닫기',
        cancel: '취소',
        loadFailed: '공간을 불러오지 못했습니다.',
      },
      directory: {
        users: '사용자',
        groups: '그룹',
        searchPlaceholder: '이름으로 검색',
        loading: '검색 중…',
        noMatches: '검색 결과가 없습니다.',
        unavailable: '목록을 불러올 수 없습니다. 접근 권한을 확인해 주세요.',
        retry: '다시 시도',
        loadMore: '더 보기',
        currentUser: '본인',
        locked: '고정',
      },
    },
  },
  'en-US': {
    whiteboardApp: {
      title: 'Whiteboard',
      unsavedChanges: 'This whiteboard has unsaved changes. Leave anyway?',
      loading: 'Loading...',
      libraries: 'Libraries',
      conflict: {
        title: 'Someone else saved this whiteboard first.',
        description:
          'Reloading discards your changes. Keep mine replaces the latest saved version with what is on your screen.',
        reload: 'Reload latest',
        keep: 'Keep mine',
      },
      imageUploadFailed: 'Could not store an image. Saving again continues the upload.',
      imageUnsupported: 'Only PNG, JPEG, GIF and WebP images can be stored. SVG is saved as PNG.',
      imageTooLarge: 'Images must be 10 MB or smaller.',
      imageLoadFailed: 'Some images could not be loaded.',
      spaces: {
        label: 'Space',
        global: 'Company space',
        select: 'Select space',
        create: 'Create space',
        createTitle: 'New space',
        namePlaceholder: 'Space name',
        descriptionPlaceholder: 'Description (optional)',
        created: 'Created the space "{{name}}".',
        createFailed: 'Could not create the space.',
        members: 'Members',
        membersTitle: '{{name}} members and roles',
        delete: 'Delete space',
        deleteConfirm: 'Delete the space "{{name}}"?',
        deleteFailed: 'Could not delete the space.',
        close: 'Close',
        cancel: 'Cancel',
        loadFailed: 'Could not load spaces.',
      },
      directory: {
        users: 'Users',
        groups: 'Groups',
        searchPlaceholder: 'Search by name',
        loading: 'Searching…',
        noMatches: 'No matches.',
        unavailable: 'The list is unavailable. Check your access.',
        retry: 'Retry',
        loadMore: 'Load more',
        currentUser: 'You',
        locked: 'Locked',
      },
    },
  },
};

// Server error codes (`{error:{code}}`): AI-DO's catalog messages
// (apps/api/src/ai_do_api/core/i18n_catalog.py) for its codes, miy text for
// the new ones.
const ERRORS = {
  'ko-KR': {
    whiteboard_not_found: '화이트보드를 찾을 수 없습니다.',
    whiteboard_workspace_context_required: '화이트보드 요청에는 워크스페이스 컨텍스트가 필요합니다.',
    whiteboard_target_edit_access_required: '대상 편집 권한이 필요합니다.',
    whiteboard_edit_access_required: '화이트보드 편집 권한이 필요합니다.',
    whiteboard_manage_access_required: '화이트보드 관리 권한이 필요합니다.',
    whiteboard_archive_before_permanent_delete: '영구 삭제 전에 화이트보드를 먼저 보관 처리해야 합니다.',
    whiteboard_share_access_required: '화이트보드 공유 권한이 필요합니다.',
    whiteboard_owner_already_has_full_access: '소유자는 이미 전체 권한을 가지고 있습니다.',
    whiteboard_shared_users_workspace_required: '공유 대상 사용자는 같은 워크스페이스의 멤버여야 합니다.',
    workspace_not_found: '워크스페이스를 찾을 수 없습니다.',
    auth_user_not_found: '사용자를 찾을 수 없습니다.',
    whiteboard_revision_conflict: '다른 사람이 이 화이트보드를 먼저 저장했습니다.',
    whiteboard_scene_file_invalid: '화이트보드 이미지는 이 화이트보드에 저장된 파일이어야 합니다.',
    whiteboard_space_not_empty: '공간의 화이트보드를 먼저 영구 삭제해야 합니다.',
    whiteboard_directory_unavailable: '사용자 목록을 불러올 수 없습니다.',
    media_unsupported_file_type: 'PNG, JPEG, GIF, WebP 이미지만 저장할 수 있습니다.',
    media_file_size_limit_exceeded: '10MB 이하의 이미지만 저장할 수 있습니다.',
    media_upload_failed: '이미지를 저장하지 못했습니다.',
    acl_denied: '권한이 없습니다.',
    acl_revision_conflict: '다른 사람이 먼저 변경했습니다. 다시 불러온 뒤 시도해 주세요.',
    acl_last_manager: '관리자가 한 명 이상 있어야 합니다.',
    acl_owner_protected: '공간을 만든 사람의 권한은 바꿀 수 없습니다.',
    acl_directory_unavailable: '사용자 목록을 확인할 수 없습니다. 잠시 뒤 다시 시도해 주세요.',
  },
  'en-US': {
    whiteboard_not_found: 'Whiteboard not found.',
    whiteboard_workspace_context_required: 'Whiteboard requests require a workspace context.',
    whiteboard_target_edit_access_required: 'Target edit access required.',
    whiteboard_edit_access_required: 'Whiteboard edit access required.',
    whiteboard_manage_access_required: 'Whiteboard manage access required.',
    whiteboard_archive_before_permanent_delete: 'Archive the whiteboard before permanent deletion.',
    whiteboard_share_access_required: 'Whiteboard share access required.',
    whiteboard_owner_already_has_full_access: 'Owner already has full access.',
    whiteboard_shared_users_workspace_required: 'Shared users must be members of the same workspace.',
    workspace_not_found: 'Workspace not found.',
    auth_user_not_found: 'User not found.',
    whiteboard_revision_conflict: 'Someone else saved this whiteboard first.',
    whiteboard_scene_file_invalid: 'Whiteboard images must be files stored for this whiteboard.',
    whiteboard_space_not_empty: "Permanently delete the space's whiteboards first.",
    whiteboard_directory_unavailable: 'The people list is unavailable.',
    media_unsupported_file_type: 'Only PNG, JPEG, GIF and WebP images can be stored.',
    media_file_size_limit_exceeded: 'Images must be 10 MB or smaller.',
    media_upload_failed: 'Could not store the image.',
    acl_denied: 'You do not have permission.',
    acl_revision_conflict: 'Someone changed this first. Reload and try again.',
    acl_last_manager: 'At least one manager is required.',
    acl_owner_protected: "The creator's role cannot be changed.",
    acl_directory_unavailable: 'The people directory is unavailable. Try again later.',
  },
};

const out = {};
for (const locale of ['ko-KR', 'en-US']) {
  const tree = { common: pick(locale, ['common']), shell: { nav: {} }, apps: {}, errors: ERRORS[locale] };
  for (const key of NAV_KEYS) tree.shell.nav[key] = pick(locale, ['shell', 'nav', key]);
  tree.apps.whiteboard = pick(locale, ['apps', 'whiteboard']);
  for (const path of BORROWED) set(tree.apps, path, pick(locale, ['apps', ...path]));
  Object.assign(tree.apps, MIY[locale]);
  out[locale] = tree;
}
writeFileSync(
  target,
  `// Generated by scripts/gen-i18n-resources.mjs from AI-DO 0002af062
// apps/web/src/platform/i18n/resources.ts. Do not edit by hand.
export const resources = ${JSON.stringify(out, null, 2)} as const;
`,
);
