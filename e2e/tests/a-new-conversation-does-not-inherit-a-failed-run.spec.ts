/* 2026-09-10: nowy kontekst nie dziedziczy porażki poprzedniego biegu.
 * Kliknięcia przechodzą przez produkcyjny renderer; atrapa stoi tylko na granicy Tauri. */
import { afterAll, expect, it } from 'vitest';

import type { PastRun, PastRunRow } from '../../src/sections/run/io';
import type { RunningApp, TauriCall } from '../harness';
import { closeEverything, openApp } from '../harness';

const FOLDER = '/projects/fresh-run-context';
const NAME = 'Fresh context';
const ERROR = 'This step did not deliver a valid plan document.';
const workflow = {
  format: 1,
  id: 'fresh-context',
  name: NAME,
  steps: ['plan', 'qa'].map((id) => ({
    kind: 'agent',
    id,
    name: id === 'plan' ? 'Plan implementation' : 'QA',
    agent: 'planner',
    instructions: 'Do this step.',
    overrides: {},
    copies: 1,
    skills: 'all',
    folder: { use: 'fresh-copy' },
    handover: 'notes',
  })),
  links: [{ from: 'plan', to: 'qa' }],
};
const past: PastRun = {
  folder: '20260909-212616__failed',
  when: '2026-09-09 21:26',
  title: NAME,
  state: 'failed',
  workflowFile: 'fresh.json',
  steps: [
    {
      id: 'old-plan',
      tile: 'plan',
      name: 'Old planner',
      agent: 'Planner',
      state: 'failed',
      summary: '',
      error: ERROR,
      costUsd: null,
      lines: [],
    },
  ],
  handoffs: [],
  branches: [],
  said: null,
};
const row: PastRunRow = { ...past, steps: 1, costUsd: null };
const copies = (value: unknown) => Array.from({ length: 12 }, () => ({ value }));
const base = {
  list_workspaces: copies([{ id: FOLDER, name: NAME, folder: FOLDER }]),
  list_workflows: copies([{ path: 'fresh.json', workflow }]),
  load_workflow: copies({ workflow, revision: 'r1' }),
  check_workflow: copies([]),
  list_agents: copies([{ id: 'planner', name: 'Planner', summary: '', skills: [] }]),
};

async function showProgress(
  app: RunningApp,
  call: TauriCall,
  status: 'running' | 'failed',
  index: number,
): Promise<void> {
  const match = /^__CHANNEL__:(\d+)$/u.exec(String(call.args['lines']));
  expect(match).not.toBeNull();
  await app.page.evaluate(
    ({ slot, status, index, name, error }) => {
      const callback = (globalThis as unknown as Record<string, unknown>)[slot];
      if (typeof callback !== 'function') throw new Error('The real Channel callback is missing');
      const failed = status === 'failed';
      (callback as (payload: unknown) => void)({
        index,
        message: [
          {
            kind: 'runProgress',
            agent: 'Loadout',
            runId: 'this-run',
            name,
            status,
            startedAt: Date.UTC(2026, 8, 10, 0, 0),
            endedAt: failed ? Date.UTC(2026, 8, 10, 0, 1) : null,
            steps: ['plan', 'qa'].map((id) => ({
              id,
              tileId: id,
              name: id === 'plan' ? 'Plan implementation' : 'QA',
              kind: 'agent',
              state:
                id === 'plan' ? (failed ? 'failed' : 'running') : failed ? 'skipped' : 'pending',
              carriedOn: false,
              processStarted: id === 'plan',
              error: failed ? error : '',
              dependsOn: id === 'plan' ? [] : ['plan'],
            })),
          },
        ],
      });
    },
    { slot: '_' + String(match?.[1]), status, index, name: NAME, error: ERROR },
  );
}

afterAll(closeEverything, 30_000);

it('opens ready with old failures available only when history is requested', async () => {
  const app = await openApp({
    replies: { ...base, list_runs: copies([row]), read_run: copies(past) },
  });
  try {
    await expect
      .poll(async () =>
        (await app.calls()).some(
          (call) => call.cmd === 'list_runs' && call.args['folder'] === FOLDER,
        ),
      )
      .toBe(true);
    await app.page.evaluate(
      () =>
        new Promise<void>((resolve) =>
          requestAnimationFrame(() => requestAnimationFrame(() => resolve())),
        ),
    );
    expect(await app.page.locator('[data-run-state]').innerText()).toMatch(/^Ready to run/iu);
    expect(await app.page.locator('[data-step]').allTextContents()).toEqual(
      expect.arrayContaining([expect.stringContaining('waiting')]),
    );
    expect(await app.page.locator('[data-step="old-plan"]').count()).toBe(0);
    expect((await app.calls()).filter((call) => call.cmd === 'read_run')).toHaveLength(0);

    await app.page.getByRole('textbox', { name: 'Command line', exact: true }).fill('/history');
    await app.page.getByRole('textbox', { name: 'Command line', exact: true }).press('Enter');
    await app.page.locator(`[data-history-row="${past.folder}"]`).click();
    await expect
      .poll(() => app.page.locator(`[data-past-run="${past.folder}"]`).innerText())
      .toContain(ERROR);
    expect(
      (await app.calls()).filter((call) => call.cmd === 'run_workflow' || call.cmd === 'stop_run'),
    ).toHaveLength(0);
    await app.page.screenshot({ path: '/tmp/loadout-fresh-context-history.png' });
  } finally {
    await app.close();
  }
}, 90_000);

it('opens a fresh terminal before another Run click and preserves the outcome in its own tab', async () => {
  const app = await openApp({ replies: { ...base, run_workflow: [{ deferred: 'finish' }] } });
  try {
    await app.page.locator('button[data-workflow-run="manual"]').click();
    await expect
      .poll(async () => (await app.calls()).filter((call) => call.cmd === 'run_workflow').length)
      .toBe(1);
    const call = (await app.calls()).find((call) => call.cmd === 'run_workflow');
    if (call === undefined) throw new Error('Run did not reach the boundary');
    await showProgress(app, call, 'failed', 0);
    await app.settle('finish', { value: null });
    await expect.poll(() => app.page.locator('[data-run-state]').innerText()).toMatch(/^Failed/iu);
    await app.page.getByRole('button', { name: 'New terminal', exact: true }).click();
    await expect
      .poll(() => app.page.locator('[data-run-state]').innerText())
      .toMatch(/^Ready to run/iu);
    expect((await app.page.locator('[data-step]').allTextContents()).join(' ')).not.toContain(
      ERROR,
    );
    await app.page.screenshot({ path: '/tmp/loadout-fresh-context-terminal.png' });
    await app.page.locator(`[data-tab="${FOLDER}"]`).click();
    await expect.poll(() => app.page.locator('[data-run-state]').innerText()).toMatch(/^Failed/iu);
    expect(await app.page.locator('[data-step="plan"]').innerText()).toContain(ERROR);
    expect((await app.calls()).filter((call) => call.cmd === 'run_workflow')).toHaveLength(1);
    expect((await app.calls()).filter((call) => call.cmd === 'stop_run')).toHaveLength(0);
  } finally {
    await app.close();
  }
}, 90_000);

it('still discovers running work and keeps Stop reachable when the window opens', async () => {
  const app = await openApp({
    replies: { ...base, list_runs: copies([{ ...row, state: 'running' }]) },
  });
  try {
    await expect.poll(() => app.page.locator('[data-run-state]').innerText()).toMatch(/^Running/iu);
    expect(await app.page.getByRole('button', { name: 'Stop', exact: true }).count()).toBe(1);
    expect(
      (await app.calls()).filter((call) => call.cmd === 'read_run' || call.cmd === 'stop_run'),
    ).toHaveLength(0);
  } finally {
    await app.close();
  }
}, 90_000);
