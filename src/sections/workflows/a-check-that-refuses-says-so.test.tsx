import { renderToStaticMarkup } from 'react-dom/server';
import { expect, it, vi } from 'vitest';

import type { WorkflowFile } from '../../state/workflows';
import { WorkflowEditor } from './editor';

const CHECK_REFUSAL = 'Loadout could not check this workflow because the checker is unavailable.';

const spy = vi.hoisted(() => ({
  made: new Map<
    string,
    {
      getState: () => {
        document: WorkflowFile;
        recheck: () => Promise<void>;
        saveNow: () => Promise<void>;
        commit: (next: WorkflowFile) => void;
      };
    }
  >(),
}));

vi.mock('./io', () => ({
  write: () => Promise.resolve('r2'),
  check: () => Promise.reject(CHECK_REFUSAL),
}));

vi.mock('../../state/workflows', async (importOriginal) => {
  const real = await importOriginal<typeof import('../../state/workflows')>();
  return {
    ...real,
    createWorkflowStore: (
      io: Parameters<typeof real.createWorkflowStore>[0],
      open: WorkflowFile,
      revision?: string | null,
    ) => {
      const standing = spy.made.get(open.id);
      if (standing !== undefined) return standing;
      const made = real.createWorkflowStore(io, open, revision);
      spy.made.set(open.id, made as never);
      return made;
    },
  };
});

const DOCUMENT: WorkflowFile = {
  format: 1,
  id: 'wf_ship',
  name: 'Ship',
  steps: [],
  links: [],
};

function editor(): string {
  const noop = () => undefined;
  return renderToStaticMarkup(
    <WorkflowEditor
      path="ship.json"
      document={DOCUMENT}
      revision="r1"
      agents={[]}
      onClose={noop}
      onRun={noop}
      onCreateAgent={noop}
    />,
  );
}

it('shows a refused check as its own sentence without calling the saved file unsaved', async () => {
  editor();
  const store = spy.made.get(DOCUMENT.id);
  if (store === undefined) throw new Error('the editor built no store to receive the refusal');

  await store.getState().recheck();
  const refused = editor();
  expect(refused).toContain('data-could-not-check');
  expect(refused).toContain(CHECK_REFUSAL);

  store.getState().commit({ ...store.getState().document, name: 'Ship it' });
  await store.getState().saveNow();
  const afterSave = editor();
  expect(afterSave).toContain(CHECK_REFUSAL);
  expect(
    afterSave,
    'the file saved successfully, but the checker refusal was presented as a save refusal',
  ).not.toContain('data-could-not-save');
});
