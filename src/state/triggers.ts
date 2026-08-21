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
  TriggerClaim,
  TriggerCadence,
  TriggerDraft,
  TriggerDelivery,
  TriggerEntry,
  TriggerIo,
  TriggerIssue,
  TriggerSnapshot,
} from '../sections/triggers/io';
import type { TriggerConnectionState, TriggerWorkflowOption } from '../sections/triggers/form';
import { list as listWorkflows } from '../sections/workflows/io';

/** The interval is named so the scheduler and the application share one answer. */
export const TRIGGER_WATCH_INTERVAL_MS = 60_000;

function cadenceMilliseconds(minutes: TriggerCadence): number {
  return minutes * TRIGGER_WATCH_INTERVAL_MS;
}

export interface TriggerClock {
  now(): number;
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
    deliveryReference: TriggerClaim | null,
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

export type TriggerMutationResult =
  { readonly ok: true } | { readonly ok: false; readonly refusal: string };

export interface TriggersState {
  readonly triggers: readonly TriggerView[];
  readonly workflows: readonly TriggerWorkflowOption[];
  readonly connection: TriggerConnectionState;
  /** A library-level refusal. Per-trigger refusals live on their one row. */
  readonly said: string | null;
  load(): Promise<void>;
  toggle(slug: string, enabled: boolean): Promise<void>;
  create(draft: TriggerDraft): Promise<TriggerMutationResult>;
  update(expected: TriggerSnapshot, draft: TriggerDraft): Promise<TriggerMutationResult>;
  remove(expected: TriggerSnapshot): Promise<TriggerMutationResult>;
  testConnection(slug: string | null, apiKey: string | null): Promise<boolean>;
  resetEditorFeedback(): void;
  tick(): Promise<void>;
  startWatching(): void;
  stopWatching(): void;
}

export type TriggersStore = UseBoundStore<StoreApi<TriggersState>>;

interface ToggleRefusal {
  /** The exact status object installed by the failed write, so later polling cannot be mistaken for it. */
  readonly status: Extract<TriggerVisibleStatus, { readonly kind: 'refused' }>;
  /** The useful row state hidden temporarily by that write refusal. */
  readonly before: TriggerVisibleStatus;
}

interface LibraryLoad {
  readonly request: { epoch: number; readonly mutation: number };
  readonly promise: Promise<void>;
}

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
  let connectionRequest = 0;
  let libraryMutation = 0;
  let libraryLoad: LibraryLoad | null = null;
  const checking = new Set<string>();
  const pending = new Map<string, TriggerDelivery>();
  const refreshedAfterRun = new Map<string, string>();
  const refreshAfterChecking = new Map<string, number>();
  const toggleRefusals = new Map<string, ToggleRefusal>();
  /* One heartbeat serves every trigger, but each file owns its next due time. Keeping this
   * outside React state prevents a minute tick from repainting the whole library. */
  const nextDue = new Map<string, number>();

  return create<TriggersState>()((set, get) => {
    const scheduleFromNow = (trigger: ConfiguredTriggerEntry): void => {
      if (!watching || !trigger.enabled) {
        nextDue.delete(trigger.slug);
        return;
      }
      nextDue.set(trigger.slug, clock.now() + cadenceMilliseconds(trigger.pollEveryMinutes));
    };

    const reconcileSchedule = (
      fresh: readonly TriggerView[],
      before: ReadonlyMap<string, TriggerView>,
    ): void => {
      if (!watching) return;
      const live = new Set<string>();
      for (const trigger of fresh) {
        if (trigger.problem !== undefined || !trigger.enabled) continue;
        live.add(trigger.slug);
        const previous = before.get(trigger.slug);
        const unchanged =
          previous !== undefined &&
          previous.problem === undefined &&
          previous.enabled &&
          previous.pollEveryMinutes === trigger.pollEveryMinutes;
        if (!unchanged || !nextDue.has(trigger.slug)) scheduleFromNow(trigger);
      }
      for (const slug of nextDue.keys()) {
        if (!live.has(slug)) nextDue.delete(slug);
      }
    };

    const forget = (slug: string): void => {
      nextDue.delete(slug);
      checking.delete(slug);
      pending.delete(slug);
      refreshedAfterRun.delete(slug);
      refreshAfterChecking.delete(slug);
      toggleRefusals.delete(slug);
    };

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

    const refuse = (
      slug: string,
      error: unknown,
      fallback: string,
      epoch: number,
      includeDisabled = false,
    ): void => {
      const current = configuredNow(slug);
      if (current === null || (!current.enabled && !includeDisabled)) return;
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
          if (epoch !== generation) return;
          if (sentence === null) {
            const current = configuredNow(slug);
            if (current === null) return;
            const deliveryId = delivery.claim.deliveryId;
            /* The command resolves only after the run settles. Ask Rust for its durable receipt
             * now rather than leaving a finished run labelled busy until the next minute. One
             * refresh per delivery also prevents a dishonest Pending adapter from spinning. */
            if (refreshedAfterRun.get(slug) === deliveryId) return;
            refreshedAfterRun.set(slug, deliveryId);
            if (checking.has(slug)) {
              refreshAfterChecking.set(slug, epoch);
            } else {
              void pollOne(slug, epoch, true);
            }
            return;
          }
          const current = configuredNow(slug);
          if (current?.status.kind === 'accepted') return;
          refuse(slug, sentence, 'Loadout could not start that trigger.', epoch, true);
        })
        .catch((error: unknown) => {
          refuse(slug, error, 'Loadout could not start that trigger.', epoch, true);
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

    const pollOne = async (
      slug: string,
      epoch: number,
      completionReceipt = false,
    ): Promise<void> => {
      if (checking.has(slug)) return;
      checking.add(slug);
      try {
        const result = await io.checkTrigger(slug);
        if (epoch !== generation) return;
        const current = configuredNow(slug);
        if (current === null || (!current.enabled && !completionReceipt)) return;

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
          if (completionReceipt) {
            pending.set(slug, result.delivery);
            setStatus(slug, { kind: 'busy', delivery: result.delivery }, epoch);
          } else {
            await handlePending(slug, result.delivery, epoch);
          }
        }
      } catch (error) {
        refuse(slug, error, `Loadout could not check ${slug}.`, epoch, completionReceipt);
      } finally {
        checking.delete(slug);
        const refreshEpoch = refreshAfterChecking.get(slug);
        if (refreshEpoch !== undefined) {
          refreshAfterChecking.delete(slug);
          const current = configuredNow(slug);
          if (refreshEpoch === generation && current !== null) {
            void pollOne(slug, refreshEpoch, true);
          }
        }
      }
    };

    const poll = async (epoch: number, onlyDue = false): Promise<void> => {
      const enabled = get().triggers.filter(
        (trigger): trigger is ConfiguredTriggerView =>
          trigger.problem === undefined && trigger.enabled,
      );
      const now = clock.now();
      const due = onlyDue
        ? enabled.filter((trigger) => {
            const at = nextDue.get(trigger.slug);
            if (at === undefined) {
              scheduleFromNow(trigger);
              return false;
            }
            if (at > now) return false;
            /* Advance before asking. A slow question therefore cannot create a queue of
             * overlapping catch-up requests for the same slug. */
            nextDue.set(trigger.slug, now + cadenceMilliseconds(trigger.pollEveryMinutes));
            return true;
          })
        : enabled;
      await Promise.all(due.map((trigger) => pollOne(trigger.slug, epoch)));
    };

    const loadLibrary = (): Promise<void> => {
      if (libraryLoad !== null) {
        /* Root and TriggersScreen can request the same read in adjacent effects. The newest
         * application epoch is allowed to consume that one result; Stop still invalidates it. */
        libraryLoad.request.epoch = generation;
        return libraryLoad.promise;
      }

      const request = { epoch: generation, mutation: libraryMutation };
      const promise = Promise.all([io.listTriggers(), run.listWorkflows()])
        .then(([entries, listed]) => {
          if (request.epoch !== generation || request.mutation !== libraryMutation) return;
          choices = toChoices(listed);
          const before = new Map(get().triggers.map((trigger) => [trigger.slug, trigger]));
          const fresh = entries.map((entry) => viewOf(entry, choices, before.get(entry.slug)));
          reconcileSchedule(fresh, before);
          set({
            triggers: fresh,
            workflows: choices.map(({ path, name }) => ({ path, name })),
            said: null,
          });
        })
        .catch((error: unknown) => {
          if (request.epoch !== generation || request.mutation !== libraryMutation) return;
          set({ said: why(error, 'Loadout could not read your triggers.') });
        });
      const load = { request, promise };
      libraryLoad = load;
      const release = (): void => {
        if (libraryLoad === load) libraryLoad = null;
      };
      promise.then(release, release);
      return promise;
    };

    return {
      triggers: [],
      workflows: [],
      connection: { kind: 'idle' },
      said: null,

      load: loadLibrary,

      toggle: async (slug, enabled) => {
        const before = configuredNow(slug);
        if (before === null) return;
        try {
          const saved = await io.setTriggerEnabled(slug, enabled);
          libraryMutation += 1;
          const refused = toggleRefusals.get(slug);
          set((state) => ({
            triggers: state.triggers.map((trigger) => {
              if (trigger.slug !== slug) return trigger;
              /* A successful retry removes only the write refusal it is recovering from. A poll
               * or launch refusal that arrived meanwhile is a different fact and must survive. */
              const previous =
                refused !== undefined && trigger.status === refused.status
                  ? withStatus(trigger, refused.before)
                  : trigger;
              return viewOf(saved, choices, previous);
            }),
          }));
          if (saved.problem === undefined) scheduleFromNow(saved);
          toggleRefusals.delete(slug);
        } catch (error) {
          const still = configuredNow(slug);
          if (still === null) return;
          const side = still.enabled ? 'on' : 'off';
          const prior = toggleRefusals.get(slug);
          const refused = {
            status: {
              kind: 'refused',
              sentence: why(error, `Loadout could not save that trigger, so it is still ${side}.`),
            },
            /* Repeated failed retries keep the last meaningful status, not the first refusal. */
            before: prior?.status === still.status ? prior.before : still.status,
          } satisfies ToggleRefusal;
          toggleRefusals.set(slug, refused);
          set((state) => ({
            triggers: state.triggers.map((trigger) =>
              trigger.slug === slug ? withStatus(trigger, refused.status) : trigger,
            ),
          }));
        }
      },

      create: async (draft) => {
        try {
          const saved = await io.createTrigger(draft);
          libraryMutation += 1;
          set((state) => ({
            triggers: [
              ...state.triggers.filter((trigger) => trigger.slug !== saved.slug),
              viewOf(saved, choices),
            ],
            said: null,
          }));
          scheduleFromNow(saved);
          return { ok: true };
        } catch (error) {
          return {
            ok: false,
            refusal: why(error, 'Loadout could not save that trigger.'),
          };
        }
      },

      update: async (expected, draft) => {
        try {
          const saved = await io.updateTrigger(expected.slug, expected, draft);
          libraryMutation += 1;
          set((state) => ({
            triggers: state.triggers.map((trigger) =>
              trigger.slug === expected.slug ? viewOf(saved, choices, trigger) : trigger,
            ),
            said: null,
          }));
          scheduleFromNow(saved);
          return { ok: true };
        } catch (error) {
          return {
            ok: false,
            refusal: why(error, 'Loadout could not save that trigger.'),
          };
        }
      },

      remove: async (expected) => {
        try {
          await io.deleteTrigger(expected.slug, expected);
          libraryMutation += 1;
          forget(expected.slug);
          set((state) => ({
            triggers: state.triggers.filter((trigger) => trigger.slug !== expected.slug),
            said: null,
          }));
          return { ok: true };
        } catch (error) {
          return {
            ok: false,
            refusal: why(error, 'Loadout could not delete that trigger.'),
          };
        }
      },

      testConnection: async (slug, apiKey) => {
        const request = connectionRequest + 1;
        connectionRequest = request;
        set({ connection: { kind: 'testing' } });
        try {
          await io.testLinearConnection(slug, apiKey);
          if (request !== connectionRequest) return false;
          set({
            connection: { kind: 'worked', sentence: 'Linear connection works.' },
          });
          return true;
        } catch (error) {
          if (request !== connectionRequest) return false;
          set({
            connection: {
              kind: 'refused',
              sentence: why(error, 'Loadout could not test that Linear connection.'),
            },
          });
          return false;
        }
      },

      resetEditorFeedback: () => {
        connectionRequest += 1;
        set({ connection: { kind: 'idle' } });
      },

      tick: () => poll(generation),

      startWatching: () => {
        if (watching) return;
        watching = true;
        generation += 1;
        const epoch = generation;
        nextDue.clear();
        for (const trigger of get().triggers) {
          if (trigger.problem === undefined) scheduleFromNow(trigger);
        }
        /* Root calls only startWatching. An empty singleton therefore has to load here before
         * the first interval; injected stores that were explicitly seeded keep that seed. */
        if (get().triggers.length === 0) void get().load();
        watchHandle = clock.setInterval(() => {
          if (!watching || epoch !== generation) return;
          void poll(epoch, true);
        }, TRIGGER_WATCH_INTERVAL_MS);
      },

      stopWatching: () => {
        generation += 1;
        if (!watching) return;
        watching = false;
        clock.clearInterval(watchHandle);
        watchHandle = null;
        nextDue.clear();
      },
    };
  });
}

const DISK: TriggerIo = {
  listTriggers: triggerIo.listTriggers,
  setTriggerEnabled: triggerIo.setTriggerEnabled,
  checkTrigger: triggerIo.checkTrigger,
  createTrigger: triggerIo.createTrigger,
  updateTrigger: triggerIo.updateTrigger,
  deleteTrigger: triggerIo.deleteTrigger,
  testLinearConnection: triggerIo.testLinearConnection,
};

const WINDOW_CLOCK: TriggerClock = {
  now: () => Date.now(),
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
