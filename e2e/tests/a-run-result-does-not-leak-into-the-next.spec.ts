/* 0.4.1: prawdziwe kliknięcia i kanał Tauri w przeglądarce. Księgę oraz procesy
 * sprawdzają testy work_plan_candidate_becomes_a_version po rustowej stronie granicy. */
import { afterAll, expect, it } from 'vitest';
import { closeEverything, openApp } from '../harness';
import type { RunningApp, TauriCall } from '../harness';

const FOLDER = '/projects/run-result-isolation';
const workflow = {
  format: 1,
  id: 'result-isolation',
  name: 'Result isolation',
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
const replies = (value: unknown) => Array.from({ length: 24 }, () => ({ value }));

async function send(
  app: RunningApp,
  call: TauriCall,
  index: number,
  status: string,
  runId: string,
) {
  const match = /^__CHANNEL__:(\d+)$/u.exec(String(call.args['lines']));
  expect(match).not.toBeNull();
  const failed = status === 'failed';
  const message = [
    {
      kind: 'runProgress',
      agent: 'Loadout',
      runId,
      name: workflow.name,
      status,
      startedAt: Date.UTC(2026, 8, 9, 21, 26),
      endedAt: failed ? Date.UTC(2026, 8, 9, 21, 46) : null,
      steps: ['plan', 'qa-1', 'qa-2'].map((id) => ({
        id,
        tileId: id === 'plan' ? 'plan' : 'qa',
        name: id === 'plan' ? 'Plan implementation' : 'QA',
        kind: 'agent',
        state: id === 'plan' ? (failed ? 'failed' : 'running') : failed ? 'skipped' : 'pending',
        carriedOn: false,
        processStarted: id === 'plan',
        error: failed
          ? id === 'plan'
            ? 'No plan was published.'
            : 'Skipped: Plan implementation did not pass.'
          : '',
        dependsOn: id === 'plan' ? [] : ['plan'],
      })),
    },
  ];
  await app.page.evaluate(
    ({ slot, index, message }) => {
      const callback = (globalThis as unknown as Record<string, unknown>)[slot];
      if (typeof callback !== 'function') throw new Error('The real Channel callback is missing');
      (callback as (payload: unknown) => void)({ index, message });
    },
    { slot: '_' + String(match?.[1]), index, message },
  );
}

afterAll(closeEverything, 30_000);
it('shows a failed result and starts fresh with distinct attempts after another click', async () => {
  const app = await openApp({
    replies: {
      list_workspaces: replies([{ id: FOLDER, name: 'Result isolation', folder: FOLDER }]),
      list_workflows: replies([{ path: 'result.json', workflow }]),
      load_workflow: replies({ workflow, revision: 'r1' }),
      check_workflow: replies([]),
      list_agents: replies([{ id: 'planner', name: 'Planner', summary: '', skills: [] }]),
      run_workflow: [{ deferred: 'first-run' }, { deferred: 'next-run' }],
    },
  });
  try {
    const start = app.page.locator('button[data-workflow-run="manual"]');
    await start.click();
    await expect
      .poll(async () => (await app.calls()).filter((call) => call.cmd === 'run_workflow').length)
      .toBe(1);
    const first = (await app.calls()).find((call) => call.cmd === 'run_workflow');
    if (first === undefined) throw new Error('The Run click did not reach Rust');
    await send(app, first, 0, 'failed', 'run-first');
    await app.settle('first-run', { value: null });
    await expect
      .poll(() => app.page.locator('[data-run-state]').innerText())
      .toMatch(/^Failed .*20 min/iu);
    expect(await app.page.locator('[data-run-head]').getAttribute('data-run-tone')).toBe('failed');
    expect(await app.page.locator('[data-step]').count()).toBe(3);
    expect(await app.page.locator('[data-step="qa-1"]').innerText()).toContain('not run');
    await start.click();
    await expect
      .poll(async () => (await app.calls()).filter((call) => call.cmd === 'run_workflow').length)
      .toBe(2);
    const next = (await app.calls()).filter((call) => call.cmd === 'run_workflow')[1];
    if (next === undefined) throw new Error('The next Run click did not reach Rust');
    await send(app, next, 0, 'running', 'run-next');
    await send(app, first, 1, 'failed', 'run-first');
    await expect
      .poll(() => app.page.locator('[data-step="qa-1"]').innerText())
      .toContain('waiting');
    expect(await app.page.locator('[data-step]').count()).toBe(3);
    expect(await app.page.locator('[data-step="plan"]').innerText()).not.toContain(
      'No plan was published.',
    );
    expect(await app.page.locator('[data-run-state]').innerText()).toMatch(/^Running/iu);
    await app.page.screenshot({ path: '/tmp/loadout-041-run-isolation.png' });
    await app.settle('next-run', { value: null });
  } finally {
    await app.close();
  }
}, 45_000);
