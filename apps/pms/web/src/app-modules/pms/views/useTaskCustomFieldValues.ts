import { useCallback, useEffect, useRef, useState } from 'react';
import { setTaskCustomFieldValue } from '../api/pms-api';
import type { AppApi } from '../../../platform/runtime/runtime-context';
import { apiErrorMessage } from '../../../platform/api/api-error';

/**
 * miy reinforcement: a task's custom field values in the detail panel. A save
 * shows at once; the stored form (a number in one spelling, options in field
 * order) replaces it when the server answers, unless a newer save of the same
 * field started meanwhile. A failed save puts the previous value back.
 */
export function useTaskCustomFieldValues({
  api,
  taskId,
  initialValues,
  onSaved,
  failedMessage,
}: {
  api: AppApi | null;
  taskId: string;
  initialValues: Record<string, string> | undefined;
  onSaved?: () => void | Promise<void>;
  failedMessage: string;
}) {
  const [values, setValues] = useState<Record<string, string>>(
    () => initialValues ?? {},
  );
  const [error, setError] = useState<string | null>(null);
  const savesRef = useRef<Record<string, number>>({});

  // A reloaded task brings fresh values; fields with a save in flight keep
  // what the person chose.
  useEffect(() => {
    setValues((current) => {
      const next: Record<string, string> = { ...(initialValues ?? {}) };
      for (const [fieldId, sequence] of Object.entries(savesRef.current)) {
        if (sequence > 0 && fieldId in current) next[fieldId] = current[fieldId];
      }
      return next;
    });
  }, [initialValues]);

  const save = useCallback(
    async (fieldId: string, value: string): Promise<boolean> => {
      if (!api) return false;
      const sequence = (savesRef.current[fieldId] ?? 0) + 1;
      savesRef.current[fieldId] = sequence;
      let previous = '';
      setValues((current) => {
        previous = current[fieldId] ?? '';
        return { ...current, [fieldId]: value };
      });
      setError(null);
      try {
        const saved = await setTaskCustomFieldValue(api, taskId, {
          field_id: fieldId,
          value,
        });
        if (savesRef.current[fieldId] === sequence) {
          setValues((current) => ({ ...current, [fieldId]: saved.value }));
        }
        void onSaved?.();
        return true;
      } catch (caught) {
        if (savesRef.current[fieldId] === sequence) {
          setValues((current) => ({ ...current, [fieldId]: previous }));
        }
        setError(apiErrorMessage(caught, failedMessage));
        return false;
      } finally {
        if (savesRef.current[fieldId] === sequence) {
          savesRef.current[fieldId] = 0;
        }
      }
    },
    [api, failedMessage, onSaved, taskId],
  );

  return { values, save, error, clearError: () => setError(null) };
}
