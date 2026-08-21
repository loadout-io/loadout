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
  it('launches the real workflow choice with the durable claim and canonical issue task', async () => {
    const launched = vi.fn<TriggerRunPath['launchRun']>(async () => null);
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

  it('launchRun forwards a trigger claim to the existing run_workflow invocation', async () => {
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

  it('carries an explicit null claim for an ordinary manual Start', async () => {
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
