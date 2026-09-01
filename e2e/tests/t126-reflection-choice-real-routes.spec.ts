/* T-126 AC-2: the visible reflection choice reaches every human route through the mounted app.
 *
 * Nothing below imports the start policy, request store, component handlers or setters. The
 * only inputs are visible controls in Chromium; the only transport witness is the real
 * window.__TAURI_INTERNALS__ tape installed by e2e/harness.ts.
 */
import { afterAll, beforeAll, describe, expect, it } from 'vitest';

import type { RunningApp, TauriReply } from '../harness';
import { closeEverything, openApp } from '../harness';

const FOLDER = '/Users/somebody/Projects/loadout-reflection-routes';
const WORKSPACE = { id: FOLDER, name: 'Reflection routes', folder: FOLDER };
const WORKFLOW = {
  path: 'ship.json',
  workflow: {
    format: 1,
    id: 'wf-t126-routes',
    name: 'Ship it',
    steps: [
      {
        kind: 'agent',
        id: 's_build',
        name: 'Build',
        agent: 'agent-t126',
        overrides: {},
        copies: 1,
        instructions: 'Build the requested change.',
        skills: 'all',
        folder: { use: 'project' },
        handover: 'notes',
        at: { x: 24, y: 24 },
      },
    ],
    links: [],
  },
};
const AGENT = {
  id: 'agent-t126',
  name: 'Builder',
  summary: 'Builds the requested change',
  skills: [],
};

const RUN_SCREEN = 'main[data-section="run"]';
const MANUAL = 'button[data-workflow-run="manual"]';
const COMMAND = 'input[aria-label="Command line"]';
const REFLECTION = 'Learn from this run';
const FIRST_CALL_LIMIT = 4_000;
const SILENCE = 350;
const TEST_LIMIT = 20_000;

type Route = 'manual button' | '/run command' | 'workflow editor';

function copies<T>(value: T, count = 16): readonly { readonly value: T }[] {
  return Array.from({ length: count }, () => ({ value }));
}

function scene(): Readonly<Record<string, readonly TauriReply[]>> {
  return {
    list_workspaces: copies([WORKSPACE]),
    list_workflows: copies([WORKFLOW]),
    /* 2026-08-28: otwarcie oddaje plik RAZEM z rewizją, na której okno go czyta — bez niej
     * zapis nie ma czego porównać z dyskiem (`commands::workflows::OpenWorkflow`). */
    load_workflow: copies({ workflow: WORKFLOW.workflow, revision: 'r1' }, 4),
    check_workflow: copies([], 12),
    list_agents: copies([AGENT]),
    list_skills: copies([]),
    run_workflow: [{ value: null }],
  };
}

async function openRun(): Promise<RunningApp> {
  const app = await openApp({ replies: scene() });
  await app.page.locator(RUN_SCREEN).waitFor({ state: 'attached', timeout: FIRST_CALL_LIMIT });
  await app.page.locator(MANUAL).waitFor({ state: 'visible', timeout: FIRST_CALL_LIMIT });
  return app;
}

async function choose(app: RunningApp, enabled: boolean): Promise<void> {
  const checkbox = app.page.getByLabel(REFLECTION, { exact: true });
  expect(
    await checkbox.count(),
    'the mounted Run screen has no visible Learn from this run checkbox',
  ).toBe(1);
  expect(await checkbox.isChecked(), 'a fresh Run screen must opt in by default').toBe(true);
  if (!enabled) await checkbox.click();
  expect(await checkbox.isChecked(), 'the DOM did not rerender the checkbox choice').toBe(enabled);
}

async function takeRoute(app: RunningApp, route: Route): Promise<void> {
  if (route === 'manual button') {
    await app.page.locator(MANUAL).click();
    return;
  }
  if (route === '/run command') {
    await app.page.locator(COMMAND).fill('/run Ship it');
    await app.page.locator(COMMAND).press('Enter');
    return;
  }
  await app.page.locator('[data-section-switch="workflows"]').click();
  const screen = app.page.locator('main[data-section="workflows"]');
  await screen.locator('[data-tile]').first().click();
  const run = screen.getByRole('button', { name: 'Run', exact: true });
  await run.waitFor({ state: 'visible', timeout: FIRST_CALL_LIMIT });
  await run.click();
}

async function firstRunCall(app: RunningApp): Promise<readonly unknown[]> {
  const deadline = Date.now() + FIRST_CALL_LIMIT;
  while (Date.now() < deadline) {
    const calls = (await app.calls()).filter((call) => call.cmd === 'run_workflow');
    if (calls.length > 0) {
      await app.page.waitForTimeout(SILENCE);
      return (await app.calls()).filter((call) => call.cmd === 'run_workflow');
    }
    await app.page.waitForTimeout(25);
  }
  return [];
}

async function remountRun(app: RunningApp): Promise<void> {
  await app.page.locator('[data-section-switch="agents"]').click();
  await app.page.locator('[data-section-switch="run"]').click();
  await app.page.locator(RUN_SCREEN).waitFor({ state: 'attached', timeout: FIRST_CALL_LIMIT });
  await app.page.waitForTimeout(SILENCE);
}

beforeAll(async () => {
  const warm = await openApp();
  await warm.close();
}, 180_000);

afterAll(async () => {
  await closeEverything();
}, 30_000);

describe('the reflection choice is visible state and crosses each real route once', () => {
  it(
    'starts checked and visibly rerenders false then true',
    async () => {
      const app = await openRun();
      try {
        const checkbox = app.page.getByLabel(REFLECTION, { exact: true });
        expect(await checkbox.count(), 'the mounted screen has no reflection checkbox').toBe(1);
        expect(await checkbox.isChecked()).toBe(true);
        await checkbox.click();
        expect(await checkbox.isChecked()).toBe(false);
        await checkbox.click();
        expect(await checkbox.isChecked()).toBe(true);
      } finally {
        await app.close();
      }
    },
    TEST_LIMIT,
  );

  for (const enabled of [false, true]) {
    for (const route of ['manual button', '/run command', 'workflow editor'] as const) {
      it(
        `${route} sends reflectionEnabled=${String(enabled)} exactly once`,
        async () => {
          const app = await openRun();
          try {
            await choose(app, enabled);
            await takeRoute(app, route);
            const calls = await firstRunCall(app);
            expect(
              calls.length,
              `${route} did not produce exactly one run_workflow during the quiet window`,
            ).toBe(1);
            const sent = calls[0] as { readonly args: Record<string, unknown> };
            expect(sent.args['reflectionEnabled']).toBe(enabled);
            await remountRun(app);
            const after = (await app.calls()).filter((call) => call.cmd === 'run_workflow');
            expect(after.length, 'remounting Run consumed the same request a second time').toBe(1);
          } finally {
            await app.close();
          }
        },
        TEST_LIMIT,
      );
    }
  }
});
