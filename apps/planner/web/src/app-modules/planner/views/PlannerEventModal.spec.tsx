import { act, fireEvent, render, screen } from '@testing-library/react';
import type { AppRuntime } from '@miy/sdk';
import { expect, it, vi } from 'vitest';

import { i18n } from '../../../platform/i18n';
import { RuntimeProvider } from '../../../platform/runtime/runtime-context';
import { PlannerEventModal } from './PlannerEventModal';

it('keeps an open event draft when the display language changes', async () => {
  const api = vi.fn().mockResolvedValue({
    id: 'event-1',
    title: 'Saved title',
    description: '',
    location: '',
    start: '2026-11-23T10:00:00Z',
    end: '2026-11-23T11:00:00Z',
    allDay: false,
  });
  const runtime = { api } as unknown as AppRuntime;
  await i18n.changeLanguage('ko-KR');
  try {
    render(
      <RuntimeProvider runtime={runtime}>
        <PlannerEventModal isOpen eventId="event-1" timeZone="UTC" onClose={() => {}} />
      </RuntimeProvider>,
    );
    const title = await screen.findByDisplayValue('Saved title');
    fireEvent.change(title, { target: { value: 'Unsaved title' } });

    await act(() => i18n.changeLanguage('en-US'));
    expect(screen.getByRole('textbox', { name: 'Title' })).toBe(title);
    expect((title as HTMLInputElement).value).toBe('Unsaved title');
    expect(api).toHaveBeenCalledTimes(1);
  } finally {
    await act(() => i18n.changeLanguage('ko-KR'));
  }
});
