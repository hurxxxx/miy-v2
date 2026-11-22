import { useCallback, useEffect, useRef, useState } from 'react';
import { useTranslation } from 'react-i18next';
import { Trash2 } from 'lucide-react';
import { useConfirm, useFeedback } from '@miy/ui';

import { useAppApi } from '../../../platform/runtime/runtime-context';
import {
  deleteDocsTemplate,
  listDocsTemplates,
  type DocsTemplate,
} from '../api/docs-api';

/**
 * Choose a template of the space for a new doc or page (miy, 보강; AI-DO had
 * no templates at the base commit, so there are no built-in ones). Empty
 * means a blank page. The template's creator or a space admin may delete it
 * here.
 */
export function DocsTemplatePicker({
  spaceId,
  value,
  onChange,
}: {
  spaceId: string;
  value: string | null;
  onChange: (templateId: string | null) => void;
}) {
  const { t } = useTranslation(['apps', 'common']);
  const api = useAppApi();
  const feedback = useFeedback();
  const { confirm, confirmDialog } = useConfirm();
  const [templates, setTemplates] = useState<DocsTemplate[]>([]);
  const [failed, setFailed] = useState(false);
  // A late answer for another space never replaces this space's list.
  const requested = useRef(spaceId);

  const load = useCallback(async () => {
    requested.current = spaceId;
    try {
      const response = await listDocsTemplates(api, spaceId);
      if (requested.current !== spaceId) return;
      setTemplates(response.items);
      setFailed(false);
    } catch {
      if (requested.current !== spaceId) return;
      setTemplates([]);
      setFailed(true);
    }
  }, [api, spaceId]);

  useEffect(() => {
    void load();
  }, [load]);

  useEffect(() => {
    if (value && !templates.some((template) => template.id === value)) onChange(null);
  }, [onChange, templates, value]);

  const selected = templates.find((template) => template.id === value) ?? null;

  const remove = async (template: DocsTemplate) => {
    const ok = await confirm({
      title: t('apps:docsApp.templates.deleteConfirm', { title: template.title }),
      description: t('apps:docsApp.templates.deleteDescription'),
      confirmLabel: t('common:actions.delete'),
      cancelLabel: t('common:actions.cancel'),
      variant: 'danger',
    });
    if (!ok) return;
    try {
      await deleteDocsTemplate(api, template.id);
      onChange(null);
      void load();
    } catch (caught) {
      feedback.error(
        caught instanceof Error && caught.message
          ? caught.message
          : t('apps:docsApp.actionFailed'),
      );
    }
  };

  if (!templates.length && !failed) return null;

  return (
    <div>
      {confirmDialog}
      <label className="app-text-caption mb-1 block text-app-ink/55" htmlFor="docs-template-picker">
        {t('apps:docsApp.templates.label')}
      </label>
      <div className="flex items-center gap-2">
        <select
          id="docs-template-picker"
          className="app-field-input min-w-0 flex-1"
          value={value ?? ''}
          disabled={failed}
          onChange={(event) => onChange(event.target.value || null)}
        >
          <option value="">{t('apps:docsApp.templates.blank')}</option>
          {templates.map((template) => (
            <option key={template.id} value={template.id}>
              {template.title}
            </option>
          ))}
        </select>
        {selected?.can_delete ? (
          <button
            type="button"
            onClick={() => void remove(selected)}
            aria-label={t('apps:docsApp.templates.delete', { title: selected.title })}
            className="rounded p-2 text-app-ink/55 hover:bg-app-surface-hover hover:text-app-danger-text"
          >
            <Trash2 size={15} />
          </button>
        ) : null}
      </div>
      {failed ? (
        <p className="app-text-caption mt-1 text-app-ink/55">{t('apps:docsApp.templates.loadFailed')}</p>
      ) : selected?.description ? (
        <p className="app-text-caption mt-1 text-app-ink/55">{selected.description}</p>
      ) : null}
    </div>
  );
}
