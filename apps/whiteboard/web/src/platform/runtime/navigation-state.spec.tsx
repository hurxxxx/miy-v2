import { act, render } from '@testing-library/react';
import { expect, it, vi } from 'vitest';

import {
  NavigationStateProvider,
  useNavigateAfterEditorSave,
  usePrepareToLeaveEditor,
} from './navigation-state';

it('keeps the current editor open until its draft can be saved', async () => {
  const navigate = vi.fn();
  let finishSave: (saved: boolean) => void = () => undefined;
  const save = vi.fn().mockImplementationOnce(() => new Promise<boolean>((resolve) => {
    finishSave = resolve;
  })).mockResolvedValueOnce(true);

  function EditorAndSidebar() {
    usePrepareToLeaveEditor(save);
    const afterEditorSave = useNavigateAfterEditorSave();
    return <button onClick={() => afterEditorSave(navigate)}>Go to hub</button>;
  }

  const view = render(
    <NavigationStateProvider report={vi.fn()} message="Unsaved work">
      <EditorAndSidebar />
    </NavigationStateProvider>,
  );
  const link = view.getByRole('button', { name: 'Go to hub' });
  act(() => {
    link.click();
    link.click();
  });
  expect(save).toHaveBeenCalledTimes(1);
  expect(navigate).not.toHaveBeenCalled();

  await act(async () => { finishSave(false); });
  expect(navigate).not.toHaveBeenCalled();

  await act(async () => { link.click(); });
  expect(navigate).toHaveBeenCalledTimes(1);
});
