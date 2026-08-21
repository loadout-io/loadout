/* AC-5 dla T-74: heartbeat jest wspólny, ale termin należy do triggera. Test używa czasu
 * wstrzykniętego do magazynu; liczenie samych wywołań `setInterval` przechodziłoby dla czterech
 * pól cadence, których produkcja nigdy nie czyta. */
import { existsSync, readFileSync } from 'node:fs';
import { dirname, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';
import { describe, expect, it, vi } from 'vitest';

import type {
  ConfiguredTriggerEntry,
  TriggerDraft,
  TriggerIo,
  TriggerSnapshot,
} from '../sections/triggers/io';
import { createTriggersStore, TRIGGER_WATCH_INTERVAL_MS } from './triggers';
import type { TriggerClock, TriggerRunPath, TriggerView } from './triggers';

const ROOT = resolve(dirname(fileURLToPath(import.meta.url)), '..', '..');

class FakeClock implements TriggerClock {
  readonly callbacks = new Map<number, () => void>();
  readonly periods: number[] = [];
  readonly cleared: number[] = [];
  private current: number;
  private next = 1;

  constructor(atMinute = 0) {
    this.current = atMinute * TRIGGER_WATCH_INTERVAL_MS;
  }

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

  async advanceMinutes(minutes: number): Promise<void> {
    for (let minute = 0; minute < minutes; minute += 1) {
      this.current += TRIGGER_WATCH_INTERVAL_MS;
      for (const callback of [...this.callbacks.values()]) callback();
      await settle();
    }
  }
}

const RUN: TriggerRunPath = {
  listWorkflows: async () => [],
  launchRun: async () => null,
  atOnce: () => 3,
};

function entry(slug: string, pollEveryMinutes: 1 | 5 | 15 | 60, enabled = true) {
  return {
    slug,
    source: 'linear',
    condition: 'assigned-to-me',
    workflow: 'analysis.json',
    workflowName: 'Analysis',
    enabled,
    pollEveryMinutes,
    hasApiKey: true as const,
    status: { kind: 'unchecked' as const },
  } satisfies TriggerView;
}

function snapshotOf(one: ConfiguredTriggerEntry): TriggerSnapshot {
  return {
    slug: one.slug,
    source: one.source,
    condition: one.condition,
    workflow: one.workflow,
    enabled: one.enabled,
    pollEveryMinutes: one.pollEveryMinutes,
    hasApiKey: one.hasApiKey,
  };
}

function editorDefaults(): Pick<
  TriggerIo,
  'createTrigger' | 'updateTrigger' | 'deleteTrigger' | 'testLinearConnection'
> {
  return {
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
}

async function settle(): Promise<void> {
  await Promise.resolve();
  await Promise.resolve();
  await Promise.resolve();
}

describe('each trigger keeps its own polling cadence on the root heartbeat', () => {
  it('asks 1, 5, 15 and 60 minute triggers only on their own due minutes', async () => {
    const checked = vi.fn(async (_slug: string) => ({ status: 'armed' as const }));
    const io: TriggerIo = {
      ...editorDefaults(),
      listTriggers: async () => [],
      setTriggerEnabled: async () => entry('unused', 1),
      checkTrigger: checked,
    };
    const clock = new FakeClock();
    const store = createTriggersStore(io, clock, RUN);
    store.setState({
      triggers: [entry('one', 1), entry('five', 5), entry('fifteen', 15), entry('hour', 60)],
    });
    store.getState().startWatching();

    expect(clock.periods).toEqual([TRIGGER_WATCH_INTERVAL_MS]);
    await clock.advanceMinutes(1);
    await settle();
    expect(checked.mock.calls.map(([slug]) => slug)).toEqual(['one']);

    await clock.advanceMinutes(4);
    await settle();
    expect(checked.mock.calls.filter(([slug]) => slug === 'one')).toHaveLength(5);
    expect(checked.mock.calls.filter(([slug]) => slug === 'five')).toHaveLength(1);
    expect(checked.mock.calls.filter(([slug]) => slug === 'fifteen')).toHaveLength(0);

    await clock.advanceMinutes(10);
    await settle();
    expect(checked.mock.calls.filter(([slug]) => slug === 'five')).toHaveLength(3);
    expect(checked.mock.calls.filter(([slug]) => slug === 'fifteen')).toHaveLength(1);

    await clock.advanceMinutes(45);
    await settle();
    expect(checked.mock.calls.filter(([slug]) => slug === 'one')).toHaveLength(60);
    expect(checked.mock.calls.filter(([slug]) => slug === 'five')).toHaveLength(12);
    expect(checked.mock.calls.filter(([slug]) => slug === 'fifteen')).toHaveLength(4);
    expect(checked.mock.calls.filter(([slug]) => slug === 'hour')).toHaveLength(1);
  });

  it('skips disabled entries and never overlaps a slow question for one slug', async () => {
    let release: (() => void) | undefined;
    const waiting = new Promise<void>((resolve) => {
      release = resolve;
    });
    const checked = vi.fn(async () => {
      await waiting;
      return { status: 'armed' as const };
    });
    const clock = new FakeClock();
    const store = createTriggersStore(
      {
        ...editorDefaults(),
        listTriggers: async () => [],
        setTriggerEnabled: async () => entry('unused', 1),
        checkTrigger: checked,
      },
      clock,
      RUN,
    );
    store.setState({ triggers: [entry('live', 1), entry('off', 1, false)] });
    store.getState().startWatching();
    await clock.advanceMinutes(5);
    await settle();
    expect(checked).toHaveBeenCalledTimes(1);
    expect(checked).toHaveBeenCalledWith('live');

    release?.();
    await settle();
    await clock.advanceMinutes(1);
    await settle();
    expect(checked).toHaveBeenCalledTimes(2);
  });

  it('recalculates the next due minute from a confirmed cadence edit', async () => {
    const original = entry('changing', 5);
    const saved = { ...original, pollEveryMinutes: 15 } satisfies TriggerView;
    const updated = vi.fn(
      async (_slug: string, _expected: TriggerSnapshot, _draft: TriggerDraft) => saved,
    );
    const checked = vi.fn(async (_slug: string) => ({ status: 'armed' as const }));
    const clock = new FakeClock();
    const store = createTriggersStore(
      {
        ...editorDefaults(),
        listTriggers: async () => [],
        setTriggerEnabled: async () => original,
        updateTrigger: updated,
        checkTrigger: checked,
      },
      clock,
      RUN,
    );
    store.setState({ triggers: [original] });
    store.getState().startWatching();
    await clock.advanceMinutes(1);
    await settle();

    await store
      .getState()
      .update(snapshotOf(original), {
        source: 'linear',
        condition: 'assigned-to-me',
        workflow: 'analysis.json',
        pollEveryMinutes: 15,
        apiKey: null,
      })
      .catch(() => false);
    expect(updated).toHaveBeenCalledTimes(1);

    await clock.advanceMinutes(14);
    await settle();
    expect(checked).not.toHaveBeenCalled();
    await clock.advanceMinutes(1);
    await settle();
    expect(checked).toHaveBeenCalledTimes(1);
  });

  it('starts loaded and newly created triggers from their own confirmation minute', async () => {
    const loaded = {
      slug: 'loaded-at-59',
      source: 'linear',
      condition: 'assigned-to-me',
      workflow: 'analysis.json',
      enabled: true,
      pollEveryMinutes: 15 as const,
      hasApiKey: true,
    };
    const created = {
      ...loaded,
      slug: 'created-at-74',
      pollEveryMinutes: 60 as const,
    };
    const checked = vi.fn(async (_slug: string) => ({ status: 'armed' as const }));
    const clock = new FakeClock(59);
    const store = createTriggersStore(
      {
        ...editorDefaults(),
        listTriggers: async () => [loaded],
        setTriggerEnabled: async () => loaded,
        createTrigger: async () => created,
        checkTrigger: checked,
      },
      clock,
      RUN,
    );

    store.getState().startWatching();
    await settle();
    await clock.advanceMinutes(14);
    expect(checked).not.toHaveBeenCalled();
    await clock.advanceMinutes(1);
    expect(checked).toHaveBeenCalledWith(loaded.slug);

    await store.getState().create({
      source: 'linear',
      condition: 'assigned-to-me',
      workflow: 'analysis.json',
      pollEveryMinutes: 60,
      apiKey: 'lin_api_explicit_create_key',
    });
    checked.mockClear();
    await clock.advanceMinutes(59);
    expect(checked.mock.calls.some(([slug]) => slug === created.slug)).toBe(false);
    await clock.advanceMinutes(1);
    expect(checked.mock.calls.filter(([slug]) => slug === created.slug)).toHaveLength(1);
  });

  it('Stop clears the heartbeat, invalidates a late result and remains mounted at the root', async () => {
    let release: (() => void) | undefined;
    const waiting = new Promise<void>((resolve) => {
      release = resolve;
    });
    const clock = new FakeClock();
    const store = createTriggersStore(
      {
        ...editorDefaults(),
        listTriggers: async () => [],
        setTriggerEnabled: async () => entry('one', 1),
        checkTrigger: async () => {
          await waiting;
          return { status: 'armed' };
        },
      },
      clock,
      RUN,
    );
    store.setState({ triggers: [entry('one', 1)] });
    store.getState().startWatching();
    await clock.advanceMinutes(1);
    await Promise.resolve();
    store.getState().stopWatching();
    release?.();
    await settle();

    expect(clock.cleared).toHaveLength(1);
    expect(clock.callbacks.size).toBe(0);
    expect(store.getState().triggers[0]?.status).toEqual({ kind: 'unchecked' });

    const main = resolve(ROOT, 'src/main.tsx');
    const source = existsSync(main) ? readFileSync(main, 'utf8') : '';
    const code = source.replace(/\/\*[\s\S]*?\*\//g, ' ').replace(/\/\/.*$/gm, ' ');
    expect(code).toMatch(/useTriggers\.getState\(\)\.startWatching\(\)/);
    expect(code).toMatch(/useTriggers\.getState\(\)\.stopWatching\(\)/);
  });
});
