"""Regenerate web/src/platform/i18n/resources.ts from AI-DO.

Copies, Korean and English, verbatim from AI-DO
apps/web/src/platform/i18n/resources.ts:
  - common
  - shell.searchSpace and shell.appRuntime (task picker, notices)
  - the shell.nav titles of PMS's own sidebar items
  - apps.pms
and builds the `errors` namespace from AI-DO's server catalog
apps/api/src/ai_do_api/core/i18n_catalog.py (every `pms.*` code plus
auth.user_not_found, platform.app_access_required, validation.request_invalid;
the miy backend answers AI-DO's dotted code with `.` replaced by `_`). The miy
additions are listed in MIY_PMS, MIY_TASK_DETAIL and MIY_ERRORS below. The block starts are those of AI-DO
commit 0002af062; the asserts stop if the source moved.

    python3 scripts/gen-i18n-resources.py \
        /projects/ai-do/prod/apps/web/src/platform/i18n/resources.ts \
        /projects/ai-do/prod/apps/api/src/ai_do_api/core/i18n_catalog.py \
        web/src/platform/i18n/resources.ts
"""
import importlib.util
import re
import sys
from pathlib import Path

src = Path(sys.argv[1]).read_text(encoding='utf-8').split('\n')
catalog_path = Path(sys.argv[2])
out = Path(sys.argv[3])


def block(first_line, expected):
    """The object block starting at 1-based `first_line` through its closing brace."""
    head = src[first_line - 1]
    assert head == expected, (first_line, head, expected)
    indent = len(head) - len(head.lstrip(' '))
    close = ' ' * indent + '},'
    for index in range(first_line, len(src)):
        if src[index] == close:
            return '\n'.join(src[first_line - 1:index + 1])
    raise AssertionError(('unclosed', first_line))


def line(number, expected):
    text = src[number - 1]
    assert text.strip() == expected, (number, text, expected)
    return text


def reindent(text, delta):
    return '\n'.join((' ' * delta + item) if item.strip() else '' for item in text.split('\n'))


BLOCKS = {
    'ko': {
        'common': block(3, '    common: {'),
        'searchSpace': block(81, '      searchSpace: {'),
        'appRuntime': block(247, '      appRuntime: {'),
        'nav': [line(562, "'pms-tasks-assigned': '내게 배정됨',"), line(563, "'pms-tasks-today': '오늘 및 지연',")],
        'pms': block(18144, '      pms: {'),
    },
    'en': {
        'common': block(19299, '    common: {'),
        'searchSpace': block(19376, '      searchSpace: {'),
        'appRuntime': block(19544, '      appRuntime: {'),
        'nav': [line(19861, "'pms-tasks-assigned': 'Assigned to me',"), line(19862, "'pms-tasks-today': 'Today & Overdue',")],
        'pms': block(37738, '      pms: {'),
    },
}

# miy additions inside apps.pms (inserted right after `pms: {` so the AI-DO keys stay verbatim).
MIY_PMS = {
    'ko': """        unsavedChanges: '저장되지 않은 작업이 있습니다. 이동하시겠습니까?',
        sidebarLabel: 'PMS 탐색',
        attachmentPreviewFailed: '첨부파일을 불러오지 못했습니다.',
        milestoneSettings: {
          tab: '마일스톤',
          heading: '마일스톤',
          hint: '이 리스트의 마일스톤과 스페이스 전체 마일스톤입니다. 스페이스 전체 마일스톤은 스페이스의 모든 리스트에서 쓸 수 있습니다.',
          empty: '마일스톤이 없습니다.',
          titlePlaceholder: '마일스톤 이름',
          add: '마일스톤 추가',
          scopeLabel: '범위',
          scopeList: '이 리스트',
          scopeSpace: '스페이스 전체',
          statusLabel: '상태',
          status: {
            planned: '예정',
            active: '진행 중',
            complete: '완료',
          },
          startDate: '시작일',
          dueDate: '마감일',
          noDueDate: '마감일 없음',
          progress: '{{done}}/{{total}} 완료',
          edit: '{{name}} 수정',
          delete: '{{name}} 삭제',
          deleteTitle: '마일스톤 삭제',
          deleteConfirm: '"{{name}}" 마일스톤을 삭제할까요? 연결된 태스크는 남고 마일스톤만 비워집니다.',
          readOnly: '스페이스 소유자와 관리자만 마일스톤을 바꿀 수 있습니다.',
          calendarTitle: '마일스톤: {{title}}',
          ganttRow: '마일스톤',
          spaceBadge: '스페이스',
          errors: {
            loadFailed: '마일스톤을 불러오지 못했습니다.',
            createFailed: '마일스톤을 만들지 못했습니다.',
            updateFailed: '마일스톤을 저장하지 못했습니다.',
            deleteFailed: '마일스톤을 삭제하지 못했습니다.',
          },
        },
        customFields: {
          tab: '필드',
          heading: '사용자 정의 필드',
          hint: '이 리스트의 태스크에 더 기록할 항목입니다. 표 보기의 열로 보이고 필터와 정렬에 쓸 수 있습니다.',
          empty: '사용자 정의 필드가 없습니다.',
          namePlaceholder: '필드 이름',
          typeLabel: '형식',
          types: {
            text: '텍스트',
            number: '숫자',
            date: '날짜',
            select: '단일 선택',
            multi_select: '다중 선택',
            user: '사용자',
          },
          options: '선택지',
          optionsPlaceholder: '선택지 입력 후 Enter',
          optionsRequired: '선택지를 하나 이상 추가하세요.',
          addOption: '선택지 추가',
          removeOption: '{{name}} 선택지 제거',
          renameOption: '{{name}} 선택지 이름',
          add: '필드 추가',
          edit: '{{name}} 수정',
          delete: '{{name}} 삭제',
          moveUp: '{{name}} 위로',
          moveDown: '{{name}} 아래로',
          deleteTitle: '필드 삭제',
          deleteConfirm: '"{{name}}" 필드를 삭제할까요? 모든 태스크에 기록된 값도 함께 삭제됩니다.',
          optionsRemovedTitle: '선택지 제거',
          optionsRemovedConfirm: '없앤 선택지를 고른 태스크에서는 그 값이 지워집니다. 저장할까요?',
          readOnly: '스페이스 소유자와 관리자만 필드를 바꿀 수 있습니다.',
          emptyValue: '비어 있음',
          choose: '선택',
          clearValue: '{{name}} 값 지우기',
          filterLabel: '필드',
          filterPlaceholder: '값 입력',
          pill: '{{name}}: {{value}}',
          sortBy: '{{name}} 기준 정렬',
          errors: {
            loadFailed: '필드를 불러오지 못했습니다.',
            createFailed: '필드를 만들지 못했습니다.',
            updateFailed: '필드를 저장하지 못했습니다.',
            deleteFailed: '필드를 삭제하지 못했습니다.',
            reorderFailed: '필드 순서를 저장하지 못했습니다.',
            valueFailed: '값을 저장하지 못했습니다.',
          },
        },
        templateSettings: {
          tab: '템플릿',
          heading: '태스크 템플릿',
          hint: '스페이스의 모든 리스트에서 새 태스크를 만들 때 쓸 수 있습니다. 태스크 상세의 "템플릿으로 저장"으로도 만들 수 있습니다.',
          empty: '템플릿이 없습니다.',
          create: '새 템플릿',
          manage: '템플릿 관리',
          name: '템플릿 이름',
          taskTitle: '태스크 제목',
          taskTitleHint: '비워 두면 템플릿 이름을 씁니다.',
          priority: '우선순위',
          status: '시작 상태',
          description: '설명',
          checklist: '체크리스트',
          checklistPlaceholder: '항목 입력 후 Enter',
          labels: '라벨',
          labelsHint: '태스크를 만들 리스트에 같은 이름의 라벨이 있으면 붙습니다.',
          subtasks: '하위 태스크',
          subtaskPlaceholder: '하위 태스크 제목 입력 후 Enter',
          subtaskDetails: '체크리스트 {{checklist}}개 · 라벨 {{labels}}개',
          removeItem: '{{name}} 제거',
          summary: '하위 태스크 {{subtasks}}개 · 체크리스트 {{checklist}}개',
          edit: '{{name}} 수정',
          delete: '{{name}} 삭제',
          deleteTitle: '템플릿 삭제',
          deleteConfirm: '"{{name}}" 템플릿을 삭제할까요? 이미 만든 태스크는 그대로입니다.',
          readOnly: '뷰어는 템플릿을 바꿀 수 없습니다.',
          saveAsTemplate: '템플릿으로 저장',
          saveAsTemplateDescription: '이 태스크의 설명, 체크리스트, 라벨, 하위 태스크를 스페이스 템플릿으로 저장합니다. 담당자, 날짜, 첨부, 댓글은 저장하지 않습니다.',
          saved: '템플릿 "{{name}}"을(를) 저장했습니다.',
          applied: '템플릿: {{name}}',
          appliedDetails: '만들 때 하위 태스크 {{subtasks}}개, 체크리스트 {{checklist}}개, 라벨 {{labels}}개가 함께 추가됩니다.',
          removeApplied: '템플릿 적용 해제',
          errors: {
            loadFailed: '템플릿을 불러오지 못했습니다.',
            saveFailed: '템플릿을 저장하지 못했습니다.',
            deleteFailed: '템플릿을 삭제하지 못했습니다.',
          },
        },
        savedFilters: {
          share: '스페이스에 공유',
          shared: '공유됨',
          sharedBy: '{{name}} 님이 공유',
          shareAction: '{{label}} 스페이스에 공유',
          unshareAction: '{{label}} 공유 해제',
          rename: '{{label}} 이름 바꾸기',
          update: '{{label}}을(를) 현재 조건으로 바꾸기',
          loading: '저장된 필터를 불러오는 중...',
          errors: {
            loadFailed: '저장된 필터를 불러오지 못했습니다.',
            saveFailed: '필터를 저장하지 못했습니다.',
            deleteFailed: '필터를 삭제하지 못했습니다.',
            defaultFailed: '기본 필터를 바꾸지 못했습니다.',
          },
        },
""",
    'en': """        unsavedChanges: 'You have unsaved changes. Leave anyway?',
        sidebarLabel: 'PMS navigation',
        attachmentPreviewFailed: 'Could not load the attachment.',
        milestoneSettings: {
          tab: 'Milestones',
          heading: 'Milestones',
          hint: 'Milestones of this list and of the whole space. A space-wide milestone can be used in every list of the space.',
          empty: 'No milestones yet.',
          titlePlaceholder: 'Milestone name',
          add: 'Add milestone',
          scopeLabel: 'Scope',
          scopeList: 'This list',
          scopeSpace: 'Whole space',
          statusLabel: 'Status',
          status: {
            planned: 'Planned',
            active: 'Active',
            complete: 'Complete',
          },
          startDate: 'Start date',
          dueDate: 'Due date',
          noDueDate: 'No due date',
          progress: '{{done}}/{{total}} done',
          edit: 'Edit {{name}}',
          delete: 'Delete {{name}}',
          deleteTitle: 'Delete milestone',
          deleteConfirm: 'Delete the milestone "{{name}}"? Its tasks stay; only the milestone is removed from them.',
          readOnly: 'Only space owners and admins can change milestones.',
          calendarTitle: 'Milestone: {{title}}',
          ganttRow: 'Milestones',
          spaceBadge: 'Space',
          errors: {
            loadFailed: 'Could not load milestones.',
            createFailed: 'Could not create the milestone.',
            updateFailed: 'Could not save the milestone.',
            deleteFailed: 'Could not delete the milestone.',
          },
        },
        customFields: {
          tab: 'Fields',
          heading: 'Custom fields',
          hint: 'Extra details recorded on the tasks of this list. They appear as columns in the table view and can be used to filter and sort.',
          empty: 'No custom fields yet.',
          namePlaceholder: 'Field name',
          typeLabel: 'Type',
          types: {
            text: 'Text',
            number: 'Number',
            date: 'Date',
            select: 'Single select',
            multi_select: 'Multi select',
            user: 'Person',
          },
          options: 'Options',
          optionsPlaceholder: 'Type an option and press Enter',
          optionsRequired: 'Add at least one option.',
          addOption: 'Add option',
          removeOption: 'Remove option {{name}}',
          renameOption: 'Name of option {{name}}',
          add: 'Add field',
          edit: 'Edit {{name}}',
          delete: 'Delete {{name}}',
          moveUp: 'Move {{name}} up',
          moveDown: 'Move {{name}} down',
          deleteTitle: 'Delete field',
          deleteConfirm: 'Delete the field "{{name}}"? The values recorded on every task are deleted too.',
          optionsRemovedTitle: 'Remove options',
          optionsRemovedConfirm: 'Tasks that chose a removed option lose that value. Save?',
          readOnly: 'Only space owners and admins can change fields.',
          emptyValue: 'Empty',
          choose: 'Choose',
          clearValue: 'Clear {{name}}',
          filterLabel: 'Fields',
          filterPlaceholder: 'Enter a value',
          pill: '{{name}}: {{value}}',
          sortBy: 'Sort by {{name}}',
          errors: {
            loadFailed: 'Could not load the fields.',
            createFailed: 'Could not create the field.',
            updateFailed: 'Could not save the field.',
            deleteFailed: 'Could not delete the field.',
            reorderFailed: 'Could not save the field order.',
            valueFailed: 'Could not save the value.',
          },
        },
        templateSettings: {
          tab: 'Templates',
          heading: 'Task templates',
          hint: 'Available when creating a task in any list of the space. You can also make one with "Save as template" in a task.',
          empty: 'No templates yet.',
          create: 'New template',
          manage: 'Manage templates',
          name: 'Template name',
          taskTitle: 'Task title',
          taskTitleHint: 'Leave empty to use the template name.',
          priority: 'Priority',
          status: 'Starting status',
          description: 'Description',
          checklist: 'Checklist',
          checklistPlaceholder: 'Type an item and press Enter',
          labels: 'Labels',
          labelsHint: 'Added when the list has a label with the same name.',
          subtasks: 'Subtasks',
          subtaskPlaceholder: 'Type a subtask title and press Enter',
          subtaskDetails: '{{checklist}} checklist items · {{labels}} labels',
          removeItem: 'Remove {{name}}',
          summary: '{{subtasks}} subtasks · {{checklist}} checklist items',
          edit: 'Edit {{name}}',
          delete: 'Delete {{name}}',
          deleteTitle: 'Delete template',
          deleteConfirm: 'Delete the template "{{name}}"? Tasks already made from it stay as they are.',
          readOnly: 'Viewers cannot change templates.',
          saveAsTemplate: 'Save as template',
          saveAsTemplateDescription: "Saves this task's description, checklist, labels and subtasks as a template of the space. Assignees, dates, attachments and comments are not saved.",
          saved: 'Saved the template "{{name}}".',
          applied: 'Template: {{name}}',
          appliedDetails: 'Creating adds {{subtasks}} subtasks, {{checklist}} checklist items and {{labels}} labels.',
          removeApplied: 'Stop using the template',
          errors: {
            loadFailed: 'Could not load templates.',
            saveFailed: 'Could not save the template.',
            deleteFailed: 'Could not delete the template.',
          },
        },
        savedFilters: {
          share: 'Share with the space',
          shared: 'Shared',
          sharedBy: 'Shared by {{name}}',
          shareAction: 'Share {{label}} with the space',
          unshareAction: 'Stop sharing {{label}}',
          rename: 'Rename {{label}}',
          update: 'Replace {{label}} with the current conditions',
          loading: 'Loading saved filters...',
          errors: {
            loadFailed: 'Could not load saved filters.',
            saveFailed: 'Could not save the filter.',
            deleteFailed: 'Could not delete the filter.',
            defaultFailed: 'Could not change the default filter.',
          },
        },
""",
}

# miy additions inside apps.pms.taskDetail (after `commentPlaceholder`).
MIY_TASK_DETAIL = {
    'ko': """          sendComment: '댓글 보내기',
          retryAttachments: '첨부 {{count}}개 다시 시도',
""",
    'en': """          sendComment: 'Send comment',
          retryAttachments: 'Retry uploads ({{count}})',
""",
}


def pms_with_additions(text, lang):
    head, rest = text.split('\n', 1)
    marker = '\n          commentPlaceholder: '
    at = rest.index(marker)
    line_end = rest.index('\n', at + 1)
    assert rest.count(marker) == 1, 'commentPlaceholder moved'
    rest = rest[:line_end + 1] + MIY_TASK_DETAIL[lang] + rest[line_end + 1:]
    return head + '\n' + MIY_PMS[lang] + rest


def load_catalog():
    spec = importlib.util.spec_from_file_location('aido_i18n_catalog', catalog_path)
    module = importlib.util.module_from_spec(spec)
    sys.modules['aido_i18n_catalog'] = module  # dataclasses need the module registered
    spec.loader.exec_module(module)
    return module.MESSAGES


MESSAGES = load_catalog()
CODES = sorted(key for key in MESSAGES if key.startswith('pms.')) + [
    'auth.user_not_found',
    'platform.app_access_required',
    'validation.request_invalid',
]
# runtime.api keeps only the error code, so per-request values are unknown on
# the screen. The limit is a constant (api-error.ts fills it); the other two
# sentences lose their value list, as the planner did for its two such codes.
PARAMLESS = {
    'pms.status_in_use': {
        'ko-KR': '상태를 삭제할 수 없습니다. 태스크가 사용 중입니다.',
        'en-US': 'Cannot delete status: task(s) are using it.',
    },
    'pms.status_mode_incompatible': {
        'ko-KR': 'Space에 없는 상태가 사용 중입니다.',
        'en-US': 'Some task statuses are not available in the Space workflow.',
    },
}


def ts_string(value):
    return "'" + value.replace('\\', '\\\\').replace("'", "\\'") + "'"


# Codes the miy backend added for the reinforcement (docs/PORTING.md §10).
MIY_ERRORS = {
    'pms.milestone_date_range_invalid': {
        'ko-KR': '마일스톤 시작일은 마감일보다 늦을 수 없습니다.',
        'en-US': 'Milestone start date cannot be after its due date.',
    },
    'pms.custom_field_value_invalid': {
        'ko-KR': '필드 형식에 맞지 않는 값입니다.',
        'en-US': 'The value does not fit the field type.',
    },
    'pms.custom_field_option_duplicate': {
        'ko-KR': '선택지는 서로 달라야 합니다.',
        'en-US': 'Each option of a custom field must be different.',
    },
    'pms.custom_field_name_exists': {
        'ko-KR': '이 리스트에 같은 이름의 필드가 이미 있습니다.',
        'en-US': 'A custom field with this name already exists in this list.',
    },
    'pms.custom_field_limit_reached': {
        'ko-KR': '리스트에는 필드를 50개까지 만들 수 있습니다.',
        'en-US': 'A list can have at most 50 custom fields.',
    },
    'pms.template_wrong_space': {
        'ko-KR': '이 리스트가 속한 스페이스의 템플릿이 아닙니다.',
        'en-US': "Template does not belong to this list's space.",
    },
    'pms.saved_filter_not_found': {
        'ko-KR': '저장된 필터를 찾을 수 없습니다.',
        'en-US': 'Saved filter not found.',
    },
    'pms.saved_filter_limit_reached': {
        'ko-KR': '보기마다 필터를 100개까지 저장할 수 있습니다.',
        'en-US': 'A view can have at most 100 saved filters per person.',
    },
    'pms.saved_filter_manage_denied': {
        'ko-KR': '이 필터는 만든 사람이나 스페이스 소유자·관리자만 바꿀 수 있습니다.',
        'en-US': 'Only the owner or a space owner or admin can change this saved filter.',
    },
}


def errors(locale_code):
    body = []
    for code in CODES:
        message = PARAMLESS.get(code, MESSAGES[code])[locale_code]
        message = re.sub(r'\{([a-z_]+)\}', r'{{\1}}', message)
        assert '{' not in message.replace('{{limit_mb}}', ''), (code, message)
        body.append(f"      {code.replace('.', '_')}: {ts_string(message)},")
    for code, messages in MIY_ERRORS.items():
        assert code not in MESSAGES, code
        body.append(f"      {code.replace('.', '_')}: {ts_string(messages[locale_code])},")
    return '    errors: {\n' + '\n'.join(body) + '\n    },'


def locale(code, lang):
    b = BLOCKS[lang]
    return '\n'.join([
        f"  '{code}': {{",
        b['common'],
        '    shell: {',
        b['searchSpace'],
        b['appRuntime'],
        '      nav: {',
        *b['nav'],
        '      },',
        '    },',
        '    apps: {',
        pms_with_additions(b['pms'], lang),
        '    },',
        errors(code),
        '  },',
    ])


text = '\n'.join([
    '// Generated from AI-DO (commit 0002af062) apps/web/src/platform/i18n/resources.ts:',
    '// common, shell.searchSpace, shell.appRuntime, the PMS shell.nav titles,',
    '// apps.pms (Korean and English, verbatim), plus the `errors`',
    '// namespace from AI-DO\'s server catalog (core/i18n_catalog.py, codes with `.`',
    '// replaced by `_`; see platform/api/api-error.ts). miy additions:',
    '// apps.pms.{unsavedChanges, sidebarLabel, attachmentPreviewFailed,',
    '// milestoneSettings, customFields, templateSettings, savedFilters},',
    '// apps.pms.taskDetail.{sendComment, retryAttachments} and the reinforcement',
    '// error codes. Regenerate with scripts/gen-i18n-resources.py.',
    'export const resources = {',
    locale('ko-KR', 'ko'),
    locale('en-US', 'en'),
    '} as const;',
    '',
])
out.write_text(text, encoding='utf-8')
print('wrote', out, len(text.split('\n')), 'lines,', len(CODES), 'error codes')
