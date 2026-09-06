/* WF-11: komunikacja jest świadomym wyborem, nie nowym domyślnym prawem kroku. */
import { afterAll, expect, it } from 'vitest';
import { closeEverything, openApp } from '../../../e2e/harness';
import type { TauriReply } from '../../../e2e/harness';
const PROJECT = '/work/step-messages';
const AGENT = {
  schema: 1,
  id: '01990000-0000-7000-8000-000000000011',
  name: 'Collaborator',
  summary: 'Works with another step',
  instructions: 'Do the work.',
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
  Array.from({ length: 8 }, () => ({ value }));
afterAll(closeEverything, 30_000);
it('starts with messages disabled and persists only an explicit choice', async () => {
  const app = await openApp({
    replies: {
      list_workspaces: replies([{ id: PROJECT, folder: PROJECT, name: 'Messages' }]),
      list_agents: replies([
        { kind: 'healthy', value: AGENT, path: 'collaborator.md', revision: 'initial' },
      ]),
      save_agent: replies('saved'),
    },
  });
  try {
    await app.page.locator('[data-section-switch="agents"]').click();
    await app.page.locator(`[data-agent="${AGENT.id}"]`).click();
    await app.page.getByRole('button', { name: 'More settings', exact: true }).click();
    const choice = app.page.getByRole('checkbox', {
      name: 'Allow messages between steps',
      exact: true,
    });
    expect(await choice.count(), 'the real form cannot opt into messages').toBe(1);
    expect(await choice.isChecked()).toBe(false);
    await choice.check();
    await app.page.getByRole('button', { name: 'Save', exact: true }).click();
    await expect
      .poll(async () => (await app.calls()).filter((one) => one.cmd === 'save_agent'))
      .toHaveLength(1);
    expect(
      (await app.calls()).find((one) => one.cmd === 'save_agent')?.args['agent'],
    ).toMatchObject({ agentMessages: true, reachesTheWeb: false, fileAccess: 'look-only' });
  } finally {
    await app.close();
  }
}, 90_000);
