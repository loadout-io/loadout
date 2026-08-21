import { create } from 'zustand';
import type { StoreApi, UseBoundStore } from 'zustand';

import type { Choice, Listed } from '../sections/run/choices';
import type {
  BrokenTriggerEntry,
  ConfiguredTriggerEntry,
  TriggerDelivery,
  TriggerIo,
  TriggerIssue,
} from '../sections/triggers/io';

/** The interval is named so the scheduler test proves both ticks use the product value. */
export const TRIGGER_WATCH_INTERVAL_MS = 60_000;

export interface TriggerClock {
  setInterval(callback: () => void, milliseconds: number): unknown;
  clearInterval(handle: unknown): void;
}

/** The only route from the watcher to a run. It deliberately exposes launchRun, not run/io. */
export interface TriggerRunPath {
  listWorkflows(): Promise<readonly Listed[]>;
  launchRun(
    choice: Choice | null,
    atOnce: number,
    task: string | null,
    claim: TriggerDelivery['claim'] | null,
  ): Promise<string | null>;
  atOnce(): number;
}

export type TriggerVisibleStatus =
  | { readonly kind: 'unchecked' }
  | { readonly kind: 'armed' }
  | { readonly kind: 'busy'; readonly delivery: TriggerDelivery }
  | { readonly kind: 'refused'; readonly sentence: string }
  | {
      readonly kind: 'accepted';
      readonly workflow: string;
      readonly receiptAt: number;
    };

/** Healthy rows resolve a workflow; a broken file has no workflow value to resolve. */
export type TriggerView =
  | (ConfiguredTriggerEntry & {
      readonly workflowName: string | null;
      readonly status: TriggerVisibleStatus;
    })
  | (BrokenTriggerEntry & {
      readonly workflowName?: never;
      readonly status: TriggerVisibleStatus;
    });

export interface TriggersState {
  readonly triggers: readonly TriggerView[];
  load(): Promise<void>;
  toggle(slug: string, enabled: boolean): Promise<void>;
  tick(): Promise<void>;
  startWatching(): void;
  stopWatching(): void;
}

export type TriggersStore = UseBoundStore<StoreApi<TriggersState>>;

/** Canonical task text is specified here so the watcher cannot invent a second shape. */
export function taskForIssue(_issue: TriggerIssue): string {
  return '';
}

/**
 * Compileable T-65 scaffold. The actions intentionally do nothing: every acceptance test must
 * therefore fail on missing behaviour, never while Vitest is collecting an absent module.
 */
export function createTriggersStore(
  io: TriggerIo,
  clock: TriggerClock,
  run: TriggerRunPath,
): TriggersStore {
  void io;
  void clock;
  void run;
  return create<TriggersState>()(() => ({
    triggers: [],
    load: async () => undefined,
    toggle: async () => undefined,
    tick: async () => undefined,
    startWatching: () => undefined,
    stopWatching: () => undefined,
  }));
}

const UNWIRED_IO: TriggerIo = {
  listTriggers: async () => [],
  setTriggerEnabled: async (slug, enabled) => ({
    slug,
    source: '',
    condition: '',
    workflow: '',
    enabled,
  }),
  checkTrigger: async () => ({ status: 'armed' }),
};

const WINDOW_CLOCK: TriggerClock = {
  setInterval: (callback, milliseconds) => globalThis.setInterval(callback, milliseconds),
  clearInterval: (handle) => globalThis.clearInterval(handle as ReturnType<typeof setInterval>),
};

const UNWIRED_RUN: TriggerRunPath = {
  listWorkflows: async () => [],
  launchRun: async () => 'not implemented',
  atOnce: () => 1,
};

/** Production-shaped singleton. Root mounting stays red until it starts and stops this watcher. */
export const useTriggers = createTriggersStore(UNWIRED_IO, WINDOW_CLOCK, UNWIRED_RUN);
