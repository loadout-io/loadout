/* WF-25: rzeczywisty React/effect/klik. Rust jest atrapą granicy; prawdziwe sh i lease
 * dowodzi moduł a_live_preview_keeps_its_working_copy po drugiej stronie. */
import { afterAll, describe, expect, it } from 'vitest';
import { closeEverything, openApp } from '../harness';

const FOLDER = '/projects/preview-discovery';
const CWD = FOLDER + '/.loadout/runs/01950000-0000-7000-8000-000000000001/work/s_preview';
const WORKSPACE = { id: FOLDER, name: 'Preview discovery', folder: FOLDER };
const FILE = {
  path: 'preview-discovery.json',
  workflow: {
    format: 1,
    id: 'wf-preview-discovery',
    name: 'Preview discovery',
    steps: [
      {
        kind: 'serve',
        id: 's_preview',
        name: 'Preview',
        command: 'npm run dev',
        folder: { use: 'fresh-copy' },
        at: { x: 0, y: 0 },
      },
    ],
    links: [],
  },
};
const SERVICE = {
  pgid: 4242,
  command: 'npm run dev',
  alive: true,
  said: 'Listening',
  cwd: CWD,
  lifetime: 'window',
  service: {
    workspace: FOLDER,
    run_id: '01950000-0000-7000-8000-000000000001',
    node_key: 's_preview',
    service_id: '01950000-0000-7000-8000-000000000002',
    generation: 1,
  },
};
function replies(value: unknown, count = 24) {
  return Array.from({ length: count }, () => ({ value }));
}
afterAll(closeEverything, 30_000);

describe('a workflow-created preview enters the existing empty list', () => {
  it('appears after Start without a manual /start or a page reload, then opens its actual folder', async () => {
    const app = await openApp({
      replies: {
        list_workspaces: replies([WORKSPACE]),
        list_workflows: replies([FILE]),
        load_workflow: replies({ workflow: FILE.workflow, revision: 'r1' }),
        check_workflow: replies([]),
        list_processes: [{ value: [] }, ...replies([SERVICE])],
        run_workflow: [{ deferred: 'preview-discovery-run' }],
      },
    });
    try {
      const start = app.page.locator('button[data-workflow-run="manual"]');
      await start.waitFor({ state: 'visible', timeout: 5_000 });
      await app.page.waitForTimeout(150);
      expect((await app.calls()).filter((one) => one.cmd === 'list_processes').length).toBe(1);
      expect(await app.page.locator('[data-started]').count()).toBe(0);
      await start.click();
      await app.page.waitForTimeout(2_200);
      expect((await app.calls()).some((one) => one.cmd === 'run_workflow')).toBe(true);
      expect(
        await app.page.locator('[data-started]').count(),
        'Serve started after the initial empty reply, but no live observer asked again',
      ).toBe(1);
      await app.page.locator('[data-started] button').first().click();
      const panel = await app.page.locator('[data-started-output]').innerText();
      expect(panel).toContain(CWD);
      expect(panel).toContain('Keeps running after the workflow ends.');
    } finally {
      await app.close();
    }
  }, 45_000);
});
