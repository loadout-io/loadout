import { afterAll, expect, it } from 'vitest';
import { closeEverything, openApp } from '../harness';
afterAll(closeEverything, 30_000);

it('shows one Combine tile through repeated runs and shows the actual attempts', async () => {
  const folder = '/projects/loop-cards';
  const flow = {
    format: 1,
    id: 'one-card',
    name: 'One card per step',
    steps: ['combine', 'qa'].map((id) => ({
      id,
      kind: 'agent',
      name: id === 'combine' ? 'Combine' : 'QA',
      agent: 'writer',
      instructions: 'Do this step.',
      overrides: {},
      copies: 1,
      skills: 'all',
      folder: { use: 'fresh-copy' },
      handover: 'notes',
    })),
    links: [{ from: 'combine', to: 'qa' }],
  };
  const app = await openApp({
    replies: {
      list_workspaces: [{ value: [{ id: folder, folder, name: 'Loop cards' }] }],
      list_workflows: Array.from({ length: 10 }, () => ({
        value: [{ path: 'loop.json', workflow: flow }],
      })),
      load_workflow: [{ value: { workflow: flow, revision: 'r1' } }],
      check_workflow: [{ value: [] }],
      run_workflow: [{ deferred: 'finish' }],
    },
  });
  try {
    await app.page.locator('button[data-workflow-run="manual"]').click();
    await expect
      .poll(async () => (await app.calls()).filter((call) => call.cmd === 'run_workflow').length)
      .toBe(1);
    const call = (await app.calls()).find((call) => call.cmd === 'run_workflow')!;
    const match = /^__CHANNEL__:(\d+)$/u.exec(String(call.args['lines']));
    expect(match).not.toBeNull();
    await app.page.evaluate(
      ({ slot }) => {
        const send = (globalThis as unknown as Record<string, (payload: unknown) => void>)[slot]!;
        send({
          index: 0,
          message: [
            {
              kind: 'runProgress',
              agent: 'Loadout',
              runId: 'loop-run',
              name: 'One card per step',
              status: 'running',
              startedAt: Date.now(),
              endedAt: null,
              steps: [
                ['c1', 'combine', 'failed', true, []],
                ['q1', 'qa', 'succeeded', true, ['c1']],
                ['c2', 'combine', 'running', true, ['q1']],
                ['q2', 'qa', 'pending', false, ['c2']],
                ['c3', 'combine', 'pending', false, ['q2']],
                ['q3', 'qa', 'pending', false, ['c3']],
              ].map(([id, tileId, state, processStarted, dependsOn]) => ({
                id,
                tileId,
                state,
                processStarted,
                dependsOn,
                kind: 'agent',
                name: tileId === 'combine' ? 'Combine' : 'QA',
                carriedOn: false,
                error: '',
              })),
            },
          ],
        });
      },
      { slot: '_' + match![1] },
    );
    await expect.poll(() => app.page.locator('[data-step]').count()).toBe(2);
    const combine = app.page.locator('[data-step="combine"]');
    expect(await combine.innerText()).toContain('working');
    expect(await combine.innerText()).not.toContain('after QA');
    expect(await app.page.locator('[data-step="qa"]').innerText()).toContain('after Combine');
    expect(await app.page.locator('[data-step="qa"]').innerText()).toContain('waiting');
    expect(await app.page.locator('[data-run-state]').innerText()).toMatch(/^Running/iu);
    expect(await app.page.locator('[data-section="run"]').innerText()).toContain('step 1 of 2');
    const attempts = app.page
      .locator('details')
      .filter({ has: app.page.getByText('2 runs', { exact: true }) });
    await attempts.locator('summary').click();
    expect(await attempts.innerText()).toContain('Run 1 · failed');
    expect(await attempts.innerText()).toContain('Run 2 · working');
    expect(await attempts.innerText()).not.toContain('Run 3');
    await app.page.screenshot({ path: '/tmp/loadout-grouped-step-runs.png' });
    await app.settle('finish', { value: null });
  } finally {
    await app.close();
  }
}, 90_000);
