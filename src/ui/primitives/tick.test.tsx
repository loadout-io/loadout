/* UX-2: jedna kontrolka wyboru ma zachować natywne zachowanie inputa, ale własny wygląd.
 * Kryteria przeglądarkowe pytają o prawdziwy markup aplikacji; skan drzewa zamyka drogę
 * do drugiego wyglądu dodanego później obok prymitywu. */
import { readdir, readFile } from 'node:fs/promises';
import { fileURLToPath } from 'node:url';

import type { Locator } from '@playwright/test';
import { afterAll, expect, it } from 'vitest';

import type { RunningApp, TauriReply } from '../../../e2e/harness';
import { closeEverything, openApp } from '../../../e2e/harness';

const ROOT = fileURLToPath(new URL('../../..', import.meta.url));
const APPEARS = 20_000;
/* 2026-09-09 (UX-2, poprawka po weryfikacji) — PEŁNA etykieta, nie skrócona. Kontrolka dostała
   był prop `name` skracający nazwę dostępną do „Learn from runs”, więc czytnik ekranu ogłaszał
   co innego, niż widać na ekranie — a jedynym konsumentem tej krótszej nazwy był ten test.
   Prop zniknął; nazwa dostępna wraca do widocznej etykiety (niezmiennik 29). */
const LEARNING_LABEL = 'Learn from runs by default';
const LEARNING_DESCRIPTION =
  'When this is on, Loadout keeps up to three notes from each finished run for you to approve ' +
  'in Knowledge.';
const PATH = 'tick-context.json';
const AGENT = {
  schema: 1,
  id: 'tick-agent',
  name: 'Interface agent',
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
  id: 'wf-tick-context',
  name: 'Tick context',
  steps: [
    {
      kind: 'agent' as const,
      id: 'interface',
      name: 'Interface',
      agent: AGENT.id,
      overrides: {},
      copies: 1,
      instructions: 'Build the interface.',
      skills: 'all' as const,
      folder: { use: 'project' as const },
      handover: 'notes' as const,
      at: { x: 24, y: 24 },
    },
  ],
  links: [],
};
const CONTEXT_VIEW = {
  catalog: [
    {
      id: 'draft',
      title: 'Draft rules',
      description: 'This set has no ready version.',
      revision: null,
      topics: [],
      said: null,
    },
    {
      id: 'ready',
      title: 'Ready rules',
      description: 'This set is ready to use.',
      revision: 'r1',
      topics: [{ id: 'interface', title: 'Interface' }],
      said: null,
    },
  ],
  workflow: [],
  steps: [
    {
      stepId: 'interface',
      sets: [],
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

function settingsReplies(): Readonly<Record<string, readonly TauriReply[]>> {
  return {
    read_settings: copies({
      defaultLead: '',
      defaultBudgetUsd: 75,
      navCollapsed: false,
      keepLastRuns: 0,
      learnFromRuns: false,
    }),
    save_settings: copies({
      defaultLead: '',
      defaultBudgetUsd: 75,
      navCollapsed: false,
      keepLastRuns: 0,
      learnFromRuns: true,
    }),
  };
}

function contextReplies(): Readonly<Record<string, readonly TauriReply[]>> {
  return {
    list_workflows: copies([{ path: PATH, workflow: WORKFLOW }]),
    load_workflow: copies({ workflow: WORKFLOW, revision: 'workflow-r1' }),
    check_workflow: copies([]),
    list_agents: copies([AGENT]),
    list_skills: copies([]),
    resolve_workflow_context: copies(CONTEXT_VIEW),
    save_workflow: copies('workflow-r2'),
  };
}

async function openContextPanel(app: RunningApp): Promise<void> {
  await app.page.locator('[data-section-switch="workflows"]').click();
  await app.page.locator('main [data-tile]').first().click();
  await app.page.locator('main [data-step="interface"]').click();
  const panel = app.page.locator('main [data-step-panel]');
  await panel.waitFor({ state: 'visible', timeout: APPEARS });
  await panel.locator('summary').filter({ hasText: 'more settings' }).click();
  const row = panel.locator('[data-row="context"]');
  await row.waitFor({ state: 'visible', timeout: APPEARS });
  await row.locator('[data-step-context-picker] > summary').click();
}

async function paint(box: Locator): Promise<{
  readonly appearance: string;
  readonly background: string;
  readonly borderWidth: number;
  readonly cursor: string;
  readonly opacity: string;
}> {
  return box.evaluate((element) => {
    const style = getComputedStyle(element);
    return {
      appearance: style.appearance,
      background: style.backgroundColor,
      borderWidth: Number.parseFloat(style.borderTopWidth),
      cursor: style.cursor,
      opacity: style.opacity,
    };
  });
}

async function resolvedAccent(app: RunningApp): Promise<string> {
  return app.page.evaluate(() => {
    const probe = document.createElement('span');
    probe.style.backgroundColor = 'var(--color-accent)';
    document.body.append(probe);
    const accent = getComputedStyle(probe).backgroundColor;
    probe.remove();
    return accent;
  });
}

async function sourceFiles(folder: string): Promise<string[]> {
  const found: string[] = [];
  for (const entry of await readdir(folder, { withFileTypes: true })) {
    const path = `${folder}/${entry.name}`;
    if (entry.isDirectory()) found.push(...(await sourceFiles(path)));
    if (entry.isFile() && /\.tsx?$/.test(entry.name) && !/\.test\.tsx?$/.test(entry.name)) {
      found.push(path);
    }
  }
  return found;
}

function withoutComments(source: string): string {
  return source.replace(/\/\*[\s\S]*?\*\//g, ' ').replace(/^\s*\/\/.*$/gm, ' ');
}

afterAll(closeEverything, 30_000);

it('the box a person sees is drawn by the app, not by the browser', async () => {
  const app = await openApp({ replies: settingsReplies() });
  try {
    await app.page.locator('[data-section-switch="settings"]').click();
    const box = app.page.getByRole('checkbox', { name: LEARNING_LABEL, exact: true });
    await box.waitFor({ state: 'visible', timeout: APPEARS });
    await expect.poll(() => box.isChecked()).toBe(false);

    const accent = await resolvedAccent(app);
    const untouched = await paint(box);
    expect(untouched.borderWidth, 'the untouched box has no visible edge').toBeGreaterThan(0);
    expect(untouched.background, 'the untouched box already looks selected').not.toBe(accent);

    await box.click();
    await expect.poll(() => box.isChecked()).toBe(true);
    const selected = await paint(box);
    expect(selected.background, 'the selected box is not filled with the application accent').toBe(
      accent,
    );
    expect(selected.appearance, 'the browser still draws the box').toBe('none');
  } finally {
    await app.close();
  }
}, 90_000);

it('src carries exactly one checkbox, and it is the primitive', async () => {
  const files = await sourceFiles(`${ROOT}/src`);
  const found: string[] = [];
  for (const path of files) {
    const source = withoutComments(await readFile(path, 'utf8'));
    for (const _match of source.matchAll(/type="checkbox"/g)) {
      found.push(path.slice(ROOT.length + 1));
    }
  }

  expect(
    found,
    `measured ${String(found.length)} checkbox literals in: ${found.join(', ')}`,
  ).toEqual(['src/ui/primitives/tick.tsx']);
});

it('a set with no ready version is visibly off and refuses the click', async () => {
  const app = await openApp({ replies: contextReplies() });
  try {
    await openContextPanel(app);
    const row = app.page.locator('[data-row="context"]');
    const unavailable = row.getByRole('checkbox', { name: 'Draft rules', exact: true });
    const available = row.getByRole('checkbox', { name: 'Ready rules', exact: true });
    expect(await unavailable.isDisabled()).toBe(true);
    expect(await unavailable.isChecked()).toBe(false);
    expect(await available.isChecked()).toBe(false);

    const unavailablePaint = await paint(unavailable);
    const availablePaint = await paint(available);
    expect(unavailablePaint.opacity, 'the unavailable set looks enabled').not.toBe(
      availablePaint.opacity,
    );
    expect(unavailablePaint.cursor, 'the unavailable set does not look unclickable').toBe(
      'not-allowed',
    );

    await unavailable.evaluate((element) => (element as HTMLInputElement).click());
    expect(await unavailable.isChecked()).toBe(false);
    await available.click();
    await expect.poll(() => available.isChecked()).toBe(true);
  } finally {
    await app.close();
  }
}, 90_000);

it('the name is the label alone and the sentence arrives as its description', async () => {
  const app = await openApp({ replies: settingsReplies() });
  try {
    await app.page.locator('[data-section-switch="settings"]').click();
    const box = app.page.getByRole('checkbox', { name: LEARNING_LABEL, exact: true });
    expect(await box.count(), 'the short label is not the checkbox name').toBe(1);
    expect(
      await app.page
        .getByRole('checkbox', {
          name: `${LEARNING_LABEL} ${LEARNING_DESCRIPTION}`,
          exact: true,
        })
        .count(),
      'the explanatory sentence grew the checkbox name',
    ).toBe(0);

    const describedBy = await box.getAttribute('aria-describedby');
    expect(describedBy, 'the checkbox has no route to its explanation').not.toBeNull();
    expect(
      await app.page.locator(`#${String(describedBy)}`).innerText(),
      'the checkbox points at a missing or different explanation',
    ).toBe(LEARNING_DESCRIPTION);
  } finally {
    await app.close();
  }
}, 90_000);
