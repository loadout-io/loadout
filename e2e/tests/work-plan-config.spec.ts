/* WP-02: Plan crosses the mounted editor and the production Tauri adapter. Rust owns the
 * graph semantics; this browser boundary proves that a human can set, save and reopen it. */
import { afterAll, beforeAll, describe, expect, it } from 'vitest';

import type { RunningApp, TauriCall, TauriReply } from '../harness';
import { closeEverything, openApp } from '../harness';

const PATH = 'work-plan-config.json';
const APPEARS = 20_000;
const AGENT = {
  schema: 1,
  id: 'planner-agent',
  name: 'Planner agent',
  summary: 'Plans the requested work',
  color: 'clay',
  instructions: 'Plan the requested work.',
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

function workflow(plan?: { mode: 'create' | 'use' }) {
  return {
    format: plan === undefined ? (1 as const) : (3 as const),
    id: 'wf-work-plan-config',
    name: 'Work plan config',
    steps: [
      {
        kind: 'agent' as const,
        id: 'planner',
        name: 'Planner',
        agent: AGENT.id,
        overrides: {},
        copies: 1,
        instructions: 'Plan the requested work.',
        skills: 'all' as const,
        folder: { use: 'project' as const },
        handover: 'notes' as const,
        at: { x: 24, y: 24 },
        ...(plan === undefined ? {} : { plan }),
      },
    ],
    links: [],
  };
}

function inheritedWorkflow() {
  return {
    format: 3 as const,
    id: 'wf-work-plan-config',
    name: 'Work plan config',
    steps: [
      {
        ...workflow({ mode: 'create' }).steps[0],
      },
      {
        ...workflow().steps[0],
        id: 'worker',
        name: 'Worker',
        instructions: 'Build the feature.',
        at: { x: 24, y: 168 },
      },
    ],
    links: [{ from: 'planner', to: 'worker' }],
  };
}

function copies<T>(value: T, count = 20): readonly { readonly value: T }[] {
  return Array.from({ length: count }, () => ({ value }));
}

function view(mode: 'create' | 'use', said: string | null = null) {
  return {
    steps: [
      {
        stepId: 'planner',
        mode,
        source: null,
        earlier: [],
        said,
      },
    ],
    warnings: [],
  };
}

function inheritedView() {
  return {
    steps: [
      ...view('create').steps,
      {
        stepId: 'worker',
        mode: 'use' as const,
        source: {
          stepId: 'planner',
          name: 'Planner',
          said: 'Inherited from Planner.',
        },
        earlier: [{ stepId: 'planner', name: 'Planner', said: 'Take the same plan as Planner.' }],
        said: null,
      },
    ],
    warnings: [],
  };
}

function scene(
  loaded: readonly TauriReply[],
  resolved: readonly TauriReply[] = copies(view('create')),
): Readonly<Record<string, readonly TauriReply[]>> {
  return {
    list_workflows: copies([{ path: PATH, workflow: workflow() }]),
    load_workflow: loaded,
    check_workflow: copies([]),
    list_agents: copies([AGENT]),
    list_skills: copies([]),
    resolve_workflow_context: copies(null),
    resolve_workflow_plan: resolved,
    save_workflow: copies('workflow-r2'),
  };
}

async function openPlanPanel(app: RunningApp, stepId = 'planner'): Promise<void> {
  await app.page.locator('main [data-tile]').first().click();
  await app.page.locator(`main [data-step="${stepId}"]`).click();
  const panel = app.page.locator('main [data-step-panel]');
  await panel.waitFor({ state: 'visible', timeout: APPEARS });
  await panel.locator('[data-more-settings] > summary').click();
  await panel.locator('[data-row="plan"]').waitFor({ state: 'visible', timeout: APPEARS });
  await panel.locator('[data-row="plan"] [data-step-plan-picker] > summary').click();
}

async function saves(app: RunningApp): Promise<readonly TauriCall[]> {
  return (await app.calls()).filter((call) => call.cmd === 'save_workflow');
}

beforeAll(async () => {
  const warm = await openApp();
  await warm.close();
}, 180_000);

afterAll(async () => {
  await closeEverything();
}, 30_000);

describe('Plan in the real workflow editor', () => {
  it('saves Create and shows it again after reopening the workflow', async () => {
    const app = await openApp({
      replies: scene([
        { value: { workflow: workflow(), revision: 'workflow-r1' } },
        { value: { workflow: workflow({ mode: 'create' }), revision: 'workflow-r2' } },
      ]),
    });
    try {
      await app.page.locator('[data-section-switch="workflows"]').click();
      await openPlanPanel(app);
      const row = app.page.locator('[data-row="plan"]');
      await row.getByRole('radio', { name: 'Create', exact: true }).click();

      await expect.poll(() => saves(app), { timeout: APPEARS }).not.toHaveLength(0);
      const saved = (await saves(app)).at(-1);
      expect(saved?.args['workflow']).toMatchObject({
        steps: [{ id: 'planner', plan: { mode: 'create' } }],
      });
      await expect
        .poll(() => app.page.locator('[data-more-settings] > summary').innerText())
        .toContain('Plan: Create');

      await app.page.getByRole('button', { name: 'All workflows', exact: true }).click();
      await openPlanPanel(app);
      expect(
        await app.page
          .locator('[data-row="plan"]')
          .getByRole('radio', { name: 'Create', exact: true })
          .isChecked(),
      ).toBe(true);
    } finally {
      await app.close();
    }
  }, 90_000);

  it('puts a named resolver refusal beside the Plan control', async () => {
    const refusal =
      'Planner needs a plan, but no step creates one. Set one earlier step to Plan: Create.';
    const use = workflow({ mode: 'use' });
    const app = await openApp({
      replies: scene(
        copies({ workflow: use, revision: 'workflow-r1' }),
        copies(view('use', refusal)),
      ),
    });
    try {
      await app.page.locator('[data-section-switch="workflows"]').click();
      await openPlanPanel(app);
      await expect
        .poll(() => app.page.locator('[data-row="plan"]').innerText(), { timeout: APPEARS })
        .toContain(refusal);
    } finally {
      await app.close();
    }
  }, 90_000);

  it('shows inherited Use and names the step that supplied it', async () => {
    const inherited = inheritedWorkflow();
    const app = await openApp({
      replies: scene(
        copies({ workflow: inherited, revision: 'workflow-r1' }),
        copies(inheritedView()),
      ),
    });
    try {
      await app.page.locator('[data-section-switch="workflows"]').click();
      await openPlanPanel(app, 'worker');
      await expect
        .poll(() => app.page.locator('[data-row="plan"]').innerText(), { timeout: APPEARS })
        .toContain('Inherited from Planner.');
      await expect
        .poll(() => app.page.locator('[data-more-settings] > summary').innerText())
        .toContain('Plan: Use');
    } finally {
      await app.close();
    }
  }, 90_000);
});
