/* T-151 AC-2: Run never crosses before the visible workflow revision is durable.
 *
 * The tests use only visible controls and the real e2e harness tape. They never call the
 * workflow store, saveNow, requestRun, or a component handler directly.
 */
import { afterAll, beforeAll, describe, expect, it } from 'vitest';

import type { RunningApp, TauriCall, TauriReply } from '../harness';
import { closeEverything, openApp } from '../harness';

const FOLDER = '/Users/somebody/Projects/t151-visible-revision';
const WORKSPACE = { id: FOLDER, name: 'T151 visible revision', folder: FOLDER };
const PATH = 't151.json';
const INITIAL = workflowNamed('T151 initial revision');
const ENTRY = { path: PATH, workflow: INITIAL };
const AGENT = {
  id: 'agent-t151',
  name: 'T151 Builder',
  summary: 'Exercises the visible revision boundary',
  skills: [],
};

const SWITCH = '[data-section-switch="workflows"]';
const SCREEN = 'main[data-section="workflows"]';
const TILE = 'main [data-tile]';
const NAME = 'input[aria-label="Workflow name"]';
const CANVAS = '[data-testid="rf__wrapper"]';
const APPEARS = 6_000;

function workflowNamed(name: string) {
  return {
    format: 1 as const,
    id: 'wf-t151-visible-revision',
    name,
    steps: [
      {
        kind: 'agent' as const,
        id: 's_build',
        name: 'Build',
        agent: 'agent-t151',
        overrides: {},
        copies: 1,
        instructions: 'Build the requested change.',
        skills: 'all' as const,
        folder: { use: 'project' as const },
        handover: 'notes' as const,
        at: { x: 24, y: 24 },
      },
    ],
    links: [],
  };
}

function copies<T>(value: T, count = 20): readonly { readonly value: T }[] {
  return Array.from({ length: count }, () => ({ value }));
}

function baseScene(
  loads: readonly unknown[] = [INITIAL, INITIAL, INITIAL, INITIAL],
): Record<string, readonly TauriReply[]> {
  return {
    list_workspaces: copies([WORKSPACE]),
    list_workflows: copies([ENTRY]),
    load_workflow: loads.map((value) => ({ value })),
    check_workflow: copies([]),
    list_agents: copies([AGENT]),
    list_skills: copies([]),
  };
}

async function openEditor(replies: Record<string, readonly TauriReply[]>): Promise<RunningApp> {
  const app = await openApp({ replies });
  await app.page.locator(SWITCH).click();
  await app.page.locator(SCREEN).waitFor({ state: 'attached', timeout: APPEARS });
  await app.page.locator(TILE).first().click();
  await app.page.locator(NAME).waitFor({ state: 'visible', timeout: APPEARS });
  return app;
}

async function waitForCalls(
  app: RunningApp,
  command: string,
  count = 1,
): Promise<readonly TauriCall[]> {
  const deadline = Date.now() + APPEARS;
  while (Date.now() < deadline) {
    const calls = (await app.calls()).filter((call) => call.cmd === command);
    if (calls.length >= count) return calls;
    await app.page.waitForTimeout(25);
  }
  return (await app.calls()).filter((call) => call.cmd === command);
}

function sentWorkflow(call: TauriCall | undefined): Record<string, unknown> {
  const workflow = call?.args['workflow'];
  return typeof workflow === 'object' && workflow !== null
    ? (workflow as Record<string, unknown>)
    : {};
}

beforeAll(async () => {
  const warm = await openApp();
  await warm.close();
}, 180_000);

afterAll(async () => {
  await closeEverything();
}, 30_000);

describe('Run never starts from a workflow revision older than the visible edit', () => {
  it('waits for an older save and at least the visible revision before run_workflow crosses', async () => {
    const replies = baseScene();
    replies['save_workflow'] = [
      { deferred: 'older-save' },
      { deferred: 'visible-save' },
    ];
    replies['run_workflow'] = [{ value: null }];
    const app = await openEditor(replies);
    try {
      await app.page.locator(NAME).fill('T151 older in flight');
      expect((await waitForCalls(app, 'save_workflow')).length).toBe(1);

      await app.page.locator(NAME).fill('T151 visible at Run');
      await app.page.locator(CANVAS).getByRole('button', { name: 'Run', exact: true }).click();
      await app.page.waitForTimeout(550);

      const whileOlderWasPending = await app.calls();
      await app.settle('older-save', { value: null });
      const saves = await waitForCalls(app, 'save_workflow', 2);
      const whileVisibleWasPending = await app.calls();
      await app.settle('visible-save', { value: null });
      const runs = await waitForCalls(app, 'run_workflow');
      const tape = await app.calls();
      const savePositions = tape
        .map((call, index) => ({ call, index }))
        .filter(({ call }) => call.cmd === 'save_workflow')
        .map(({ index }) => index);
      const runPosition = tape.findIndex((call) => call.cmd === 'run_workflow');

      expect(
        whileOlderWasPending.filter((call) => call.cmd === 'save_workflow').length,
        'the visible revision started writing before the older in-flight save had finished',
      ).toBe(1);
      expect(
        whileOlderWasPending.filter((call) => call.cmd === 'run_workflow').length,
        'run_workflow crossed while the older save still blocked the visible revision',
      ).toBe(0);
      expect(saves.length, 'the visible revision never followed the older save').toBe(2);
      expect(sentWorkflow(saves[0])['name']).toBe('T151 older in flight');
      expect(sentWorkflow(saves[1])['name']).toBe('T151 visible at Run');
      expect(
        whileVisibleWasPending.filter((call) => call.cmd === 'run_workflow').length,
        'run_workflow crossed before the visible revision had finished saving',
      ).toBe(0);
      expect(runs.length, 'Run never started after at least the visible revision was confirmed').toBe(
        1,
      );
      expect(runs[0]?.args['fileName']).toBe(PATH);
      expect(savePositions.length).toBeGreaterThanOrEqual(2);
      expect(savePositions[0]).toBeLessThan(savePositions[1] ?? -1);
      expect(savePositions[1]).toBeLessThan(runPosition);
    } finally {
      await app.close();
    }
  }, 60_000);

  it('keeps a refused Run in the editor and a later successful save unlocks it', async () => {
    const replies = baseScene();
    replies['save_workflow'] = [
      { error: 'T151 injected save refusal' },
      { value: null },
    ];
    replies['run_workflow'] = [{ value: null }];
    const app = await openEditor(replies);
    try {
      await app.page.locator(NAME).fill('T151 refused revision');
      await app.page.locator(CANVAS).getByRole('button', { name: 'Run', exact: true }).click();
      await waitForCalls(app, 'save_workflow');
      await app.page.waitForTimeout(100);

      const stayedInEditor = await app.page.locator(NAME).count();
      const refusal = await app.page.locator('[data-could-not-save]').allInnerTexts();
      const runsAfterRefusal = (await app.calls()).filter(
        (call) => call.cmd === 'run_workflow',
      ).length;

      if (stayedInEditor === 1) {
        await app.page.locator(NAME).fill('T151 recovered revision');
        await app.page.locator(CANVAS).getByRole('button', { name: 'Run', exact: true }).click();
        await waitForCalls(app, 'run_workflow');
      }

      const saves = (await app.calls()).filter((call) => call.cmd === 'save_workflow');
      const runs = (await app.calls()).filter((call) => call.cmd === 'run_workflow');
      expect(stayedInEditor, 'a refused save navigated away from the visible unsaved edit').toBe(1);
      expect(refusal.join(' ').toLowerCase()).toContain('not saved');
      expect(runsAfterRefusal, 'Run started even though its captured revision was refused').toBe(0);
      expect(saves.length, 'the successful retry never crossed production workflow IO').toBe(2);
      expect(sentWorkflow(saves[1])['name']).toBe('T151 recovered revision');
      expect(runs.length, 'the successful retry did not unlock Run without a restart').toBe(1);
    } finally {
      await app.close();
    }
  }, 60_000);
});
