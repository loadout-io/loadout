import { existsSync, readFileSync } from 'node:fs';
import { dirname, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';
import { describe, expect, it, vi } from 'vitest';

import type { TriggerIo } from '../sections/triggers/io';
import { createTriggersStore, TRIGGER_WATCH_INTERVAL_MS } from './triggers';
import type { TriggerClock, TriggerRunPath, TriggerView } from './triggers';

const ROOT = resolve(dirname(fileURLToPath(import.meta.url)), '..', '..');

class FakeClock implements TriggerClock {
  readonly periods: number[] = [];
  readonly callbacks = new Map<number, () => void>();
  readonly cleared: number[] = [];
  private next = 1;
  private current = 0;

  now(): number {
    return this.current;
  }

  setInterval(callback: () => void, milliseconds: number): number {
    const handle = this.next;
    this.next += 1;
    this.periods.push(milliseconds);
    this.callbacks.set(handle, callback);
    return handle;
  }

  clearInterval(handle: unknown): void {
    if (typeof handle !== 'number') return;
    this.cleared.push(handle);
    this.callbacks.delete(handle);
  }

  advance(): void {
    this.current += TRIGGER_WATCH_INTERVAL_MS;
    for (const callback of [...this.callbacks.values()]) callback();
  }
}

const RUN: TriggerRunPath = {
  listWorkflows: async () => [],
  launchRun: async () => null,
  atOnce: () => 3,
};

const REDACTED = {
  workspace: '/project',
  pollEveryMinutes: 1 as const,
  hasApiKey: true as const,
};
const EDITOR_IO: Pick<
  TriggerIo,
  'retryTrigger' | 'createTrigger' | 'updateTrigger' | 'deleteTrigger' | 'testLinearConnection'
> = {
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

function trigger(slug: string, enabled = true): TriggerView {
  return {
    slug,
    source: 'Linear',
    condition: 'Assigned to you',
    workflow: 'analysis.json',
    workflowName: 'Analysis',
    enabled,
    ...REDACTED,
    status: { kind: 'unchecked' },
  };
}

function withIo(io: TriggerIo, clock = new FakeClock()) {
  const store = createTriggersStore(io, clock, RUN);
  return { store, clock };
}

async function settle(): Promise<void> {
  await Promise.resolve();
  await Promise.resolve();
  await Promise.resolve();
}

function deferred<T>(): { readonly promise: Promise<T>; readonly resolve: (value: T) => void } {
  let release: ((value: T) => void) | undefined;
  const promise = new Promise<T>((resolve) => {
    release = resolve;
  });
  return { promise, resolve: (value) => release?.(value) };
}

describe('the trigger watcher belongs to the application lifetime', () => {
  it('asks on both named ticks and skips disabled entries', async () => {
    const checked = vi.fn(async (_slug: string) => ({ status: 'armed' as const }));
    const io: TriggerIo = {
      ...EDITOR_IO,
      listTriggers: async () => [],
      setTriggerEnabled: async (slug, enabled) => ({
        slug,
        source: 'Linear',
        condition: 'Assigned to you',
        workflow: 'analysis.json',
        enabled,
        ...REDACTED,
      }),
      checkTrigger: checked,
    };
    const { store, clock } = withIo(io);
    store.setState({ triggers: [trigger('on'), trigger('off', false)] });
    store.getState().startWatching();
    store.getState().startWatching();

    expect(clock.periods).toEqual([TRIGGER_WATCH_INTERVAL_MS]);
    expect(checked).not.toHaveBeenCalled();
    clock.advance();
    await settle();
    expect(checked.mock.calls.map(([slug]) => slug)).toEqual(['on']);
    clock.advance();
    await settle();
    expect(checked.mock.calls.map(([slug]) => slug)).toEqual(['on', 'on']);
  });

  it('does not poll an enabled legacy trigger until a workspace is repaired', async () => {
    const checked = vi.fn(async (_slug: string) => ({ status: 'armed' as const }));
    /* This is the actual old IPC shape: the property is absent, not a convenient explicit null. */
    const withoutWorkspace = {
      slug: 'legacy',
      source: 'Linear',
      condition: 'Assigned to you',
      workflow: 'analysis.json',
      enabled: true,
      pollEveryMinutes: 1 as const,
      hasApiKey: true,
    };
    const { store, clock } = withIo({
      ...EDITOR_IO,
      listTriggers: async () => [withoutWorkspace],
      setTriggerEnabled: async () => trigger('legacy'),
      checkTrigger: checked,
    });
    store.getState().startWatching();
    await settle();
    expect(store.getState().triggers[0]).toEqual(expect.objectContaining({ workspace: null }));

    clock.advance();
    await settle();
    expect(checked).not.toHaveBeenCalled();
  });

  it('loads an empty production-shaped store before its first scheduled poll', async () => {
    const listed = vi.fn(async () => [
      {
        slug: 'loaded',
        source: 'Linear',
        condition: 'assigned-to-me',
        workflow: 'analysis.json',
        enabled: true,
        ...REDACTED,
      },
    ]);
    const checked = vi.fn(async (_slug: string) => ({ status: 'armed' as const }));
    const { store, clock } = withIo({
      ...EDITOR_IO,
      listTriggers: listed,
      setTriggerEnabled: async () => trigger('loaded'),
      checkTrigger: checked,
    });

    store.getState().startWatching();
    await settle();
    expect(listed).toHaveBeenCalledTimes(1);
    expect(store.getState().triggers.map((one) => one.slug)).toEqual(['loaded']);

    clock.advance();
    await settle();
    expect(checked).toHaveBeenCalledWith('loaded');
  });

  it('never overlaps two Rust questions for one slug', async () => {
    const waiting = deferred<{ readonly status: 'armed' }>();
    const checked = vi.fn(() => waiting.promise);
    const { store, clock } = withIo({
      ...EDITOR_IO,
      listTriggers: async () => [],
      setTriggerEnabled: async () => trigger('one'),
      checkTrigger: checked,
    });
    store.setState({ triggers: [trigger('one')] });
    store.getState().startWatching();

    clock.advance();
    clock.advance();
    await settle();
    expect(checked).toHaveBeenCalledTimes(1);

    waiting.resolve({ status: 'armed' });
    await settle();
    clock.advance();
    await settle();
    expect(checked).toHaveBeenCalledTimes(2);
  });

  it('continues with the other slug after one refusal', async () => {
    const checked = vi.fn((slug: string) =>
      slug === 'refuses'
        ? Promise.reject('Linear could not be reached.')
        : Promise.resolve({ status: 'armed' as const }),
    );
    const { store, clock } = withIo({
      ...EDITOR_IO,
      listTriggers: async () => [],
      setTriggerEnabled: async () => trigger('one'),
      checkTrigger: checked,
    });
    store.setState({ triggers: [trigger('refuses'), trigger('continues')] });
    store.getState().startWatching();
    clock.advance();
    await settle();

    expect(checked.mock.calls.map(([slug]) => slug)).toEqual(['refuses', 'continues']);
    expect(store.getState().triggers[0]?.status).toEqual({
      kind: 'refused',
      sentence: 'Linear could not be reached.',
    });
  });

  it('clears the schedule and performs no later poll after stopWatching', async () => {
    const checked = vi.fn(async (_slug: string) => ({ status: 'armed' as const }));
    const { store, clock } = withIo({
      ...EDITOR_IO,
      listTriggers: async () => [],
      setTriggerEnabled: async () => trigger('one'),
      checkTrigger: checked,
    });
    store.setState({ triggers: [trigger('one')] });
    store.getState().startWatching();
    clock.advance();
    await settle();
    store.getState().stopWatching();
    clock.advance();
    await settle();

    expect(checked).toHaveBeenCalledTimes(1);
    expect(clock.cleared).toHaveLength(1);
    expect(clock.callbacks.size).toBe(0);
  });

  it('ignores a check result that arrives after stopWatching invalidates its generation', async () => {
    const waiting = deferred<{ readonly status: 'armed' }>();
    const checked = vi.fn(() => waiting.promise);
    const { store, clock } = withIo({
      ...EDITOR_IO,
      listTriggers: async () => [],
      setTriggerEnabled: async () => trigger('one'),
      checkTrigger: checked,
    });
    store.setState({ triggers: [trigger('one')] });
    store.getState().startWatching();
    clock.advance();
    await Promise.resolve();
    expect(checked).toHaveBeenCalledTimes(1);

    store.getState().stopWatching();
    waiting.resolve({ status: 'armed' });
    await settle();
    expect(store.getState().triggers[0]?.status).toEqual({ kind: 'unchecked' });
  });

  it('starts and stops the watcher in the production root, even if Triggers is never opened', () => {
    const path = resolve(ROOT, 'src/main.tsx');
    const source = existsSync(path) ? readFileSync(path, 'utf8') : '';
    const code = source.replace(/\/\*[\s\S]*?\*\//g, ' ').replace(/\/\/.*$/gm, ' ');
    expect(code).toMatch(/useTriggers\.getState\(\)\.startWatching\(\)/);
    expect(code).toMatch(/useTriggers\.getState\(\)\.stopWatching\(\)/);
    expect(code.indexOf('startWatching()')).toBeLessThan(code.indexOf('<App'));
  });
});
