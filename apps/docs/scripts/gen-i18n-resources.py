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
        trash: {
          title: '휴지통',
          description: '이 공간에서 삭제한 문서와 페이지입니다. 복원하거나 영구 삭제할 수 있고, 자동으로 비워지지 않습니다.',
          empty: '휴지통이 비어 있습니다.',
          kindDoc: '문서',
          kindPage: '"{{doc}}"의 페이지',
          subpages: '하위 페이지 {{count}}개',
          trashedBy: '{{name}} · {{time}} 삭제',
          parentInTrash: '상위 페이지도 휴지통에 있어 맨 위로 복원됩니다',
          restore: '복원',
          restored: '"{{title}}"을(를) 복원했습니다.',
          restoredToRoot: '"{{title}}"을(를) 복원했습니다. 상위 페이지가 휴지통에 있어 문서 맨 위에 두었습니다.',
          deleteForever: '영구 삭제',
          deleteConfirm: '"{{title}}"을(를) 영구 삭제할까요?',
          deleteDescription: '그 안의 페이지, 버전 기록, 댓글과 첨부파일까지 함께 지워지며 되돌릴 수 없습니다.',
        },
        versions: {
          open: '버전 기록',
          title: '버전 기록',
          loadFailed: '버전 기록을 불러오지 못했습니다.',
          revision: '{{revision}}번째 저장',
          current: '현재',
          restoredFrom: '{{revision}}번째 저장에서 복원',
          unknownAuthor: '알 수 없는 사용자',
          restore: '이 버전으로 복원',
          restoreConfirm: '{{revision}}번째 저장 내용으로 복원할까요?',
          restoreDescription: '지금 내용은 버전 기록에 남고, 이 버전의 본문이 새 저장으로 추가됩니다.',
          restored: '{{revision}}번째 저장 내용으로 복원했습니다.',
          conflict: '다른 사람이 이 페이지를 먼저 저장했습니다. 버전 기록을 다시 확인한 뒤 복원하세요.',
          saveFirst: '저장되지 않은 변경 내용이 있습니다. 먼저 저장한 뒤 다시 시도하세요.',
        },
        comments: {
          title: '댓글',
          loadFailed: '댓글을 불러오지 못했습니다.',
          empty: '아직 댓글이 없습니다.',
          placeholder: '댓글을 남기세요. @로 사람을 언급할 수 있습니다.',
          replyPlaceholder: '답글을 남기세요',
          send: '댓글 남기기',
          reply: '답글',
          sendReply: '답글 남기기',
          resolve: '해결',
          reopen: '다시 열기',
          resolvedBy: '{{name}} 님이 해결함',
          showResolved: '해결된 댓글 {{count}}개 보기',
          hideResolved: '해결된 댓글 숨기기',
          deleted: '삭제된 댓글입니다.',
          deletedAuthor: '삭제됨',
          edited: '(수정됨)',
          deleteConfirm: '댓글을 삭제할까요?',
          deleteDescription: '언급과 알림도 함께 거둬집니다.',
          mentionSuggestions: '언급할 사람',
          mentionHint: '@이름으로 언급 · Ctrl+Enter로 보내기',
        },
        templates: {
          label: '템플릿',
          blank: '빈 페이지',
          delete: '"{{title}}" 템플릿 삭제',
          deleteConfirm: '"{{title}}" 템플릿을 삭제할까요?',
          deleteDescription: '이 템플릿으로 만든 문서와 페이지는 그대로 남습니다.',
          loadFailed: '템플릿을 불러오지 못했습니다.',
          saveAsTemplate: '템플릿으로 저장',
          saveTitle: '템플릿 이름',
          saveDescription: '이 페이지의 저장된 본문을 공간의 템플릿으로 남깁니다.',
          saved: '"{{title}}" 템플릿을 저장했습니다.',
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
          deleteConfirm: 'Delete the space "{{name}}"? Docs in its trash and their attachments are deleted too.',
          deleteFailed: 'Could not delete the space.',
        },
        trash: {
          title: 'Trash',
          description: 'Docs and pages deleted in this space. Restore them or delete them for good; the trash is never emptied automatically.',
          empty: 'The trash is empty.',
          kindDoc: 'Doc',
          kindPage: 'Page of "{{doc}}"',
          subpages: '{{count}} subpage(s)',
          trashedBy: 'Deleted by {{name}} · {{time}}',
          parentInTrash: 'Its parent page is in the trash too, so it returns at the top',
          restore: 'Restore',
          restored: 'Restored "{{title}}".',
          restoredToRoot: 'Restored "{{title}}". Its parent page is in the trash, so it is at the top of the doc.',
          deleteForever: 'Delete forever',
          deleteConfirm: 'Delete "{{title}}" forever?',
          deleteDescription: 'Its pages, version history, comments and attachments are deleted too. This cannot be undone.',
        },
        versions: {
          open: 'Version history',
          title: 'Version history',
          loadFailed: 'Could not load the version history.',
          revision: 'Save #{{revision}}',
          current: 'Current',
          restoredFrom: 'Restored from save #{{revision}}',
          unknownAuthor: 'Unknown person',
          restore: 'Restore this version',
          restoreConfirm: 'Restore save #{{revision}}?',
          restoreDescription: 'The current content stays in the history; this version is added as a new save.',
          restored: 'Restored save #{{revision}}.',
          conflict: 'Someone else saved this page first. Check the history again before restoring.',
          saveFirst: 'You have unsaved changes. Save them first, then try again.',
        },
        comments: {
          title: 'Comments',
          loadFailed: 'Could not load the comments.',
          empty: 'No comments yet.',
          placeholder: 'Leave a comment. Mention people with @.',
          replyPlaceholder: 'Write a reply',
          send: 'Comment',
          reply: 'Reply',
          sendReply: 'Send reply',
          resolve: 'Resolve',
          reopen: 'Reopen',
          resolvedBy: 'Resolved by {{name}}',
          showResolved: 'Show {{count}} resolved comment(s)',
          hideResolved: 'Hide resolved comments',
          deleted: 'This comment was deleted.',
          deletedAuthor: 'Deleted',
          edited: '(edited)',
          deleteConfirm: 'Delete this comment?',
          deleteDescription: 'Its mentions and notifications are withdrawn too.',
          mentionSuggestions: 'People to mention',
          mentionHint: 'Mention with @name · Ctrl+Enter to send',
        },
        templates: {
          label: 'Template',
          blank: 'Blank page',
          delete: 'Delete template "{{title}}"',
          deleteConfirm: 'Delete the template "{{title}}"?',
          deleteDescription: 'Docs and pages made from it stay as they are.',
          loadFailed: 'Could not load the templates.',
          saveAsTemplate: 'Save as template',
          saveTitle: 'Template name',
          saveDescription: 'Keeps the saved content of this page as a template of the space.',
          saved: 'Saved the template "{{title}}".',
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
# docs_attachment_request_conflict, media_upload_failed, the acl_* codes and
# the 보강 codes (trash, versions, comments, templates) are miy additions.
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
        'docs_not_in_trash': '휴지통에 있는 항목이 아닙니다.',
        'docs_version_not_found': '버전을 찾을 수 없습니다.',
        'docs_comment_not_found': '댓글을 찾을 수 없습니다.',
        'docs_comment_request_conflict': '같은 요청 ID로 다른 댓글을 남길 수 없습니다.',
        'docs_comment_author_required': '댓글을 쓴 사람만 수정할 수 있습니다.',
        'docs_comment_reply_not_resolvable': '답글은 해결 처리할 수 없습니다. 첫 댓글에서 해결하세요.',
        'docs_mention_unknown_user': '디렉터리에서 찾을 수 없는 사람을 언급했습니다.',
        'docs_template_not_found': '템플릿을 찾을 수 없습니다.',
        'docs_template_manage_access_required': '템플릿을 관리할 권한이 없습니다.',
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
        'docs_not_in_trash': 'The item is not in the trash.',
        'docs_version_not_found': 'Version not found.',
        'docs_comment_not_found': 'Comment not found.',
        'docs_comment_request_conflict': 'A different comment cannot reuse this request ID.',
        'docs_comment_author_required': 'Only the author can edit this comment.',
        'docs_comment_reply_not_resolvable': 'Replies cannot be resolved. Resolve the first comment.',
        'docs_mention_unknown_user': 'A mentioned person was not found in the directory.',
        'docs_template_not_found': 'Template not found.',
        'docs_template_manage_access_required': 'You may not manage this template.',
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
    '// spaces, docs managers, trash, versions, comments, templates) and the',
    '// `errors` namespace (AI-DO server catalog',
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
