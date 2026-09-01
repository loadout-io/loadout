/* T-151 AC-3: the mounted agent panel exposes all three file-location choices.
 *
 * Every change below starts with a real radio click, crosses save_workflow, closes the real
 * editor, and comes back through load_workflow. No store action or component handler is called.
 */
import { afterAll, beforeAll, describe, expect, it } from 'vitest';

import type { RunningApp, TauriCall, TauriReply } from '../harness';
import { closeEverything, openApp } from '../harness';

type Use = 'project' | 'fresh-copy' | 'same-copy';

const PATH = 't151-folders.json';
const LABELS: Readonly<Record<Use, string>> = {
  project: 'Work in the project folder',
  'fresh-copy': 'Start in a new copy of the files',
  'same-copy': 'Continue in the same files as the previous step',
};
const ORDER: readonly Use[] = ['fresh-copy', 'same-copy', 'project'];
const AGENT = {
  schema: 1,
  id: 'agent-t151-folders',
  name: 'T151 Builder',
  summary: 'Exercises file-location choices',
  color: 'slate',
  instructions: 'Build the requested change.',
  runsWith: 'claude-code',
  model: 'haiku',
  thinking: 'balanced',
  fileAccess: 'look-only',
  giveUpAfterMinutes: 20,
  tools: 'everything',
  reachesTheWeb: false,
  skills: [],
  connections: [],
  writeResultsTo: 'handoffs/result.md',
};

const SWITCH = '[data-section-switch="workflows"]';
const SCREEN = 'main[data-section="workflows"]';
const LIST_TILE = 'main [data-tile]';
const STEP = 'main [data-step="s_build"]';
const PANEL = 'main [data-step-panel]';
const APPEARS = 6_000;

function workflowWith(use: Use) {
  return {
    format: 1 as const,
    id: 'wf-t151-folder-choices',
    name: 'T151 folder choices',
    steps: [
      {
        kind: 'agent' as const,
        id: 's_build',
        name: 'Build',
        agent: AGENT.id,
        overrides: {},
        copies: 1,
        instructions: 'Build the requested change.',
        skills: 'all' as const,
        folder: { use },
        handover: 'notes' as const,
        at: { x: 24, y: 24 },
      },
    ],
    links: [],
  };
}

const ENTRY = { path: PATH, workflow: workflowWith('project') };

function copies<T>(value: T, count = 20): readonly { readonly value: T }[] {
  return Array.from({ length: count }, () => ({ value }));
}

function scene(): Readonly<Record<string, readonly TauriReply[]>> {
  return {
    list_workflows: copies([ENTRY]),
    load_workflow: [
      { value: workflowWith('project') },
      { value: workflowWith('fresh-copy') },
      { value: workflowWith('same-copy') },
      { value: workflowWith('project') },
    ],
    check_workflow: copies([]),
    list_agents: copies([AGENT]),
    list_skills: copies([]),
    save_workflow: copies(null),
  };
}

async function waitForSaves(app: RunningApp, count: number): Promise<readonly TauriCall[]> {
  const deadline = Date.now() + APPEARS;
  while (Date.now() < deadline) {
    const saves = (await app.calls()).filter((call) => call.cmd === 'save_workflow');
    if (saves.length >= count) return saves;
    await app.page.waitForTimeout(25);
  }
  return (await app.calls()).filter((call) => call.cmd === 'save_workflow');
}

async function openPanel(app: RunningApp): Promise<void> {
  await app.page.locator(LIST_TILE).first().click();
  await app.page.locator(STEP).click();
  await app.page.locator(PANEL).waitFor({ state: 'visible', timeout: APPEARS });
}

async function assertExactlyOneChoice(app: RunningApp, expected: Use): Promise<void> {
  const radios = app.page.locator(`${PANEL} input[type="radio"]`);
  expect(
    await radios.count(),
    'the agent panel must expose project, new copy, and previous-step files as three controls',
  ).toBe(3);
  for (const use of Object.keys(LABELS) as Use[]) {
    const control = app.page.getByRole('radio', { name: LABELS[use], exact: true });
    expect(await control.count(), `the panel has no unambiguous control for ${use}`).toBe(1);
    expect(await control.isChecked(), `${use} has the wrong selected state`).toBe(use === expected);
  }
}

function folderIn(call: TauriCall | undefined): string {
  const workflow = call?.args['workflow'];
  if (typeof workflow !== 'object' || workflow === null) return '';
  const steps = (workflow as Record<string, unknown>)['steps'];
  if (!Array.isArray(steps)) return '';
  const step = steps[0];
  if (typeof step !== 'object' || step === null) return '';
  const folder = (step as Record<string, unknown>)['folder'];
  if (typeof folder !== 'object' || folder === null) return '';
  const use = (folder as Record<string, unknown>)['use'];
  return typeof use === 'string' ? use : '';
}

beforeAll(async () => {
  const warm = await openApp();
  await warm.close();
}, 180_000);

afterAll(async () => {
  await closeEverything();
}, 30_000);

describe('an agent has three visible, exclusive file-location choices', () => {
  it('saves and reloads project, new-copy, and previous-step intent without migration', async () => {
    const app = await openApp({ replies: scene() });
    try {
      await app.page.locator(SWITCH).click();
      await app.page.locator(SCREEN).waitFor({ state: 'attached', timeout: APPEARS });
      await openPanel(app);
      await assertExactlyOneChoice(app, 'project');

      let savesSeen = 0;
      for (const use of ORDER) {
        await app.page.getByRole('radio', { name: LABELS[use], exact: true }).click();
        savesSeen += 1;
        const saves = await waitForSaves(app, savesSeen);
        expect(saves.length, `choosing ${use} did not cross production workflow IO`).toBe(savesSeen);
        expect(folderIn(saves[savesSeen - 1]), `the ${use} choice saved a different intent`).toBe(
          use,
        );
        expect(
          await app.page.locator(`${PANEL} input[type="radio"]:checked`).count(),
          'changing one file-location choice left more than one selected',
        ).toBe(1);

        await app.page.getByRole('button', { name: 'All workflows', exact: true }).click();
        await openPanel(app);
        await assertExactlyOneChoice(app, use);
      }

      const panelText = (await app.page.locator(PANEL).innerText()).toLowerCase();
      expect(panelText).not.toContain('worktree');
      expect(panelText).not.toContain('branch');
      expect(panelText).not.toContain('automatically merge');
    } finally {
      await app.close();
    }
  }, 90_000);
});
