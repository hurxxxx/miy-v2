import { fireEvent, render, screen } from '@testing-library/react';
import { beforeEach, describe, expect, it, vi } from 'vitest';

import { i18n } from '../../../platform/i18n/i18n';
import type { PmsCustomField } from '../api/pms-api';
import { CustomFieldValueEditor } from './CustomFieldValueEditor';

function field(overrides: Partial<PmsCustomField>): PmsCustomField {
  return {
    id: 'f',
    list_id: 'l',
    name: 'Points',
    field_type: 'number',
    options: null,
    sort_order: 0,
    ...overrides,
  };
}

describe('CustomFieldValueEditor (miy reinforcement)', () => {
  beforeEach(async () => {
    await i18n.changeLanguage('ko-KR');
  });

  it('keeps what is being typed when a value arrives meanwhile', () => {
    const onCommit = vi.fn();
    const { rerender } = render(
      <CustomFieldValueEditor field={field({})} onCommit={onCommit} value="1" />,
    );
    const box = screen.getByRole('textbox', { name: 'Points' }) as HTMLInputElement;
    fireEvent.change(box, { target: { value: '42' } });
    rerender(
      <CustomFieldValueEditor field={field({})} onCommit={onCommit} value="2" />,
    );
    expect(box.value).toBe('42');
    fireEvent.keyDown(box, { key: 'Enter' });
    fireEvent.blur(box);
    expect(onCommit).toHaveBeenCalledWith('42');
  });

  it('does not send a number that is not one', () => {
    const onCommit = vi.fn();
    render(
      <CustomFieldValueEditor field={field({})} onCommit={onCommit} value="1" />,
    );
    const box = screen.getByRole('textbox', { name: 'Points' }) as HTMLInputElement;
    fireEvent.change(box, { target: { value: 'many' } });
    fireEvent.blur(box);
    expect(onCommit).not.toHaveBeenCalled();
    expect(box.value).toBe('1');
  });

  it('picks people among the space members', () => {
    const onCommit = vi.fn();
    render(
      <CustomFieldValueEditor
        field={field({ name: 'Owner', field_type: 'user' })}
        members={[
          {
            primary_org_unit_name: null,
            user_id: 'u-1',
            full_name: 'Ada',
            email: '',
            is_admin: false,
            role: 'member',
            joined_at: '',
          },
        ]}
        onCommit={onCommit}
        value=""
      />,
    );
    fireEvent.change(screen.getByRole('combobox', { name: 'Owner' }), {
      target: { value: 'u-1' },
    });
    expect(onCommit).toHaveBeenCalledWith('u-1');
  });

  it('toggles options of a multi select in option order', () => {
    const onCommit = vi.fn(async () => true);
    render(
      <CustomFieldValueEditor
        field={field({ name: 'Tags', field_type: 'multi_select', options: ['UI', 'API'] })}
        onCommit={onCommit}
        value='["API"]'
      />,
    );
    fireEvent.click(screen.getByRole('button', { name: 'Tags' }));
    fireEvent.click(screen.getByRole('option', { name: 'UI' }));
    expect(onCommit).toHaveBeenCalledWith('["UI","API"]');
  });
});
