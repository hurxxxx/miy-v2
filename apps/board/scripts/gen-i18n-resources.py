"""Regenerate web/src/platform/i18n/resources.ts from AI-DO.

Copies these blocks of AI-DO apps/web/src/platform/i18n/resources.ts
(Korean and English) verbatim and adds the miy keys below:

- common;
- apps.community (community screens, channel names, editor);
- the announcement keys of apps.home (AnnouncementsBoard);
- apps.admin.console.community (channel manager);
- the banned-word keys of apps.admin.console.dmSafety (filter terms), without
  its DM-specific filterScope text (the board has its own, apps.board.filterScope).

The line ranges are those of AI-DO commit 0002af062; the asserts stop if the
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


def reindent(text, delta):
    lines = []
    for line in text.split('\n'):
        if delta < 0:
            assert line[:(-delta)].strip() == '' or line.strip() == '', line
            lines.append(line[-delta:] if line.strip() else '')
        else:
            lines.append((' ' * delta + line) if line.strip() else '')
    return '\n'.join(lines)


RANGES = {
    'ko': {
        'common': (3, 79, '    common: {'),
        'community': (5664, 5823, '      community: {'),
        'home': (17285, 17302, "        announcements: '공지사항',"),
        'channelAdmin': (10083, 10124, '          community: {'),
        'dmFailed': (7910, 7911, "            loadFailed: '신고를 불러오지 못했습니다.',"),
        'filterTitle': (7913, 7913, "            filterTitle: '금칙어',"),
        'filterRest': (7916, 7919, "            filterInput: '금칙어 (최대 50자)',"),
    },
    'en': {
        'common': (19299, 19374, '    common: {'),
        'community': (25089, 25249, '      community: {'),
        'home': (36859, 36876, "        announcements: 'Announcements',"),
        'channelAdmin': (29561, 29602, '          community: {'),
        'dmFailed': (27350, 27351, "            loadFailed: 'Could not load reports.',"),
        'filterTitle': (27353, 27353, "            filterTitle: 'Filter terms',"),
        'filterRest': (27356, 27359, "            filterInput: 'Filter term (up to 50 characters)',"),
    },
}


def take(lang, name):
    first, last, head = RANGES[lang][name]
    text = block(first, last)
    assert text.split('\n')[0] == head, (lang, name, text.split('\n')[0])
    return text


for lang in RANGES:
    for name in ('common', 'community', 'channelAdmin'):
        last = take(lang, name).split('\n')[-1]
        assert last.strip() == '},', (lang, name, last)

BOARD = {
    'ko': """      board: {
        unsavedChanges: '저장되지 않은 작업이 있습니다. 이동하시겠습니까?',
        title: '게시판',
        notices: '공지사항',
        admin: '게시판 관리',
        notFound: '요청한 화면을 찾을 수 없습니다.',
        loading: '불러오는 중...',
        loadFailed: '게시판 정보를 불러오지 못했습니다.',
        managerRequired: '게시판 관리자만 볼 수 있습니다.',
        bootstrap: {
          title: '게시판 관리자가 아직 없습니다',
          body: '처음 한 사람이 관리자를 맡을 수 있습니다. 관리자는 채널·금칙어·전사 공지·공간을 관리하고, 익명 글의 작성자와 관리자 전용 글을 볼 수 있습니다.',
          claim: '내가 관리자 맡기',
          claimed: '게시판 관리자가 되었습니다.',
          failed: '관리자를 맡지 못했습니다. 이미 다른 사람이 맡았을 수 있습니다.',
        },
        adminTabs: {
          channels: '채널',
          filterTerms: '금칙어',
          managers: '관리자',
        },
        filterScope:
          '글 제목·본문과 댓글에 들어 있으면 등록을 거절합니다(띄어쓰기·대소문자·기호 무시). AI 판정은 없고 첨부 이미지는 보지 않습니다.',
        managers: {
          description: '게시판 관리자는 채널·금칙어·전사 공지·공간을 관리합니다. 플랫폼 관리자 권한과는 별개입니다.',
          add: '관리자 추가',
          added: '{{name}} 님을 관리자로 추가했습니다.',
          addFailed: '관리자를 추가하지 못했습니다.',
          remove: '{{name}} 해제',
          removed: '{{name}} 님을 관리자에서 해제했습니다.',
          removeFailed: '관리자를 해제하지 못했습니다.',
          you: '나',
          empty: '관리자가 없습니다.',
        },
        spaces: {
          label: '공간',
          select: '공간 선택',
          none: '참여 중인 공간이 없습니다. 공간에 참여하면 그 공간의 공지가 여기에 보입니다.',
          create: '공간 만들기',
          createTitle: '새 공간',
          namePlaceholder: '공간 이름',
          descriptionPlaceholder: '공간 설명 (선택)',
          created: '"{{name}}" 공간을 만들었습니다.',
          createFailed: '공간을 만들지 못했습니다.',
          members: '멤버 관리',
          membersTitle: '{{name}} 멤버와 권한',
          delete: '공간 삭제',
          deleteConfirm: '"{{name}}" 공간과 그 공지를 모두 삭제할까요?',
          deleteFailed: '공간을 삭제하지 못했습니다.',
          close: '닫기',
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
      },""",
    'en': """      board: {
        unsavedChanges: 'You have unsaved changes. Leave anyway?',
        title: 'Board',
        notices: 'Announcements',
        admin: 'Board settings',
        notFound: 'The requested screen was not found.',
        loading: 'Loading...',
        loadFailed: 'Could not load the board.',
        managerRequired: 'Only board managers can see this.',
        bootstrap: {
          title: 'The board has no manager yet',
          body: 'The first person can take the role. Managers run channels, filter terms, company announcements and spaces, and can see who wrote anonymous and admin-only posts.',
          claim: 'Become the manager',
          claimed: 'You are now a board manager.',
          failed: 'Could not take the role. Someone else may have taken it.',
        },
        adminTabs: {
          channels: 'Channels',
          filterTerms: 'Filter terms',
          managers: 'Managers',
        },
        filterScope:
          'Post titles, bodies and comments containing a term are rejected (ignoring spaces, case and symbols). There is no AI judgment and attached images are not inspected.',
        managers: {
          description: 'Board managers run channels, filter terms, company announcements and spaces. This is separate from platform administration.',
          add: 'Add manager',
          added: '{{name}} is now a manager.',
          addFailed: 'Could not add the manager.',
          remove: 'Remove {{name}}',
          removed: '{{name}} is no longer a manager.',
          removeFailed: 'Could not remove the manager.',
          you: 'You',
          empty: 'No managers.',
        },
        spaces: {
          label: 'Space',
          select: 'Select space',
          none: 'You are not in any space. Announcements of spaces you join appear here.',
          create: 'New space',
          createTitle: 'New space',
          namePlaceholder: 'Space name',
          descriptionPlaceholder: 'Description (optional)',
          created: 'Created the space "{{name}}".',
          createFailed: 'Could not create the space.',
          members: 'Members',
          membersTitle: '{{name}} members and roles',
          delete: 'Delete space',
          deleteConfirm: 'Delete the space "{{name}}" and all its announcements?',
          deleteFailed: 'Could not delete the space.',
          close: 'Close',
          loadFailed: 'Could not load spaces.',
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

# AI-DO server catalog messages for the API error codes (`.` → `_`), see
# platform/api/api-error.ts. The announcement and admin messages name board
# roles instead of AI-DO's workspace/platform administrators; board_*,
# media_upload_failed and the acl_* codes are miy additions.
ERRORS = {
    'ko': {
        'community_channel_not_found': '커뮤니티 채널을 찾을 수 없습니다.',
        'community_channel_key_exists': '커뮤니티 채널 key가 이미 존재합니다.',
        'community_channel_delete_blocked': '기본 채널 또는 게시글이 있는 채널은 삭제할 수 없습니다. 비활성화를 사용하세요.',
        'community_channel_read_only': '읽기 전용 채널에는 글이나 댓글을 작성할 수 없습니다.',
        'community_post_not_found': '커뮤니티 글을 찾을 수 없습니다.',
        'community_post_number_busy': '같은 게시판에 다른 글이 등록되는 중입니다. 잠시 후 다시 등록해 주세요.',
        'community_comment_not_found': '커뮤니티 댓글을 찾을 수 없습니다.',
        'community_not_author': '작성자 또는 관리자만 변경할 수 있습니다.',
        'community_password_required': '비밀글에는 비밀번호가 필요합니다.',
        'community_invalid_password': '비밀번호가 올바르지 않습니다.',
        'community_admin_only_content': '관리자만 볼 수 있는 커뮤니티 글입니다.',
        'announcements_not_found': '공지사항을 찾을 수 없습니다.',
        'announcements_admin_required': '공지사항은 공간 관리자만 작성·수정·삭제할 수 있습니다.',
        'announcements_company_admin_required': '사내 주요 공지는 게시판 관리자만 작성·수정·삭제할 수 있습니다.',
        'announcements_space_required': '공간 공지에는 공간이 필요합니다.',
        'ugc_content_rejected': '운영 정책상 보낼 수 없는 표현이 포함되어 있습니다.',
        'ugc_filter_term_invalid': '금칙어는 1~{{max}}자로 입력해 주세요. 합자·네모 글자(예: ㍿)는 풀어 쓴 글자 수로 셉니다.',
        'ugc_filter_term_duplicate': '이미 등록된 금칙어입니다.',
        'ugc_filter_term_limit': '금칙어는 최대 {{limit}}개까지 등록할 수 있습니다.',
        'ugc_filter_term_not_found': '금칙어를 찾을 수 없습니다.',
        'media_unsupported_file_type': '지원하지 않는 이미지 형식입니다. PNG, JPEG, GIF, WebP만 올릴 수 있습니다.',
        'media_file_size_limit_exceeded': '파일 크기가 10MB를 넘습니다.',
        'media_upload_failed': '파일을 저장하지 못했습니다.',
        'board_manager_required': '게시판 관리자 권한이 필요합니다.',
        'board_manager_exists': '이미 게시판 관리자가 있습니다.',
        'board_manager_unknown_user': '디렉터리에서 찾을 수 없는 사용자입니다.',
        'board_manager_not_found': '게시판 관리자를 찾을 수 없습니다.',
        'board_last_manager': '마지막 게시판 관리자는 해제할 수 없습니다.',
        'board_space_not_found': '공간을 찾을 수 없습니다.',
        'acl_denied': '권한이 없습니다.',
        'validation_request_invalid': '요청 값이 올바르지 않습니다.',
    },
    'en': {
        'community_channel_not_found': 'Community channel not found.',
        'community_channel_key_exists': 'Community channel key already exists.',
        'community_channel_delete_blocked': 'Default channels or channels with posts cannot be deleted. Disable the channel instead.',
        'community_channel_read_only': 'Read-only community channels do not allow posts or comments.',
        'community_post_not_found': 'Community post not found.',
        'community_post_number_busy': 'Another post is being added to this channel. Try again shortly.',
        'community_comment_not_found': 'Community comment not found.',
        'community_not_author': 'Only the author or an administrator can change it.',
        'community_password_required': 'A password is required for secret posts.',
        'community_invalid_password': 'Password is invalid.',
        'community_admin_only_content': 'Only administrators can view this community post.',
        'announcements_not_found': 'Announcement not found.',
        'announcements_admin_required': 'Only space admins can create, edit, or delete announcements.',
        'announcements_company_admin_required': 'Only board managers can manage company-wide announcements.',
        'announcements_space_required': 'Space announcements need a space.',
        'ugc_content_rejected': 'This contains wording that is not allowed by the content rules.',
        'ugc_filter_term_invalid': 'Filter terms must be 1 to {{max}} characters. Ligatures and squared characters (such as ㍿) count as their expanded form.',
        'ugc_filter_term_duplicate': 'This filter term already exists.',
        'ugc_filter_term_limit': 'You can register up to {{limit}} filter terms.',
        'ugc_filter_term_not_found': 'The filter term was not found.',
        'media_unsupported_file_type': 'Unsupported image type. Only PNG, JPEG, GIF and WebP can be uploaded.',
        'media_file_size_limit_exceeded': 'The file is larger than 10 MB.',
        'media_upload_failed': 'The file could not be stored.',
        'board_manager_required': 'Board manager access required.',
        'board_manager_exists': 'A board manager already exists.',
        'board_manager_unknown_user': 'The person was not found in the directory.',
        'board_manager_not_found': 'The board manager was not found.',
        'board_last_manager': 'The last board manager cannot be removed.',
        'board_space_not_found': 'Space not found.',
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
    home = reindent(take(lang, 'home'), 0)
    return '\n'.join([
        f"  '{code}': {{",
        take(lang, 'common'),
        '    apps: {',
        take(lang, 'community'),
        '      home: {',
        home,
        '      },',
        '      admin: {',
        '        console: {',
        take(lang, 'channelAdmin'),
        '          dmSafety: {',
        take(lang, 'dmFailed'),
        take(lang, 'filterTitle'),
        take(lang, 'filterRest'),
        '          },',
        '        },',
        '      },',
        BOARD[lang],
        '    },',
        errors(lang),
        '  },',
    ])


text = '\n'.join([
    '// Generated from AI-DO apps/web/src/platform/i18n/resources.ts at 0002af062:',
    '// common, apps.community, the announcement keys of apps.home,',
    '// apps.admin.console.community and the filter-term keys of',
    '// apps.admin.console.dmSafety, Korean and English, copied verbatim. miy',
    '// additions: apps.board (board shell, managers, spaces, filter scope) and the',
    '// `errors` namespace (AI-DO server catalog messages for the API error codes,',
    '// see platform/api/api-error.ts). Regenerate with scripts/gen-i18n-resources.py.',
    'export const resources = {',
    locale('ko-KR', 'ko'),
    locale('en-US', 'en'),
    '} as const;',
    '',
])
out.write_text(text, encoding='utf-8')
print('wrote', out, len(text.split('\n')), 'lines')
