/* WF-28: rzeczywisty formularz zapisuje prawa do jednej aplikacji, bez domyślnego Stop. */
import { afterAll, expect, it } from 'vitest';
import { closeEverything, openApp } from '../../../e2e/harness';
import type { TauriReply } from '../../../e2e/harness';

const PROJECT = '/work/app-permissions';
const AGENT = {
  schema: 1,
  id: '01990000-0000-7000-8000-000000000028',
  name: 'Browser tester',
  summary: 'Checks the running app',
  instructions: 'Check the prepared app.',
  color: 'slate',
  runsWith: 'claude-code',
  model: 'opus',
  thinking: 'balanced',
  fileAccess: 'look-only',
  reachesTheWeb: false,
  giveUpAfterMinutes: 20,
  writeResultsTo: '',
  tools: 'everything',
  skills: [],
  connections: [],
};
const replies = (value: unknown): readonly TauriReply[] =>
  Array.from({ length: 12 }, () => ({ value }));
afterAll(closeEverything, 30_000);

it('starts with no allowed apps and saves only the selected app and operations', async () => {
  const app = await openApp({
    replies: {
      list_workspaces: replies([{ id: PROJECT, folder: PROJECT, name: 'App permissions' }]),
      list_agents: replies([
        { kind: 'healthy', value: AGENT, path: 'tester.md', revision: 'initial' },
      ]),
      save_agent: replies('saved'),
    },
  });
  try {
    await app.page.locator('[data-section-switch="agents"]').click();
    await app.page.locator(`[data-agent="${AGENT.id}"]`).click();
    await app.page.getByRole('button', { name: 'More settings', exact: true }).click();
    expect(
      await app.page.getByText('No apps allowed.', { exact: true }).count(),
      'the real form does not show the default refusal',
    ).toBe(1);
    await app.page.getByRole('button', { name: 'Allow an app', exact: true }).click();
    await app.page.getByRole('textbox', { name: 'App step 1', exact: true }).fill('s_preview');
    await app.page.getByRole('checkbox', { name: 'Read app 1', exact: true }).check();
    await app.page.getByRole('checkbox', { name: 'Start app 1', exact: true }).check();
    expect(
      await app.page.getByRole('checkbox', { name: 'Stop app 1', exact: true }).isChecked(),
    ).toBe(false);
    expect(
      await app.page.getByRole('checkbox', { name: 'Restart app 1', exact: true }).isChecked(),
    ).toBe(false);
    await app.page.getByRole('button', { name: 'Save', exact: true }).click();
    await expect
      .poll(async () => (await app.calls()).filter((call) => call.cmd === 'save_agent'))
      .toHaveLength(1);
    const saved = (await app.calls()).find((call) => call.cmd === 'save_agent')?.args['agent'];
    expect(saved).toMatchObject({
      serviceAccess: [{ service: 's_preview', operations: ['read', 'start'] }],
      reachesTheWeb: false,
      fileAccess: 'look-only',
    });
    expect(
      (await app.calls()).some(
        (call) => call.cmd === 'run_workflow' || call.cmd === 'start_process',
      ),
    ).toBe(false);
  } finally {
    await app.close();
  }
}, 90_000);
