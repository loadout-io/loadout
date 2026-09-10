import { afterAll, expect, it } from 'vitest';
import { closeEverything, openApp } from '../../../e2e/harness';
const refusal =
  '‘Final plan’: Model ‘gpt-6’ is not offered by this app. Refresh models and choose one from the list. Nothing has started.';
const copies = (value: unknown) => Array.from({ length: 12 }, () => ({ value }));
afterAll(closeEverything, 30_000);
it('shows the named model refusal after pressing Run and lets the user try again', async () => {
  const folder = '/tmp/loadout-model-refusal';
  const app = await openApp({
    replies: {
      list_workspaces: copies([{ id: folder, name: 'Models', folder }]),
      list_agents: copies([]),
      list_skills: copies([]),
      list_workflows: copies([
        {
          kind: 'healthy',
          value: {
            path: 'models.json',
            workflow: {
              format: 1,
              id: 'models',
              name: 'Models',
              steps: [
                {
                  kind: 'agent',
                  id: 'plan',
                  name: 'Final plan',
                  agent: 'planner',
                  instructions: 'Plan',
                  folder: { use: 'project' },
                },
              ],
              links: [],
            },
          },
        },
      ]),
      run_workflow: [{ error: refusal }],
    },
  });
  try {
    await app.page.locator('button[data-workflow-run="manual"]').click();
    const message = app.page.locator('main [data-line]').filter({ hasText: refusal });
    await message.waitFor({ state: 'visible', timeout: 10_000 });
    expect(await message.innerText()).toContain('Final plan');
    expect(await message.innerText()).toContain('gpt-6');
    expect(await app.page.locator('button[data-workflow-run="manual"]').isEnabled()).toBe(true);
    expect((await app.calls()).filter((c) => c.cmd === 'run_workflow')).toHaveLength(1);
  } finally {
    await app.close();
  }
}, 90_000);
