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

it('saves copies and failure handling from the folded rows into the workflow file', async () => {
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
    },
  });
  try {
    await app.page.locator('[data-section-switch="workflows"]').click();
    await app.page.getByRole('button', { name: NAME, exact: false }).first().click();
    await app.page.locator('main [data-step="build"]').click();
    const panel = app.page.locator('main [data-step-panel]');
    await panel.waitFor({ state: 'visible', timeout: 20_000 });

    const more = panel.locator('[data-more-settings]');
    expect(await more.getAttribute('open')).toBeNull();
    await panel.locator('[data-more-settings] > summary').click();
    await panel.getByRole('spinbutton', { name: 'How many at once', exact: true }).fill('3');
    await panel
      .getByRole('combobox', { name: 'If this step does not pass', exact: true })
      .selectOption('stop');

    await expect
      .poll(async () => {
        const saved = (await app.calls()).filter((call) => call.cmd === 'save_workflow').at(-1)
          ?.args['workflow'];
        if (typeof saved !== 'object' || saved === null) return null;
        const steps = (saved as { steps?: unknown }).steps;
        if (!Array.isArray(steps)) return null;
        const step = steps[0];
        if (typeof step !== 'object' || step === null) return null;
        return {
          copies: (step as { copies?: unknown }).copies,
          whenItFails: (step as { whenItFails?: unknown }).whenItFails,
        };
      })
      .toEqual({ copies: 3, whenItFails: 'stop' });
  } finally {
    await app.close();
  }
}, 90_000);
