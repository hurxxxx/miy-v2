import { describe, expect, it } from 'vitest';

import {
  initialPlannerEventModalState,
  isPlannerEventModalEdited,
  plannerEventModalEditableFields,
  plannerEventModalReducer,
} from './planner-event-modal-model';

// miy: the dialog reports unsaved edits to the portal (setNavigationState).
describe('planner event dialog unsaved-edit detection', () => {
  const now = new Date(2026, 4, 4, 9, 15);
  const opened = initialPlannerEventModalState(null, now);
  const baseline = plannerEventModalEditableFields(opened);

  it('is clean as opened and after reverting a change', () => {
    expect(isPlannerEventModalEdited(baseline, opened)).toBe(false);
    const typed = plannerEventModalReducer(opened, { type: 'setTitle', title: 'x' });
    expect(isPlannerEventModalEdited(baseline, typed)).toBe(true);
    const reverted = plannerEventModalReducer(typed, { type: 'setTitle', title: '' });
    expect(isPlannerEventModalEdited(baseline, reverted)).toBe(false);
  });

  it('ignores loading and saving flags but sees date and mode changes', () => {
    const saving = plannerEventModalReducer(opened, { type: 'saveStart' });
    expect(isPlannerEventModalEdited(baseline, saving)).toBe(false);
    const allDay = plannerEventModalReducer(opened, { type: 'setAllDay', allDay: true });
    expect(isPlannerEventModalEdited(baseline, allDay)).toBe(true);
    const moved = plannerEventModalReducer(opened, {
      type: 'setEndDateValue',
      endDateValue: '2026-05-09',
    });
    expect(isPlannerEventModalEdited(baseline, moved)).toBe(true);
  });
});
