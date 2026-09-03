import { expect, it, vi } from 'vitest';

import type { WorkflowFile, WorkflowIo } from './workflows';
import { createWorkflowStore } from './workflows';

const OPEN: WorkflowFile = {
  format: 1,
  id: 'wf_ship',
  name: 'Ship',
  steps: [],
  links: [],
};

it('flushes the newest document immediately when the editor leaves', async () => {
  const save = vi.fn((_file: WorkflowFile, _revision: string | null) => Promise.resolve('r2'));
  const io: WorkflowIo = {
    save,
    check: () => Promise.resolve([]),
    saveAgent: () => Promise.resolve(),
  };
  const store = createWorkflowStore(io, OPEN, 'r1');
  const latest = { ...OPEN, name: 'Ship it' };

  store.getState().commit(latest);
  await store.getState().flush();

  expect(
    save,
    'leaving before the 400 ms autosave delay did not send the last visible document to disk',
  ).toHaveBeenCalledTimes(1);
  expect(save.mock.calls[0]?.[0]).toBe(latest);
  expect(store.getState().savedDocument).toBe(latest);
});
