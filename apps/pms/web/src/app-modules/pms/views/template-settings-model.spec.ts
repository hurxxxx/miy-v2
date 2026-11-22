import { describe, expect, it } from 'vitest';

import type { PmsTaskTemplate } from '../api/pms-api';
import {
  createTemplateDraft,
  isTemplateDraftDirty,
  templateDraftToInput,
  templateExtras,
} from './template-settings-model';

const template = {
  id: 't-1',
  team_id: 's-1',
  name: 'Release',
  title: 'Release v1',
  description: '',
  description_blocks: null,
  default_status: 'todo',
  default_priority: 'high',
  checklist_items: [{ text: 'Tag' }],
  labels: [{ name: 'qa', color: '#0f766e' }],
  subtasks: [
    {
      title: 'Smoke test',
      description: '',
      description_blocks: null,
      priority: 'low',
      checklist_items: [{ text: 'Login' }],
      labels: [],
    },
  ],
  subtask_count: 1,
  checklist_count: 1,
  created_by_id: 'u-1',
  created_at: '',
  updated_at: '',
} as PmsTaskTemplate;

describe('template settings model (miy reinforcement)', () => {
  it('edits a template as a draft and sends trimmed content', () => {
    const draft = createTemplateDraft(template, 'backlog');
    expect(draft.checklist).toEqual(['Tag']);
    expect(isTemplateDraftDirty(draft, draft)).toBe(false);
    const edited = {
      ...draft,
      name: ' Release v2 ',
      checklist: ['Tag', '  ', 'Publish'],
      subtasks: [...draft.subtasks, { ...draft.subtasks[0], title: 'x' }],
    };
    expect(isTemplateDraftDirty(draft, edited)).toBe(true);
    const input = templateDraftToInput(edited);
    expect(input.name).toBe('Release v2');
    expect(input.checklist_items).toEqual([{ text: 'Tag' }, { text: 'Publish' }]);
    expect(input.subtasks).toHaveLength(1);
    expect(input.description_blocks).toBeNull();
  });

  it('starts a new template in the list default status', () => {
    expect(createTemplateDraft(null, 'backlog').defaultStatus).toBe('backlog');
  });

  it('counts what a template adds', () => {
    expect(templateExtras(template)).toEqual({ checklist: 1, subtasks: 1, labels: 1 });
  });
});
