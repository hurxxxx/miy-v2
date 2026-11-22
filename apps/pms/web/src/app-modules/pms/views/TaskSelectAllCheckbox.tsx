import { getTaskSelectionState } from './bulk-action-bar-model';

/** Header checkbox that selects or clears every row of one task table. */
export function TaskSelectAllCheckbox({
  label,
  onChange,
  selectedIds,
  taskIds,
}: {
  label: string;
  onChange: (selected: boolean) => void;
  selectedIds: ReadonlySet<string>;
  taskIds: readonly string[];
}) {
  const selectionState = getTaskSelectionState(taskIds, selectedIds);
  return (
    <input
      aria-label={label}
      checked={selectionState === 'all'}
      className="size-3.5 cursor-pointer rounded accent-app-accent disabled:cursor-not-allowed disabled:opacity-40"
      disabled={taskIds.length === 0}
      onChange={() => onChange(selectionState !== 'all')}
      ref={(node) => {
        if (node) node.indeterminate = selectionState === 'some';
      }}
      type="checkbox"
    />
  );
}
