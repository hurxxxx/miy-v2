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
additions are listed in MIY_PMS below. The block starts are those of AI-DO
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
""",
    'en': """        unsavedChanges: 'You have unsaved changes. Leave anyway?',
        sidebarLabel: 'PMS navigation',
        attachmentPreviewFailed: 'Could not load the attachment.',
""",
}


def pms_with_additions(text, lang):
    head, rest = text.split('\n', 1)
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


def errors(locale_code):
    body = []
    for code in CODES:
        message = PARAMLESS.get(code, MESSAGES[code])[locale_code]
        message = re.sub(r'\{([a-z_]+)\}', r'{{\1}}', message)
        assert '{' not in message.replace('{{limit_mb}}', ''), (code, message)
        body.append(f"      {code.replace('.', '_')}: {ts_string(message)},")
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
    '// apps.pms.{unsavedChanges, sidebarLabel,',
    '// attachmentPreviewFailed}. Regenerate with scripts/gen-i18n-resources.py.',
    'export const resources = {',
    locale('ko-KR', 'ko'),
    locale('en-US', 'en'),
    '} as const;',
    '',
])
out.write_text(text, encoding='utf-8')
print('wrote', out, len(text.split('\n')), 'lines,', len(CODES), 'error codes')
