/* WF-12: ustawienia aktywnego projektu mają prawdziwy wybór, listę źródeł i zapis zakresu.
 * Granica Tauri jest jedyną atrapą; człowiek otwiera normalny ekran Settings i klika kontrolkę.
 */
import { afterAll, expect, it } from 'vitest';

import { closeEverything, openApp } from '../../../e2e/harness';
import type { TauriReply } from '../../../e2e/harness';

const PROJECT = '/Users/somebody/Projects/instruction-fixture';
const SETTINGS = {
  instructions: { enabled: false, includeLocal: false },
  leadInstructions: null,
  sources: [
    { path: 'AGENTS.md', kind: 'agents', directory: '', paths: [], bytes: 31, local: false },
    { path: 'web/CLAUDE.md', kind: 'claude', directory: 'web', paths: [], bytes: 47, local: false },
  ],
  limits: { files: 256, fileBytes: 65536, totalBytes: 524288 },
};

function replies(value: unknown): readonly TauriReply[] {
  return Array.from({ length: 8 }, () => ({ value }));
}

afterAll(async () => {
  await closeEverything();
}, 30_000);

it('does not opt in by opening the folder and saves only the explicit project choice', async () => {
  const app = await openApp({
    replies: {
      list_workspaces: replies([{ id: PROJECT, folder: PROJECT, name: 'Instruction fixture' }]),
      read_project_settings: replies(SETTINGS),
      save_project_settings: replies({
        ...SETTINGS,
        instructions: { enabled: true, includeLocal: false },
      }),
    },
  });
  try {
    await app.page.locator('[data-section-switch="settings"]').click();
    await app.page.locator('[data-settings-screen]').waitFor({ state: 'visible' });
    const checkbox = app.page.getByRole('checkbox', {
      name: 'Use project instructions',
      exact: true,
    });
    expect(
      await checkbox.count(),
      'there is no explicit project-instruction control on the real settings screen',
    ).toBe(1);
    expect(await checkbox.isChecked()).toBe(false);
    const section = app.page.getByRole('region', { name: 'Project instructions', exact: true });
    expect(await section.textContent()).toContain('AGENTS.md');
    expect(await section.textContent()).toContain('web/CLAUDE.md');
    expect(await section.textContent()).toContain('256');
    expect((await app.calls()).filter((one) => one.cmd === 'save_project_settings')).toHaveLength(
      0,
    );
    await checkbox.check();
    await expect
      .poll(async () => (await app.calls()).filter((one) => one.cmd === 'save_project_settings'))
      .toHaveLength(1);
    const call = (await app.calls()).find((one) => one.cmd === 'save_project_settings');
    expect(call?.args).toEqual({ folder: PROJECT, patch: { instructions: { enabled: true } } });
    expect(
      (await app.calls()).filter(
        (one) => one.cmd === 'start_process' || one.cmd === 'run_workflow',
      ),
    ).toHaveLength(0);
  } finally {
    await app.close();
  }
}, 90_000);

it('shows the refused lead message once and keeps its unsent text in the real Entry', async () => {
  const said =
    'The lead agent could not start: Project instructions could not be supplied, so this message was not sent. Project instructions at missing-policy.md could not be read.';
  const app = await openApp({
    replies: {
      list_workspaces: replies([{ id: PROJECT, folder: PROJECT, name: 'Instruction fixture' }]),
      say_to_orchestrator: [{ error: said }],
    },
  });
  try {
    const field = app.page.getByRole('textbox', { name: 'Command line', exact: true });
    await field.fill('Keep this unsent message');
    await field.press('Enter');
    await expect
      .poll(async () => (await app.calls()).filter((call) => call.cmd === 'say_to_orchestrator'))
      .toHaveLength(1);
    await expect.poll(async () => app.page.getByText(said, { exact: false }).count()).toBe(1);
    expect(await field.inputValue()).toBe('Keep this unsent message');
  } finally {
    await app.close();
  }
}, 90_000);

it('shows the frozen supplied sources separately from native loading in real history', async () => {
  const folder = '20260905-190000__019b0012-0000-7000-8000-000000000012';
  const row = {
    folder,
    when: '2026-09-05 19:00',
    title: 'Frozen rules',
    workflowFile: 'rules.json',
    state: 'succeeded',
    steps: 1,
    costUsd: 0,
    said: null,
  };
  const opened = {
    ...row,
    handoffs: [],
    branches: [],
    steps: [
      {
        id: 'wf12-step',
        tile: 'reader',
        name: 'Read rules',
        agent: 'Reader',
        state: 'succeeded',
        summary: '',
        error: '',
        costUsd: 0,
        lines: [],
        projectInstructions: [
          {
            path: 'web/AGENTS.md',
            kind: 'agents',
            directory: 'web',
            paths: ['web/**'],
            bytes: 47,
            digest: '12345678'.repeat(8),
            local: false,
          },
        ],
        whatLoadoutDidNotGive:
          'This step also read a plugin from this project that Loadout did not give it',
      },
    ],
  };
  const app = await openApp({
    replies: {
      list_workspaces: replies([{ id: PROJECT, folder: PROJECT, name: 'Instructions' }]),
      list_runs: replies([row]),
      read_run: replies(opened),
    },
  });
  try {
    const field = app.page.getByRole('textbox', { name: 'Command line', exact: true });
    await field.fill('/history');
    await field.press('Enter');
    await app.page.locator(`button[data-history-row="${folder}"]`).click();
    const supplied = app.page.locator('[data-project-instructions]');
    expect(await supplied.count()).toBe(1);
    await supplied.locator('summary').first().click();
    expect(await supplied.innerText()).toContain('web/AGENTS.md');
    expect(await supplied.innerText()).toContain('folder web');
    expect(await supplied.innerText()).toContain('47 bytes');
    expect(await supplied.innerText()).not.toContain('also read a plugin');
    expect(await app.page.getByText('This step also read a plugin', { exact: false }).count()).toBe(
      1,
    );
  } finally {
    await app.close();
  }
}, 90_000);
