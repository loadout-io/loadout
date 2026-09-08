/* CT-05: the real editor shows the effective choice and keeps Context behind More settings.
 * The browser crosses the production IO adapter; Rust semantics are covered by the paired
 * integration module because this harness deliberately stops at the Tauri boundary. */
import { afterAll, beforeAll, describe, expect, it } from 'vitest';

import type { RunningApp, TauriReply } from '../harness';
import { closeEverything, openApp } from '../harness';

const PATH = 'context-selection.json';
const APPEARS = 20_000;
const AGENT = {
  schema: 1,
  id: 'frontend-agent',
  name: 'Frontend agent',
  summary: 'Builds the interface',
  color: 'clay',
  instructions: 'Implement the requested interface.',
  runsWith: 'claude-code',
  model: 'opus',
  thinking: 'balanced',
  fileAccess: 'work-freely',
  giveUpAfterMinutes: 20,
  tools: 'everything',
  reachesTheWeb: false,
  skills: [],
  connections: [],
  writeResultsTo: 'handoffs/result.md',
};
const WORKFLOW = {
  format: 2 as const,
  id: 'wf-context-selection',
  name: 'Context selection',
  context: {
    schema: 1 as const,
    sets: [{ id: 'S', revision: 'r1', topics: ['checkout', 'returns'] }],
  },
  steps: [
    {
      kind: 'agent' as const,
      id: 'frontend',
      name: 'Frontend',
      agent: AGENT.id,
      overrides: {},
      copies: 1,
      instructions: 'Build checkout.',
      skills: 'all' as const,
      folder: { use: 'project' as const },
      handover: 'notes' as const,
      at: { x: 24, y: 24 },
      context: {
        schema: 1 as const,
        inheritWorkflow: true,
        exclude: [],
        sets: [{ id: 'S', revision: 'r1', topics: ['checkout'] }],
      },
    },
  ],
  links: [],
};
const VIEW = {
  catalog: [
    {
      id: 'S',
      title: 'Store rules',
      description: 'Checkout and returns',
      revision: 'r1',
      topics: [
        { id: 'checkout', title: 'Checkout' },
        { id: 'returns', title: 'Returns' },
      ],
      said: null,
    },
  ],
  workflow: [
    {
      id: 'S',
      title: 'Store rules',
      revision: 'r1',
      selectedTopics: ['checkout', 'returns'],
      topics: [
        { id: 'checkout', title: 'Checkout' },
        { id: 'returns', title: 'Returns' },
      ],
      source: 'workflow',
      update: null,
      said: null,
    },
  ],
  steps: [
    {
      stepId: 'frontend',
      sets: [
        {
          id: 'S',
          title: 'Store rules',
          revision: 'r1',
          selectedTopics: ['checkout'],
          topics: [
            { id: 'checkout', title: 'Checkout' },
            { id: 'returns', title: 'Returns' },
          ],
          source: 'step',
          update: null,
          said: null,
        },
      ],
      omitted: [],
      inheritsWorkflow: true,
      protectedScope: false,
      said: null,
    },
  ],
  warnings: [],
};

function copies<T>(value: T, count = 20): readonly { readonly value: T }[] {
  return Array.from({ length: count }, () => ({ value }));
}

function scene(
  contextReplies: readonly TauriReply[] = copies(VIEW),
): Readonly<Record<string, readonly TauriReply[]>> {
  return {
    list_workflows: copies([{ path: PATH, workflow: WORKFLOW }]),
    load_workflow: copies({ workflow: WORKFLOW, revision: 'workflow-r1' }),
    check_workflow: copies([]),
    list_agents: copies([AGENT]),
    list_skills: copies([]),
    resolve_workflow_context: contextReplies,
    save_workflow: copies('workflow-r2'),
  };
}

async function openContextPanel(app: RunningApp): Promise<void> {
  await app.page.locator('[data-section-switch="workflows"]').click();
  await app.page.locator('main [data-tile]').first().click();
  await app.page.locator('main [data-step="frontend"]').click();
  const panel = app.page.locator('main [data-step-panel]');
  await panel.waitFor({ state: 'visible', timeout: APPEARS });
  await panel.locator('summary').filter({ hasText: 'more settings' }).click();
  await panel.locator('[data-row="context"]').waitFor({ state: 'visible', timeout: APPEARS });
}

beforeAll(async () => {
  const warm = await openApp();
  await warm.close();
}, 180_000);

afterAll(async () => {
  await closeEverything();
}, 30_000);

describe('workflow and step context selection', () => {
  it('a narrowed step says it added the set to this step, not that it came from the workflow', async () => {
    const app = await openApp({ replies: scene() });
    try {
      await openContextPanel(app);
      const row = app.page.locator('[data-row="context"]');

      await expect
        .poll(() => row.innerText(), { timeout: APPEARS })
        .toContain('Added to this step');
      expect(await row.innerText()).toContain('1 topic');
      expect(await row.innerText()).not.toContain('From workflow');
      expect(await app.page.locator('[data-workflow-context]').count()).toBe(1);
      expect(await app.page.locator('main [data-step-panel] > [data-row]').count()).toBe(5);

      const picker = app.page.locator('[data-workflow-context]');
      expect(await picker.locator('summary').first().innerText()).toContain('1 selected');
      await picker.locator('summary').first().click();
      await picker.getByRole('searchbox').fill('not this set');
      await expect.poll(() => picker.getByText('Store rules', { exact: true }).count()).toBe(0);
      await picker.getByRole('searchbox').fill('store');
      await expect.poll(() => picker.getByText('Store rules', { exact: true }).count()).toBe(1);
      expect(
        await picker.getByRole('checkbox', { name: 'Store rules', exact: true }).isChecked(),
      ).toBe(true);

      await row.locator('[data-step-context-picker] > summary').click();
      await row.getByRole('checkbox', { name: 'Use workflow context', exact: true }).click();
      await expect
        .poll(
          async () => (await app.calls()).filter((call) => call.cmd === 'save_workflow').length,
          { timeout: APPEARS },
        )
        .toBeGreaterThan(0);
      const saved = (await app.calls()).findLast((call) => call.cmd === 'save_workflow');
      expect(saved?.args['workflow']).toMatchObject({
        steps: [{ context: { inheritWorkflow: false } }],
      });
    } finally {
      await app.close();
    }
  }, 90_000);

  it('shows a missing pinned version in the panel while the same draft remains saveable', async () => {
    const missing = structuredClone(VIEW);
    const selected = missing.steps[0]?.sets[0];
    if (selected === undefined) throw new Error('the fixture lost its selected context');
    selected.said =
      'Context set S version r1 is not available now. This draft can still be saved; open Context and choose or build a ready version before starting.';
    const app = await openApp({ replies: scene(copies(missing)) });
    try {
      await openContextPanel(app);
      const row = app.page.locator('[data-row="context"]');
      await expect
        .poll(() => row.innerText(), { timeout: APPEARS })
        .toContain('This draft can still be saved');

      await app.page.locator('#workflow-name').fill('Context selection draft');
      await expect
        .poll(
          async () => (await app.calls()).filter((call) => call.cmd === 'save_workflow').length,
          { timeout: APPEARS },
        )
        .toBeGreaterThan(0);
      const saved = (await app.calls()).findLast((call) => call.cmd === 'save_workflow');
      expect(saved?.args['workflow']).toMatchObject({
        context: WORKFLOW.context,
        steps: [{ context: WORKFLOW.steps[0]?.context }],
      });
    } finally {
      await app.close();
    }
  }, 90_000);

  it('puts malformed and conflicting selections in the actual Context row', async () => {
    const refusal =
      'Context set S is selected more than once in the same list. Keep one selection.';
    const app = await openApp({
      replies: scene(Array.from({ length: 20 }, () => ({ error: refusal }))),
    });
    try {
      await openContextPanel(app);
      await expect
        .poll(() => app.page.locator('[data-row="context"]').innerText(), { timeout: APPEARS })
        .toContain(refusal);
    } finally {
      await app.close();
    }
  }, 90_000);

  it('requires a new topic choice when an update removed the pinned topic', async () => {
    const changed = structuredClone(VIEW);
    const catalog = changed.catalog[0];
    const selected = changed.steps[0]?.sets[0];
    if (catalog === undefined || selected === undefined) {
      throw new Error('the fixture lost its context version');
    }
    catalog.revision = 'r2';
    catalog.topics = [{ id: 'payment', title: 'Payment' }];
    selected.update = 'Update available';
    const app = await openApp({ replies: scene(copies(changed)) });
    try {
      await openContextPanel(app);
      const row = app.page.locator('[data-row="context"]');
      await row.locator('[data-step-context-picker] > summary').click();
      await row.locator('[data-context-update] > summary').click();
      expect(await row.getByRole('button', { name: 'Update', exact: true }).count()).toBe(0);

      await row.locator('[data-context-update] details > summary').click();
      await row.getByRole('checkbox', { name: 'Payment', exact: true }).click();
      await row.getByRole('button', { name: 'Update', exact: true }).click();
      await expect
        .poll(
          async () => (await app.calls()).filter((call) => call.cmd === 'save_workflow').length,
          { timeout: APPEARS },
        )
        .toBeGreaterThan(0);
      const saved = (await app.calls()).findLast((call) => call.cmd === 'save_workflow');
      expect(saved?.args['workflow']).toMatchObject({
        steps: [{ context: { sets: [{ id: 'S', revision: 'r2', topics: ['payment'] }] } }],
      });
    } finally {
      await app.close();
    }
  }, 90_000);
});
