/* WF-06: prawdziwa historia i kliknięcie, nie samo pole ze ścieżką w magazynie.
 * Rust ma osobne kryterium odczytu retained folderu. Tu jedyną atrapą jest granica Tauri
 * istniejącego e2e/harness; test nie woła handlera ani settera panelu za człowieka.
 */
import { afterAll, expect, it } from 'vitest';

import { closeEverything, openApp } from '../../../e2e/harness';
import type { TauriReply } from '../../../e2e/harness';

const PROJECT = '/Users/somebody/Projects/plain-folder';
const FOLDER = '20260905-160000__019b0006-0000-7000-8000-000000000006';
const RESULT = `${PROJECT}/.loadout/runs/${FOLDER}/work/writer`;
const ROW = {
  folder: FOLDER,
  when: '2026-09-05 16:00',
  title: 'Keep the only result',
  workflowFile: 'keep-the-only-result.json',
  state: 'succeeded',
  steps: 1,
  costUsd: null,
  said: null,
};
const OPENED = {
  ...ROW,
  steps: [
    {
      id: '019b0006-0000-7000-8000-000000000016',
      tile: 'writer',
      name: 'Write the result',
      agent: 'Scribe',
      state: 'succeeded',
      summary: 'Changed the files.',
      error: '',
      costUsd: null,
      lines: [],
    },
  ],
  handoffs: [],
  branches: [],
  resultFolders: [{ workKey: 'writer', step: 'Write the result', path: RESULT, state: 'changed' }],
};

function replies(value: unknown): readonly TauriReply[] {
  return Array.from({ length: 8 }, () => ({ value }));
}

afterAll(async () => {
  await closeEverything();
}, 30_000);

it('opens exactly the retained result after opening the run through the actual history controls', async () => {
  const app = await openApp({
    replies: {
      list_workspaces: replies([{ id: PROJECT, folder: PROJECT, name: 'Plain folder' }]),
      list_runs: replies([ROW]),
      read_run: replies(OPENED),
    },
  });
  try {
    const field = app.page.locator('[aria-label="Command line"]');
    await field.fill('/history');
    await field.press('Enter');
    await app.page.locator(`button[data-history-row="${FOLDER}"]`).click();
    await app.page.locator(`[data-past-run="${FOLDER}"]`).waitFor({ state: 'attached' });
    const open = app.page.getByRole('button', { name: 'Open result folder', exact: true });
    expect(
      await open.count(),
      'the actual history screen has no usable result-folder control',
    ).toBe(1);
    expect(await app.page.locator(`[data-past-run="${FOLDER}"]`).textContent()).toContain(RESULT);
    await open.click();
    await expect
      .poll(async () => (await app.calls()).filter((one) => one.cmd === 'open_result_folder'))
      .toHaveLength(1);
    const call = (await app.calls()).find((one) => one.cmd === 'open_result_folder');
    expect(call?.args).toEqual({ folder: PROJECT, run: FOLDER, workKey: 'writer' });
  } finally {
    await app.close();
  }
}, 90_000);

it('shows the exact folders before a separate destructive confirmation and honors Keep folders', async () => {
  const app = await openApp({
    replies: {
      list_workspaces: replies([{ id: PROJECT, folder: PROJECT, name: 'Plain folder' }]),
      list_runs: replies([ROW]),
      read_run: replies(OPENED),
      forget_run: replies([]),
    },
  });
  try {
    const field = app.page.locator('[aria-label="Command line"]');
    await field.fill('/history');
    await field.press('Enter');
    await app.page.locator(`button[data-history-row="${FOLDER}"]`).click();
    await app.page.getByRole('button', { name: 'Forget this run', exact: true }).click();
    const confirmation = app.page.getByRole('region', { name: 'Confirm removing saved results' });
    await confirmation.waitFor({ state: 'visible' });
    expect(await confirmation.textContent()).toContain(RESULT);
    expect((await app.calls()).filter((one) => one.cmd === 'forget_run')).toHaveLength(0);
    await confirmation.getByRole('button', { name: 'Keep folders', exact: true }).click();
    expect((await app.calls()).filter((one) => one.cmd === 'forget_run')).toHaveLength(0);
    await app.page.getByRole('button', { name: 'Forget this run', exact: true }).click();
    await confirmation
      .getByRole('button', { name: 'Forget these folders and this run', exact: true })
      .click();
    await expect
      .poll(async () => (await app.calls()).filter((one) => one.cmd === 'forget_run'))
      .toHaveLength(1);
    const call = (await app.calls()).find((one) => one.cmd === 'forget_run');
    expect(call?.args).toEqual({ folder: PROJECT, run: FOLDER, confirmedResultFolders: [RESULT] });
  } finally {
    await app.close();
  }
}, 90_000);
