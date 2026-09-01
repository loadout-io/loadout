/* AC-2 dla T-74: testujemy handlery przekazane przez PRAWDZIWY `TriggersScreen`, a potem
 * pytamy magazyn i ponowny markup. Sam formularz z atrapami przechodziłby dla martwego panelu,
 * a sama wartość magazynu łamałaby niezmiennik 29 — człowiek widzi ekran, nie Promise. */
import { Children, isValidElement } from 'react';
import type { ReactElement, ReactNode } from 'react';
import { renderToStaticMarkup } from 'react-dom/server';
import { beforeEach, describe, expect, it, vi } from 'vitest';

import { createTriggersStore } from '../../state/triggers';
import type { TriggerClock, TriggerRunPath } from '../../state/triggers';
import { useWorkspaces } from '../../state/workspaces';
import { TriggerForm } from './form';
import type { TriggerFormProps } from './form';
import type {
  ConfiguredTriggerEntry,
  TriggerDraft,
  TriggerEntry,
  TriggerIo,
  TriggerSnapshot,
} from './io';
import TriggersScreen from './index';
import type { OpenedTriggerEditor, TriggerEditorController, TriggerEditorState } from './index';
import { TriggerRow } from './row';
import type { TriggerRowProps } from './row';

const CLOCK: TriggerClock = {
  now: () => 0,
  setInterval: () => 1,
  clearInterval: () => undefined,
};
const RUN: TriggerRunPath = {
  listWorkflows: async () => [],
  launchRun: async () => null,
  atOnce: () => 3,
};

const KEY = 'lin_api_1234567890123456789012345678901234567890';
const WORKSPACE = '/project';
const OTHER_WORKSPACE = '/other-project';
const DRAFT: TriggerDraft = {
  source: 'linear',
  condition: 'assigned-to-me',
  workflow: 'analysis.json',
  workspace: WORKSPACE,
  pollEveryMinutes: 5,
  apiKey: KEY,
};
const ENTRY = {
  slug: 'linear-0198ca82-ded0-7000-8000-000000000074',
  source: DRAFT.source,
  condition: DRAFT.condition,
  workflow: DRAFT.workflow,
  workspace: DRAFT.workspace,
  enabled: true,
  pollEveryMinutes: DRAFT.pollEveryMinutes,
  hasApiKey: true,
} satisfies ConfiguredTriggerEntry;
const EXPECTED: TriggerSnapshot = { ...ENTRY };

const CREATE: OpenedTriggerEditor = {
  mode: 'create',
  value: {
    connector: 'linear',
    apiKey: KEY,
    workflow: DRAFT.workflow,
    workspace: DRAFT.workspace,
    pollEveryMinutes: DRAFT.pollEveryMinutes,
  },
};
const BLANK_CREATE: OpenedTriggerEditor = {
  mode: 'create',
  value: {
    connector: '',
    apiKey: '',
    workflow: '',
    workspace: '',
    pollEveryMinutes: 1,
  },
};
const EDIT: OpenedTriggerEditor = {
  mode: 'edit',
  value: {
    connector: 'linear',
    apiKey: '',
    workflow: 'verify.json',
    workspace: DRAFT.workspace,
    pollEveryMinutes: 15,
  },
  expected: EXPECTED,
};

function deferred<T>(): {
  readonly promise: Promise<T>;
  readonly resolve: (value: T) => void;
  readonly reject: (reason: unknown) => void;
} {
  let release: ((value: T) => void) | undefined;
  let refuse: ((reason: unknown) => void) | undefined;
  const promise = new Promise<T>((resolve, reject) => {
    release = resolve;
    refuse = reject;
  });
  return {
    promise,
    resolve: (value) => release?.(value),
    reject: (reason) => refuse?.(reason),
  };
}

function occurrences(text: string, needle: string): number {
  return text.split(needle).length - 1;
}

function ioWith(overrides: Partial<TriggerIo> = {}): TriggerIo {
  return {
    listTriggers: async () => [],
    setTriggerEnabled: async () => ENTRY,
    checkTrigger: async () => ({ status: 'armed' }),
    resumeTrigger: async () => {
      throw new Error('not used');
    },
    retryTrigger: async () => {
      throw new Error('not used');
    },
    createTrigger: async () => ENTRY,
    updateTrigger: async () => ENTRY,
    deleteTrigger: async () => undefined,
    testLinearConnection: async () => undefined,
    ...overrides,
  };
}

function controller(initial: TriggerEditorState): TriggerEditorController {
  const controlled = {
    state: initial,
    change: (next: TriggerEditorState) => {
      controlled.state = next;
    },
  };
  return controlled;
}

function observeForm(
  store: ReturnType<typeof createTriggersStore>,
  editor: TriggerEditorController,
): TriggerFormProps {
  let form: TriggerFormProps | null = null;
  function Probe(props: TriggerFormProps): ReactElement {
    form = props;
    return <TriggerForm {...props} />;
  }
  renderToStaticMarkup(<TriggersScreen store={store} editor={editor} form={Probe} />);
  expect(form, 'the true screen never mounted its form or passed it live handlers').not.toBeNull();
  return form as unknown as TriggerFormProps;
}

function openSavedThroughRealRow(
  store: ReturnType<typeof createTriggersStore>,
  editor: TriggerEditorController,
): void {
  let observed: TriggerRowProps | null = null;
  function Probe(props: TriggerRowProps): ReactElement {
    observed = props;
    return <TriggerRow {...props} />;
  }
  renderToStaticMarkup(<TriggersScreen store={store} editor={editor} row={Probe} />);
  expect(observed, 'the true screen never mounted its saved trigger row').not.toBeNull();
  const control = findControl(
    TriggerRow(observed as unknown as TriggerRowProps),
    'data-trigger-open',
  );
  expect(control, 'the real saved trigger row has no Edit control').not.toBeNull();
  expect(control?.props.onClick, 'the real saved trigger row has no Edit handler').toBeTypeOf(
    'function',
  );
  control?.props.onClick?.();
}

interface ControlProps {
  readonly children?: ReactNode;
  readonly type?: string;
  readonly disabled?: boolean;
  readonly 'data-trigger-action'?: string;
  readonly 'data-trigger-field'?: string;
  readonly 'data-trigger-form'?: unknown;
  readonly 'data-trigger-open'?: unknown;
  readonly onChange?: (event: { readonly target: { readonly value: string } }) => unknown;
  readonly onClick?: () => unknown;
  readonly onSubmit?: (event: { readonly preventDefault: () => void }) => unknown;
}

function findControl(
  node: ReactNode,
  marker: 'data-trigger-action' | 'data-trigger-field' | 'data-trigger-form' | 'data-trigger-open',
  value?: string,
): ReactElement<ControlProps> | null {
  if (!isValidElement(node)) return null;
  const props = node.props as ControlProps;
  if (props[marker] !== undefined && (value === undefined || props[marker] === value)) {
    return node as ReactElement<ControlProps>;
  }
  for (const child of Children.toArray(props.children)) {
    const found = findControl(child, marker, value);
    if (found !== null) return found;
  }
  return null;
}

function realForm(props: TriggerFormProps): ReactElement {
  return TriggerForm(props);
}

function changeThroughRealForm(props: TriggerFormProps, field: string, value: string): void {
  const control = findControl(realForm(props), 'data-trigger-field', field);
  expect(control, `the real ${field} control is missing`).not.toBeNull();
  expect(control?.props.onChange, `the real ${field} control has no change handler`).toBeTypeOf(
    'function',
  );
  control?.props.onChange?.({ target: { value } });
}

function clickThroughRealForm(props: TriggerFormProps, action: string): unknown {
  const control = findControl(realForm(props), 'data-trigger-action', action);
  expect(control, `the real ${action} control is missing`).not.toBeNull();
  expect(control?.props.onClick, `the real ${action} control has no click handler`).toBeTypeOf(
    'function',
  );
  return control?.props.onClick?.();
}

function submitThroughRealForm(props: TriggerFormProps): unknown {
  const tree = realForm(props);
  const save = findControl(tree, 'data-trigger-action', 'save');
  expect(save, 'the real Save control is missing').not.toBeNull();
  expect(save?.props.type).toBe('submit');
  expect(save?.props.disabled).toBe(false);
  const form = findControl(tree, 'data-trigger-form');
  expect(form?.props.onSubmit, 'the real Save form has no Save handler').toBeTypeOf('function');
  return form?.props.onSubmit?.({ preventDefault: () => undefined });
}

function capture(
  io: TriggerIo,
  opened: OpenedTriggerEditor,
  options: { readonly entry?: ConfiguredTriggerEntry; readonly confirmingDelete?: boolean } = {},
) {
  const store = createTriggersStore(io, CLOCK, RUN);
  if (options.entry !== undefined) {
    store.setState({
      triggers: [
        {
          ...options.entry,
          workspace: options.entry.workspace ?? null,
          workflowName: 'Analysis',
          status: { kind: 'armed' },
        },
      ],
    });
  }
  store.setState({
    workflows: [
      { path: 'analysis.json', name: 'Analysis' },
      { path: 'verify.json', name: 'Verify' },
    ],
  });
  const editor = controller({
    opened,
    confirmingDelete: options.confirmingDelete ?? false,
  });
  return { store, editor, form: observeForm(store, editor) };
}

beforeEach(() => {
  useWorkspaces.setState({
    all: [
      { id: WORKSPACE, name: 'Project', folder: WORKSPACE },
      { id: OTHER_WORKSPACE, name: 'Other project', folder: OTHER_WORKSPACE },
    ],
    activeId: WORKSPACE,
    said: null,
  });
});

describe('the four setup actions cross the true screen and the disk-backed store', () => {
  it('tests the entered key without saving or changing the library, then shows success', async () => {
    const tested = vi.fn(async (_slug: string | null, _apiKey: string | null) => undefined);
    const created = vi.fn(async (_draft: TriggerDraft) => ENTRY);
    const io = ioWith({ testLinearConnection: tested, createTrigger: created });
    const { store, editor, form } = capture(io, CREATE);

    await Promise.resolve(clickThroughRealForm(form, 'test')).catch(() => undefined);
    expect(tested).toHaveBeenCalledTimes(1);
    expect(tested).toHaveBeenCalledWith(null, KEY);
    expect(created).not.toHaveBeenCalled();
    expect(store.getState().triggers).toEqual([]);

    const visible = renderToStaticMarkup(<TriggersScreen store={store} editor={editor} />);
    expect(visible).toContain('Linear connection works.');
  });

  it('tests the saved key from the real Edit panel without asking the window for it', async () => {
    const tested = vi.fn(async (_slug: string | null, _apiKey: string | null) => undefined);
    const { store, editor, form } = capture(ioWith({ testLinearConnection: tested }), EDIT, {
      entry: ENTRY,
    });

    await Promise.resolve(clickThroughRealForm(form, 'test')).catch(() => undefined);
    expect(tested).toHaveBeenCalledTimes(1);
    expect(tested).toHaveBeenCalledWith(ENTRY.slug, null);
    const visible = renderToStaticMarkup(<TriggersScreen store={store} editor={editor} />);
    expect(visible).toContain('Linear connection works.');
    expect(visible).not.toContain(KEY);
  });

  it('creates with every picked value and changes the list only after Rust confirms', async () => {
    const saved = deferred<ConfiguredTriggerEntry>();
    const create = vi.fn((_draft: TriggerDraft) => saved.promise);
    const { store, editor } = capture(ioWith({ createTrigger: create }), BLANK_CREATE);

    changeThroughRealForm(observeForm(store, editor), 'connector', 'linear');
    changeThroughRealForm(observeForm(store, editor), 'apiKey', KEY);
    changeThroughRealForm(observeForm(store, editor), 'workspace', WORKSPACE);
    changeThroughRealForm(observeForm(store, editor), 'cadence', '5');
    changeThroughRealForm(observeForm(store, editor), 'workflow', 'analysis.json');

    /* 2026-08-21: the editor owns an explicit target. A side-menu switch between editing and
     * Save must not retarget the trigger behind the person's back. */
    useWorkspaces.getState().activate(OTHER_WORKSPACE);

    const ready = observeForm(store, editor);
    const saving = Promise.resolve(submitThroughRealForm(ready)).catch(() => undefined);
    const duplicate = Promise.resolve(submitThroughRealForm(ready)).catch(() => undefined);
    expect(create).toHaveBeenCalledTimes(1);
    expect(create).toHaveBeenCalledWith(DRAFT);
    expect(store.getState().triggers).toEqual([]);
    expect(renderToStaticMarkup(<TriggersScreen store={store} editor={editor} />)).toContain(
      'Saving…',
    );

    saved.resolve(ENTRY);
    await Promise.all([saving, duplicate]);
    expect(store.getState().triggers.map((trigger) => trigger.slug)).toEqual([ENTRY.slug]);
    expect(editor.state.opened).toBeNull();
  });

  it('edits by redacted saved copy and sends null for an untouched replacement key', async () => {
    const saved = deferred<ConfiguredTriggerEntry>();
    const update = vi.fn(
      (_slug: string, _expected: TriggerSnapshot, _draft: TriggerDraft) => saved.promise,
    );
    const { store, editor } = capture(ioWith({ updateTrigger: update }), EDIT, { entry: ENTRY });

    const saving = Promise.resolve(submitThroughRealForm(observeForm(store, editor))).catch(
      () => undefined,
    );
    expect(update).toHaveBeenCalledWith(ENTRY.slug, EXPECTED, {
      source: 'linear',
      condition: 'assigned-to-me',
      workflow: 'verify.json',
      workspace: WORKSPACE,
      pollEveryMinutes: 15,
      apiKey: null,
    });
    expect(store.getState().triggers[0]).toEqual(
      expect.objectContaining({ workflow: 'analysis.json', pollEveryMinutes: 5 }),
    );

    saved.resolve({ ...ENTRY, workflow: 'verify.json', pollEveryMinutes: 15 });
    await saving;
    expect(store.getState().triggers[0]).toEqual(
      expect.objectContaining({ workflow: 'verify.json', pollEveryMinutes: 15 }),
    );
    expect(editor.state.opened).toBeNull();
  });

  it('keeps a legacy trigger editable but blocks Save until its workspace is explicit', async () => {
    const legacy: ConfiguredTriggerEntry = { ...ENTRY, workspace: null, enabled: false };
    const expected: TriggerSnapshot = { ...legacy, workspace: legacy.workspace ?? null };
    const repaired: ConfiguredTriggerEntry = { ...legacy, workspace: WORKSPACE };
    const update = vi.fn(
      async (_slug: string, _expected: TriggerSnapshot, _draft: TriggerDraft) => repaired,
    );
    const store = createTriggersStore(ioWith({ updateTrigger: update }), CLOCK, RUN);
    store.setState({
      triggers: [
        {
          ...legacy,
          workspace: legacy.workspace ?? null,
          workflowName: 'Analysis',
          status: { kind: 'unchecked' },
        },
      ],
      workflows: [{ path: 'analysis.json', name: 'Analysis' }],
    });
    const editor = controller({ opened: null, confirmingDelete: false });

    openSavedThroughRealRow(store, editor);
    const blocked = observeForm(store, editor);
    expect(editor.state.opened).toEqual(
      expect.objectContaining({
        mode: 'edit',
        value: expect.objectContaining({ workspace: '' }),
        expected,
      }),
    );
    expect(findControl(realForm(blocked), 'data-trigger-action', 'save')?.props.disabled).toBe(
      true,
    );
    expect(renderToStaticMarkup(<TriggersScreen store={store} editor={editor} />)).toContain(
      'Choose an available workspace to save this trigger.',
    );

    changeThroughRealForm(blocked, 'workspace', WORKSPACE);
    await Promise.resolve(submitThroughRealForm(observeForm(store, editor)));
    expect(update).toHaveBeenCalledWith(legacy.slug, expected, {
      source: 'linear',
      condition: 'assigned-to-me',
      workflow: legacy.workflow,
      workspace: WORKSPACE,
      pollEveryMinutes: legacy.pollEveryMinutes,
      apiKey: null,
    });
  });

  it('removes an old missing-workspace retry refusal after the real Save repairs the target', async () => {
    const legacy: ConfiguredTriggerEntry = { ...ENTRY, workspace: null };
    const expected: TriggerSnapshot = { ...legacy, workspace: null };
    const repaired: ConfiguredTriggerEntry = { ...legacy, workspace: WORKSPACE };
    const update = vi.fn(async () => repaired);
    const store = createTriggersStore(ioWith({ updateTrigger: update }), CLOCK, RUN);
    store.setState({
      triggers: [
        {
          ...legacy,
          workspace: null,
          workflowName: 'Analysis',
          status: {
            kind: 'accepted',
            workflow: 'Analysis',
            workspace: null,
            receiptAt: Date.UTC(2026, 7, 21, 2, 12, 9, 700),
            retryRefusal: 'Choose a workspace before saving this trigger.',
          },
        },
      ],
      workflows: [{ path: 'analysis.json', name: 'Analysis' }],
    });
    const editor = controller({ opened: null, confirmingDelete: false });

    openSavedThroughRealRow(store, editor);
    changeThroughRealForm(observeForm(store, editor), 'workspace', WORKSPACE);
    await Promise.resolve(submitThroughRealForm(observeForm(store, editor)));

    expect(update).toHaveBeenCalledWith(legacy.slug, expected, {
      source: 'linear',
      condition: 'assigned-to-me',
      workflow: legacy.workflow,
      workspace: WORKSPACE,
      pollEveryMinutes: legacy.pollEveryMinutes,
      apiKey: null,
    });
    const visible = renderToStaticMarkup(<TriggersScreen store={store} editor={editor} />);
    expect(visible).toContain('Analysis · Project · Every 5 minutes');
    expect(visible).toContain(
      'Started Analysis in an unspecified workspace at 2026-08-21 02:12:09 UTC. · Run again',
    );
    expect(visible).not.toContain('Choose a workspace before saving this trigger.');
  });

  it('does not let a Save confirmation erase a newer Run again refusal', async () => {
    const saved = deferred<ConfiguredTriggerEntry>();
    const update = vi.fn(async () => saved.promise);
    const retryTrigger = vi.fn(async () => {
      throw new Error('That trigger run is still active.');
    });
    const { store, editor } = capture(ioWith({ updateTrigger: update, retryTrigger }), EDIT, {
      entry: ENTRY,
    });
    store.setState((state) => ({
      triggers: state.triggers.map((trigger) => ({
        ...trigger,
        status: {
          kind: 'accepted' as const,
          workflow: 'Analysis',
          workspace: WORKSPACE,
          receiptAt: Date.UTC(2026, 7, 21, 2, 12, 9, 700),
          retryRefusal: 'An older refusal.',
        },
      })),
    }));

    const saving = Promise.resolve(submitThroughRealForm(observeForm(store, editor)));
    await store.getState().runAgain(ENTRY.slug);
    saved.resolve({ ...ENTRY, workflow: 'verify.json', pollEveryMinutes: 15 });
    await saving;

    expect(retryTrigger).toHaveBeenCalledTimes(1);
    const visible = renderToStaticMarkup(<TriggersScreen store={store} editor={editor} />);
    expect(visible).toContain('That trigger run is still active. · Run again');
    expect(visible).not.toContain('An older refusal.');
  });

  it('keeps the panel values and puts a refused Save on the true screen', async () => {
    const refusal = 'Loadout could not save that trigger. Check the workflow and try again.';
    const io = ioWith({ createTrigger: async () => Promise.reject(refusal) });
    const { store, editor, form } = capture(io, CREATE);

    await Promise.resolve(submitThroughRealForm(form)).catch(() => undefined);
    const visible = renderToStaticMarkup(<TriggersScreen store={store} editor={editor} />);
    expect(visible).toContain(refusal);
    expect(occurrences(visible, refusal)).toBe(1);
    expect(visible).toContain('value="analysis.json"');
    expect(visible).toContain('value="5"');
    expect(visible).not.toContain(KEY);
  });

  it('asks before Delete, lets Cancel keep the file, then waits for disk before removing', async () => {
    const removed = deferred<void>();
    const remove = vi.fn((_slug: string, _expected: TriggerSnapshot) => removed.promise);
    const io = ioWith({ deleteTrigger: remove });
    const first = capture(io, EDIT, { entry: ENTRY });

    clickThroughRealForm(first.form, 'cancel');
    expect(first.editor.state.opened).toBeNull();
    expect(
      renderToStaticMarkup(<TriggersScreen store={first.store} editor={first.editor} />),
    ).not.toContain('data-trigger-editor');
    expect(remove).not.toHaveBeenCalled();

    first.editor.change({
      opened: EDIT,
      confirmingDelete: false,
    });
    const reopened = observeForm(first.store, first.editor);
    clickThroughRealForm(reopened, 'delete');
    expect(first.editor.state.confirmingDelete).toBe(true);
    expect(remove).not.toHaveBeenCalled();

    const visible = renderToStaticMarkup(
      <TriggersScreen store={first.store} editor={first.editor} />,
    );
    expect(visible).toContain('Any saved issue waiting to start will be discarded.');

    const confirmation = observeForm(first.store, first.editor);
    clickThroughRealForm(confirmation, 'keep');
    expect(first.editor.state.confirmingDelete).toBe(false);
    expect(remove).not.toHaveBeenCalled();

    clickThroughRealForm(observeForm(first.store, first.editor), 'delete');
    const readyToDelete = observeForm(first.store, first.editor);
    const deleting = Promise.resolve(clickThroughRealForm(readyToDelete, 'confirm-delete')).catch(
      () => undefined,
    );
    const duplicate = Promise.resolve(clickThroughRealForm(readyToDelete, 'confirm-delete')).catch(
      () => undefined,
    );
    expect(remove).toHaveBeenCalledTimes(1);
    expect(remove).toHaveBeenCalledWith(ENTRY.slug, EXPECTED);
    expect(first.store.getState().triggers).toHaveLength(1);
    expect(
      renderToStaticMarkup(<TriggersScreen store={first.store} editor={first.editor} />),
    ).toContain('Deleting…');

    removed.resolve();
    await Promise.all([deleting, duplicate]);
    expect(first.store.getState().triggers).toHaveLength(0);
    expect(first.editor.state.opened).toBeNull();
  });

  it('puts refused connection and Delete requests once on the true screen', async () => {
    const connectionRefusal = 'Linear rejected that API key.';
    const connection = capture(
      ioWith({ testLinearConnection: async () => Promise.reject(connectionRefusal) }),
      CREATE,
    );
    await Promise.resolve(clickThroughRealForm(connection.form, 'test')).catch(() => undefined);
    const refusedConnection = renderToStaticMarkup(
      <TriggersScreen store={connection.store} editor={connection.editor} />,
    );
    expect(occurrences(refusedConnection, connectionRefusal)).toBe(1);

    const deleteRefusal =
      'A run from this trigger is starting. Wait for it to finish, then delete the trigger.';
    const deleting = capture(
      ioWith({ deleteTrigger: async () => Promise.reject(deleteRefusal) }),
      EDIT,
      { entry: ENTRY },
    );
    clickThroughRealForm(deleting.form, 'delete');
    await Promise.resolve(
      clickThroughRealForm(observeForm(deleting.store, deleting.editor), 'confirm-delete'),
    ).catch(() => undefined);
    const refusedDelete = renderToStaticMarkup(
      <TriggersScreen store={deleting.store} editor={deleting.editor} />,
    );
    expect(occurrences(refusedDelete, deleteRefusal)).toBe(1);
    expect(deleting.store.getState().triggers).toHaveLength(1);
    expect(deleting.editor.state.opened?.mode).toBe('edit');
  });

  it('keeps a newly opened editor when an earlier Save confirms or refuses late', async () => {
    const createdEntry: ConfiguredTriggerEntry = {
      ...ENTRY,
      slug: 'linear-0198ca82-ded0-7000-8000-000000000175',
    };
    const saved = deferred<ConfiguredTriggerEntry>();
    const savingFirst = capture(ioWith({ createTrigger: () => saved.promise }), CREATE, {
      entry: ENTRY,
    });
    const firstSave = Promise.resolve(submitThroughRealForm(savingFirst.form));
    clickThroughRealForm(savingFirst.form, 'cancel');
    openSavedThroughRealRow(savingFirst.store, savingFirst.editor);
    expect(savingFirst.editor.state.opened).toEqual(
      expect.objectContaining({ mode: 'edit', expected: EXPECTED }),
    );

    saved.resolve(createdEntry);
    await firstSave;
    expect(savingFirst.store.getState().triggers.map(({ slug }) => slug)).toEqual([
      ENTRY.slug,
      createdEntry.slug,
    ]);
    expect(savingFirst.editor.state.opened).toEqual(
      expect.objectContaining({ mode: 'edit', expected: EXPECTED }),
    );

    const refused = deferred<ConfiguredTriggerEntry>();
    const refusal = 'That trigger changed while the earlier Save was waiting.';
    const refusingFirst = capture(ioWith({ createTrigger: () => refused.promise }), CREATE, {
      entry: ENTRY,
    });
    const refusedSave = Promise.resolve(submitThroughRealForm(refusingFirst.form));
    clickThroughRealForm(refusingFirst.form, 'cancel');
    openSavedThroughRealRow(refusingFirst.store, refusingFirst.editor);

    refused.reject(refusal);
    await refusedSave;
    const visible = renderToStaticMarkup(
      <TriggersScreen store={refusingFirst.store} editor={refusingFirst.editor} />,
    );
    expect(visible).not.toContain(refusal);
    expect(refusingFirst.editor.state.opened).toEqual(
      expect.objectContaining({ mode: 'edit', expected: EXPECTED }),
    );
    expect(refusingFirst.store.getState().triggers.map(({ slug }) => slug)).toEqual([ENTRY.slug]);
  });

  it('does not let one stale library read undo confirmed Create, Edit or Delete', async () => {
    const staleCreate = deferred<TriggerEntry[]>();
    const creating = capture(
      ioWith({ listTriggers: () => staleCreate.promise, createTrigger: async () => ENTRY }),
      CREATE,
    );
    const createLoad = creating.store.getState().load();
    await Promise.resolve(submitThroughRealForm(creating.form));
    staleCreate.resolve([]);
    await createLoad;
    expect(creating.store.getState().triggers.map(({ slug }) => slug)).toEqual([ENTRY.slug]);

    const staleEdit = deferred<TriggerEntry[]>();
    const editedEntry: ConfiguredTriggerEntry = {
      ...ENTRY,
      workflow: 'verify.json',
      pollEveryMinutes: 15,
    };
    const editing = capture(
      ioWith({ listTriggers: () => staleEdit.promise, updateTrigger: async () => editedEntry }),
      EDIT,
      { entry: ENTRY },
    );
    const editLoad = editing.store.getState().load();
    await Promise.resolve(submitThroughRealForm(editing.form));
    staleEdit.resolve([ENTRY]);
    await editLoad;
    expect(editing.store.getState().triggers[0]).toEqual(
      expect.objectContaining({ workflow: 'verify.json', pollEveryMinutes: 15 }),
    );

    const staleDelete = deferred<TriggerEntry[]>();
    const deleting = capture(ioWith({ listTriggers: () => staleDelete.promise }), EDIT, {
      entry: ENTRY,
    });
    const deleteLoad = deleting.store.getState().load();
    clickThroughRealForm(deleting.form, 'delete');
    await Promise.resolve(
      clickThroughRealForm(observeForm(deleting.store, deleting.editor), 'confirm-delete'),
    );
    staleDelete.resolve([ENTRY]);
    await deleteLoad;
    expect(deleting.store.getState().triggers).toEqual([]);
  });
});
