/* WF-24 RED: od historii do rzeczywistego Open, bez tury modelu ani Startu workflow. */
import { afterAll, expect, it } from 'vitest';
import { closeEverything, openApp } from '../../../../e2e/harness';
import type { TauriReply } from '../../../../e2e/harness';

const PROJECT = '/work/restore-history';
const RUN = '019b0006-0000-7000-8000-000000000024';
const FOLDER = `20260906-001000__${RUN}`;
const ROW = {
  folder: FOLDER,
  when: '2026-09-06 00:10',
  title: 'Saved files',
  workflowFile: 'saved.json',
  state: 'succeeded',
  steps: 1,
  costUsd: null,
  said: null,
};
const replies = (value: unknown): readonly TauriReply[] =>
  Array.from({ length: 8 }, () => ({ value }));
afterAll(closeEverything, 30_000);

it('the person previews exact files, restores to a new folder, and opens it without invoking a model', async () => {
  const destination = `${PROJECT}/.loadout/restored/result-24`;
  const app = await openApp({
    replies: {
      list_workspaces: replies([{ id: PROJECT, folder: PROJECT, name: 'Restore history' }]),
      list_runs: replies([ROW]),
      read_run: replies({
        ...ROW,
        steps: [],
        handoffs: [],
        branches: [],
        resultFolders: [],
        savedResults: [
          { resultId: 'writer', name: 'Writer', kind: 'git', kept: false, available: true },
        ],
      }),
      prepare_result_restore: replies({
        previewId: 'restore-24',
        source: { workspace: PROJECT, runId: RUN },
        resultId: 'writer',
        oid: 'a'.repeat(40),
        files: ['result.txt'],
        bytes: 28,
        folder: destination,
        said: 'Restore 1 saved file to a new folder. The project will stay unchanged.',
      }),
      restore_result: replies({
        folder: destination,
        said: 'The saved files are ready in a new folder.',
      }),
      open_restored_folder: replies(null),
    },
  });
  try {
    const field = app.page.locator('[aria-label="Command line"]');
    await field.fill('/history');
    await field.press('Enter');
    await app.page.locator(`button[data-history-row="${FOLDER}"]`).click();
    await app.page.locator(`[data-past-run="${FOLDER}"]`).waitFor({ state: 'attached' });
    const restore = app.page.getByRole('button', { name: 'Restore saved files', exact: true });
    expect(await restore.count(), 'A saved result has no real restore control in History').toBe(1);
    await restore.click();
    const review = app.page.getByRole('region', { name: 'Review saved files', exact: true });
    await review.waitFor({ state: 'visible' });
    expect(await review.innerText()).toContain('The project will stay unchanged.');
    expect((await app.calls()).find((one) => one.cmd === 'prepare_result_restore')?.args).toEqual({
      folder: PROJECT,
      sourceRunId: RUN,
      resultId: 'writer',
    });
    expect((await app.calls()).filter((one) => one.cmd === 'restore_result')).toHaveLength(0);
    await review.getByRole('button', { name: 'Restore files', exact: true }).click();
    const open = app.page.getByRole('button', { name: 'Open restored folder', exact: true });
    await open.waitFor({ state: 'visible' });
    await open.click();
    await expect
      .poll(async () => (await app.calls()).filter((one) => one.cmd === 'open_restored_folder'))
      .toHaveLength(1);
    expect((await app.calls()).find((one) => one.cmd === 'restore_result')?.args).toEqual({
      folder: PROJECT,
      previewId: 'restore-24',
      originalIntent: 'Restore files',
    });
    expect((await app.calls()).find((one) => one.cmd === 'open_restored_folder')?.args).toEqual({
      folder: PROJECT,
      restoredFolder: destination,
    });
    expect(
      (await app.calls()).filter((one) =>
        ['say_to_lead', 'run_workflow', 'start_replay', 'accept_lead_start'].includes(one.cmd),
      ),
    ).toHaveLength(0);
  } finally {
    await app.close();
  }
}, 90_000);

it('keeping a result is explicit and allowing cleanup asks separately without deleting it', async () => {
  const app = await openApp({
    replies: {
      list_workspaces: replies([{ id: PROJECT, folder: PROJECT, name: 'Restore history' }]),
      list_runs: replies([ROW]),
      read_run: replies({
        ...ROW,
        steps: [],
        handoffs: [],
        branches: [],
        resultFolders: [],
        savedResults: [
          {
            resultId: 'writer',
            name: 'Writer',
            kind: 'git',
            kept: false,
            available: true,
            cleanupWarning:
              'Allow cleanup to remove the saved Writer result? No files will be removed now.',
          },
        ],
      }),
      set_result_kept: [
        { value: { kept: true, said: 'This result will be kept.' } },
        {
          value: {
            kept: false,
            said: 'Cleanup may remove this result. No files were removed now.',
          },
        },
      ],
    },
  });
  try {
    const field = app.page.locator('[aria-label="Command line"]');
    await field.fill('/history');
    await field.press('Enter');
    await app.page.locator(`button[data-history-row="${FOLDER}"]`).click();
    await app.page.locator(`[data-past-run="${FOLDER}"]`).waitFor({ state: 'attached' });
    const keep = app.page.getByRole('button', { name: 'Keep result', exact: true });
    expect(await keep.count(), 'History cannot protect a selected saved result').toBe(1);
    await keep.click();
    const allow = app.page.getByRole('button', { name: 'Allow cleanup', exact: true });
    await allow.waitFor({ state: 'visible' });
    await allow.click();
    const review = app.page.getByRole('region', { name: 'Review result cleanup', exact: true });
    await review.waitFor({ state: 'visible' });
    expect(await review.innerText()).toContain('saved Writer result');
    expect((await app.calls()).filter((one) => one.cmd === 'set_result_kept')).toHaveLength(1);
    await review.getByRole('button', { name: 'Allow cleanup of this result', exact: true }).click();
    await expect
      .poll(async () => (await app.calls()).filter((one) => one.cmd === 'set_result_kept'))
      .toHaveLength(2);
    expect(
      (await app.calls()).filter((one) => one.cmd === 'set_result_kept').map((one) => one.args),
    ).toEqual([
      {
        folder: PROJECT,
        sourceRunId: RUN,
        resultId: 'writer',
        kept: true,
        originalIntent: 'Keep result',
      },
      {
        folder: PROJECT,
        sourceRunId: RUN,
        resultId: 'writer',
        kept: false,
        originalIntent: 'Allow cleanup of this result',
      },
    ]);
    expect(
      (await app.calls()).filter((one) => ['forget_run', 'say_to_lead'].includes(one.cmd)),
    ).toHaveLength(0);
  } finally {
    await app.close();
  }
}, 90_000);
