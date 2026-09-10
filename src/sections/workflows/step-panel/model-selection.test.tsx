/* UX-1: dwa wiersze schowane pod More settings nadal przechodzą przez prawdziwy edytor,
 * autosave i argument pliku wysyłany do Tauri. Jedyną atrapą jest granica Tauri. */
import { afterAll, expect, it } from 'vitest';

import { closeEverything, openApp } from '../../../../e2e/harness';
import type { TauriReply } from '../../../../e2e/harness';

const NAME = 'Folded settings';
const AGENT = {
  schema: 1,
  id: 'folded-settings-agent',
  name: 'Builder',
  summary: 'Builds the requested change',
  color: 'clay',
  instructions: 'Build the requested change.',
  runsWith: 'claude-code',
  model: 'opus',
  thinking: 'balanced',
  fileAccess: 'work-freely',
  giveUpAfterMinutes: 20,
  writeResultsTo: 'handoffs/result.md',
  tools: 'everything',
  reachesTheWeb: false,
  skills: [],
  connections: [],
};
const DOCUMENT = {
  format: 1 as const,
  id: 'ux1-folded-settings',
  name: NAME,
  steps: [
    {
      kind: 'agent' as const,
      id: 'build',
      name: 'Build',
      agent: AGENT.id,
      overrides: {},
      copies: 1,
      instructions: 'Build the requested change.',
      skills: 'all' as const,
      folder: { use: 'project' as const },
      handover: 'notes' as const,
      at: { x: 24, y: 24 },
    },
  ],
  links: [],
};

const replies = (value: unknown): readonly TauriReply[] =>
  Array.from({ length: 16 }, () => ({ value }));

afterAll(closeEverything, 30_000);

it('refreshes model choices, names an invalid model and saves the selected step override', async () => {
  const model = (id: string) => ({
    id,
    displayName: id,
    description: 'Available from the app',
    isDefault: true,
    hidden: false,
    aliases: [],
    efforts: ['medium'],
  });
  const app = await openApp({
    replies: {
      list_workflows: replies([
        { kind: 'healthy', value: { path: 'folded.json', workflow: DOCUMENT } },
      ]),
      load_workflow: replies({ workflow: DOCUMENT, revision: 'original-revision' }),
      check_workflow: replies([]),
      list_agents: replies([AGENT]),
      list_skills: replies([]),
      resolve_workflow_context: replies(null),
      resolve_workflow_plan: replies(null),
      save_workflow: replies('saved-revision'),
      list_agent_models: [
        { value: { models: [model('new-model')] } },
        { value: { models: [model('newer-model')] } },
        { error: 'CLI unavailable' },
      ],
    },
  });
  try {
    await app.page.locator('[data-section-switch="workflows"]').click();
    await app.page.getByRole('button', { name: NAME, exact: false }).first().click();
    await app.page.locator('main [data-step="build"]').click();
    const panel = app.page.locator('main [data-step-panel]');
    await panel.waitFor({ state: 'visible', timeout: 20_000 });
    await panel.locator('[data-more-settings] > summary').click();
    const picker = panel.locator('[data-model-picker]');
    await picker.getByRole('button', { name: 'new-model · Recommended', exact: false }).waitFor();
    expect(await picker.locator('[role="alert"]').textContent()).toContain(
      'opus is not offered by Claude Code',
    );
    expect(await picker.locator('input').inputValue()).toBe('opus');
    await picker.getByRole('button', { name: 'Refresh models', exact: true }).click();
    await picker.getByRole('button', { name: 'newer-model · Recommended', exact: false }).click();
    await expect
      .poll(async () => {
        const saved = (await app.calls()).filter((c) => c.cmd === 'save_workflow').at(-1)?.args[
          'workflow'
        ] as { steps?: { overrides?: { model?: string } }[] } | undefined;
        return saved?.steps?.[0]?.overrides?.model;
      })
      .toBe('newer-model');
    expect(await picker.locator('[role="alert"]').count()).toBe(0);
    await picker.getByRole('button', { name: 'Refresh models', exact: true }).click();
    await picker
      .getByText('Could not check models. Check sign-in and refresh.', { exact: true })
      .waitFor();
    expect(await picker.locator('input').inputValue()).toBe('newer-model');
    expect(
      await picker.getByRole('button', { name: 'newer-model · Recommended', exact: false }).count(),
    ).toBe(0);
  } finally {
    await app.close();
  }
}, 90_000);
