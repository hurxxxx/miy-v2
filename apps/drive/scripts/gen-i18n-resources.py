"""Regenerate web/src/platform/i18n/resources.ts from AI-DO.

Copies these blocks of AI-DO apps/web/src/platform/i18n/resources.ts
(Korean and English) verbatim and adds the miy keys below:

- common;
- apps.files (file manager, search, upload progress; the RAG corpus,
  ingress and chat keys come along unused).

Two AI-DO sentences name the workspace, which drive calls a space; they are
replaced (WORDING). The navigation labels `files-all` / `files-search`
(AI-DO shell.nav) are in apps.drive.nav. miy additions: apps.drive (spaces,
rename, move, versions, trash, cancel/retry, bundle download) and the
`errors` namespace (AI-DO server catalog messages for the API error codes,
see platform/api/api-error.ts).

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


RANGES = {
    'ko': {
        'common': (3, 79, '    common: {'),
        'files': (5824, 6120, '      files: {'),
        'nav': (518, 519, "        'files-all': '전체 파일',"),
    },
    'en': {
        'common': (19299, 19374, '    common: {'),
        'files': (25250, 25546, '      files: {'),
        'nav': (19817, 19818, "        'files-all': 'All Files',"),
    },
}

WORDING = {
    'ko': [
        ("          workspace: '워크스페이스 공유',", "          workspace: '공간 공유',"),
        (
            "            '이 워크스페이스에 파일이나 새 폴더를 추가할 권한이 없습니다.',",
            "            '이 공간에 파일이나 새 폴더를 추가할 권한이 없습니다.',",
        ),
    ],
    'en': [
        ("          workspace: 'Workspace shared',", "          workspace: 'Shared with space',"),
        (
            "            'You do not have permission to add files or folders in this workspace.',",
            "            'You do not have permission to add files or folders in this space.',",
        ),
    ],
}


def take(lang, name):
    first, last, head = RANGES[lang][name]
    text = block(first, last)
    assert text.split('\n')[0] == head, (lang, name, text.split('\n')[0])
    return text


for lang in RANGES:
    for name in ('common', 'files'):
        last = take(lang, name).split('\n')[-1]
        assert last.strip() == '},', (lang, name, last)

DRIVE = {
    'ko': """      drive: {
        unsavedChanges: '저장되지 않은 작업이나 진행 중인 업로드가 있습니다. 이동하시겠습니까?',
        title: '드라이브',
        retry: '다시 시도',
        nav: {
          all: '{{all}}',
          search: '{{search}}',
          trash: '휴지통',
        },
        spaces: {
          label: '공간 이름',
          select: '공간 선택',
          none: '참여 중인 공간이 없습니다. 공간을 만들거나 다른 사람이 공유하면 여기에서 파일을 관리할 수 있습니다.',
          create: '공간 만들기',
          createTitle: '새 공간',
          namePlaceholder: '예: 설계팀 자료',
          created: '"{{name}}" 공간을 만들었습니다.',
          createFailed: '공간을 만들지 못했습니다.',
          rename: '공간 설정',
          renameTitle: '공간 설정',
          renameFailed: '공간 이름을 바꾸지 못했습니다.',
          members: '멤버 관리',
          membersTitle: '{{name}} 멤버와 권한',
          membersHint: '사용자나 그룹을 이름으로 찾아 추가합니다. 보기는 공유된 파일을 열고, 멤버는 파일과 폴더를 올리며, 관리자는 모든 파일과 멤버를 관리합니다.',
          delete: '공간 삭제',
          deleteConfirm: '"{{name}}" 공간과 그 안의 모든 파일·폴더·휴지통을 영구히 삭제할까요? 되돌릴 수 없습니다.',
          deleteFailed: '공간을 삭제하지 못했습니다.',
          loadFailed: '공간을 불러오지 못했습니다.',
        },
        actions: {
          rename: '이름 바꾸기',
          move: '이동',
          versions: '버전',
          uploadVersion: '새 버전 올리기',
          restore: '복원',
          deleteForever: '영구 삭제',
          emptyTrash: '휴지통 비우기',
        },
        rename: {
          title: '이름 바꾸기',
          label: '파일 이름',
          failed: '이름을 바꾸지 못했습니다.',
        },
        move: {
          title_one: '{{count}}개 항목 이동',
          title_other: '{{count}}개 항목 이동',
          target: '옮길 위치를 고르세요.',
          submit: '여기로 이동',
          failed: '이동하지 못했습니다.',
          done_one: '{{count}}개 항목을 옮겼습니다.',
          done_other: '{{count}}개 항목을 옮겼습니다.',
        },
        versions: {
          title: '{{name}} 버전',
          current: '현재',
          version: '버전 {{number}}',
          uploadedBy: '{{name}} · {{date}}',
          empty: '버전이 없습니다.',
          loadFailed: '버전을 불러오지 못했습니다.',
        },
        trash: {
          title: '휴지통',
          description: '삭제한 항목은 영구 삭제하기 전까지 여기에 남습니다. 복원하면 원래 폴더로 돌아가고, 그 폴더가 없으면 전체 파일로 돌아갑니다.',
          empty: '휴지통이 비어 있습니다.',
          loadFailed: '휴지통을 불러오지 못했습니다.',
          restored: '복원했습니다.',
          restoreFailed: '복원하지 못했습니다.',
          deleted: '영구 삭제했습니다.',
          deleteFailed: '영구 삭제하지 못했습니다.',
          confirmDelete_one: '선택한 {{count}}개 항목을 영구 삭제할까요? 되돌릴 수 없습니다.',
          confirmDelete_other: '선택한 {{count}}개 항목을 영구 삭제할까요? 되돌릴 수 없습니다.',
          confirmEmpty: '휴지통의 항목을 모두 영구 삭제할까요? 되돌릴 수 없습니다.',
          deletedAt: '삭제 {{date}}',
          deletedBy: '{{name}} 님이 삭제',
          location: '원래 위치: {{path}}',
          folderFiles_one: '파일 {{count}}개',
          folderFiles_other: '파일 {{count}}개',
          select: '{{name}} 선택',
        },
        upload: {
          cancelAction: '업로드 취소',
          retryAction: '실패한 파일부터 다시 올리기',
          cancelled: '업로드를 취소했습니다. 이미 올린 파일은 남아 있습니다.',
        },
        bulkDownload: {
          started_one: '{{count}}개 파일 다운로드를 시작합니다.',
          started_other: '{{count}}개 파일을 하나씩 내려받습니다.',
        },
        download: {
          unavailable: '이 화면에서는 다운로드를 시작할 수 없습니다. 드라이브 앱에서 다시 시도해 주세요.',
        },
        search: {
          description: '파일 이름과 소유자·폴더·형식으로 접근 가능한 파일을 찾습니다.',
          queryPlaceholder: '파일 이름이나 소유자로 검색',
          methods: {
            filename: '파일 이름',
            metadata: '메타데이터',
          },
        },
      },""",
    'en': """      drive: {
        unsavedChanges: 'You have unsaved changes or uploads in progress. Leave anyway?',
        title: 'Drive',
        retry: 'Retry',
        nav: {
          all: '{{all}}',
          search: '{{search}}',
          trash: 'Trash',
        },
        spaces: {
          label: 'Space name',
          select: 'Select a space',
          none: 'You are not in any space yet. Create one, or ask someone to share theirs, to manage files here.',
          create: 'New space',
          createTitle: 'New space',
          namePlaceholder: 'e.g. Design team files',
          created: 'Created the "{{name}}" space.',
          createFailed: 'Could not create the space.',
          rename: 'Space settings',
          renameTitle: 'Space settings',
          renameFailed: 'Could not rename the space.',
          members: 'Members',
          membersTitle: '{{name}} members and roles',
          membersHint: 'Find people or groups by name. Viewers open shared files, members upload files and folders, and admins manage every file and member.',
          delete: 'Delete space',
          deleteConfirm: 'Permanently delete the "{{name}}" space with all its files, folders and trash? This cannot be undone.',
          deleteFailed: 'Could not delete the space.',
          loadFailed: 'Could not load spaces.',
        },
        actions: {
          rename: 'Rename',
          move: 'Move',
          versions: 'Versions',
          uploadVersion: 'Upload new version',
          restore: 'Restore',
          deleteForever: 'Delete forever',
          emptyTrash: 'Empty trash',
        },
        rename: {
          title: 'Rename',
          label: 'File name',
          failed: 'Could not rename.',
        },
        move: {
          title_one: 'Move {{count}} item',
          title_other: 'Move {{count}} items',
          target: 'Choose where to move.',
          submit: 'Move here',
          failed: 'Could not move.',
          done_one: 'Moved {{count}} item.',
          done_other: 'Moved {{count}} items.',
        },
        versions: {
          title: 'Versions of {{name}}',
          current: 'Current',
          version: 'Version {{number}}',
          uploadedBy: '{{name}} · {{date}}',
          empty: 'No versions.',
          loadFailed: 'Could not load versions.',
        },
        trash: {
          title: 'Trash',
          description: 'Deleted items stay here until you delete them forever. Restoring returns an item to its folder, or to All Files when that folder is gone.',
          empty: 'The trash is empty.',
          loadFailed: 'Could not load the trash.',
          restored: 'Restored.',
          restoreFailed: 'Could not restore.',
          deleted: 'Deleted forever.',
          deleteFailed: 'Could not delete forever.',
          confirmDelete_one: 'Delete the selected item forever? This cannot be undone.',
          confirmDelete_other: 'Delete the {{count}} selected items forever? This cannot be undone.',
          confirmEmpty: 'Delete everything in the trash forever? This cannot be undone.',
          deletedAt: 'Deleted {{date}}',
          deletedBy: 'by {{name}}',
          location: 'From: {{path}}',
          folderFiles_one: '{{count}} file',
          folderFiles_other: '{{count}} files',
          select: 'Select {{name}}',
        },
        upload: {
          cancelAction: 'Cancel upload',
          retryAction: 'Retry from the failed file',
          cancelled: 'Upload cancelled. Files already uploaded stay.',
        },
        bulkDownload: {
          started_one: 'Starting the download of {{count}} file.',
          started_other: 'Downloading {{count}} files one by one.',
        },
        download: {
          unavailable: 'Downloads cannot start from this screen. Try again in the Drive app.',
        },
        search: {
          description: 'Find files you can open by name, owner, folder or type.',
          queryPlaceholder: 'Search by file name or owner',
          methods: {
            filename: 'File name',
            metadata: 'Metadata',
          },
        },
      },""",
}

ERRORS = {
    'ko': {
        'files_invalid_visibility': '지원하지 않는 파일 공개 범위입니다.',
        'files_name_required': '이름을 입력해야 합니다.',
        'files_folder_not_found': '폴더를 찾을 수 없습니다.',
        'files_file_not_found': '파일을 찾을 수 없습니다.',
        'files_file_access_required': '파일 접근 권한이 필요합니다.',
        'files_delete_access_required': '삭제 또는 수정 권한이 필요합니다.',
        'files_parent_manage_access_required': '상위 폴더를 관리할 권한이 필요합니다.',
        'files_visibility_change_not_allowed': '폴더 권한은 생성 후 변경할 수 없습니다.',
        'files_file_size_limit_exceeded': '파일은 {{limit_mb}} MB 이하만 업로드할 수 있습니다.',
        'files_archive_limit_exceeded': '묶음 다운로드는 최대 {{limit_files}}개 파일까지 가능합니다.',
        'files_storage_upload_failed': '파일 저장소 업로드에 실패했습니다.',
        'files_storage_download_failed': '파일 저장소 다운로드에 실패했습니다.',
        'files_preview_unsupported_type': '이 파일 형식은 바로보기를 지원하지 않습니다.',
        'files_selection_required': '파일 또는 폴더를 하나 이상 선택해야 합니다.',
        'files_space_not_found': '공간을 찾을 수 없습니다.',
        'files_workspace_access_required': '이 공간에 파일이나 새 폴더를 추가할 권한이 없습니다.',
        'files_move_into_itself': '폴더를 자기 자신이나 하위 폴더로 옮길 수 없습니다.',
        'files_scope_changed': '처리하는 동안 선택한 항목이 바뀌었습니다. 다시 시도해 주세요.',
        'files_trash_item_not_found': '휴지통에 없는 항목입니다.',
        'files_download_frame_required': '다운로드는 드라이브 화면에서 시작해야 합니다.',
        'files_preview_too_large': '10MB가 넘는 이미지는 미리보기 대신 다운로드해 주세요.',
        'files_storage_quota_exceeded': '이 앱의 저장 한도에 도달했습니다. 휴지통을 비우거나 관리자에게 한도를 문의하세요.',
        'files_storage_unavailable': '파일 저장 공간이 부족합니다. 잠시 후 다시 시도해 주세요.',
        'files_upload_result_unknown': '저장소의 응답을 받지 못했습니다. 다시 시도하면 같은 업로드를 이어 갑니다.',
        'files_upload_not_found': '업로드를 찾을 수 없습니다.',
        'files_upload_cancelled': '취소된 업로드입니다.',
        'files_upload_completed': '이미 완료된 업로드입니다.',
        'files_upload_incomplete': '아직 모든 내용이 올라가지 않았습니다.',
        'files_upload_transfer_mismatch': '이 업로드에 맞지 않는 전송 방식입니다.',
        'files_part_offset': '이어 올릴 위치가 맞지 않습니다. 다시 시도해 주세요.',
        'files_part_too_large': '한 번에 보낼 수 있는 조각은 8MB까지입니다.',
        'files_search_strategy_unsupported': '파일 이름·메타데이터 검색만 사용할 수 있습니다.',
        'acl_denied': '권한이 없습니다.',
        'acl_conflict': '다른 사람이 먼저 권한을 바꿨습니다. 다시 불러온 뒤 시도해 주세요.',
        'acl_directory_unavailable': '디렉터리를 확인하지 못했습니다. 잠시 후 다시 시도해 주세요.',
        'validation_request_invalid': '요청 값이 올바르지 않습니다.',
    },
    'en': {
        'files_invalid_visibility': 'Unsupported file visibility.',
        'files_name_required': 'Name is required.',
        'files_folder_not_found': 'Folder not found.',
        'files_file_not_found': 'File not found.',
        'files_file_access_required': 'File access required.',
        'files_delete_access_required': 'Delete or update access required.',
        'files_parent_manage_access_required': 'Parent folder manage access required.',
        'files_visibility_change_not_allowed': 'Folder visibility cannot be changed after creation.',
        'files_file_size_limit_exceeded': 'Files must be {{limit_mb}} MB or smaller.',
        'files_archive_limit_exceeded': 'Bundle downloads are limited to {{limit_files}} files.',
        'files_storage_upload_failed': 'File storage upload failed.',
        'files_storage_download_failed': 'File storage download failed.',
        'files_preview_unsupported_type': 'This file type cannot be previewed.',
        'files_selection_required': 'Select at least one file or folder.',
        'files_space_not_found': 'Space not found.',
        'files_workspace_access_required': 'You do not have permission to add files or folders in this space.',
        'files_move_into_itself': 'A folder cannot be moved into itself or one of its subfolders.',
        'files_scope_changed': 'The selection changed while it was being processed. Try again.',
        'files_trash_item_not_found': 'The item is not in the trash.',
        'files_download_frame_required': 'Downloads start from the Drive screen.',
        'files_preview_too_large': 'Images larger than 10 MB cannot be previewed. Download the file instead.',
        'files_storage_quota_exceeded': 'The storage limit of this app has been reached. Empty the trash or ask an administrator about the limit.',
        'files_storage_unavailable': 'File storage is running low. Try again later.',
        'files_upload_result_unknown': 'The storage answer was lost. Retrying continues the same upload.',
        'files_upload_not_found': 'Upload not found.',
        'files_upload_cancelled': 'This upload was cancelled.',
        'files_upload_completed': 'This upload has already completed.',
        'files_upload_incomplete': 'Not all of the file has arrived yet.',
        'files_upload_transfer_mismatch': 'This upload expects another transfer.',
        'files_part_offset': 'The upload resumed at the wrong position. Try again.',
        'files_part_too_large': 'A part is at most 8 MB.',
        'files_search_strategy_unsupported': 'Only name and metadata search is available.',
        'acl_denied': 'You do not have access.',
        'acl_conflict': 'Someone changed the access first. Reload and try again.',
        'acl_directory_unavailable': 'The directory could not be checked. Try again later.',
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


def files_block(lang):
    text = take(lang, 'files')
    for before, after in WORDING[lang]:
        assert text.count(before) == 1, (lang, before)
        text = text.replace(before, after)
    return text


def drive_block(lang):
    nav = take(lang, 'nav').split('\n')
    all_label = nav[0].split(': ', 1)[1].rstrip(',')
    search_label = nav[1].split(': ', 1)[1].rstrip(',')
    assert nav[1].strip().startswith("'files-search'"), nav[1]
    return (
        DRIVE[lang]
        .replace("'{{all}}'", all_label)
        .replace("'{{search}}'", search_label)
    )


def locale(code, lang):
    return '\n'.join([
        f"  '{code}': {{",
        take(lang, 'common'),
        '    apps: {',
        files_block(lang),
        drive_block(lang),
        '    },',
        errors(lang),
        '  },',
    ])


text = '\n'.join([
    '// Generated from AI-DO apps/web/src/platform/i18n/resources.ts at 0002af062:',
    '// common and apps.files, Korean and English, copied verbatim apart from two',
    '// sentences that name the workspace (a drive space). miy additions:',
    '// apps.drive (AI-DO shell nav labels, spaces, rename, move, versions, trash,',
    '// upload cancel/retry, bundle download) and the `errors` namespace (AI-DO',
    '// server catalog messages for the API error codes, see',
    '// platform/api/api-error.ts). Regenerate with scripts/gen-i18n-resources.py.',
    'export const resources = {',
    locale('ko-KR', 'ko'),
    locale('en-US', 'en'),
    '} as const;',
    '',
])
out.write_text(text, encoding='utf-8')
print('wrote', out, len(text.split('\n')), 'lines')
