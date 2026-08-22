import type { ComponentType, ReactElement } from 'react';
import { useEffect, useRef, useState, useSyncExternalStore } from 'react';

import { useTriggers } from '../../state/triggers';
import type { TriggersStore } from '../../state/triggers';
import { activeWorkspace, useWorkspaces } from '../../state/workspaces';
import { sectionEntry } from '../../ui/sections';
import type { Section } from '../../ui/sections';
import { TriggerRow } from './row';
import type { TriggerRowProps } from './row';
import { TriggerForm } from './form';
import type { TriggerFormProps, TriggerFormValue } from './form';
import type { TriggerDraft, TriggerSnapshot } from './io';

export type OpenedTriggerEditor =
  | { readonly mode: 'create'; readonly value: TriggerFormValue }
  | {
      readonly mode: 'edit';
      readonly value: TriggerFormValue;
      readonly expected: TriggerSnapshot;
    };

export interface TriggerEditorState {
  readonly opened: OpenedTriggerEditor | null;
  readonly confirmingDelete: boolean;
  readonly busy?: 'idle' | 'saving' | 'deleting';
  readonly refusal?: string | null;
  readonly revision?: number;
}

/** Controlled seam used by static-render judges; production uses the same transitions in state. */
export interface TriggerEditorController {
  readonly state: TriggerEditorState;
  readonly change: (next: TriggerEditorState) => void;
}

export interface TriggerCreateControlProps {
  readonly onCreate: () => void;
}

export function DefaultCreateControl({ onCreate }: TriggerCreateControlProps): ReactElement {
  return (
    <button
      data-create-trigger
      type="button"
      className="h-9 rounded-sm bg-accent px-4 text-ui text-bg"
      onClick={onCreate}
    >
      Create trigger
    </button>
  );
}

export interface TriggersScreenProps {
  readonly store?: TriggersStore;
  /** A test seam for proving that visible row controls and the store share real handlers. */
  readonly row?: ComponentType<TriggerRowProps>;
  /** The real screen owns the form; a probe may observe the handlers it receives. */
  readonly form?: ComponentType<TriggerFormProps>;
  /** Same controller in production and static tests: controls must cause observable transitions. */
  readonly editor?: TriggerEditorController;
  /** Captures the real Create transition without requiring jsdom. */
  readonly createControl?: ComponentType<TriggerCreateControlProps>;
}

export default function TriggersScreen({
  store = useTriggers,
  row: Row = TriggerRow,
  form: Form = TriggerForm,
  editor: controlledEditor,
  createControl: CreateControl = DefaultCreateControl,
}: TriggersScreenProps): ReactElement {
  const state = useSyncExternalStore(store.subscribe, store.getState, store.getState);
  const workspaceState = useSyncExternalStore(
    useWorkspaces.subscribe,
    useWorkspaces.getState,
    useWorkspaces.getState,
  );
  const [ownEditor, setOwnEditor] = useState<TriggerEditorState>({
    opened: null,
    confirmingDelete: false,
    busy: 'idle',
    refusal: null,
    revision: 0,
  });
  const editor: TriggerEditorController = controlledEditor ?? {
    state: ownEditor,
    change: setOwnEditor,
  };
  const latestEditor = useRef(editor.state);
  latestEditor.current = editor.state;
  const mutationsInFlight = useRef(new Set<number>());
  const currentEditor = (): TriggerEditorState => controlledEditor?.state ?? latestEditor.current;
  const changeEditor = (next: TriggerEditorState): void => {
    latestEditor.current = next;
    editor.change(next);
  };
  const replaceEditor = (next: OpenedTriggerEditor | null): void => {
    const revision = (currentEditor().revision ?? 0) + 1;
    changeEditor({
      opened: next,
      confirmingDelete: false,
      busy: 'idle',
      refusal: null,
      revision,
    });
  };
  const opened = editor.state.opened;

  useEffect(() => {
    void store.getState().load();
  }, [store]);

  const toggle = (slug: string, enabled: boolean): Promise<void> =>
    store.getState().toggle(slug, enabled);
  const runAgain = (slug: string): Promise<void> => store.getState().runAgain(slug);
  const empty = sectionEntry('triggers' as Section).empty;

  const openCreate = (): void => {
    store.getState().resetEditorFeedback();
    /* 2026-08-21: migawka powstaje teraz, nie przy Save. Przełączenie bocznego menu w trakcie
     * edycji nie może po cichu zmienić targetu, który nadal widać w kontrolce Workspace. */
    const workspace = activeWorkspace()?.folder ?? '';
    replaceEditor({
      mode: 'create',
      value: {
        connector: '',
        apiKey: '',
        workflow: state.workflows[0]?.path ?? '',
        workspace,
        pollEveryMinutes: 1,
      },
    });
  };

  const openSaved = (slug: string): void => {
    const trigger = state.triggers.find((one) => one.slug === slug);
    if (trigger === undefined || trigger.problem !== undefined) return;
    store.getState().resetEditorFeedback();
    replaceEditor({
      mode: 'edit',
      value: {
        connector: 'linear',
        apiKey: '',
        workflow: trigger.workflow,
        workspace: trigger.workspace ?? '',
        pollEveryMinutes: trigger.pollEveryMinutes,
      },
      expected: {
        slug: trigger.slug,
        source: trigger.source,
        condition: trigger.condition,
        workflow: trigger.workflow,
        workspace: trigger.workspace,
        enabled: trigger.enabled,
        pollEveryMinutes: trigger.pollEveryMinutes,
        hasApiKey: trigger.hasApiKey,
      },
    });
  };

  const draftOf = (value: TriggerFormValue): TriggerDraft => ({
    source: value.connector,
    condition: 'assigned-to-me',
    workflow: value.workflow,
    workspace: value.workspace,
    pollEveryMinutes: value.pollEveryMinutes,
    apiKey: value.apiKey.trim() === '' ? null : value.apiKey,
  });

  const saveOpened = (): Promise<void> => {
    if (opened === null) return Promise.resolve();
    const current = currentEditor();
    const revision = current.revision ?? 0;
    if (current.opened !== opened || mutationsInFlight.current.has(revision)) {
      return Promise.resolve();
    }
    mutationsInFlight.current.add(revision);
    const draft = draftOf(opened.value);
    /* The explicit Save owns the one-way secret transfer. Once that request has started,
     * keep the useful choices but remove the key from the rendered tree. */
    changeEditor({
      ...current,
      opened: { ...opened, value: { ...opened.value, apiKey: '' } },
      confirmingDelete: false,
      busy: 'saving',
      refusal: null,
      revision,
    });
    const saving =
      opened.mode === 'create'
        ? store.getState().create(draft)
        : store.getState().update(opened.expected, draft);
    return saving
      .then((result) => {
        const latest = currentEditor();
        if ((latest.revision ?? 0) !== revision || latest.opened === null) return;
        if (result.ok) {
          store.getState().resetEditorFeedback();
          replaceEditor(null);
        } else {
          changeEditor({ ...latest, busy: 'idle', refusal: result.refusal });
        }
      })
      .finally(() => {
        mutationsInFlight.current.delete(revision);
      });
  };

  const deleteOpened = (): Promise<void> => {
    if (opened === null || opened.mode !== 'edit') return Promise.resolve();
    const current = currentEditor();
    const revision = current.revision ?? 0;
    if (current.opened !== opened || mutationsInFlight.current.has(revision)) {
      return Promise.resolve();
    }
    mutationsInFlight.current.add(revision);
    changeEditor({ ...current, busy: 'deleting', refusal: null, revision });
    return store
      .getState()
      .remove(opened.expected)
      .then((result) => {
        const latest = currentEditor();
        if ((latest.revision ?? 0) !== revision || latest.opened === null) return;
        if (result.ok) {
          store.getState().resetEditorFeedback();
          replaceEditor(null);
        } else {
          changeEditor({ ...latest, busy: 'idle', refusal: result.refusal });
        }
      })
      .finally(() => {
        mutationsInFlight.current.delete(revision);
      });
  };

  return (
    <section data-triggers-screen className="flex h-full flex-col">
      <header className="flex h-13 items-center border-b border-line bg-panel px-4">
        <h1 className="text-title text-ink">Triggers</h1>
        {state.triggers.length === 0 ? null : (
          <div className="ml-auto">
            <CreateControl onCreate={openCreate} />
          </div>
        )}
      </header>

      <div className="flex min-h-0 flex-1">
        <div className="min-h-0 flex-1 overflow-auto p-4">
          {state.said === null ? null : (
            <p className="mb-3 max-w-160 text-body text-attend">{state.said}</p>
          )}

          {state.triggers.length === 0 ? (
            <div className="flex h-full flex-col items-center justify-center gap-3">
              <span className="flex size-8 items-center justify-center rounded-md border border-dashed border-line-strong text-muted">
                ◇
              </span>
              <p data-empty className="text-body text-ink">
                {empty}
              </p>
              <p className="max-w-120 text-center text-body text-muted">
                Connect Linear and choose what should run when a new issue arrives.
              </p>
              <CreateControl onCreate={openCreate} />
            </div>
          ) : (
            <ul className="overflow-hidden rounded-md border border-line bg-panel">
              {state.triggers.map((trigger) => (
                <Row
                  key={trigger.slug}
                  trigger={trigger}
                  workspaces={workspaceState.all}
                  onToggle={toggle}
                  onRunAgain={runAgain}
                  onOpen={openSaved}
                />
              ))}
            </ul>
          )}
        </div>

        {opened === null ? null : (
          <aside
            data-trigger-editor
            className="min-h-0 w-83 overflow-auto border-l border-line bg-panel p-4"
          >
            <Form
              mode={opened.mode}
              value={opened.value}
              workflows={state.workflows}
              workspaces={workspaceState.all}
              hasSavedKey={opened.mode === 'edit' ? opened.expected.hasApiKey : false}
              refusal={editor.state.refusal ?? null}
              connection={state.connection}
              confirmingDelete={editor.state.confirmingDelete}
              busy={editor.state.busy ?? 'idle'}
              onChange={(value) => {
                store.getState().resetEditorFeedback();
                const current = currentEditor();
                if (current.opened !== opened || (current.busy ?? 'idle') !== 'idle') return;
                changeEditor({
                  ...current,
                  opened: { ...opened, value },
                  confirmingDelete: false,
                  refusal: null,
                });
              }}
              onTest={() =>
                store
                  .getState()
                  .testConnection(
                    opened.mode === 'edit' ? opened.expected.slug : null,
                    draftOf(opened.value).apiKey,
                  )
                  .then(() => undefined)
              }
              onSave={saveOpened}
              onCancel={() => {
                store.getState().resetEditorFeedback();
                replaceEditor(null);
              }}
              {...(opened.mode === 'edit'
                ? {
                    onDelete: () => {
                      const current = currentEditor();
                      if (current.opened !== opened || (current.busy ?? 'idle') !== 'idle') return;
                      changeEditor({
                        ...current,
                        confirmingDelete: true,
                        refusal: null,
                      });
                    },
                    onConfirmDelete: deleteOpened,
                    onKeep: () => {
                      const current = currentEditor();
                      if (current.opened !== opened || (current.busy ?? 'idle') !== 'idle') return;
                      changeEditor({ ...current, confirmingDelete: false, refusal: null });
                    },
                  }
                : {})}
            />
          </aside>
        )}
      </div>
    </section>
  );
}
