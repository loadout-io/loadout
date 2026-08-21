/* AC-1 dla T-74: człowiek konfiguruje Linear na prawdziwym ekranie, bez ręcznego JSON-a.
 *
 * Słaba wersja asertowałaby samotny `TriggerForm`. Właśnie tak martwa funkcja przechodziła
 * zielone kryteria przed niezmiennikiem 29. Każdy markup niżej pochodzi z `TriggersScreen`;
 * `opened` jest tym samym szwem statycznego renderu, którego sekcja Agents używa dla panelu,
 * bo `renderToStaticMarkup` nie wykonuje kliknięcia ani efektu. */
import { Children, isValidElement } from 'react';
import type { ReactElement, ReactNode } from 'react';
import { renderToStaticMarkup } from 'react-dom/server';
import { describe, expect, it } from 'vitest';

import { createTriggersStore } from '../../state/triggers';
import type { TriggerClock, TriggerRunPath, TriggerView } from '../../state/triggers';
import type { TriggerIo } from './io';
import TriggersScreen, { DefaultCreateControl } from './index';
import type {
  OpenedTriggerEditor,
  TriggerCreateControlProps,
  TriggerEditorController,
  TriggerEditorState,
} from './index';
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

const IO: TriggerIo = {
  listTriggers: async () => [],
  setTriggerEnabled: async () => {
    throw new Error('not used');
  },
  checkTrigger: async () => ({ status: 'armed' }),
  createTrigger: async () => {
    throw new Error('not used');
  },
  updateTrigger: async () => {
    throw new Error('not used');
  },
  deleteTrigger: async () => {
    throw new Error('not used');
  },
  testLinearConnection: async () => {
    throw new Error('not used');
  },
};

const HEALTHY: TriggerView = {
  slug: 'linear-one',
  source: 'linear',
  condition: 'assigned-to-me',
  workflow: 'analysis.json',
  workflowName: 'Analysis',
  enabled: true,
  pollEveryMinutes: 5,
  hasApiKey: true,
  status: { kind: 'armed' },
};

const BROKEN: TriggerView = {
  slug: 'broken-file',
  problem: 'This trigger file is not valid JSON.',
  status: { kind: 'refused', sentence: 'This trigger file is not valid JSON.' },
};

const CREATE: OpenedTriggerEditor = {
  mode: 'create',
  value: { connector: '', apiKey: '', workflow: 'analysis.json', pollEveryMinutes: 1 },
};

const EDIT: OpenedTriggerEditor = {
  mode: 'edit',
  value: { connector: 'linear', apiKey: '', workflow: 'verify.json', pollEveryMinutes: 15 },
  expected: {
    slug: HEALTHY.slug,
    source: HEALTHY.source,
    condition: HEALTHY.condition,
    workflow: HEALTHY.workflow,
    enabled: HEALTHY.enabled,
    pollEveryMinutes: HEALTHY.pollEveryMinutes,
    hasApiKey: HEALTHY.hasApiKey,
  },
};

function screen(
  options: {
    readonly triggers?: readonly TriggerView[];
    readonly opened?: OpenedTriggerEditor;
    readonly editor?: TriggerEditorController;
  } = {},
): string {
  const store = createTriggersStore(IO, CLOCK, RUN);
  store.setState({
    triggers: options.triggers ?? [],
    workflows: [
      { path: 'analysis.json', name: 'Analysis' },
      { path: 'verify.json', name: 'Verify' },
    ],
  });
  const editor =
    options.editor ?? controller({ opened: options.opened ?? null, confirmingDelete: false });
  return renderToStaticMarkup(<TriggersScreen store={store} editor={editor} />);
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

function occurrences(text: string, needle: string): number {
  return text.split(needle).length - 1;
}

function row(markup: string, slug: string): string {
  return (
    new RegExp(`<li[^>]*data-trigger-row=["']${slug}["'][^>]*>[\\s\\S]*?<\\/li>`).exec(
      markup,
    )?.[0] ?? ''
  );
}

interface OpenControlProps {
  readonly children?: ReactNode;
  readonly 'data-trigger-open'?: unknown;
  readonly onClick?: () => void;
}

function openControlIn(node: ReactNode): ReactElement<OpenControlProps> | null {
  if (!isValidElement(node)) return null;
  const props = node.props as OpenControlProps;
  if (props['data-trigger-open'] !== undefined) {
    return node as ReactElement<OpenControlProps>;
  }
  for (const child of Children.toArray(props.children)) {
    const found = openControlIn(child);
    if (found !== null) return found;
  }
  return null;
}

function clickDefaultCreate(onCreate: () => void): void {
  const control = DefaultCreateControl({ onCreate });
  const props = control.props as {
    readonly 'data-create-trigger'?: unknown;
    readonly onClick?: () => void;
  };
  expect(props['data-create-trigger'], 'the production Create control lost its marker').toBe(true);
  expect(props.onClick, 'the production Create control has no handler').toBeTypeOf('function');
  props.onClick?.();
}

describe('the real Triggers screen owns the whole Linear setup', () => {
  it('replaces the hand-written-file instruction with exactly one live Create action', () => {
    const markup = screen();
    expect(occurrences(markup, 'data-create-trigger')).toBe(1);
    expect(markup).toContain('Create trigger');
    expect(markup).not.toMatch(/add a trigger file|triggers folder|\.json/i);
  });

  it('uses the visible Create transition to mount the editor on the next true render', () => {
    const editor = controller({ opened: null, confirmingDelete: false });
    let create: (() => void) | null = null;
    function Probe(props: TriggerCreateControlProps): ReactElement {
      create = props.onCreate;
      return <DefaultCreateControl {...props} />;
    }
    const store = createTriggersStore(IO, CLOCK, RUN);
    renderToStaticMarkup(<TriggersScreen store={store} editor={editor} createControl={Probe} />);
    expect(
      create,
      'the real screen never gave its visible Create control a handler',
    ).not.toBeNull();
    clickDefaultCreate(create as unknown as () => void);
    expect(editor.state).toEqual({
      opened: {
        mode: 'create',
        value: { connector: '', apiKey: '', workflow: '', pollEveryMinutes: 1 },
      },
      confirmingDelete: false,
      busy: 'idle',
      refusal: null,
      revision: 1,
    });
    expect(renderToStaticMarkup(<TriggersScreen store={store} editor={editor} />)).toContain(
      'data-trigger-editor',
    );
  });

  it('loads the production workflow choices before Create uses their real name and path', async () => {
    const run: TriggerRunPath = {
      ...RUN,
      listWorkflows: async () => [
        {
          path: 'loaded-analysis.json',
          workflow: {
            format: 1,
            id: '0198ca82-ded0-7000-8000-000000000174',
            name: 'Loaded analysis',
            steps: [],
            links: [],
          },
        },
      ],
    };
    const io: TriggerIo = {
      ...IO,
      createTrigger: async () => ({
        slug: 'linear-loaded-choice',
        source: 'linear',
        condition: 'assigned-to-me',
        workflow: 'loaded-analysis.json',
        enabled: true,
        pollEveryMinutes: 5,
        hasApiKey: true,
      }),
    };
    const store = createTriggersStore(io, CLOCK, run);
    await store.getState().load();
    const editor = controller({ opened: null, confirmingDelete: false });
    let create: (() => void) | null = null;
    function Probe(props: TriggerCreateControlProps): ReactElement {
      create = props.onCreate;
      return <DefaultCreateControl {...props} />;
    }
    renderToStaticMarkup(<TriggersScreen store={store} editor={editor} createControl={Probe} />);
    clickDefaultCreate(create as unknown as () => void);

    expect(editor.state.opened).toEqual({
      mode: 'create',
      value: {
        connector: '',
        apiKey: '',
        workflow: 'loaded-analysis.json',
        pollEveryMinutes: 1,
      },
    });
    const markup = renderToStaticMarkup(<TriggersScreen store={store} editor={editor} />);
    expect(markup).toContain('Loaded analysis');
    expect(markup).toContain('value="loaded-analysis.json"');

    await store.getState().create({
      source: 'linear',
      condition: 'assigned-to-me',
      workflow: 'loaded-analysis.json',
      pollEveryMinutes: 5,
      apiKey: 'lin_api_explicit_save_key',
    });
    expect(store.getState().triggers[0]?.workflowName).toBe('Loaded analysis');
  });

  it('renders the complete create form inside that screen, with one real connector', () => {
    const markup = screen({ opened: CREATE });
    expect(markup).toContain('data-trigger-editor');
    for (const label of ['Connector', 'Linear API key', 'When', 'Check every', 'Workflow']) {
      expect(markup).toContain(label);
    }
    expect(markup).toContain('An issue is assigned to you');
    expect(markup.replaceAll('&amp;', '&')).toContain(
      'Create or copy it in Linear Settings → Security & access.',
    );
    expect(markup).toMatch(/<input[^>]*type="password"/);
    expect(occurrences(markup, 'value="linear"')).toBe(1);
    expect(markup).not.toMatch(/Jira|ClickUp|Slack/);
    for (const cadence of [1, 5, 15, 60]) {
      expect(markup).toContain(`value="${String(cadence)}"`);
    }
    expect(markup).toContain('Checks run while Loadout is open.');
    expect(markup).toContain('Analysis');
    expect(markup).toContain('value="analysis.json"');
    for (const action of ['Test connection', 'Save', 'Cancel']) expect(markup).toContain(action);
    expect(markup).toContain('Enter a Linear API key to save this trigger.');
  });

  it('keeps Save visibly unavailable until a workflow is chosen', () => {
    const markup = screen({
      opened: {
        mode: 'create',
        value: {
          connector: 'linear',
          apiKey: 'lin_api_ready_for_an_explicit_save',
          workflow: '',
          pollEveryMinutes: 1,
        },
      },
    });
    expect(markup).toContain('Choose a workflow to save this trigger.');
    expect(markup).toMatch(/data-trigger-action="save"[^>]*disabled=""/);
  });

  it('edits with a saved-key fact and an empty replacement field, never the secret', () => {
    const secret = 'lin_api_this_value_must_never_return_to_the_window';
    const markup = screen({ triggers: [HEALTHY], opened: EDIT });
    expect(markup).toContain('A Linear key is saved.');
    expect(markup).toMatch(/<input[^>]*type="password"[^>]*value=""/);
    expect(markup).not.toContain(secret);
    expect(markup).toContain('value="verify.json"');
    expect(markup).toContain('value="15"');
  });

  it('opens every healthy row for editing without taking away its switch', () => {
    const one = row(screen({ triggers: [HEALTHY] }), HEALTHY.slug);
    expect(one).toContain('data-trigger-open');
    expect(occurrences(one, 'data-trigger-toggle')).toBe(1);
    expect(occurrences(one, 'data-trigger-text')).toBeLessThanOrEqual(4);
    expect(one).toContain('Every 5 minutes');
  });

  it('carries the healthy row handler into the edit panel on the true screen', () => {
    const editor = controller({ opened: null, confirmingDelete: false });
    const store = createTriggersStore(IO, CLOCK, RUN);
    store.setState({ triggers: [HEALTHY] });
    let observed: TriggerRowProps | null = null;
    function Probe(props: TriggerRowProps): ReactElement {
      observed = props;
      return <TriggerRow {...props} />;
    }
    renderToStaticMarkup(<TriggersScreen store={store} editor={editor} row={Probe} />);
    expect(observed, 'the real screen never rendered its healthy row').not.toBeNull();
    const open = openControlIn(TriggerRow(observed as unknown as TriggerRowProps));
    expect(
      open,
      'the real row never rendered a live control for its screen transition',
    ).not.toBeNull();
    expect(open?.props.onClick, 'the visible row control has no handler').toBeTypeOf('function');
    open?.props.onClick?.();
    expect(editor.state.opened).toEqual({
      mode: 'edit',
      value: {
        connector: 'linear',
        apiKey: '',
        workflow: HEALTHY.workflow,
        pollEveryMinutes: HEALTHY.pollEveryMinutes,
      },
      expected: {
        slug: HEALTHY.slug,
        source: HEALTHY.source,
        condition: HEALTHY.condition,
        workflow: HEALTHY.workflow,
        enabled: HEALTHY.enabled,
        pollEveryMinutes: HEALTHY.pollEveryMinutes,
        hasApiKey: HEALTHY.hasApiKey,
      },
    });
    expect(renderToStaticMarkup(<TriggersScreen store={store} editor={editor} />)).toContain(
      'data-trigger-editor',
    );
  });

  it('keeps an unreadable hand-written file as a named problem with zero controls', () => {
    const one = row(screen({ triggers: [BROKEN] }), BROKEN.slug);
    expect(one).toContain(BROKEN.problem);
    expect(one).not.toMatch(/<(?:button|input|select)\b/);
  });
});
