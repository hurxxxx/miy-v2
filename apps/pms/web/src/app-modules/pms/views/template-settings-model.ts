import type {
  PmsTaskTemplate,
  PmsTaskTemplateInput,
  PmsTemplateLabel,
  PmsTemplatePriority,
  PmsTemplateSubtask,
} from '../api/pms-api';

/** The template editor's form (miy reinforcement). */
export type TemplateDraft = {
  name: string;
  title: string;
  defaultPriority: PmsTemplatePriority;
  defaultStatus: string;
  descriptionBlocks: Record<string, unknown>[] | null;
  checklist: string[];
  labels: PmsTemplateLabel[];
  subtasks: PmsTemplateSubtask[];
};

export function createTemplateDraft(
  template: PmsTaskTemplate | null,
  defaultStatus: string,
): TemplateDraft {
  return {
    name: template?.name ?? '',
    title: template?.title ?? '',
    defaultPriority: template?.default_priority ?? 'medium',
    defaultStatus: template?.default_status ?? defaultStatus,
    descriptionBlocks: template?.description_blocks ?? null,
    checklist: (template?.checklist_items ?? []).map((item) => item.text),
    labels: template?.labels ?? [],
    subtasks: template?.subtasks ?? [],
  };
}

export function isTemplateDraftDirty(
  initial: TemplateDraft,
  draft: TemplateDraft,
): boolean {
  return JSON.stringify(initial) !== JSON.stringify(draft);
}

/** What the server takes: trimmed, empty checklist rows and short subtask
 *  titles left out (a task title needs two characters). */
export function templateDraftToInput(draft: TemplateDraft): PmsTaskTemplateInput {
  return {
    name: draft.name.trim(),
    title: draft.title.trim(),
    default_priority: draft.defaultPriority,
    default_status: draft.defaultStatus,
    description_blocks: draft.descriptionBlocks?.length
      ? draft.descriptionBlocks
      : null,
    checklist_items: draft.checklist
      .map((text) => text.trim())
      .filter(Boolean)
      .map((text) => ({ text })),
    labels: draft.labels,
    subtasks: draft.subtasks
      .map((subtask) => ({ ...subtask, title: subtask.title.trim() }))
      .filter((subtask) => subtask.title.length >= 2),
  };
}

/** How many checklist items, subtasks and labels a template adds. */
export function templateExtras(template: PmsTaskTemplate) {
  return {
    checklist: template.checklist_items?.length ?? 0,
    subtasks: template.subtasks?.length ?? template.subtask_count ?? 0,
    labels: template.labels?.length ?? 0,
  };
}
