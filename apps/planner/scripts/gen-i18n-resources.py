"""Regenerate web/src/platform/i18n/resources.ts from AI-DO.

Copies the common, shell.personalWidgets and apps.planner blocks (Korean and
English) of AI-DO apps/web/src/platform/i18n/resources.ts verbatim and adds the
miy keys below. The line ranges are those of AI-DO commit 0002af062; the
asserts stop if the source moved.

    python3 scripts/gen-i18n-resources.py \
        ../ai-do/prod/apps/web/src/platform/i18n/resources.ts \
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

ko_common = block(3, 79)          #     common: { ... },
ko_widgets = block(158, 229)      #       personalWidgets: { ... },
ko_planner = block(17942, 18143)  #       planner: { ... },
en_common = block(19299, 19374)
en_widgets = block(19452, 19528)
en_planner = block(37531, 37737)

for name, text, first in [('ko common', ko_common, '    common: {'), ('ko widgets', ko_widgets, '      personalWidgets: {'),
                          ('ko planner', ko_planner, '      planner: {'), ('en common', en_common, '    common: {'),
                          ('en widgets', en_widgets, '      personalWidgets: {'), ('en planner', en_planner, '      planner: {')]:
    lines = text.split('\n')
    assert lines[0] == first, (name, lines[0])
    assert lines[-1] in ('    },', '      },'), (name, lines[-1])

HOLIDAYS = {
    'ko': """        unsavedChanges: '저장되지 않은 작업이 있습니다. 이동하시겠습니까?',
        holidays: {
          unavailable: '휴일 정보 없음',
          unavailableDetail: '{{years}}년 공휴일 정보가 없어 휴일을 표시하지 않습니다.',
        },
""",
    'en': """        unsavedChanges: 'You have unsaved changes. Leave anyway?',
        holidays: {
          unavailable: 'No holiday data',
          unavailableDetail: 'Holidays are not shown: there is no public holiday data for {{years}}.',
        },
""",
}
WIDGET_ROUTES = {
    'ko': """        openPanel: '패널 화면으로 열기',
""",
    'en': """        openPanel: 'Open as a panel',
""",
}

def planner_with_additions(text, lang):
    # Insert the miy additions right after `planner: {` so the AI-DO keys stay verbatim.
    head, rest = text.split('\n', 1)
    return head + '\n' + HOLIDAYS[lang] + rest

def widgets_with_additions(text, lang):
    head, rest = text.split('\n', 1)
    return head + '\n' + WIDGET_ROUTES[lang] + rest

ERRORS = {
    'ko': {
        'calendar_invalid_iso_datetime': 'ISO 날짜/시간이 올바르지 않습니다.',
        'calendar_range_to_after_from': "범위의 'to'는 'from'보다 이후여야 합니다.",
        'calendar_range_too_large': '범위가 최대 {{days}}일을 초과했습니다.',
        'calendar_unknown_source_types': '알 수 없는 소스 유형입니다.',
        'planner_all_day_date_required': '종일 Planner 이벤트에는 YYYY-MM-DD 형식의 시작/종료일이 필요합니다.',
        'planner_event_end_after_start': 'Planner 이벤트 종료 시각은 시작 시각보다 이후여야 합니다.',
        'planner_event_not_found': 'Planner 이벤트를 찾을 수 없습니다.',
        'planner_list_range_required': "Planner 이벤트 목록에는 'from'과 'to'를 함께 제공해야 합니다.",
        'planner_update_start_end_required': 'Planner 이벤트 업데이트에는 start와 end를 함께 제공해야 합니다.',
        'personal_widgets_todo_not_found': '개인 할 일을 찾을 수 없습니다.',
        'validation_request_invalid': '요청 값이 올바르지 않습니다.',
    },
    'en': {
        'calendar_invalid_iso_datetime': 'Invalid ISO date/datetime.',
        'calendar_range_to_after_from': "Range 'to' must be strictly after 'from'.",
        'calendar_range_too_large': 'Range exceeds maximum {{days}} days.',
        'calendar_unknown_source_types': 'Unknown source types.',
        'planner_all_day_date_required': 'All-day planner events require YYYY-MM-DD start/end.',
        'planner_event_end_after_start': 'Planner event end must be after start.',
        'planner_event_not_found': 'Planner event not found.',
        'planner_list_range_required': "Planner event list requires both 'from' and 'to' together.",
        'planner_update_start_end_required': 'Planner event updates must provide both start and end together.',
        'personal_widgets_todo_not_found': 'Personal todo not found.',
        'validation_request_invalid': 'Request validation failed.',
    },
}

def errors(lang):
    body = ''.join(f"      {key}: {value!r},\n".replace("\\'", "'") if "'" not in value else f'      {key}: "{value}",\n'
                   for key, value in ERRORS[lang].items())
    return '    errors: {\n' + body + '    },'

def locale(code, lang, common, widgets, planner):
    return '\n'.join([
        f"  '{code}': {{",
        common,
        '    shell: {',
        widgets_with_additions(widgets, lang),
        '    },',
        '    apps: {',
        planner_with_additions(planner, lang),
        '    },',
        errors(lang),
        '  },',
    ])

text = '\n'.join([
    '// Generated from AI-DO apps/web/src/platform/i18n/resources.ts at 0002af062:',
    '// common, shell.personalWidgets and apps.planner, Korean and English, copied',
    '// verbatim. miy additions: apps.planner.holidays (year without holiday data),\n// apps.planner.unsavedChanges (navigation guard message),',
    '// shell.personalWidgets.openPanel (panel route link) and the `errors`',
    '// namespace (AI-DO server catalog messages for the API error codes, see',
    '// platform/api/api-error.ts). Regenerate with scripts/gen-i18n-resources.py.',
    'export const resources = {',
    locale('ko-KR', 'ko', ko_common, ko_widgets, ko_planner),
    locale('en-US', 'en', en_common, en_widgets, en_planner),
    '} as const;',
    '',
])
out.write_text(text, encoding='utf-8')
print('wrote', out, len(text.split('\n')), 'lines')
