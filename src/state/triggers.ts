import { create } from 'zustand';
import type { StoreApi, UseBoundStore } from 'zustand';

import { why } from '../ipc/why';
import { choiceFor, toChoices } from '../sections/run/choices';
import type { Choice, Listed } from '../sections/run/choices';
import { launchRun } from '../sections/run/launch';
import { atOnce as chosenAtOnce } from '../sections/run/limits/chosen';
import * as triggerIo from '../sections/triggers/io';
import type {
  BrokenTriggerEntry,
  ConfiguredTriggerEntry,
  TriggerDelivery,
  TriggerEntry,
  TriggerIo,
  TriggerIssue,
} from '../sections/triggers/io';
import { list as listWorkflows } from '../sections/workflows/io';

/** The interval is named so the scheduler and the application share one answer. */
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

export type ConfiguredTriggerView = ConfiguredTriggerEntry & {
  readonly workflowName: string | null;
  readonly status: TriggerVisibleStatus;
};

export type BrokenTriggerView = BrokenTriggerEntry & {
  readonly workflowName?: never;
  readonly status: TriggerVisibleStatus;
};

/** Healthy rows resolve a workflow; a broken file has no workflow value to resolve. */
export type TriggerView = ConfiguredTriggerView | BrokenTriggerView;

export interface TriggersState {
  readonly triggers: readonly TriggerView[];
  /** A library-level refusal. Per-trigger refusals live on their one row. */
  readonly said: string | null;
  load(): Promise<void>;
  toggle(slug: string, enabled: boolean): Promise<void>;
  tick(): Promise<void>;
  startWatching(): void;
  stopWatching(): void;
}

export type TriggersStore = UseBoundStore<StoreApi<TriggersState>>;

/** The task has no URL: identity, title and body are the three fields the contract names. */
export function taskForIssue(issue: TriggerIssue): string {
  return `${issue.identifier}: ${issue.title}\n\n${issue.body}`;
}

function withStatus(trigger: TriggerView, status: TriggerVisibleStatus): TriggerView {
  if (trigger.problem !== undefined) return { ...trigger, status };
  return { ...trigger, status };
}

function viewOf(
  entry: TriggerEntry,
  choices: readonly Choice[],
  previous?: TriggerView,
): TriggerView {
  if (entry.problem !== undefined) {
    return {
      slug: entry.slug,
      problem: entry.problem,
      status: { kind: 'refused', sentence: entry.problem },
    };
  }

  const previousConfigured =
    previous !== undefined && previous.problem === undefined ? previous : null;
  const found = choiceFor(choices, entry.workflow);
  const workflowName =
    found?.name ??
    (previousConfigured?.workflow === entry.workflow ? previousConfigured.workflowName : null);
  return {
    ...entry,
    workflowName,
    status: previousConfigured?.status ?? { kind: 'unchecked' },
  };
}

/**
 * One store owns the timer, in-flight checks and durable status mirror. None of those belong to
 * the Triggers screen: switching sections must not stop the clock or permit a second question.
 */
export function createTriggersStore(
  io: TriggerIo,
  clock: TriggerClock,
  run: TriggerRunPath,
): TriggersStore {
  let choices: readonly Choice[] = [];
  let workflowsInFlight: Promise<readonly Choice[]> | null = null;
  let watchHandle: unknown = null;
  let watching = false;
  let generation = 0;
  const checking = new Set<string>();
  const pending = new Map<string, TriggerDelivery>();

  return create<TriggersState>()((set, get) => {
    const configuredNow = (slug: string): ConfiguredTriggerView | null => {
      const trigger = get().triggers.find((one) => one.slug === slug);
      if (trigger === undefined || trigger.problem !== undefined) return null;
      return trigger;
    };

    const setStatus = (slug: string, status: TriggerVisibleStatus, epoch: number): void => {
      if (epoch !== generation) return;
      set((state) => ({
        triggers: state.triggers.map((trigger) =>
          trigger.slug === slug ? withStatus(trigger, status) : trigger,
        ),
      }));
    };

    const currentChoices = (): Promise<readonly Choice[]> => {
      if (workflowsInFlight !== null) return workflowsInFlight;
      const request = run.listWorkflows().then((listed) => toChoices(listed));
      workflowsInFlight = request;
      const release = (): void => {
        if (workflowsInFlight === request) workflowsInFlight = null;
      };
      request.then(release, release);
      return request;
    };

    const rememberChoices = (fresh: readonly Choice[], epoch: number): void => {
      if (epoch !== generation) return;
      choices = fresh;
      set((state) => ({
        triggers: state.triggers.map((trigger) => {
          if (trigger.problem !== undefined) return trigger;
          return {
            ...trigger,
            workflowName: choiceFor(fresh, trigger.workflow)?.name ?? null,
          };
        }),
      }));
    };

    const refuse = (slug: string, error: unknown, fallback: string, epoch: number): void => {
      const current = configuredNow(slug);
      if (current === null || !current.enabled) return;
      setStatus(slug, { kind: 'refused', sentence: why(error, fallback) }, epoch);
    };

    const watchLaunch = (
      slug: string,
      delivery: TriggerDelivery,
      choice: Choice | null,
      epoch: number,
    ): void => {
      let launched: Promise<string | null>;
      try {
        launched = run.launchRun(
          choice,
          run.atOnce(),
          taskForIssue(delivery.issue),
          delivery.claim,
        );
      } catch (error) {
        refuse(slug, error, 'Loadout could not start that trigger.', epoch);
        return;
      }

      /* A successful promise resolves only when the whole run ends. It is not an acceptance
       * receipt. Accepted comes solely from a later Rust poll with its durable receiptAt. */
      void launched
        .then((sentence) => {
          if (sentence === null || epoch !== generation) return;
          const current = configuredNow(slug);
          if (current?.status.kind === 'accepted') return;
          refuse(slug, sentence, 'Loadout could not start that trigger.', epoch);
        })
        .catch((error: unknown) => {
          refuse(slug, error, 'Loadout could not start that trigger.', epoch);
        });
    };

    const handlePending = async (
      slug: string,
      delivery: TriggerDelivery,
      epoch: number,
    ): Promise<void> => {
      pending.set(slug, delivery);
      setStatus(slug, { kind: 'busy', delivery }, epoch);

      try {
        const fresh = await currentChoices();
        if (epoch !== generation) return;
        const current = configuredNow(slug);
        if (current === null || !current.enabled) return;
        rememberChoices(fresh, epoch);
        watchLaunch(slug, delivery, choiceFor(fresh, delivery.claim.workflow), epoch);
      } catch (error) {
        refuse(slug, error, 'Loadout could not read workflows for that trigger.', epoch);
      }
    };

    const pollOne = async (slug: string, epoch: number): Promise<void> => {
      if (checking.has(slug)) return;
      checking.add(slug);
      try {
        const result = await io.checkTrigger(slug);
        if (epoch !== generation) return;
        const current = configuredNow(slug);
        if (current === null || !current.enabled) return;

        if (result.status === 'armed') {
          pending.delete(slug);
          setStatus(slug, { kind: 'armed' }, epoch);
        } else if (result.status === 'busy') {
          const held = pending.get(slug);
          if (held !== undefined) setStatus(slug, { kind: 'busy', delivery: held }, epoch);
        } else if (result.status === 'accepted') {
          pending.delete(slug);
          setStatus(
            slug,
            {
              kind: 'accepted',
              /* The accepted workflow was frozen in the claim. The config file may have changed
               * meanwhile, so the row's current workflowName is not authoritative for receipt. */
              workflow: choiceFor(choices, result.workflow)?.name ?? result.workflow,
              receiptAt: result.receiptAt,
            },
            epoch,
          );
        } else {
          await handlePending(slug, result.delivery, epoch);
        }
      } catch (error) {
        refuse(slug, error, `Loadout could not check ${slug}.`, epoch);
      } finally {
        checking.delete(slug);
      }
    };

    const poll = async (epoch: number): Promise<void> => {
      const enabled = get().triggers.filter(
        (trigger): trigger is ConfiguredTriggerView =>
          trigger.problem === undefined && trigger.enabled,
      );
      await Promise.all(enabled.map((trigger) => pollOne(trigger.slug, epoch)));
    };

    return {
      triggers: [],
      said: null,

      load: async () => {
        const epoch = generation;
        try {
          const [entries, listed] = await Promise.all([io.listTriggers(), run.listWorkflows()]);
          if (epoch !== generation) return;
          choices = toChoices(listed);
          const before = new Map(get().triggers.map((trigger) => [trigger.slug, trigger]));
          set({
            triggers: entries.map((entry) => viewOf(entry, choices, before.get(entry.slug))),
            said: null,
          });
        } catch (error) {
          if (epoch !== generation) return;
          set({ said: why(error, 'Loadout could not read your triggers.') });
        }
      },

      toggle: async (slug, enabled) => {
        const before = configuredNow(slug);
        if (before === null) return;
        try {
          const saved = await io.setTriggerEnabled(slug, enabled);
          set((state) => ({
            triggers: state.triggers.map((trigger) =>
              trigger.slug === slug ? viewOf(saved, choices, trigger) : trigger,
            ),
          }));
        } catch (error) {
          const still = configuredNow(slug);
          if (still === null) return;
          const side = still.enabled ? 'on' : 'off';
          set((state) => ({
            triggers: state.triggers.map((trigger) =>
              trigger.slug === slug
                ? withStatus(trigger, {
                    kind: 'refused',
                    sentence: why(
                      error,
                      `Loadout could not save that trigger, so it is still ${side}.`,
                    ),
                  })
                : trigger,
            ),
          }));
        }
      },

      tick: () => poll(generation),

      startWatching: () => {
        if (watching) return;
        watching = true;
        generation += 1;
        const epoch = generation;
        /* Root calls only startWatching. An empty singleton therefore has to load here before
         * the first interval; injected stores that were explicitly seeded keep that seed. */
        if (get().triggers.length === 0) void get().load();
        watchHandle = clock.setInterval(() => {
          if (!watching || epoch !== generation) return;
          void poll(epoch);
        }, TRIGGER_WATCH_INTERVAL_MS);
      },

      stopWatching: () => {
        generation += 1;
        if (!watching) return;
        watching = false;
        clock.clearInterval(watchHandle);
        watchHandle = null;
      },
    };
  });
}

const DISK: TriggerIo = {
  listTriggers: triggerIo.listTriggers,
  setTriggerEnabled: triggerIo.setTriggerEnabled,
  checkTrigger: triggerIo.checkTrigger,
};

const WINDOW_CLOCK: TriggerClock = {
  setInterval: (callback, milliseconds) => globalThis.setInterval(callback, milliseconds),
  clearInterval: (handle) => globalThis.clearInterval(handle as ReturnType<typeof setInterval>),
};

const RUN_PATH: TriggerRunPath = {
  listWorkflows,
  launchRun,
  atOnce: chosenAtOnce,
};

/** One production store for the root watcher and the screen; section changes never recreate it. */
export const useTriggers = createTriggersStore(DISK, WINDOW_CLOCK, RUN_PATH);
