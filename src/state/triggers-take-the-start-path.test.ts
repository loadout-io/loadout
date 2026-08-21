import { existsSync, readFileSync } from 'node:fs';
import { dirname, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';
import { beforeEach, describe, expect, it, vi } from 'vitest';

import type { Choice, Listed } from '../sections/run/choices';
import { GONE_FROM_DISK, launchRun, NOTHING_TO_RUN, NO_FOLDER } from '../sections/run/launch';
import { start } from '../sections/run/io';
import type {
  TriggerClaim,
  TriggerDelivery,
  TriggerIo,
  TriggerIssue,
  TriggerPoll,
} from '../sections/triggers/io';
import { useWorkspaces } from './workspaces';
import { createTriggersStore, taskForIssue } from './triggers';
import type { TriggerClock, TriggerRunPath, TriggerView } from './triggers';

const { invoked } = vi.hoisted(() => ({
  invoked: vi.fn((..._args: unknown[]) => Promise.resolve(undefined)),
}));
vi.mock('@tauri-apps/api/core', () => ({
  invoke: invoked,
  Channel: class {
    public onmessage: ((message: unknown) => void) | null = null;
  },
}));

const ROOT = resolve(dirname(fileURLToPath(import.meta.url)), '..', '..');
const ISSUE: TriggerIssue = {
  id: 'issue-db-id',
  identifier: 'LIN-42',
  title: 'Fix the timeout handoff',
  url: 'https://linear.example/LIN-42',
  body: 'The completed analysis must reach the next step.',
  updatedAt: '2026-08-21T01:52:00.000Z',
};
const CLAIM: TriggerClaim = {
  slug: 'assigned-to-me',
  deliveryId: 'delivery-7',
  workflow: 'analysis.json',
  runId: '0198ca82-ded0-7000-8000-000000000042',
};
const DELIVERY: TriggerDelivery = { claim: CLAIM, issue: ISSUE, createdAt: 1_787_278_329_700 };
const TASK = 'LIN-42: Fix the timeout handoff\n\nThe completed analysis must reach the next step.';

const CHOICE: Choice = {
  path: 'analysis.json',
  name: 'Analysis',
  steps: [{ id: 'analyse', name: 'Analysis', state: 'pending' }],
};

const LISTED: Listed = {
  path: 'analysis.json',
  workflow: {
    format: 1,
    id: 'workflow-id',
    name: 'Analysis',
    steps: [{ kind: 'checkpoint', id: 'analyse', name: 'Analysis', at: { x: 0, y: 0 } }],
    links: [],
  },
};

const CLOCK: TriggerClock = {
  setInterval: () => 1,
  clearInterval: () => undefined,
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

function view(): TriggerView {
  return {
    slug: CLAIM.slug,
    source: 'Linear',
    condition: 'Assigned to you',
    workflow: CLAIM.workflow,
    workflowName: 'Analysis',
    enabled: true,
    status: { kind: 'armed' },
  };
}

function triggerIo(): TriggerIo {
  return {
    listTriggers: async () => [],
    setTriggerEnabled: async (slug, enabled) => ({
      slug,
      source: 'Linear',
      condition: 'Assigned to you',
      workflow: CLAIM.workflow,
      enabled,
    }),
    checkTrigger: async () => ({ status: 'pending', delivery: DELIVERY }),
  };
}

beforeEach(() => {
  invoked.mockReset();
  invoked.mockResolvedValue(undefined);
  useWorkspaces.setState({ all: [], activeId: null, said: null });
});

describe('a trigger takes the same launch path as Start', () => {
  it('launches the real workflow choice with the durable delivery reference and canonical issue task', async () => {
    const launched = vi.fn<TriggerRunPath['launchRun']>(
      () => new Promise<string | null>(() => undefined),
    );
    const run: TriggerRunPath = {
      listWorkflows: async () => [LISTED],
      launchRun: launched,
      atOnce: () => 4,
    };
    const store = createTriggersStore(triggerIo(), CLOCK, run);
    store.setState({ triggers: [view()] });
    await store.getState().tick();

    expect(taskForIssue(ISSUE)).toBe(TASK);
    expect(launched).toHaveBeenCalledTimes(1);
    expect(launched).toHaveBeenCalledWith(CHOICE, 4, TASK, CLAIM);
  });

  it('launchRun forwards a trigger delivery reference to the existing run_workflow invocation', async () => {
    useWorkspaces.setState({
      all: [{ id: '/project', name: 'Project', folder: '/project' }],
      activeId: '/project',
      said: null,
    });
    await launchRun(CHOICE, 4, TASK, CLAIM);

    expect(invoked).toHaveBeenCalledTimes(1);
    expect(invoked).toHaveBeenCalledWith(
      'run_workflow',
      expect.objectContaining({
        fileName: 'analysis.json',
        howManyAtOnce: 4,
        folder: '/project',
        task: TASK,
        claim: CLAIM,
      }),
    );
  });

  it('keeps pending visible until Rust returns its later durable acceptance time', async () => {
    let polls = 0;
    const acceptedAt = DELIVERY.createdAt + 9_000;
    const io: TriggerIo = {
      listTriggers: async () => [],
      setTriggerEnabled: async () => view(),
      checkTrigger: async () => {
        polls += 1;
        return polls === 1
          ? { status: 'pending' as const, delivery: DELIVERY }
          : { status: 'accepted' as const, workflow: CLAIM.workflow, receiptAt: acceptedAt };
      },
    };
    const runEnded = deferred<string | null>();
    const launched = vi.fn<TriggerRunPath['launchRun']>(() => runEnded.promise);
    const store = createTriggersStore(io, CLOCK, {
      listWorkflows: async () => [LISTED],
      launchRun: launched,
      atOnce: () => 4,
    });
    store.setState({ triggers: [view()] });

    await store.getState().tick();
    expect(store.getState().triggers[0]?.status).toEqual({ kind: 'busy', delivery: DELIVERY });
    expect(store.getState().triggers[0]?.status).not.toEqual(
      expect.objectContaining({ kind: 'accepted', receiptAt: DELIVERY.createdAt }),
    );

    runEnded.resolve(null);
    await vi.waitFor(() => {
      expect(polls).toBe(2);
      expect(store.getState().triggers[0]?.status).toEqual({
        kind: 'accepted',
        workflow: 'Analysis',
        receiptAt: acceptedAt,
      });
    });
  });

  it('runs the result refresh after an overlapping check releases the slug', async () => {
    const acceptedAt = DELIVERY.createdAt + 12_000;
    const overlappingCheck = deferred<TriggerPoll>();
    const runEnded = deferred<string | null>();
    let polls = 0;
    const io: TriggerIo = {
      listTriggers: async () => [],
      setTriggerEnabled: async () => view(),
      checkTrigger: () => {
        polls += 1;
        if (polls === 1) {
          return Promise.resolve({ status: 'pending', delivery: DELIVERY });
        }
        if (polls === 2) return overlappingCheck.promise;
        return Promise.resolve({
          status: 'accepted',
          workflow: CLAIM.workflow,
          receiptAt: acceptedAt,
        });
      },
    };
    const store = createTriggersStore(io, CLOCK, {
      listWorkflows: async () => [LISTED],
      launchRun: () => runEnded.promise,
      atOnce: () => 4,
    });
    store.setState({ triggers: [view()] });

    await store.getState().tick();
    const overlapping = store.getState().tick();
    expect(polls).toBe(2);

    runEnded.resolve(null);
    await Promise.resolve();
    expect(polls, 'the completion must not overlap the check already holding this slug').toBe(2);

    overlappingCheck.resolve({ status: 'busy' });
    await overlapping;
    await vi.waitFor(() => {
      expect(polls).toBe(3);
      expect(store.getState().triggers[0]?.status).toEqual({
        kind: 'accepted',
        workflow: 'Analysis',
        receiptAt: acceptedAt,
      });
    });
  });

  it('reconciles an overlapping completion while disabled without launching the delivery again', async () => {
    const overlappingCheck = deferred<TriggerPoll>();
    const runEnded = deferred<string | null>();
    const acceptedAt = DELIVERY.createdAt + 15_000;
    let polls = 0;
    const io: TriggerIo = {
      listTriggers: async () => [],
      setTriggerEnabled: async (slug, enabled) => ({
        slug,
        source: 'Linear',
        condition: 'assigned-to-me',
        workflow: CLAIM.workflow,
        enabled,
      }),
      checkTrigger: () => {
        polls += 1;
        if (polls === 1) {
          return Promise.resolve({ status: 'pending', delivery: DELIVERY });
        }
        if (polls === 2) return overlappingCheck.promise;
        return Promise.resolve({
          status: 'accepted',
          workflow: CLAIM.workflow,
          receiptAt: acceptedAt,
        });
      },
    };
    const launched = vi.fn<TriggerRunPath['launchRun']>(() => runEnded.promise);
    const store = createTriggersStore(io, CLOCK, {
      listWorkflows: async () => [LISTED],
      launchRun: launched,
      atOnce: () => 4,
    });
    store.setState({ triggers: [view()] });

    await store.getState().tick();
    const overlapping = store.getState().tick();
    expect(polls).toBe(2);
    runEnded.resolve(null);
    await Promise.resolve();
    await store.getState().toggle(CLAIM.slug, false);

    overlappingCheck.resolve({ status: 'busy' });
    await overlapping;
    await vi.waitFor(() => {
      expect(store.getState().triggers[0]?.enabled).toBe(false);
      expect(store.getState().triggers[0]?.status).toEqual({
        kind: 'accepted',
        workflow: 'Analysis',
        receiptAt: acceptedAt,
      });
    });
    expect(polls).toBe(3);
    expect(
      launched,
      'the one result check must not launch an already handled delivery again',
    ).toHaveBeenCalledTimes(1);
  });

  it.each(['resolved sentence', 'rejected promise'] as const)(
    'shows a launch refusal after the trigger was disabled: %s',
    async (outcome) => {
      const launch = deferred<string | null>();
      let polls = 0;
      const refusal = 'That trigger run could not be accepted.';
      const io: TriggerIo = {
        listTriggers: async () => [],
        setTriggerEnabled: async (slug, enabled) => ({
          slug,
          source: 'Linear',
          condition: 'assigned-to-me',
          workflow: CLAIM.workflow,
          enabled,
        }),
        checkTrigger: async () => {
          polls += 1;
          return { status: 'pending', delivery: DELIVERY };
        },
      };
      const store = createTriggersStore(io, CLOCK, {
        listWorkflows: async () => [LISTED],
        launchRun: () => launch.promise,
        atOnce: () => 4,
      });
      store.setState({ triggers: [view()] });
      await store.getState().tick();
      await store.getState().toggle(CLAIM.slug, false);

      if (outcome === 'resolved sentence') launch.resolve(refusal);
      else launch.reject(refusal);
      await vi.waitFor(() => {
        expect(store.getState().triggers[0]?.status).toEqual({
          kind: 'refused',
          sentence: refusal,
        });
      });
      expect(store.getState().triggers[0]?.enabled).toBe(false);
      expect(polls).toBe(1);
    },
  );

  it('carries an explicit null trigger reference for an ordinary manual Start', async () => {
    await start('analysis.json', 2, { name: 'Analysis', steps: CHOICE.steps }, '/project', null);
    const args = invoked.mock.calls[0]?.[1] as Record<string, unknown> | undefined;
    expect(args).toBeDefined();
    expect(args).toHaveProperty('claim', null);
  });

  it('preserves every existing launch refusal, including Rust text word for word', async () => {
    expect(await launchRun(null, 2)).toBe(GONE_FROM_DISK);
    expect(await launchRun({ ...CHOICE, steps: [] }, 2)).toBe(NOTHING_TO_RUN);
    expect(await launchRun(CHOICE, 2)).toBe(NO_FOLDER);

    useWorkspaces.setState({
      all: [{ id: '/project', name: 'Project', folder: '/project' }],
      activeId: '/project',
      said: null,
    });
    const rustSaid = 'That run is already going. Press Stop first.';
    invoked.mockRejectedValueOnce(rustSaid);
    expect(await launchRun(CHOICE, 2, TASK, CLAIM)).toBe(rustSaid);
  });

  it('does not bypass launchRun through run/io, invoke or the optimistic window state', () => {
    const path = resolve(ROOT, 'src/state/triggers.ts');
    const source = existsSync(path) ? readFileSync(path, 'utf8') : '';
    const code = source.replace(/\/\*[\s\S]*?\*\//g, ' ').replace(/\/\/.*$/gm, ' ');
    expect(code).not.toMatch(/from\s+['"][^'"]*sections\/run\/io['"]/);
    expect(code).not.toMatch(/\binvoke\s*\(/);
    expect(code).not.toMatch(/\buseRun\b|RunState\.workflow|workflow\s*!==\s*['"]{2}/);
    expect(code).toMatch(/from\s+['"][^'"]*sections\/run\/launch['"]/);
  });
});
