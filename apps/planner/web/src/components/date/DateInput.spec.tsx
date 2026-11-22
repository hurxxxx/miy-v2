import { cleanup, fireEvent, render, screen } from '@testing-library/react';
import type { ReactElement } from 'react';
import { afterEach, describe, expect, it, vi } from 'vitest';

import { DateInput } from './DateInput';

afterEach(cleanup);

function openPicker(ui: ReactElement) {
  render(ui);
  fireEvent.click(screen.getByRole('button'));
}

/** 2026-03: 1일 일요일, 4일 수요일, 7일 토요일. */
function dayButton(day: number): HTMLElement {
  return screen.getByRole('button', { name: new RegExp(`3월 ${day}일`) });
}

describe('DateInput calendar decoration', () => {
  const base = {
    locale: 'ko-KR',
    onValueChange: vi.fn(),
    value: '2026-03-04',
  };

  it('keeps the plain look and the date-only label by default', () => {
    openPicker(<DateInput {...base} />);

    for (const day of [1, 4, 7]) {
      const button = dayButton(day);
      expect(button.className).not.toContain('text-rose-500');
      expect(button.className).not.toContain('text-blue-700');
      expect(button.getAttribute('title')).toBeNull();
      expect(button.getAttribute('data-marker')).toBeNull();
      expect(button.querySelector('span')).toBeNull();
    }
  });

  it('colours Sunday red and Saturday blue only when asked', () => {
    openPicker(<DateInput {...base} weekendColors />);

    expect(dayButton(1).className).toContain('text-rose-500');
    expect(dayButton(7).className).toContain('text-blue-700');
    expect(dayButton(5).className).toContain('text-app-ink');
    expect(dayButton(5).className).not.toContain('text-rose-500');
  });

  it('shows a marked day in red with its label as title, aria text and a dot', () => {
    openPicker(
      <DateInput
        {...base}
        dayMarkers={{
          '2026-03-03': { label: '등록된 휴일', tone: 'holiday' },
          '2026-03-10': { label: '공휴일, 표에 없음', tone: 'notice' },
        }}
      />,
    );

    const registered = dayButton(3);
    expect(registered.className).toContain('text-rose-500');
    expect(registered.getAttribute('title')).toBe('등록된 휴일');
    expect(registered.getAttribute('aria-label')).toContain('등록된 휴일');
    expect(registered.getAttribute('data-marker')).toBe('holiday');
    expect(registered.querySelector('span')?.className).toContain(
      'bg-rose-500',
    );

    const notice = dayButton(10);
    expect(notice.getAttribute('data-marker')).toBe('notice');
    expect(notice.querySelector('span')?.className).toContain(
      'border-rose-500',
    );
    expect(notice.querySelector('span')?.className).not.toContain(
      'bg-rose-500',
    );

    // 표식이 없는 날은 그대로다.
    expect(dayButton(4).getAttribute('title')).toBeNull();
  });

  it('still selects a marked day', () => {
    const onValueChange = vi.fn();
    openPicker(
      <DateInput
        {...base}
        dayMarkers={{ '2026-03-03': { label: '휴일', tone: 'holiday' } }}
        onValueChange={onValueChange}
      />,
    );

    fireEvent.click(dayButton(3));
    expect(onValueChange).toHaveBeenCalledWith('2026-03-03');
  });
});
