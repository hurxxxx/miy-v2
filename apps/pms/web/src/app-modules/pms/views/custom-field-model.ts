import type {
  PmsCustomField,
  PmsCustomFieldType,
  PmsTaskListMember,
  TaskFilterParams,
} from '../api/pms-api';

/**
 * miy reinforcement: custom field values as the server stores them. A multi
 * select is a JSON list in option order, a user field a member id, a number in
 * one spelling, a date `YYYY-MM-DD`; an empty string means no value.
 */

export const CUSTOM_FIELD_TYPES: readonly PmsCustomFieldType[] = [
  'text',
  'number',
  'date',
  'select',
  'multi_select',
  'user',
];

export function customFieldTypeLabelKey(type: PmsCustomFieldType): string {
  return `pms.customFields.types.${type}`;
}

export function customFieldHasOptions(type: PmsCustomFieldType): boolean {
  return type === 'select' || type === 'multi_select';
}

export function parseMultiSelectValue(value: string | undefined): string[] {
  if (!value) return [];
  try {
    const parsed: unknown = JSON.parse(value);
    return Array.isArray(parsed)
      ? parsed.filter((item): item is string => typeof item === 'string')
      : [];
  } catch {
    return [];
  }
}

/** The chosen options in the field's option order, as the server keeps them. */
export function serializeMultiSelectValue(
  field: Pick<PmsCustomField, 'options'>,
  chosen: readonly string[],
): string {
  const ordered = (field.options ?? []).filter((option) =>
    chosen.includes(option),
  );
  return ordered.length > 0 ? JSON.stringify(ordered) : '';
}

export function toggleMultiSelectOption(
  field: Pick<PmsCustomField, 'options'>,
  value: string | undefined,
  option: string,
): string {
  const chosen = parseMultiSelectValue(value);
  const next = chosen.includes(option)
    ? chosen.filter((item) => item !== option)
    : [...chosen, option];
  return serializeMultiSelectValue(field, next);
}

/** What a cell or a filter pill shows for a stored value. */
export function formatCustomFieldValue(
  field: Pick<PmsCustomField, 'field_type'>,
  value: string | undefined,
  members: readonly Pick<PmsTaskListMember, 'user_id' | 'full_name'>[] = [],
): string {
  if (!value) return '';
  if (field.field_type === 'multi_select') {
    return parseMultiSelectValue(value).join(', ');
  }
  if (field.field_type === 'user') {
    return members.find((member) => member.user_id === value)?.full_name ?? value;
  }
  return value;
}

/** Whether a typed number reads as one (the server keeps it in one spelling). */
export function isNumberDraft(value: string): boolean {
  const trimmed = value.trim();
  return trimmed === '' || Number.isFinite(Number(trimmed));
}

export function customFieldFilterCount(params: TaskFilterParams): number {
  return Object.values(params.custom_fields ?? {}).filter((value) =>
    value.trim(),
  ).length;
}

/** Set or clear one custom field condition of the filter bar. */
export function withCustomFieldFilter(
  params: TaskFilterParams,
  fieldId: string,
  value: string,
): TaskFilterParams {
  const next = { ...(params.custom_fields ?? {}) };
  if (value.trim()) next[fieldId] = value;
  else delete next[fieldId];
  return {
    ...params,
    custom_fields: Object.keys(next).length > 0 ? next : undefined,
  };
}

/** Field moved one place up (-1) or down (+1); ids in the new order. */
export function moveCustomField(
  fields: readonly Pick<PmsCustomField, 'id'>[],
  fieldId: string,
  offset: -1 | 1,
): string[] {
  const ids = fields.map((field) => field.id);
  const index = ids.indexOf(fieldId);
  const target = index + offset;
  if (index < 0 || target < 0 || target >= ids.length) return ids;
  [ids[index], ids[target]] = [ids[target], ids[index]];
  return ids;
}

/**
 * The options edit of a select field: the new options and the renames to send
 * so values follow a renamed option. Removed options clear their values, so the
 * screen asks first (see `removedOptions`).
 */
export type CustomFieldOptionDraft = { original: string | null; name: string };

export function buildOptionChange(drafts: readonly CustomFieldOptionDraft[]): {
  options: string[];
  renames: { from: string; to: string }[];
} {
  const options = drafts.map((draft) => draft.name.trim()).filter(Boolean);
  const renames = drafts
    .filter(
      (draft) =>
        draft.original !== null &&
        draft.name.trim() !== '' &&
        draft.name.trim() !== draft.original,
    )
    .map((draft) => ({ from: draft.original as string, to: draft.name.trim() }));
  return { options, renames };
}

export function removedOptions(
  before: readonly string[],
  drafts: readonly CustomFieldOptionDraft[],
): string[] {
  const kept = new Set(
    drafts
      .filter((draft) => draft.original !== null && draft.name.trim())
      .map((draft) => draft.original as string),
  );
  return before.filter((option) => !kept.has(option));
}
