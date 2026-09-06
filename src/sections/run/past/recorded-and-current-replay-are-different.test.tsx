/* WF-23 RED: historia wybiera konkretny zapis i jawny tryb, zanim wolno uruchomić kopię.
 * Prawdziwy ekran /history, atrapa tylko granicy Tauri, bez wywołania handlera za człowieka.
 */
import { afterAll, expect, it } from 'vitest';
import { closeEverything, openApp } from '../../../../e2e/harness';
import type { TauriReply } from '../../../../e2e/harness';

const PROJECT = '/work/replay-history';
const RUN = '019b0006-0000-7000-8000-000000000023';
const FOLDER = `20260906-001000__${RUN}`;
const ROW = {
  folder: FOLDER,
  when: '2026-09-06 00:10',
  title: 'Saved configuration',
  workflowFile: 'saved.json',
  state: 'succeeded',
  steps: 2,
  costUsd: null,
  said: null,
};
const OPENED = { ...ROW, steps: [], handoffs: [], branches: [], resultFolders: [] };
const replies = (value: unknown): readonly TauriReply[] =>
  Array.from({ length: 8 }, () => ({ value }));

afterAll(closeEverything, 30_000);

it('saves the addressed historical workflow as a new copy without starting work', async () => {
  const app = await openApp({
    replies: {
      list_workspaces: replies([{ id: PROJECT, folder: PROJECT, name: 'Replay history' }]),
      list_runs: replies([ROW]),
      read_run: replies(OPENED),
      copy_recorded_workflow: replies({
        fileName: 'saved-copy-new-id.json',
        workflowId: 'new-workflow-id',
        said: 'Saved Saved configuration (copy) as a new workflow. Nothing started.',
      }),
    },
  });
  try {
    const field = app.page.locator('[aria-label="Command line"]');
    await field.fill('/history');
    await field.press('Enter');
    await app.page.locator(`button[data-history-row="${FOLDER}"]`).click();
    await app.page.locator(`[data-past-run="${FOLDER}"]`).waitFor({ state: 'attached' });
    const copy = app.page.getByRole('button', { name: 'Save workflow as a new copy', exact: true });
    expect(await copy.count(), 'History cannot save an old graph without overwriting today').toBe(
      1,
    );
    await copy.click();
    await expect
      .poll(async () => (await app.calls()).filter((one) => one.cmd === 'copy_recorded_workflow'))
      .toHaveLength(1);
    expect((await app.calls()).find((one) => one.cmd === 'copy_recorded_workflow')?.args).toEqual({
      folder: PROJECT,
      sourceRunId: RUN,
    });
    await expect
      .poll(async () => app.page.locator(`[data-past-run="${FOLDER}"]`).innerText())
      .toContain('Saved Saved configuration (copy) as a new workflow. Nothing started.');
    expect(
      (await app.calls()).filter((one) =>
        ['save_workflow', 'start_replay', 'run_workflow', 'say_to_lead'].includes(one.cmd),
      ),
    ).toHaveLength(0);
  } finally {
    await app.close();
  }
}, 90_000);

it.each(['recorded', 'current'] as const)(
  '%s names its source and differences without starting a model while the person inspects it',
  async (mode) => {
    const app = await openApp({
      replies: {
        list_workspaces: replies([{ id: PROJECT, folder: PROJECT, name: 'Replay history' }]),
        list_runs: replies([ROW]),
        read_run: replies(OPENED),
        prepare_replay: replies({
          previewId: `preview-${mode}`,
          mode,
          source: { workspace: PROJECT, runId: RUN },
          title: ROW.title,
          copies: mode === 'recorded' ? 2 : 1,
          said:
            mode === 'recorded'
              ? 'Repeat the saved setup: 2 copies, using the saved starting files.'
              : 'Repeat the current setup: 1 copy. The workflow and model changed since this run.',
          limitations: ['External services and model replies may differ.'],
          budgetSaid: 'Spending limit: $4.00 for this new run.',
          configurationSaid: ['Build: historical-model, Claude Code, Work freely.'],
        }),
      },
    });
    try {
      const field = app.page.locator('[aria-label="Command line"]');
      await field.fill('/history');
      await field.press('Enter');
      await app.page.locator(`button[data-history-row="${FOLDER}"]`).click();
      await app.page.locator(`[data-past-run="${FOLDER}"]`).waitFor({ state: 'attached' });
      const label = mode === 'recorded' ? 'Repeat saved setup' : 'Repeat current setup';
      const choose = app.page.getByRole('button', { name: label, exact: true });
      expect(
        await choose.count(),
        'History cannot choose a saved setup separately from today',
      ).toBe(1);
      await choose.click();
      await expect
        .poll(async () => (await app.calls()).filter((one) => one.cmd === 'prepare_replay'))
        .toHaveLength(1);
      expect((await app.calls()).find((one) => one.cmd === 'prepare_replay')?.args).toEqual({
        folder: PROJECT,
        sourceRunId: RUN,
        selection: { kind: 'all' },
        mode,
      });
      const review = app.page.getByRole('region', { name: 'Review repeat', exact: true });
      await review.waitFor({ state: 'visible' });
      expect(await review.innerText()).toContain(
        mode === 'recorded' ? '2 copies' : 'workflow and model changed',
      );
      expect(await review.innerText()).toContain('External services and model replies may differ.');
      expect(
        await review.innerText(),
        'the confirmed limit is missing from the real preview',
      ).toContain('Spending limit: $4.00 for this new run.');
      await review.getByText('Agent settings', { exact: true }).click();
      expect(await review.innerText()).toContain(
        'Build: historical-model, Claude Code, Work freely.',
      );
      expect(
        (await app.calls()).filter((one) =>
          ['start_replay', 'start_run', 'say_to_lead'].includes(one.cmd),
        ),
      ).toHaveLength(0);
      await review.getByRole('button', { name: 'Keep viewing history', exact: true }).click();
      expect((await app.calls()).filter((one) => one.cmd === 'start_replay')).toHaveLength(0);
    } finally {
      await app.close();
    }
  },
  90_000,
);

it('the real confirmation authorizes its preview and uses the shared addressed Start without a Lead turn', async () => {
  const requestId = 'instance:history-repeat';
  const app = await openApp({
    replies: {
      list_workspaces: replies([{ id: PROJECT, folder: PROJECT, name: 'Replay history' }]),
      list_runs: replies([ROW]),
      read_run: replies(OPENED),
      prepare_replay: replies({
        previewId: requestId,
        mode: 'recorded',
        source: { workspace: PROJECT, runId: RUN },
        title: ROW.title,
        copies: 2,
        said: 'Repeat the saved setup: 2 copies, using the saved starting files.',
        limitations: [],
      }),
      start_replay: replies({
        kind: 'runRequested',
        agent: 'Lead',
        text: 'Starting Saved configuration',
        requestId,
        conversationId: 'history-window',
        workspace: PROJECT,
        title: ROW.title,
        fileName: 'saved.json',
        steps: [
          { id: 'build', name: 'Build', kind: 'agent', at: { x: 0, y: 0 }, weight: 'ordinary' },
        ],
        links: [],
      }),
      accept_lead_start: replies({ requestId, run: { workspace: PROJECT, runId: 'a-new-run' } }),
    },
  });
  try {
    const field = app.page.locator('[aria-label="Command line"]');
    await field.fill('/history');
    await field.press('Enter');
    await app.page.locator(`button[data-history-row="${FOLDER}"]`).click();
    await app.page.getByRole('button', { name: 'Repeat saved setup', exact: true }).click();
    const review = app.page.getByRole('region', { name: 'Review repeat', exact: true });
    await review.getByRole('button', { name: 'Start replay', exact: true }).click();
    await expect
      .poll(async () => (await app.calls()).filter((one) => one.cmd === 'accept_lead_start'))
      .toHaveLength(1);
    expect((await app.calls()).find((one) => one.cmd === 'start_replay')?.args).toEqual({
      folder: PROJECT,
      previewId: requestId,
      originalIntent: 'Start replay',
    });
    expect((await app.calls()).find((one) => one.cmd === 'accept_lead_start')?.args).toMatchObject({
      requestId,
    });
    expect(
      (await app.calls()).filter((one) =>
        ['run_workflow', 'say_to_lead', 'continue_run'].includes(one.cmd),
      ),
    ).toHaveLength(0);
  } finally {
    await app.close();
  }
}, 90_000);
