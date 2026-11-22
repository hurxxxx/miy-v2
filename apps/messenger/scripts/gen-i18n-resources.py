"""Regenerate web/src/platform/i18n/resources.ts from AI-DO.

Copies the common, apps.dm, apps.admin.console.dmSafety and
apps.pms.taskDetail.me entries (Korean and English) of AI-DO
apps/web/src/platform/i18n/resources.ts verbatim and adds the miy keys below.
The `errors` namespace holds AI-DO's server catalog messages
(apps/api/src/ai_do_api/core/i18n_catalog.py) for the API error codes, with
`.` replaced by `_`. Line ranges are those of AI-DO commit 0002af062; the
asserts stop if the source moved.

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


ko_common = block(3, 79)
en_common = block(19299, 19374)
ko_dm = block(6219, 6340)
en_dm = block(25645, 25771)
ko_safety = block(7888, 7920)
en_safety = block(27328, 27360)
ko_me = src[18634 - 1].strip()
en_me = src[38252 - 1].strip()

for name, text, first, last in [
    ('ko common', ko_common, '    common: {', '    },'),
    ('en common', en_common, '    common: {', '    },'),
    ('ko dm', ko_dm, '      dm: {', '      },'),
    ('en dm', en_dm, '      dm: {', '      },'),
    ('ko safety', ko_safety, '          dmSafety: {', '          },'),
    ('en safety', en_safety, '          dmSafety: {', '          },'),
]:
    lines = text.split('\n')
    assert lines[0] == first, (name, lines[0])
    assert lines[-1] == last, (name, lines[-1])
assert ko_me == "me: '나',", ko_me
assert en_me == "me: 'ME',", en_me

# miy keys, inserted right after `dm: {` so the AI-DO keys stay verbatim.
# typing*: the M4 typing indicator (AI-DO 0002af062 had none).
DM_ADDITIONS = {
    'ko': """        unsavedChanges: '보내지 않은 내용이나 진행 중인 작업이 있습니다. 이동하시겠습니까?',
        edit: '수정',
        edited: '수정됨',
        editing: '메시지 수정 중',
        cancelEdit: '수정 취소',
        saveEdit: '수정 저장',
        delete: '삭제',
        deleteTitle: '메시지를 삭제할까요?',
        deleteScope: '대화에는 "삭제된 메시지"만 남고 첨부파일도 함께 지워집니다. 되돌릴 수 없습니다.',
        deletedMessage: '삭제된 메시지',
        loadOlder: '이전 메시지 더 보기',
        restoreFailedDraft: '전송 실패한 초안 불러오기',
        mute: '이 대화 알림 끄기',
        unmute: '이 대화 알림 켜기',
        muted: '이 대화의 알림을 껐습니다.',
        unmuted: '이 대화의 알림을 켰습니다.',
        safetyReview: '신고 검토',
        typingOne: '{{name}} 님이 입력 중…',
        typingTwo: '{{first}} 님과 {{second}} 님이 입력 중…',
        typingMany: '{{count}}명이 입력 중…',
        actionErrors: {
          editFailed: '메시지를 수정하지 못했습니다.',
          deleteFailed: '메시지를 삭제하지 못했습니다.',
          muteFailed: '알림 설정을 바꾸지 못했습니다.',
          olderLoadFailed: '이전 메시지를 불러오지 못했습니다.',
        },
        manager: {
          title: '신고 검토',
          back: '대화로 돌아가기',
          noAccess: '메신저 관리자만 볼 수 있습니다.',
          claimTitle: '아직 메신저 관리자가 없습니다.',
          claimScope:
            '처음 맡는 사람이 신고 검토, 금칙어·이용규칙 설정과 다른 관리자 지정을 맡습니다. 설치한 운영 담당자가 바로 맡아 주세요.',
          claim: '관리자 맡기',
          claimed: '메신저 관리자를 맡았습니다.',
          managersTitle: '메신저 관리자',
          managerSearch: '관리자로 추가할 사용자 검색',
          managerRemove: '{{name}} 관리자 해제',
          managersFailed: '관리자를 바꾸지 못했습니다.',
          termsTitle: '사용자 작성 콘텐츠 이용규칙',
          termsVersion: '이용규칙 판',
          termsRequired: '동의해야 메시지·파일을 보낼 수 있게 하기',
          termsDocumentUrl: '이용규칙 문서 주소',
          termsContactEmail: '문의 메일',
          termsContactUrl: '문의 주소',
          termsSave: '저장',
          termsSaved: '이용규칙 설정을 저장했습니다.',
          termsFailed: '이용규칙 설정을 저장하지 못했습니다.',
          deletedBadge: '작성자가 삭제함',
          history: '수정·삭제 이력',
          historyEdit: '수정 전 내용',
          historyDelete: '삭제할 때 보존한 내용',
          historyRemoved: '(보존하지 않음)',
        },
""",
    'en': """        unsavedChanges: 'You have unsent text or work in progress. Leave anyway?',
        edit: 'Edit',
        edited: 'Edited',
        editing: 'Editing message',
        cancelEdit: 'Cancel edit',
        saveEdit: 'Save edit',
        delete: 'Delete',
        deleteTitle: 'Delete this message?',
        deleteScope: 'Only "Deleted message" stays in the conversation and its attachments are removed. This cannot be undone.',
        deletedMessage: 'Deleted message',
        loadOlder: 'Load older messages',
        restoreFailedDraft: 'Restore unsent draft',
        mute: 'Mute this conversation',
        unmute: 'Unmute this conversation',
        muted: 'Notifications for this conversation are off.',
        unmuted: 'Notifications for this conversation are on.',
        safetyReview: 'Report review',
        typingOne: '{{name}} is typing…',
        typingTwo: '{{first}} and {{second}} are typing…',
        typingMany: '{{count}} people are typing…',
        actionErrors: {
          editFailed: 'Could not edit the message.',
          deleteFailed: 'Could not delete the message.',
          muteFailed: 'Could not change the notification setting.',
          olderLoadFailed: 'Could not load older messages.',
        },
        manager: {
          title: 'Report review',
          back: 'Back to conversations',
          noAccess: 'Only messenger managers can open this page.',
          claimTitle: 'Messenger has no manager yet.',
          claimScope:
            'The first manager reviews reports, sets banned words and content rules, and appoints other managers. The operator who installed the app should take it right away.',
          claim: 'Become manager',
          claimed: 'You are now a messenger manager.',
          managersTitle: 'Messenger managers',
          managerSearch: 'Search people to add as manager',
          managerRemove: 'Remove {{name}} as manager',
          managersFailed: 'Could not change managers.',
          termsTitle: 'User content rules',
          termsVersion: 'Rules version',
          termsRequired: 'Require acceptance before sending messages or files',
          termsDocumentUrl: 'Rules document URL',
          termsContactEmail: 'Contact e-mail',
          termsContactUrl: 'Contact URL',
          termsSave: 'Save',
          termsSaved: 'Content rule settings saved.',
          termsFailed: 'Could not save the content rule settings.',
          deletedBadge: 'Deleted by the author',
          history: 'Edit and delete history',
          historyEdit: 'Before edit',
          historyDelete: 'Kept at deletion',
          historyRemoved: '(not kept)',
        },
""",
}

ERRORS = {
    # AI-DO core/i18n_catalog.py (single-brace params → i18next double braces).
    'validation_request_invalid': ('요청 값이 올바르지 않습니다.', 'Request validation failed.'),
    'dm_recipient_not_found': ('DM을 보낼 사용자를 찾을 수 없습니다.', 'DM recipient not found.'),
    'dm_recipients_not_found': ('그룹 DM 참여자 중 찾을 수 없는 사용자가 있습니다.', 'One or more group DM recipients were not found.'),
    'dm_group_participants_required': ('그룹 DM에는 본인을 제외하고 최소 2명의 참여자가 필요합니다.', 'A group DM requires at least two participants besides you.'),
    'dm_group_only': ('그룹 DM에서만 사용할 수 있는 기능입니다.', 'This action is only available for group DMs.'),
    'dm_participant_not_found': ('DM 참여자를 찾을 수 없습니다.', 'DM participant not found.'),
    'dm_manage_forbidden': ('이 그룹 DM을 관리할 권한이 없습니다.', 'You do not have permission to manage this group DM.'),
    'dm_thread_not_found': ('DM 대화를 찾을 수 없습니다.', 'DM thread not found.'),
    'dm_conversation_blocked': ('1:1 차단 상태라 이 사람과 1:1 대화를 할 수 없습니다.', '1:1 messaging with this person is blocked.'),
    'dm_safety_user_not_found': ('대상 사용자를 찾을 수 없습니다.', 'The user was not found.'),
    'dm_safety_report_target_invalid': ('신고할 수 없는 대상입니다.', 'This cannot be reported.'),
    'dm_safety_block_target_invalid': ('차단할 수 없는 대상입니다.', 'This user cannot be blocked.'),
    'dm_safety_block_limit_reached': ('1:1 차단은 최대 {{limit}}명까지 할 수 있습니다.', 'You can block up to {{limit}} people.'),
    'dm_safety_report_conflict': ('같은 신고 번호로 다른 내용이 들어왔습니다. 다시 신고해 주세요.', 'This report ID was already used with different details. Please report again.'),
    'dm_safety_report_rate_limited': ('신고가 너무 많습니다. 잠시 후 다시 시도해 주세요.', 'Too many reports. Please try again later.'),
    'dm_safety_report_not_found': ('신고를 찾을 수 없습니다.', 'The report was not found.'),
    'dm_safety_report_state_conflict': ('이미 처리된 신고입니다. 새로고침 후 확인해 주세요.', 'This report was already handled. Refresh and check again.'),
    'ugc_terms_acceptance_required': ('글·파일을 올리려면 먼저 사용자 작성 콘텐츠 이용규칙에 동의해 주세요.', 'Accept the user content rules before posting messages or files.'),
    'ugc_terms_version_mismatch': ('이용규칙이 바뀌었습니다. 새 이용규칙을 확인한 뒤 다시 동의해 주세요.', 'The content rules have changed. Review the new rules and accept again.'),
    'ugc_content_rejected': ('운영 정책상 보낼 수 없는 표현이 포함되어 있습니다.', 'This contains wording that is not allowed by the content rules.'),
    'ugc_filter_term_invalid': ('금칙어는 1~{{max}}자로 입력해 주세요. 합자·네모 글자(예: ㍿)는 풀어 쓴 글자 수로 셉니다.', 'Filter terms must be 1 to {{max}} characters. Ligatures and squared characters (such as ㍿) count as their expanded form.'),
    'ugc_filter_term_duplicate': ('이미 등록된 금칙어입니다.', 'This filter term already exists.'),
    'ugc_filter_term_limit': ('금칙어는 최대 {{limit}}개까지 등록할 수 있습니다.', 'You can register up to {{limit}} filter terms.'),
    'ugc_filter_term_not_found': ('금칙어를 찾을 수 없습니다.', 'The filter term was not found.'),
    'dm_safety_review_conflict': ('본인이 신고했거나 신고된 건은 검토할 수 없습니다.', 'You cannot review a report you filed or that concerns you.'),
    'dm_message_body_required': ('DM 메시지 또는 첨부파일을 입력해 주세요.', 'Enter a DM message or attach a file.'),
    'dm_message_not_found': ('답글을 달 DM 메시지를 찾을 수 없습니다.', 'The DM message to reply to was not found.'),
    'dm_attachment_not_found': ('DM 첨부파일을 찾을 수 없습니다.', 'DM attachment not found.'),
    'dm_attachment_empty': ('빈 파일은 첨부할 수 없습니다.', 'Empty files cannot be attached.'),
    'dm_attachment_size_limit_exceeded': ('DM 첨부파일은 {{limit_mb}}MB 이하만 업로드할 수 있습니다.', 'DM attachments must be {{limit_mb}} MB or smaller.'),
    'dm_attachment_upload_failed': ('DM 첨부파일 업로드에 실패했습니다.', 'DM attachment upload failed.'),
    'dm_attachment_already_sent': ('이미 전송된 DM 첨부파일입니다.', 'This DM attachment has already been sent.'),
    'dm_attachment_preview_unsupported': ('미리보기를 지원하지 않는 DM 첨부파일입니다.', 'This DM attachment cannot be previewed.'),
    'dm_attachment_download_failed': ('DM 첨부파일을 불러오지 못했습니다.', 'Could not load the DM attachment.'),
    # miy codes.
    'dm_directory_unavailable': ('사용자 목록을 불러오지 못했습니다. 잠시 후 다시 시도해 주세요.', 'The people directory is not available. Try again later.'),
    'dm_attachment_not_uploaded': ('첨부파일 업로드가 아직 끝나지 않았습니다.', 'The attachment has not finished uploading.'),
    'dm_attachment_already_uploaded': ('이미 업로드한 첨부파일입니다.', 'This attachment was already uploaded.'),
    'dm_attachment_size_mismatch': ('파일 크기가 처음 알린 크기와 다릅니다.', 'The file size does not match the reservation.'),
    'dm_attachment_request_conflict': ('같은 요청 번호로 다른 첨부파일이 들어왔습니다.', 'This request ID was used for another attachment.'),
    'dm_message_edit_forbidden': ('작성자만 메시지를 수정·삭제할 수 있습니다.', 'Only the author can edit or delete this message.'),
    'dm_message_not_editable': ('삭제했거나 관리자가 가린 메시지는 수정할 수 없습니다.', 'Deleted or hidden messages cannot be edited.'),
    'dm_safety_manager_required': ('메신저 관리자만 할 수 있습니다.', 'Only messenger managers can do this.'),
    'dm_safety_manager_already_assigned': ('이미 메신저 관리자가 있습니다.', 'Messenger managers are already assigned.'),
    'dm_safety_manager_not_found': ('관리자를 찾을 수 없습니다.', 'Manager not found.'),
    'dm_safety_last_manager': ('메신저 관리자는 한 명 이상 있어야 합니다.', 'At least one messenger manager must remain.'),
}


def quote(value):
    return '"' + value.replace('"', '\\"') + '"' if "'" in value else "'" + value + "'"


def errors(index):
    body = ''.join(f"      {key}: {quote(values[index])},\n" for key, values in ERRORS.items())
    return '    errors: {\n' + body + '    },'


def dm_with_additions(text, lang):
    head, rest = text.split('\n', 1)
    return head + '\n' + DM_ADDITIONS[lang] + rest


def locale(code, lang, common, dm, safety, me, index):
    return '\n'.join([
        f"  '{code}': {{",
        common,
        '    apps: {',
        dm_with_additions(dm, lang),
        '      admin: {',
        '        console: {',
        safety,
        '        },',
        '      },',
        '      pms: {',
        '        taskDetail: {',
        f'          {me}',
        '        },',
        '      },',
        '    },',
        errors(index),
        '  },',
    ])


text = '\n'.join([
    '// Generated from AI-DO apps/web/src/platform/i18n/resources.ts at 0002af062:',
    '// common, apps.dm, apps.admin.console.dmSafety and apps.pms.taskDetail.me,',
    '// Korean and English, copied verbatim. miy additions: the keys at the top of',
    '// apps.dm (edit/delete, older messages, mute, manager screen) and the `errors`',
    '// namespace (AI-DO server catalog messages for the API error codes, see',
    '// platform/api/api-error.ts). Regenerate with scripts/gen-i18n-resources.py.',
    'export const resources = {',
    locale('ko-KR', 'ko', ko_common, ko_dm, ko_safety, ko_me, 0),
    locale('en-US', 'en', en_common, en_dm, en_safety, en_me, 1),
    '} as const;',
    '',
])
out.write_text(text, encoding='utf-8')
print('wrote', out, len(text.split('\n')), 'lines')
