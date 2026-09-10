import { afterAll, expect, it } from 'vitest';
import { closeEverything, openApp } from '../harness';

afterAll(closeEverything, 30_000);

it('does not put a late workflow list from A into the trigger editor in B', async () => {
  const a = '/projects/slow-a',
    b = '/projects/fast-b';
  const app = await openApp({
    replies: {
      list_workspaces: [
        {
          value: [
            { id: a, folder: a, name: 'Slow A' },
            { id: b, folder: b, name: 'Fast B' },
          ],
        },
      ],
    },
  });
  try {
    await app.page.evaluate(
      ({ a }) => {
        const host = globalThis as unknown as {
          __TAURI_INTERNALS__: {
            invoke: (cmd: string, args?: Record<string, unknown>) => Promise<unknown>;
          };
          finishA?: () => void;
        };
        const before = host.__TAURI_INTERNALS__.invoke;
        host.__TAURI_INTERNALS__.invoke = async (cmd, args) => {
          const result = await before(cmd, args);
          if (cmd === 'list_triggers') return [];
          if (cmd !== 'list_workflows') return result;
          const name = args?.['folder'] === a ? 'Only A' : 'Only B';
          if (name === 'Only A')
            await new Promise<void>((resolve) => {
              host.finishA = resolve;
            });
          return [
            { path: 'flow.json', workflow: { format: 1, id: 'flow', name, steps: [], links: [] } },
          ];
        };
      },
      { a },
    );
    await app.page.locator('[data-section-switch="triggers"]').click();
    await expect
      .poll(() =>
        app.page.evaluate(() => typeof (globalThis as unknown as { finishA?: unknown }).finishA),
      )
      .toBe('function');
    await app.page.locator('[data-workspace-open]').click();
    await app.page.locator(`[data-workspace-pick="${b}"]`).click();
    await app.page.getByRole('button', { name: 'Create trigger', exact: true }).click();
    await app.page.evaluate(() => (globalThis as unknown as { finishA: () => void }).finishA());
    const choices = app.page.getByRole('combobox', { name: 'Workflow', exact: true });
    await expect.poll(() => choices.innerText()).toContain('Only B');
    expect(await choices.innerText()).not.toContain('Only A');
  } finally {
    await app.close();
  }
}, 90_000);

it('opens a new project with an empty Agents screen and keeps the original agent in its project', async () => {
  const a = '/projects/atlas';
  const b = '/projects/empty';
  const app = await openApp({
    replies: {
      list_workspaces: [
        {
          value: [
            { id: a, folder: a, name: 'Atlas' },
            { id: b, folder: b, name: 'Empty project' },
          ],
        },
      ],
    },
  });
  try {
    await expect
      .poll(async () => (await app.calls()).some((c) => c.cmd === 'list_workspaces'))
      .toBe(true);
    await app.page.evaluate(
      ({ a }) => {
        const host = globalThis as unknown as {
          __TAURI_INTERNALS__: {
            invoke: (command: string, args?: Record<string, unknown>) => Promise<unknown>;
          };
        };
        const before = host.__TAURI_INTERNALS__.invoke;
        host.__TAURI_INTERNALS__.invoke = async (command, args) => {
          const result = await before(command, args);
          if (command !== 'list_agents') return result;
          if (args?.['folder'] !== a) return [];
          return [
            {
              id: 'atlas-agent',
              name: 'Atlas writer',
              summary: 'Only Atlas uses this role',
              instructions: 'Keep Atlas private.',
              schema: 1,
            },
          ];
        };
      },
      { a },
    );
    await app.page.locator('[data-section-switch="agents"]').click();
    await expect.poll(() => app.page.locator('[data-agent="atlas-agent"]').count()).toBe(1);
    await app.page.locator('[data-workspace-open]').click();
    await app.page.locator(`[data-workspace-pick="${b}"]`).click();
    await expect.poll(() => app.page.locator('[data-agent="atlas-agent"]').count()).toBe(0);
    expect(await app.page.locator('[data-section="agents"]').innerText()).not.toContain(
      'Keep Atlas private.',
    );
    expect((await app.calls()).some((c) => c.cmd === 'list_agents' && c.args['folder'] === b)).toBe(
      true,
    );
    await app.page.locator('[data-workspace-open]').click();
    await app.page.locator(`[data-workspace-pick="${a}"]`).click();
    await expect.poll(() => app.page.locator('[data-agent="atlas-agent"]').count()).toBe(1);
  } finally {
    await app.close();
  }
}, 90_000);

it('switches workflows, knowledge, context and Lab without carrying over project A', async () => {
  const a = '/projects/catalog-a',
    b = '/projects/catalog-b';
  const app = await openApp({
    replies: {
      list_workspaces: [
        {
          value: [
            { id: a, folder: a, name: 'Catalog A' },
            { id: b, folder: b, name: 'Catalog B' },
          ],
        },
      ],
    },
  });
  try {
    await expect
      .poll(async () => (await app.calls()).some((c) => c.cmd === 'list_workspaces'))
      .toBe(true);
    await app.page.evaluate(
      ({ a }) => {
        const host = globalThis as unknown as {
          __TAURI_INTERNALS__: {
            invoke: (cmd: string, args?: Record<string, unknown>) => Promise<unknown>;
          };
        };
        const before = host.__TAURI_INTERNALS__.invoke;
        const catalogs: Record<string, unknown[]> = {
          list_workflows: [
            {
              path: 'atlas.json',
              workflow: {
                format: 1,
                id: 'atlas-flow',
                name: 'Atlas workflow',
                steps: [],
                links: [],
              },
            },
          ],
          list_notes: [
            {
              place: 'project',
              id: 'atlas-note',
              title: 'Atlas convention',
              rule: 'Keep this in Atlas.',
              because: 'Project-specific choice.',
              status: 'in-use',
              scope: 'everywhere',
              agent: null,
              project: null,
              from: null,
              length: 22,
              occurrences: 1,
              modified: '2026-09-10',
            },
          ],
          list_skills: [
            { name: 'atlas-skill', summary: 'Atlas procedure', fromTheInternet: false },
          ],
          list_context_sets: [
            {
              schema: 1,
              id: 'atlas-context',
              title: 'Atlas reference',
              description: '',
              archived: false,
              draftRevision: 1,
              latestReadyRevision: null,
              createdAt: '2026-09-10',
              changedAt: '2026-09-10',
            },
          ],
          list_eval_sets: [
            {
              format: 1,
              id: 'atlas-eval',
              name: 'Atlas evaluation',
              subject: { kind: 'agent', id: 'atlas-agent' },
              cases: [],
              variants: [],
            },
          ],
        };
        host.__TAURI_INTERNALS__.invoke = async (cmd, args) => {
          const result = await before(cmd, args);
          const catalog = catalogs[cmd];
          if (!catalog) return result;
          return (args?.['folder'] ?? args?.['catalogFolder']) === a ? catalog : [];
        };
      },
      { a },
    );
    for (const [section, ownText, cmd] of [
      ['workflows', 'Atlas workflow', 'list_workflows'],
      ['knowledge', 'Keep this in Atlas.', 'list_notes'],
      ['context', 'Atlas reference', 'list_context_sets'],
      ['lab', 'Atlas evaluation', 'list_eval_sets'],
    ]) {
      await app.page.locator(`[data-section-switch="${section}"]`).click();
      const screen = app.page.locator(`[data-section="${section}"]`);
      await expect.poll(() => screen.innerText()).toContain(ownText);
      await app.page.locator('[data-workspace-open]').click();
      await app.page.locator(`[data-workspace-pick="${b}"]`).click();
      await expect.poll(() => screen.innerText()).not.toContain(ownText);
      expect(
        (await app.calls()).some(
          (c) => c.cmd === cmd && (c.args['folder'] ?? c.args['catalogFolder']) === b,
        ),
      ).toBe(true);
      await app.page.locator('[data-workspace-open]').click();
      await app.page.locator(`[data-workspace-pick="${a}"]`).click();
      await expect.poll(() => screen.innerText()).toContain(ownText);
    }
  } finally {
    await app.close();
  }
}, 90_000);
