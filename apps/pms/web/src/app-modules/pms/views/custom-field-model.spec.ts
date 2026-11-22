import { describe, expect, it } from 'vitest';

import type { PmsCustomField } from '../api/pms-api';
import {
  buildOptionChange,
  formatCustomFieldValue,
  isNumberDraft,
  moveCustomField,
  parseMultiSelectValue,
  removedOptions,
  toggleMultiSelectOption,
  withCustomFieldFilter,
} from './custom-field-model';

const multi = {
  id: 'f',
  list_id: 'l',
  name: 'Tags',
  field_type: 'multi_select',
  options: ['UI', 'API', 'DB'],
  sort_order: 0,
} as PmsCustomField;

describe('custom field model (miy reinforcement)', () => {
  it('keeps multi select values in option order as the server does', () => {
    expect(parseMultiSelectValue('["DB","UI"]')).toEqual(['DB', 'UI']);
    expect(parseMultiSelectValue('not json')).toEqual([]);
    expect(toggleMultiSelectOption(multi, '["DB"]', 'UI')).toBe('["UI","DB"]');
    expect(toggleMultiSelectOption(multi, '["DB"]', 'DB')).toBe('');
  });

  it('shows people by name and lists by commas', () => {
    expect(formatCustomFieldValue(multi, '["UI","DB"]')).toBe('UI, DB');
    expect(
      formatCustomFieldValue({ field_type: 'user' }, 'u-1', [
        { user_id: 'u-1', full_name: 'Ada' },
      ]),
    ).toBe('Ada');
    expect(formatCustomFieldValue({ field_type: 'user' }, 'u-2', [])).toBe('u-2');
    expect(formatCustomFieldValue({ field_type: 'text' }, undefined)).toBe('');
  });

  it('accepts only numbers in a number box', () => {
    expect(isNumberDraft('3.5')).toBe(true);
    expect(isNumberDraft('')).toBe(true);
    expect(isNumberDraft('three')).toBe(false);
  });

  it('sets and clears one filter condition', () => {
    const params = withCustomFieldFilter({ archived_state: 'active' }, 'f', 'UI');
    expect(params.custom_fields).toEqual({ f: 'UI' });
    expect(withCustomFieldFilter(params, 'f', ' ').custom_fields).toBeUndefined();
  });

  it('moves a field one place and stops at the ends', () => {
    const fields = [{ id: 'a' }, { id: 'b' }, { id: 'c' }];
    expect(moveCustomField(fields, 'b', -1)).toEqual(['b', 'a', 'c']);
    expect(moveCustomField(fields, 'c', 1)).toEqual(['a', 'b', 'c']);
  });

  it('sends renames so values follow and tells which options go away', () => {
    const drafts = [
      { original: 'UI', name: 'Frontend' },
      { original: 'API', name: 'API' },
      { original: null, name: 'Infra' },
    ];
    expect(buildOptionChange(drafts)).toEqual({
      options: ['Frontend', 'API', 'Infra'],
      renames: [{ from: 'UI', to: 'Frontend' }],
    });
    expect(removedOptions(['UI', 'API', 'DB'], drafts)).toEqual(['DB']);
  });
});
