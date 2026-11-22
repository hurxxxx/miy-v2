import { useEffect, useRef, useState } from 'react';
import { Check, ChevronDown, X } from 'lucide-react';
import { useTranslation } from 'react-i18next';
import { DateInput } from '../../../components/date/DateInput';
import type { PmsCustomField, PmsTaskListMember } from '../api/pms-api';
import {
  formatCustomFieldValue,
  isNumberDraft,
  parseMultiSelectValue,
  toggleMultiSelectOption,
} from './custom-field-model';

const FIELD_CLASS =
  'app-field-input min-w-0 disabled:cursor-not-allowed disabled:opacity-60';
const COMPACT_FIELD_CLASS =
  'app-text-caption min-w-[7.5rem] rounded-md border border-app-border bg-app-surface-sidebar px-2 py-1 text-app-ink disabled:cursor-not-allowed disabled:opacity-60';
const OPTION_CLASS = 'bg-app-surface text-app-ink';

/**
 * One custom field value, edited in place (miy reinforcement). Text and
 * numbers keep the typed draft until Enter or leaving the field; a value that
 * arrives meanwhile (a save answering late, a list reload) does not replace
 * what is being typed.
 */
export function CustomFieldValueEditor({
  field,
  value,
  members = [],
  disabled = false,
  compact = false,
  onCommit,
}: {
  field: PmsCustomField;
  value: string | undefined;
  members?: readonly PmsTaskListMember[];
  disabled?: boolean;
  compact?: boolean;
  /** Resolves `false` when the value was not saved. */
  onCommit: (value: string) => void | Promise<boolean | void>;
}) {
  const { t } = useTranslation('apps');
  const className = compact ? COMPACT_FIELD_CLASS : FIELD_CLASS;
  const current = value ?? '';

  if (field.field_type === 'text' || field.field_type === 'number') {
    return (
      <DraftInput
        ariaLabel={field.name}
        className={className}
        disabled={disabled}
        numeric={field.field_type === 'number'}
        onCommit={onCommit}
        placeholder={t('pms.customFields.emptyValue')}
        value={current}
      />
    );
  }
  if (field.field_type === 'date') {
    return (
      <DateInput
        aria-label={field.name}
        className={className}
        disabled={disabled}
        onValueChange={(next) => {
          if ((next || '') !== current) void onCommit(next || '');
        }}
        value={current}
      />
    );
  }
  if (field.field_type === 'select' || field.field_type === 'user') {
    const choices =
      field.field_type === 'select'
        ? (field.options ?? []).map((option) => ({ value: option, label: option }))
        : members.map((member) => ({
            value: member.user_id,
            label: member.full_name,
          }));
    const known = current === '' || choices.some((choice) => choice.value === current);
    return (
      <select
        aria-label={field.name}
        className={`${className} cursor-pointer`}
        disabled={disabled}
        onChange={(event) => void onCommit(event.target.value)}
        value={current}
      >
        <option className={OPTION_CLASS} value="">
          {t('pms.customFields.emptyValue')}
        </option>
        {known ? null : (
          <option className={OPTION_CLASS} value={current}>
            {formatCustomFieldValue(field, current, members)}
          </option>
        )}
        {choices.map((choice) => (
          <option className={OPTION_CLASS} key={choice.value} value={choice.value}>
            {choice.label}
          </option>
        ))}
      </select>
    );
  }
  return (
    <MultiSelectEditor
      className={className}
      disabled={disabled}
      field={field}
      onCommit={onCommit}
      value={current}
    />
  );
}

function DraftInput({
  ariaLabel,
  className,
  disabled,
  numeric,
  onCommit,
  placeholder,
  value,
}: {
  ariaLabel: string;
  className: string;
  disabled: boolean;
  numeric: boolean;
  onCommit: (value: string) => void | Promise<boolean | void>;
  placeholder: string;
  value: string;
}) {
  const [draft, setDraft] = useState(value);
  const editingRef = useRef(false);
  useEffect(() => {
    if (!editingRef.current) setDraft(value);
  }, [value]);
  const commit = () => {
    editingRef.current = false;
    if (numeric && !isNumberDraft(draft)) {
      setDraft(value);
      return;
    }
    if (draft.trim() === value.trim()) return;
    void onCommit(draft);
  };
  return (
    <input
      aria-invalid={numeric && !isNumberDraft(draft) ? true : undefined}
      aria-label={ariaLabel}
      className={className}
      disabled={disabled}
      inputMode={numeric ? 'decimal' : undefined}
      onBlur={commit}
      onChange={(event) => {
        editingRef.current = true;
        setDraft(event.target.value);
      }}
      onClick={(event) => event.stopPropagation()}
      onKeyDown={(event) => {
        if (event.key === 'Enter') {
          event.preventDefault();
          (event.target as HTMLInputElement).blur();
        }
        if (event.key === 'Escape') {
          editingRef.current = false;
          setDraft(value);
        }
      }}
      placeholder={placeholder}
      type="text"
      value={draft}
    />
  );
}

function MultiSelectEditor({
  className,
  disabled,
  field,
  onCommit,
  value,
}: {
  className: string;
  disabled: boolean;
  field: PmsCustomField;
  onCommit: (value: string) => void | Promise<boolean | void>;
  value: string;
}) {
  const { t } = useTranslation('apps');
  const [open, setOpen] = useState(false);
  const ref = useRef<HTMLDivElement>(null);
  // Several toggles in a row build on the last chosen value, not on a value
  // whose save has not answered yet.
  const [pending, setPending] = useState<string | null>(null);
  const shown = pending ?? value;
  useEffect(() => {
    setPending(null);
  }, [value]);
  useEffect(() => {
    if (!open) return undefined;
    const close = (event: MouseEvent) => {
      if (ref.current && !ref.current.contains(event.target as Node)) {
        setOpen(false);
      }
    };
    document.addEventListener('mousedown', close);
    return () => document.removeEventListener('mousedown', close);
  }, [open]);
  const chosen = parseMultiSelectValue(shown);
  return (
    <div
      className="relative min-w-0"
      onClick={(event) => event.stopPropagation()}
      ref={ref}
    >
      <button
        aria-expanded={open}
        aria-haspopup="listbox"
        aria-label={field.name}
        className={`${className} flex w-full items-center justify-between gap-1 text-left`}
        disabled={disabled}
        onClick={() => setOpen((current) => !current)}
        type="button"
      >
        <span className={`truncate ${chosen.length ? '' : 'text-app-ink/40'}`}>
          {chosen.length ? chosen.join(', ') : t('pms.customFields.emptyValue')}
        </span>
        <ChevronDown className="shrink-0 text-app-ink/40" size={12} />
      </button>
      {open ? (
        <div
          className="absolute left-0 top-full z-[var(--ui-z-popover)] mt-1 max-h-60 min-w-[180px] overflow-y-auto rounded-lg border border-app-border bg-app-bg py-1 shadow-xl custom-scrollbar"
          role="listbox"
          aria-multiselectable
        >
          {(field.options ?? []).map((option) => {
            const selected = chosen.includes(option);
            return (
              <button
                aria-selected={selected}
                className="app-menu-item flex w-full items-center justify-between gap-2"
                key={option}
                onClick={() => {
                  const next = toggleMultiSelectOption(field, shown, option);
                  setPending(next);
                  void Promise.resolve(onCommit(next)).then((saved) => {
                    if (saved === false) setPending(null);
                  });
                }}
                role="option"
                type="button"
              >
                <span className="truncate">{option}</span>
                {selected ? <Check className="text-app-accent" size={12} /> : null}
              </button>
            );
          })}
          {chosen.length > 0 ? (
            <button
              className="app-menu-item flex w-full items-center gap-2 border-t border-app-border text-app-ink/60"
              onClick={() => {
                setPending('');
                void Promise.resolve(onCommit('')).then((saved) => {
                  if (saved === false) setPending(null);
                });
              }}
              type="button"
            >
              <X size={12} />
              {t('pms.customFields.clearValue', { name: field.name })}
            </button>
          ) : null}
        </div>
      ) : null}
    </div>
  );
}
