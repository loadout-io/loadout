import type { ReactElement } from 'react';
import { renderToStaticMarkup } from 'react-dom/server';
import { describe, expect, it } from 'vitest';

import { createTriggersStore } from '../../state/triggers';
import type { TriggerClock, TriggerRunPath, TriggerView } from '../../state/triggers';
import type { TriggerEntry, TriggerIo } from './io';
import TriggersScreen from './index';
import { TriggerRow } from './row';
import type { TriggerRowProps } from './row';

const CLOCK: TriggerClock = {
  setInterval: () => 1,
  clearInterval: () => undefined,
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
    status: { kind: 'armed' },
  },
  {
    slug: 'urgent-bugs',
    source: 'Linear',
    condition: 'Label is urgent',
    workflow: 'repair.json',
    workflowName: 'Repair',
    enabled: false,
    status: { kind: 'unchecked' },
  },
  {
    slug: 'retired-workflow',
    source: 'Linear',
    condition: 'Team is Platform',
    workflow: 'retired.json',
    workflowName: null,
    enabled: true,
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
  }),
): TriggerIo {
  return {
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

describe('the real Triggers screen explains and controls its library', () => {
  it('shows distinct sources, conditions, real workflow names and a missing workflow', () => {
    const markup = renderToStaticMarkup(<TriggersScreen store={seeded()} />);
    expect(markup).toContain('Linear');
    expect(markup).toContain('Assigned to you');
    expect(markup).toContain('Label is urgent');
    expect(markup).toContain('Analysis');
    expect(markup).toContain('Repair');
    expect(markup).toContain('retired.json');
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
      expect(occurrences(one, 'data-trigger-text')).toBeLessThanOrEqual(4);
      expect(occurrences(one, 'data-trigger-toggle')).toBe(1);
      expect(one).toMatch(/<(?:button|input)\b/);
    }
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
      },
    ]);
    await Promise.all([rootLoad, screenLoad]);

    const markup = renderToStaticMarkup(<TriggersScreen store={store} />);
    expect(diskEnabled).toBe(false);
    expect(store.getState().triggers[0]?.enabled).toBe(false);
    expect(row(markup, 'assigned-to-me')).toContain('aria-pressed="false"');
  });
});
