/* WF-14: rzeczywisty edytor → wybór → podgląd → autosave. Jedyną atrapą jest Tauri. */
import { afterAll, expect, it } from 'vitest';
import { closeEverything, openApp } from '../../../../e2e/harness';
import type { TauriReply } from '../../../../e2e/harness';

const PROJECT = '/work/input-selection';
const NAME = 'Fresh copy inputs';
const DOCUMENT = {
  format: 1,
  id: 'wf14-inputs',
  name: NAME,
  steps: [],
  links: [],
  futureField: { keep: true },
};
const replies = (value: unknown): readonly TauriReply[] =>
  Array.from({ length: 12 }, () => ({ value }));

afterAll(closeEverything, 30_000);

it('shows the selected files and privacy warning, then saves only the explicit patterns', async () => {
  const app = await openApp({
    replies: {
      list_workspaces: replies([{ id: PROJECT, folder: PROJECT, name: 'Inputs' }]),
      list_workflows: replies([
        { kind: 'healthy', value: { path: 'inputs.json', place: 'project', workflow: DOCUMENT } },
      ]),
      load_workflow: replies({ workflow: DOCUMENT, revision: 'original-revision' }),
      check_workflow: replies([]),
      save_workflow: replies('saved-revision'),
      preview_additional_inputs: replies({
        files: [
          { path: 'fixtures/local.json', bytes: 41, ignored: true, private: false },
          { path: '.env.preview', bytes: 17, ignored: true, private: true },
        ],
        totalBytes: 58,
        fileLimit: 10000,
        byteLimit: 268435456,
      }),
    },
  });
  try {
    await app.page.locator('[data-section-switch="workflows"]').click();
    await app.page.getByRole('button', { name: NAME, exact: false }).first().click();
    const summary = app.page.getByText('Files in fresh copies', { exact: true });
    expect(await summary.count(), 'the real editor has no input selection control').toBe(1);
    await summary.click();
    const field = app.page.getByRole('textbox', { name: 'Additional input patterns', exact: true });
    await field.fill('fixtures/local*.json\n.env.preview');
    await app.page.getByRole('button', { name: 'Preview selected files', exact: true }).click();
    await expect
      .poll(async () => app.page.getByText('fixtures/local.json', { exact: false }).count())
      .toBeGreaterThan(0);
    expect(
      await app.page.getByRole('region', { name: 'Fresh copy file selection' }).textContent(),
    ).toContain('may contain secrets');
    expect((await app.calls()).filter((call) => call.cmd === 'save_workflow')).toHaveLength(0);
    await app.page.getByRole('button', { name: 'Use these input patterns', exact: true }).click();
    await expect
      .poll(async () => (await app.calls()).filter((call) => call.cmd === 'save_workflow'))
      .toHaveLength(1);
    const saved = (await app.calls()).find((call) => call.cmd === 'save_workflow');
    expect(saved?.args['workflow']).toEqual({
      ...DOCUMENT,
      additionalInputs: ['fixtures/local*.json', '.env.preview'],
    });
    expect(
      (await app.calls()).find((call) => call.cmd === 'preview_additional_inputs')?.args,
    ).toEqual({ folder: PROJECT, patterns: ['fixtures/local*.json', '.env.preview'] });
    expect((await app.calls()).some((call) => call.cmd === 'run_workflow')).toBe(false);
  } finally {
    await app.close();
  }
}, 90_000);
