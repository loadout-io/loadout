import { existsSync, readFileSync } from 'node:fs';
import type { ReactElement } from 'react';
import { renderToStaticMarkup } from 'react-dom/server';
import { beforeEach, describe, expect, it, vi } from 'vitest';

import { createTriggersStore } from '../../state/triggers';
import type { TriggerClock, TriggerRunPath, TriggerView } from '../../state/triggers';
import { useWorkspaces } from '../../state/workspaces';
import type { TriggerDelivery, TriggerEntry, TriggerIo } from './io';
import TriggersScreen from './index';
import { TriggerRow } from './row';
import type { TriggerRowProps } from './row';

const CLOCK: TriggerClock = {
  now: () => 0,
  setInterval: () => 1,
  clearInterval: () => undefined,
};

const REDACTED = {
  workspace: '/project',
  pollEveryMinutes: 1 as const,
  hasApiKey: true as const,
};
const EDITOR_IO: Pick<
  TriggerIo,
  | 'resumeTrigger'
  | 'retryTrigger'
  | 'createTrigger'
  | 'updateTrigger'
  | 'deleteTrigger'
  | 'testLinearConnection'
> = {
  resumeTrigger: async () => {
    throw new Error('not used');
  },
  retryTrigger: async () => {
    throw new Error('not used');
  },
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

const RUN: TriggerRunPath = {
  listWorkflows: async () => [],
  launchRun: async () => null,
  atOnce: () => 3,
};

const LIBRARY: readonly TriggerView[] = [
  {
    slug: 'assigned-to-me',
    source: 'Linear',
    condition: 'Assigned to you',
    workflow: 'analysis.json',
    workflowName: 'Analysis',
    enabled: true,
    ...REDACTED,
    status: { kind: 'armed' },
  },
  {
    slug: 'urgent-bugs',
    source: 'Linear',
    condition: 'Label is urgent',
    workflow: 'repair.json',
    workflowName: 'Repair',
    enabled: false,
    ...REDACTED,
    status: { kind: 'unchecked' },
  },
  {
    slug: 'retired-workflow',
    source: 'Linear',
    condition: 'Team is Platform',
    workflow: 'retired.json',
    workflowName: null,
    enabled: true,
    ...REDACTED,
    status: { kind: 'unchecked' },
  },
  {
    slug: 'broken-file',
    problem: 'broken-file.json could not be read.',
    status: { kind: 'refused', sentence: 'broken-file.json could not be read.' },
  },
];

function ioWith(
  setTriggerEnabled: TriggerIo['setTriggerEnabled'] = async (slug, enabled) => ({
    slug,
    source: 'Linear',
    condition: 'Assigned to you',
    workflow: 'analysis.json',
    enabled,
    ...REDACTED,
  }),
): TriggerIo {
  return {
    ...EDITOR_IO,
    listTriggers: async () => [...LIBRARY],
    setTriggerEnabled,
    checkTrigger: async () => ({ status: 'armed' }),
  };
}

function seeded(io: TriggerIo = ioWith()) {
  const store = createTriggersStore(io, CLOCK, RUN);
  store.setState({ triggers: LIBRARY });
  return store;
}

function row(markup: string, slug: string): string {
  return (
    new RegExp(`<li[^>]*data-trigger-row=["']${slug}["'][^>]*>[\\s\\S]*?<\\/li>`).exec(
      markup,
    )?.[0] ?? ''
  );
}

function occurrences(haystack: string, needle: string): number {
  return haystack.split(needle).length - 1;
}

function directTextCarriers(markup: string): readonly string[] {
  return [...markup.matchAll(/<([a-z][a-z0-9]*)\b[^>]*>([^<]+)<\/\1>/g)]
    .map((match) => match[2]?.trim() ?? '')
    .filter((text) => text !== '');
}

/**
 * Zdania o wstrzymanym triggerze UŁOŻYŁ Rust, więc czytamy je z jego pliku zamiast przepisywać.
 *
 * Ten sam wzorzec, co w `run/skills-refusal-is-visible.test.tsx`: plik bierzemy przez
 * `existsSync(p) ? readFileSync(p) : ''`, żeby test padał na asercji o treści, nigdy na
 * otwarciu pliku (AGENTS.md §2a p. 5).
 */
const RUST = new URL('../../../src-tauri/src/commands/triggers.rs', import.meta.url);
const RUST_SOURCE = existsSync(RUST) ? readFileSync(RUST, 'utf8') : '';

/*
 * Wzorzec bierze CAŁY literał razem z cudzysłowami i zdejmuje je przez `slice`, zamiast
 * dopasowywać je w regexie. Powód jest zmierzony 2026-08-29: `checks/vocabulary.sh` czyta
 * pliki naiwnym lekserem, który zna ten sam problem dla apostrofu w komentarzu — znak
 * cudzysłowu wewnątrz literału regexa rozjeżdżał mu parowanie i kilkaset znaków KODU niżej
 * czytało się jak zdanie dla użytkownika.
 */
function rustSentence(name: string): string {
  const literal = new RegExp(`pub const ${name}: &str =\\s*(\\S.*);`).exec(RUST_SOURCE)?.[1] ?? '';
  return literal.slice(1, -1).replace(/\\(.)/g, '$1');
}

const PAUSED = rustSentence('KEY_REFUSED_SENTENCE');
const START_REFUSED = rustSentence('START_REFUSED_SENTENCE');

function deferred<T>(): {
  readonly promise: Promise<T>;
  readonly resolve: (value: T) => void;
} {
  let release: ((value: T) => void) | undefined;
  const promise = new Promise<T>((resolve) => {
    release = resolve;
  });
  return {
    promise,
    resolve: (value) => {
      release?.(value);
    },
  };
}

beforeEach(() => {
  useWorkspaces.setState({
    all: [{ id: '/project', name: 'Project', folder: '/project' }],
    activeId: '/project',
    said: null,
  });
});

describe('the real Triggers screen explains and controls its library', () => {
  it('shows distinct sources, conditions, real workflow names and a missing workflow', () => {
    const markup = renderToStaticMarkup(<TriggersScreen store={seeded()} />);
    expect(markup).toContain('Linear');
    expect(markup).toContain('Assigned to you');
    expect(markup).toContain('Label is urgent');
    expect(markup).toContain('Analysis');
    expect(markup).toContain('Repair');
    expect(markup).toContain('retired.json');
    expect(markup).toContain('Analysis · Project · Every minute');
    expect(markup).toContain('title="/project"');
    for (const trigger of LIBRARY.filter((entry) => entry.problem === undefined)) {
      expect(row(markup, trigger.slug)).not.toBe('');
    }
  });

  it('shows a malformed file as a named problem with no invented config and no toggle', () => {
    const markup = renderToStaticMarkup(<TriggersScreen store={seeded()} />);
    const broken = row(markup, 'broken-file');
    expect(broken).not.toBe('');
    expect(broken).toContain('broken-file.json could not be read.');
    expect(broken).not.toMatch(/undefined|null|unknown source|unavailable/i);
    expect(occurrences(broken, 'data-trigger-toggle')).toBe(0);
    expect(broken).not.toMatch(/<(?:button|input)\b/);
  });

  it('keeps every healthy row to four text carriers and exactly one live toggle', () => {
    const markup = renderToStaticMarkup(<TriggersScreen store={seeded()} />);
    for (const trigger of LIBRARY.filter((entry) => entry.problem === undefined)) {
      const one = row(markup, trigger.slug);
      const carriers = directTextCarriers(one);
      expect(carriers).toHaveLength(4);
      expect(occurrences(one, 'data-trigger-text')).toBe(carriers.length);
      expect(occurrences(one, 'data-trigger-toggle')).toBe(1);
      expect(occurrences(one, 'data-trigger-run-again')).toBe(0);
      expect(one).toMatch(/<(?:button|input)\b/);
    }
  });

  it('keeps a legacy row editable and turn-off-able, but cannot arm or retry it', async () => {
    const legacy = {
      slug: 'legacy-linear',
      source: 'Linear',
      condition: 'Assigned to you',
      workflow: 'analysis.json',
      workflowName: 'Analysis',
      workspace: null,
      enabled: true,
      pollEveryMinutes: 1 as const,
      hasApiKey: true,
      status: { kind: 'armed' as const },
    } satisfies TriggerView;
    const saved = vi.fn(async (_slug: string, enabled: boolean) => ({
      slug: legacy.slug,
      source: legacy.source,
      condition: legacy.condition,
      workflow: legacy.workflow,
      enabled,
      ...REDACTED,
      workspace: null,
    }));
    const store = createTriggersStore(ioWith(saved), CLOCK, RUN);
    store.setState({ triggers: [legacy] });
    const handlers = new Map<string, TriggerRowProps['onToggle']>();
    function Probe(props: TriggerRowProps): ReactElement {
      handlers.set(props.trigger.slug, props.onToggle);
      return <TriggerRow {...props} />;
    }

    const enabledMarkup = renderToStaticMarkup(<TriggersScreen store={store} row={Probe} />);
    const enabledRow = row(enabledMarkup, legacy.slug);
    expect(enabledRow).toContain('Choose a workspace in Edit before this trigger can run.');
    expect(enabledRow).toContain('data-trigger-open');
    expect(enabledRow).not.toContain('data-trigger-run-again');
    expect(enabledRow).toMatch(/data-trigger-toggle(?![^>]*disabled)/);

    await handlers.get(legacy.slug)?.(legacy.slug, false);
    expect(saved).toHaveBeenCalledTimes(1);
    const disabledRow = row(
      renderToStaticMarkup(<TriggersScreen store={store} row={Probe} />),
      legacy.slug,
    );
    expect(disabledRow).toMatch(/data-trigger-toggle[^>]*disabled/);

    await handlers.get(legacy.slug)?.(legacy.slug, true);
    expect(saved, 'a missing target must not be armed again').toHaveBeenCalledTimes(1);
  });

  it('runs an accepted delivery again through the visible handler, once, with Rust returned identity', async () => {
    const delivery: TriggerDelivery = {
      claim: {
        slug: 'assigned-to-me',
        deliveryId: 'retry-delivery-2',
        workflow: 'analysis.json',
        runId: '0198ca82-ded0-7000-8000-000000000075',
        workspace: '/project',
      },
      issue: {
        id: 'linear-issue-id',
        identifier: 'LIN-42',
        title: 'Repair the trigger',
        url: 'https://linear.example/LIN-42',
        body: 'Use the saved issue body.',
        updatedAt: '2026-08-21T02:00:00.000Z',
      },
      createdAt: 1_787_278_400_000,
    };
    const returned = deferred<TriggerDelivery>();
    const retryTrigger = vi.fn(() => returned.promise);
    const launched = vi.fn<TriggerRunPath['launchRun']>(
      () => new Promise<string | null>(() => undefined),
    );
    const io: TriggerIo = { ...ioWith(), retryTrigger };
    const store = createTriggersStore(io, CLOCK, {
      listWorkflows: async () => [
        {
          path: 'analysis.json',
          place: 'project',
          workflow: {
            format: 1,
            id: 'workflow-id',
            name: 'Analysis',
            steps: [{ kind: 'checkpoint', id: 'analyse', name: 'Analysis', at: { x: 0, y: 0 } }],
            links: [],
          },
        },
      ],
      launchRun: launched,
      atOnce: () => 4,
    });
    store.setState({
      triggers: [
        {
          slug: 'assigned-to-me',
          source: 'Linear',
          condition: 'Assigned to you',
          workflow: 'analysis.json',
          workflowName: 'Analysis',
          enabled: true,
          ...REDACTED,
          status: {
            kind: 'accepted',
            workflow: 'Analysis',
            workspace: '/project',
            receiptAt: 1_787_278_329_700,
          },
        },
      ],
    });

    const handlers = new Map<string, TriggerRowProps['onRunAgain']>();
    function Probe(props: TriggerRowProps): ReactElement {
      handlers.set(props.trigger.slug, props.onRunAgain);
      return <TriggerRow {...props} />;
    }
    const markup = renderToStaticMarkup(<TriggersScreen store={store} row={Probe} />);
    const acceptedRow = row(markup, 'assigned-to-me');
    expect(directTextCarriers(acceptedRow)).toHaveLength(4);
    expect(occurrences(acceptedRow, 'data-trigger-text')).toBe(4);
    expect(acceptedRow).toContain('data-trigger-run-again');
    expect(acceptedRow).toContain(
      'Started Analysis in Project at 2026-08-21 02:12:09 UTC. · Run again',
    );

    const runAgain = handlers.get('assigned-to-me');
    expect(runAgain, 'the visible row never received the Run again handler').toBeDefined();
    const first = runAgain?.('assigned-to-me') ?? Promise.resolve();
    const overlapping = runAgain?.('assigned-to-me') ?? Promise.resolve();
    expect(retryTrigger).toHaveBeenCalledTimes(1);
    expect(retryTrigger).toHaveBeenCalledWith('assigned-to-me');

    returned.resolve(delivery);
    await Promise.all([first, overlapping]);
    expect(launched).toHaveBeenCalledTimes(1);
    expect(launched).toHaveBeenCalledWith(
      expect.objectContaining({ path: 'analysis.json', name: 'Analysis' }),
      4,
      'LIN-42: Repair the trigger\n\nUse the saved issue body.',
      delivery.claim,
    );
  });

  it('keeps the accepted workspace frozen when the current trigger is edited to another one', () => {
    useWorkspaces.setState({
      all: [
        { id: '/archive-project', name: 'Archive project', folder: '/archive-project' },
        { id: '/current-project', name: 'Current project', folder: '/current-project' },
      ],
      activeId: '/current-project',
      said: null,
    });
    const store = createTriggersStore(ioWith(), CLOCK, RUN);
    store.setState({
      triggers: [
        {
          slug: 'edited-after-run',
          source: 'Linear',
          condition: 'Assigned to you',
          workflow: 'analysis.json',
          workflowName: 'Analysis',
          enabled: true,
          ...REDACTED,
          workspace: '/current-project',
          status: {
            kind: 'accepted',
            workflow: 'Analysis',
            workspace: '/archive-project',
            receiptAt: Date.UTC(2026, 7, 21, 2, 12, 9, 700),
          },
        },
      ],
    });

    const visible = row(renderToStaticMarkup(<TriggersScreen store={store} />), 'edited-after-run');
    expect(visible).toContain('Analysis · Current project · Every minute');
    expect(visible).toContain(
      'Started Analysis in Archive project at 2026-08-21 02:12:09 UTC. · Run again',
    );
    expect(visible).toMatch(/data-trigger-status[^>]*title="\/archive-project"/);
    expect(directTextCarriers(visible)).toHaveLength(4);
    expect(visible).not.toContain('Started Analysis in Current project');
  });

  it('offers Retry only for a refusal from the launch path', () => {
    const store = seeded();
    const configured = LIBRARY[0];
    expect(configured?.problem).toBeUndefined();
    if (configured === undefined || configured.problem !== undefined) {
      throw new Error('the retry fixture must be a configured trigger');
    }
    store.setState({
      triggers: [
        {
          ...configured,
          status: {
            kind: 'refused',
            sentence: 'Loadout could not start that trigger.',
            retryable: true,
          },
        },
      ],
    });
    const retryable = row(renderToStaticMarkup(<TriggersScreen store={store} />), 'assigned-to-me');
    expect(retryable).toContain('Loadout could not start that trigger.');
    expect(retryable).toContain('data-trigger-run-again');
    expect(retryable).toContain('Loadout could not start that trigger. · Retry');

    store.setState({
      triggers: [
        {
          ...configured,
          status: { kind: 'refused', sentence: 'Linear could not be reached.' },
        },
      ],
    });
    const unrelated = row(renderToStaticMarkup(<TriggersScreen store={store} />), 'assigned-to-me');
    expect(unrelated).not.toContain('data-trigger-run-again');
  });

  it('paused row says it stopped and offers one more try', async () => {
    expect(
      PAUSED,
      'nothing was read out of the paused wording in src-tauri/src/commands/triggers.rs, so ' +
        'this row would be judged against an empty string',
    ).not.toBe('');
    const configured = LIBRARY[0];
    if (configured === undefined || configured.problem !== undefined) {
      throw new Error('the paused fixture must be a configured trigger');
    }
    const resumeTrigger = vi.fn(async () => ({ status: 'refused' as const, sentence: PAUSED }));
    const retryTrigger = vi.fn(async () => {
      throw new Error('a trigger on hold must ask Rust to lift the hold, not for a new delivery');
    });
    const store = createTriggersStore(
      {
        ...ioWith(),
        checkTrigger: async () => ({ status: 'refused', sentence: PAUSED }),
        resumeTrigger,
        retryTrigger,
      },
      CLOCK,
      RUN,
    );
    store.setState({ triggers: [configured] });
    await store.getState().tick();

    const handlers = new Map<string, TriggerRowProps['onRunAgain']>();
    function Probe(props: TriggerRowProps): ReactElement {
      handlers.set(props.trigger.slug, props.onRunAgain);
      return <TriggerRow {...props} />;
    }
    const paused = row(
      renderToStaticMarkup(<TriggersScreen store={store} row={Probe} />),
      configured.slug,
    );
    expect(paused).toContain(`${PAUSED} · Retry`);
    expect(paused).toContain('data-trigger-run-again');
    expect(directTextCarriers(paused)).toHaveLength(4);

    await handlers.get(configured.slug)?.(configured.slug);
    expect(resumeTrigger).toHaveBeenCalledTimes(1);
    expect(resumeTrigger).toHaveBeenCalledWith(configured.slug);
    expect(retryTrigger).not.toHaveBeenCalled();
    /* Klucz nadal odrzucony: wiersz wraca do tego samego zdania i tej samej kontrolki. */
    expect(row(renderToStaticMarkup(<TriggersScreen store={store} />), configured.slug)).toContain(
      `${PAUSED} · Retry`,
    );
  });

  it('says a workflow that never starts gave up, and keeps the one way back', async () => {
    expect(
      START_REFUSED,
      'nothing was read out of the wording for a workflow that never starts in ' +
        'src-tauri/src/commands/triggers.rs, so this row would be judged against an empty string',
    ).not.toBe('');
    expect(
      START_REFUSED,
      'a workflow that never starts is shown with the wording about a refused key, so the row ' +
        'cannot tell a broken workflow from a broken key',
    ).not.toBe(PAUSED);
    const configured = LIBRARY[0];
    if (configured === undefined || configured.problem !== undefined) {
      throw new Error('the fixture for a workflow that never starts must be a configured trigger');
    }
    /* Ten workflow nadal się nie uruchamia, więc jedno kliknięcie oddaje to samo zdanie —
     * dowodem jest brak drugiej kontrolki, nie brak zdania. */
    const resumeTrigger = vi.fn(async () => ({
      status: 'refused' as const,
      sentence: START_REFUSED,
    }));
    const retryTrigger = vi.fn(async () => {
      throw new Error('a trigger that gave up must ask Rust to lift the hold, not for new work');
    });
    const launchRun = vi.fn<TriggerRunPath['launchRun']>(async () => null);
    const store = createTriggersStore(
      {
        ...ioWith(),
        checkTrigger: async () => ({ status: 'refused', sentence: START_REFUSED }),
        resumeTrigger,
        retryTrigger,
      },
      CLOCK,
      { ...RUN, launchRun },
    );
    store.setState({ triggers: [configured] });
    await store.getState().tick();

    const handlers = new Map<string, TriggerRowProps['onRunAgain']>();
    function Probe(props: TriggerRowProps): ReactElement {
      handlers.set(props.trigger.slug, props.onRunAgain);
      return <TriggerRow {...props} />;
    }
    const gaveUp = row(
      renderToStaticMarkup(<TriggersScreen store={store} row={Probe} />),
      configured.slug,
    );
    expect(gaveUp).toContain(`${START_REFUSED} · Retry`);
    expect(gaveUp).toContain('data-trigger-run-again');
    expect(directTextCarriers(gaveUp)).toHaveLength(4);
    expect(
      launchRun,
      'a trigger on hold started its run from the window anyway',
    ).not.toHaveBeenCalled();

    await handlers.get(configured.slug)?.(configured.slug);
    expect(resumeTrigger).toHaveBeenCalledWith(configured.slug);
    expect(retryTrigger).not.toHaveBeenCalled();
    expect(launchRun).not.toHaveBeenCalled();
    expect(row(renderToStaticMarkup(<TriggersScreen store={store} />), configured.slug)).toContain(
      `${START_REFUSED} · Retry`,
    );
  });

  it('keeps Run again visible when Rust says the accepted run is still active', async () => {
    const refusal = 'That trigger run is still active.';
    const store = seeded({
      ...ioWith(),
      retryTrigger: async () => {
        throw refusal;
      },
    });
    const configured = LIBRARY[0];
    expect(configured?.problem).toBeUndefined();
    if (configured === undefined || configured.problem !== undefined) {
      throw new Error('the retry fixture must be a configured trigger');
    }
    store.setState({
      triggers: [
        {
          ...configured,
          status: {
            kind: 'accepted',
            workflow: 'Analysis',
            workspace: '/project',
            receiptAt: 1_787_278_329_700,
          },
        },
      ],
    });

    const handlers = new Map<string, TriggerRowProps['onRunAgain']>();
    function Probe(props: TriggerRowProps): ReactElement {
      handlers.set(props.trigger.slug, props.onRunAgain);
      return <TriggerRow {...props} />;
    }
    renderToStaticMarkup(<TriggersScreen store={store} row={Probe} />);
    const runAgain = handlers.get('assigned-to-me');
    expect(runAgain, 'the accepted row never received the Run again handler').toBeDefined();
    await runAgain?.('assigned-to-me');
    const visible = row(renderToStaticMarkup(<TriggersScreen store={store} />), 'assigned-to-me');
    expect(visible).toContain(refusal);
    expect(visible).toContain('data-trigger-run-again');
    expect(visible).toContain(`${refusal} · Run again`);
  });

  it('uses the visible toggle handler and changes state only after disk confirmation', async () => {
    const saved = deferred<TriggerEntry>();
    const calls: Array<readonly [string, boolean]> = [];
    const store = seeded(
      ioWith((slug, enabled) => {
        calls.push([slug, enabled]);
        return saved.promise;
      }),
    );
    const handlers = new Map<string, TriggerRowProps['onToggle']>();
    function Probe(props: TriggerRowProps): ReactElement {
      handlers.set(props.trigger.slug, props.onToggle);
      return <TriggerRow {...props} />;
    }
    renderToStaticMarkup(<TriggersScreen store={store} row={Probe} />);
    const toggle = handlers.get('assigned-to-me');
    expect(
      toggle,
      'the visible row never received a toggle handler from TriggersScreen',
    ).toBeDefined();

    const changing = toggle?.('assigned-to-me', false) ?? Promise.resolve();
    expect(store.getState().triggers[0]?.enabled).toBe(true);
    expect(calls).toEqual([['assigned-to-me', false]]);

    saved.resolve({
      slug: 'assigned-to-me',
      source: 'Linear',
      condition: 'Assigned to you',
      workflow: 'analysis.json',
      enabled: false,
      ...REDACTED,
    });
    await changing;
    expect(store.getState().triggers[0]?.enabled).toBe(false);
  });

  it('keeps the old value after a refused write and puts the refusal on the real screen', async () => {
    const refusal = 'Loadout could not save that trigger, so it is still on.';
    const store = seeded(ioWith(() => Promise.reject(refusal)));
    const handlers = new Map<string, TriggerRowProps['onToggle']>();
    function Probe(props: TriggerRowProps): ReactElement {
      handlers.set(props.trigger.slug, props.onToggle);
      return <TriggerRow {...props} />;
    }
    renderToStaticMarkup(<TriggersScreen store={store} row={Probe} />);
    await handlers.get('assigned-to-me')?.('assigned-to-me', false);

    expect(store.getState().triggers[0]?.enabled).toBe(true);
    expect(renderToStaticMarkup(<TriggersScreen store={store} />)).toContain(refusal);
  });

  it('removes a toggle refusal after the retry is saved and restores the earlier row status', async () => {
    const refusal = 'Loadout could not save that trigger, so it is still on.';
    let attempts = 0;
    const store = seeded(
      ioWith(async (slug, enabled) => {
        attempts += 1;
        if (attempts === 1) throw refusal;
        return {
          slug,
          source: 'Linear',
          condition: 'Assigned to you',
          workflow: 'analysis.json',
          enabled,
          ...REDACTED,
        };
      }),
    );
    const handlers = new Map<string, TriggerRowProps['onToggle']>();
    function Probe(props: TriggerRowProps): ReactElement {
      handlers.set(props.trigger.slug, props.onToggle);
      return <TriggerRow {...props} />;
    }
    renderToStaticMarkup(<TriggersScreen store={store} row={Probe} />);
    const toggle = handlers.get('assigned-to-me');

    await toggle?.('assigned-to-me', false);
    expect(renderToStaticMarkup(<TriggersScreen store={store} />)).toContain(refusal);

    await toggle?.('assigned-to-me', false);
    const recovered = renderToStaticMarkup(<TriggersScreen store={store} />);
    expect(store.getState().triggers[0]?.enabled).toBe(false);
    expect(store.getState().triggers[0]?.status).toEqual({ kind: 'armed' });
    expect(row(recovered, 'assigned-to-me')).toContain('aria-pressed="false"');
    expect(recovered).not.toContain(refusal);
  });

  it('does not let a shared stale load undo a confirmed toggle write', async () => {
    const stale = deferred<TriggerEntry[]>();
    let reads = 0;
    let diskEnabled = true;
    const io: TriggerIo = {
      ...EDITOR_IO,
      listTriggers: () => {
        reads += 1;
        return stale.promise;
      },
      setTriggerEnabled: async (slug, enabled) => {
        diskEnabled = enabled;
        return {
          slug,
          source: 'Linear',
          condition: 'Assigned to you',
          workflow: 'analysis.json',
          enabled,
          ...REDACTED,
        };
      },
      checkTrigger: async () => ({ status: 'armed' }),
    };
    const store = seeded(io);
    const rootLoad = store.getState().load();
    const screenLoad = store.getState().load();
    expect(reads, 'root and screen must share the read already in flight').toBe(1);

    await store.getState().toggle('assigned-to-me', false);
    expect(diskEnabled).toBe(false);
    stale.resolve([
      {
        slug: 'assigned-to-me',
        source: 'Linear',
        condition: 'Assigned to you',
        workflow: 'analysis.json',
        enabled: true,
        ...REDACTED,
      },
    ]);
    await Promise.all([rootLoad, screenLoad]);

    const markup = renderToStaticMarkup(<TriggersScreen store={store} />);
    expect(diskEnabled).toBe(false);
    expect(store.getState().triggers[0]?.enabled).toBe(false);
    expect(row(markup, 'assigned-to-me')).toContain('aria-pressed="false"');
  });
});
