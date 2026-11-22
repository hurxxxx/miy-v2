"""Regenerate web/src/platform/i18n/resources.ts from AI-DO.

Copies these parts of AI-DO apps/web/src/platform/i18n/resources.ts (Korean
and English) verbatim and adds the miy keys below:

- common;
- apps.docs (the docs screens);
- the single keys the docs screens borrow from other apps: apps.planner.title,
  apps.pms.creating, apps.pms.filter.filters, apps.pms.name,
  apps.pms.bulk.remove, apps.ai.search.metadataLocation, apps.home.actionNewDoc.

The line numbers are those of AI-DO commit 0002af062; the asserts stop if the
source moved.

    python3 scripts/gen-i18n-resources.py \
        ../../../ai-do/prod/apps/web/src/platform/i18n/resources.ts \
        web/src/platform/i18n/resources.ts
"""
import sys
from pathlib import Path

src = Path(sys.argv[1]).read_text(encoding='utf-8').split('\n')
out = Path(sys.argv[2])


def block(first, last):  # 1-based inclusive line numbers
    return '\n'.join(src[first - 1:last])


RANGES = {
    'ko': {
        'common': (3, 79, '    common: {'),
        'docs': (17016, 17278, '      docs: {'),
    },
    'en': {
        'common': (19299, 19374, '    common: {'),
        'docs': (36584, 36852, '      docs: {'),
    },
}

# (line, expected text) of single borrowed keys.
SINGLE = {
    'ko': {
        'planner.title': (18132, "        title: '제목',"),
        'pms.creating': (18201, "        creating: '만드는 중...',"),
        'pms.filter.filters': (18273, "          filters: '필터',"),
        'pms.name': (18212, "        name: '이름',"),
        'pms.bulk.remove': (18360, "          remove: '제거',"),
        'ai.search.metadataLocation': (14133, "          metadataLocation: '장소',"),
        'home.actionNewDoc': (17280, "        actionNewDoc: '새 Doc',"),
    },
    'en': {
        'planner.title': (37726, "        title: 'Title',"),
        'pms.creating': (37795, "        creating: 'Creating...',"),
        'pms.filter.filters': (37865, "          filters: 'Filters',"),
        'pms.name': (37806, "        name: 'Name',"),
        'pms.bulk.remove': (37963, "          remove: 'Remove',"),
        'ai.search.metadataLocation': (33663, "          metadataLocation: 'Location',"),
        'home.actionNewDoc': (36854, "        actionNewDoc: 'New Doc',"),
    },
}


def take(lang, name):
    first, last, head = RANGES[lang][name]
    text = block(first, last)
    lines = text.split('\n')
    assert lines[0] == head, (lang, name, lines[0])
    assert lines[-1].strip() == '},', (lang, name, lines[-1])
    return text


def single(lang, key):
    line, expected = SINGLE[lang][key]
    text = src[line - 1]
    assert text == expected, (lang, key, text)
    return text.strip()


def borrowed(lang):
    s = lambda key: single(lang, key)  # noqa: E731
    return '\n'.join([
        '      planner: {',
        f"        {s('planner.title')}",
        '      },',
        '      pms: {',
        f"        {s('pms.creating')}",
        f"        {s('pms.name')}",
        '        filter: {',
        f"          {s('pms.filter.filters')}",
        '        },',
        '        bulk: {',
        f"          {s('pms.bulk.remove')}",
        '        },',
        '      },',
        '      ai: {',
        '        search: {',
        f"          {s('ai.search.metadataLocation')}",
        '        },',
        '      },',
        '      home: {',
        f"        {s('home.actionNewDoc')}",
        '      },',
    ])


DOCS_APP = {
    'ko': """      docsApp: {
        unsavedChanges: '저장되지 않은 작업이 있습니다. 이동하시겠습니까?',
        loading: '불러오는 중...',
        loadFailed: '문서 정보를 불러오지 못했습니다.',
        actionFailed: '요청을 처리하지 못했습니다.',
        copyFailed: '복사하지 못했습니다. 링크를 직접 선택해 복사하세요.',
        imageUploadFailed: '이미지를 올리지 못했습니다.',
        library: '라이브러리',
        cancel: '취소',
        close: '닫기',
        conflict: {
          message: '다른 사람이 이 페이지를 먼저 저장했습니다. 최신 내용을 다시 불러오거나 내 변경으로 덮어쓸 수 있습니다.',
          reload: '다시 불러오기',
          keepMine: '내 변경 유지',
        },
        save: {
          saving: '저장 중…',
          failed: '변경 내용을 저장하지 못했습니다. 내용은 이 화면에 남아 있습니다.',
          retry: '다시 저장',
        },
        attachments: {
          retryPending: '올리지 못한 파일 {{count}}개',
          retry: '다시 시도',
        },
        spaces: {
          label: '공간',
          global: '전체 문서',
          unavailable: '들어갈 수 없는 공간',
          title: '공간',
          description: '문서 관리자가 공간을 만들고, 공간 관리자가 멤버와 권한(보기·멤버·관리자)을 정합니다. 전체 문서 공간에는 문서를 쓸 수 있는 모든 사람이 멤버로 들어갑니다.',
          none: '관리하는 공간이 없습니다.',
          create: '공간 만들기',
          createTitle: '새 공간',
          namePlaceholder: '공간 이름',
          descriptionPlaceholder: '공간 설명 (선택)',
          created: '"{{name}}" 공간을 만들었습니다.',
          createFailed: '공간을 만들지 못했습니다.',
          members: '멤버 관리',
          membersTitle: '{{name}} 멤버와 권한',
          delete: '공간 삭제',
          deleteConfirm: '"{{name}}" 공간을 삭제할까요? 휴지통의 문서와 그 첨부도 함께 지워집니다.',
          deleteFailed: '공간을 삭제하지 못했습니다.',
        },
        settings: {
          title: '문서 설정',
        },
        bootstrap: {
          title: '문서 관리자가 아직 없습니다',
          body: '처음 한 사람이 관리자를 맡을 수 있습니다. 관리자는 공간을 만들고 전체 문서 공간의 문서를 관리합니다.',
          claim: '내가 관리자 맡기',
          claimed: '문서 관리자가 되었습니다.',
          failed: '관리자를 맡지 못했습니다. 이미 다른 사람이 맡았을 수 있습니다.',
        },
        managers: {
          title: '문서 관리자',
          description: '문서 관리자는 공간을 만들고 전체 문서 공간의 문서를 관리합니다. 플랫폼 관리자 권한과는 별개입니다.',
          add: '관리자 추가',
          added: '{{name}} 님을 관리자로 추가했습니다.',
          addFailed: '관리자를 추가하지 못했습니다.',
          remove: '{{name}} 해제',
          removed: '{{name}} 님을 관리자에서 해제했습니다.',
          removeFailed: '관리자를 해제하지 못했습니다.',
          you: '나',
          empty: '관리자가 없습니다.',
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
      },""",
    'en': """      docsApp: {
        unsavedChanges: 'You have unsaved changes. Leave anyway?',
        loading: 'Loading...',
        loadFailed: 'Could not load docs.',
        actionFailed: 'The request could not be completed.',
        copyFailed: 'Could not copy. Select the link and copy it yourself.',
        imageUploadFailed: 'Could not upload the image.',
        library: 'Library',
        cancel: 'Cancel',
        close: 'Close',
        conflict: {
          message: 'Someone else saved this page first. Reload the latest version or keep your changes over it.',
          reload: 'Reload',
          keepMine: 'Keep my changes',
        },
        save: {
          saving: 'Saving…',
          failed: 'Your changes were not saved. They are still on this screen.',
          retry: 'Save again',
        },
        attachments: {
          retryPending: '{{count}} file(s) not uploaded',
          retry: 'Retry',
        },
        spaces: {
          label: 'Space',
          global: 'All docs',
          unavailable: 'Space not available',
          title: 'Spaces',
          description: 'Docs managers create spaces; space admins choose members and roles (viewer, member, admin). Everyone who may use Docs is a member of the All docs space.',
          none: 'You manage no spaces.',
          create: 'New space',
          createTitle: 'New space',
          namePlaceholder: 'Space name',
          descriptionPlaceholder: 'Description (optional)',
          created: 'Created the space "{{name}}".',
          createFailed: 'Could not create the space.',
          members: 'Members',
          membersTitle: '{{name}} members and roles',
          delete: 'Delete space',
          deleteConfirm: 'Delete the space "{{name}}"? Archived docs in it and their attachments are deleted too.',
          deleteFailed: 'Could not delete the space.',
        },
        settings: {
          title: 'Docs settings',
        },
        bootstrap: {
          title: 'Docs has no manager yet',
          body: 'The first person can take the role. Managers create spaces and manage the docs of the All docs space.',
          claim: 'Become the manager',
          claimed: 'You are now a docs manager.',
          failed: 'Could not take the role. Someone else may have taken it.',
        },
        managers: {
          title: 'Docs managers',
          description: 'Docs managers create spaces and manage the docs of the All docs space. This is separate from platform administration.',
          add: 'Add manager',
          added: '{{name}} is now a manager.',
          addFailed: 'Could not add the manager.',
          remove: 'Remove {{name}}',
          removed: '{{name}} is no longer a manager.',
          removeFailed: 'Could not remove the manager.',
          you: 'You',
          empty: 'No managers.',
        },
        directory: {
          users: 'Users',
          groups: 'Groups',
          searchPlaceholder: 'Search by name',
          loading: 'Searching…',
          noMatches: 'No results.',
          unavailable: 'The list is unavailable. Check your access.',
          retry: 'Retry',
          loadMore: 'Load more',
          currentUser: 'You',
          locked: 'Locked',
        },
      },""",
}

# AI-DO server catalog messages (core/i18n_catalog.py) for the API error codes
# (`.` → `_`), see platform/api/api-error.ts. docs_page_revision_conflict,
# docs_space_*, docs_manager_*, docs_global_space_protected,
# docs_attachment_request_conflict, media_upload_failed and the acl_* codes
# are miy additions.
ERRORS = {
    'ko': {
        'docs_doc_not_found': '문서를 찾을 수 없습니다.',
        'docs_page_not_found': '페이지를 찾을 수 없습니다.',
        'docs_attachment_not_found': '첨부파일을 찾을 수 없습니다.',
        'docs_attachment_size_limit_exceeded': '첨부파일은 {{limit_mb}}MB까지 올릴 수 있습니다.',
        'docs_attachment_download_failed': '첨부파일을 내려받지 못했습니다. 잠시 후 다시 시도해 주세요.',
        'docs_attachment_request_conflict': '같은 요청 ID로 다른 파일을 올릴 수 없습니다.',
        'docs_requests_workspace_context_required': '문서 요청에는 워크스페이스 컨텍스트가 필요합니다.',
        'docs_parent_page_not_found': '부모 페이지를 찾을 수 없습니다.',
        'docs_page_cannot_be_own_parent': '페이지를 자기 자신의 부모로 지정할 수 없습니다.',
        'docs_page_parent_cycle': '페이지 부모 관계에 순환이 포함될 수 없습니다.',
        'docs_target_edit_access_required': '대상 편집 권한이 필요합니다.',
        'docs_collection_not_found': '컬렉션을 찾을 수 없습니다.',
        'docs_page_access_required': '페이지 접근 권한이 필요합니다.',
        'docs_doc_manage_access_required': '문서 관리 권한이 필요합니다.',
        'docs_doc_access_required': '문서 접근 권한이 필요합니다.',
        'docs_doc_edit_access_required': '문서 편집 권한이 필요합니다.',
        'docs_content_format_mismatch': '문서 콘텐츠 형식과 요청 본문이 일치하지 않습니다.',
        'docs_doc_share_access_required': '문서 공유 권한이 필요합니다.',
        'docs_owner_already_has_full_access': '소유자는 이미 전체 권한을 가지고 있습니다.',
        'docs_page_revision_conflict': '다른 사람이 이 페이지를 먼저 저장했습니다.',
        'docs_space_not_empty': '공간에 문서가 남아 있습니다. 먼저 옮기거나 삭제하세요.',
        'docs_global_space_protected': '전체 문서 공간은 바꾸거나 삭제할 수 없습니다.',
        'docs_manager_required': '문서 관리자 권한이 필요합니다.',
        'docs_manager_exists': '이미 문서 관리자가 있습니다.',
        'docs_manager_unknown_user': '디렉터리에서 찾을 수 없는 사용자입니다.',
        'docs_manager_not_found': '문서 관리자를 찾을 수 없습니다.',
        'docs_last_manager': '마지막 문서 관리자는 해제할 수 없습니다.',
        'workspace_not_found': '워크스페이스를 찾을 수 없습니다.',
        'auth_user_not_found': '사용자를 찾을 수 없습니다.',
        'media_unsupported_file_type': '지원하지 않는 이미지 형식입니다. PNG, JPEG, GIF, WebP만 올릴 수 있습니다.',
        'media_file_size_limit_exceeded': '파일 크기가 10MB를 넘습니다.',
        'media_upload_failed': '파일을 저장하지 못했습니다.',
        'media_not_found': '미디어 파일을 찾을 수 없습니다.',
        'acl_denied': '권한이 없습니다.',
        'validation_request_invalid': '요청 값이 올바르지 않습니다.',
    },
    'en': {
        'docs_doc_not_found': 'Doc not found.',
        'docs_page_not_found': 'Page not found.',
        'docs_attachment_not_found': 'Attachment not found.',
        'docs_attachment_size_limit_exceeded': 'Attachments may be at most {{limit_mb}}MB.',
        'docs_attachment_download_failed': 'Could not download the attachment. Try again shortly.',
        'docs_attachment_request_conflict': 'A different file cannot reuse this request ID.',
        'docs_requests_workspace_context_required': 'Docs requests require a workspace context.',
        'docs_parent_page_not_found': 'Parent page not found.',
        'docs_page_cannot_be_own_parent': 'Page cannot be its own parent.',
        'docs_page_parent_cycle': 'Page parent relationship cannot contain a cycle.',
        'docs_target_edit_access_required': 'Target edit access required.',
        'docs_collection_not_found': 'Collection not found.',
        'docs_page_access_required': 'Page access required.',
        'docs_doc_manage_access_required': 'Doc manage access required.',
        'docs_doc_access_required': 'Doc access required.',
        'docs_doc_edit_access_required': 'Doc edit access required.',
        'docs_content_format_mismatch': 'The request body does not match the doc content format.',
        'docs_doc_share_access_required': 'Doc share access required.',
        'docs_owner_already_has_full_access': 'Owner already has full access.',
        'docs_page_revision_conflict': 'Someone else saved this page first.',
        'docs_space_not_empty': 'The space still holds docs. Move or delete them first.',
        'docs_global_space_protected': 'The All docs space cannot be changed or deleted.',
        'docs_manager_required': 'Docs manager access required.',
        'docs_manager_exists': 'A docs manager already exists.',
        'docs_manager_unknown_user': 'The person was not found in the directory.',
        'docs_manager_not_found': 'The docs manager was not found.',
        'docs_last_manager': 'The last docs manager cannot be removed.',
        'workspace_not_found': 'Workspace not found.',
        'auth_user_not_found': 'User not found.',
        'media_unsupported_file_type': 'Unsupported image type. Only PNG, JPEG, GIF and WebP can be uploaded.',
        'media_file_size_limit_exceeded': 'The file is larger than 10 MB.',
        'media_upload_failed': 'The file could not be stored.',
        'media_not_found': 'Media file not found.',
        'acl_denied': 'You do not have access.',
        'validation_request_invalid': 'Request validation failed.',
    },
}


def ts_string(value):
    if "'" not in value:
        return "'" + value.replace('\\', '\\\\') + "'"
    return '"' + value.replace('\\', '\\\\').replace('"', '\\"') + '"'


def errors(lang):
    body = ''.join(f'      {key}: {ts_string(value)},\n' for key, value in ERRORS[lang].items())
    return '    errors: {\n' + body + '    },'


def locale(code, lang):
    return '\n'.join([
        f"  '{code}': {{",
        take(lang, 'common'),
        '    apps: {',
        take(lang, 'docs'),
        borrowed(lang),
        DOCS_APP[lang],
        '    },',
        errors(lang),
        '  },',
    ])


text = '\n'.join([
    '// Generated from AI-DO apps/web/src/platform/i18n/resources.ts at 0002af062:',
    '// common, apps.docs and the single keys the docs screens borrow from',
    '// apps.planner, apps.pms, apps.ai and apps.home, Korean and English, copied',
    '// verbatim. miy additions: apps.docsApp (frame shell, conflict notice,',
    '// spaces, docs managers) and the `errors` namespace (AI-DO server catalog',
    '// messages for the API error codes, see platform/api/api-error.ts).',
    '// Regenerate with scripts/gen-i18n-resources.py.',
    'export const resources = {',
    locale('ko-KR', 'ko'),
    locale('en-US', 'en'),
    '} as const;',
    '',
])
out.write_text(text, encoding='utf-8')
print('wrote', out, len(text.split('\n')), 'lines')
