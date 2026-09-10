import { afterAll, expect, it } from 'vitest';
import { closeEverything, openApp } from '../harness';
afterAll(closeEverything, 30_000);

it('previews individual setup cards, includes workflow dependencies and copies only the selection', async () => {
  const a = '/projects/atlas',
    b = '/projects/blank';
  const item = (category: string, id: string, name: string, requires: string[] = []) => ({
    key: category + ':' + id,
    category,
    name,
    summary: 'Reusable project setup',
    preview: 'Preview for ' + name,
    requires,
    problems: [],
    alreadyHere: false,
  });
  const app = await openApp({
    replies: {
      list_workspaces: [
        {
          value: [
            { id: b, name: 'Blank project', folder: b },
            { id: a, name: 'Atlas', folder: a },
          ],
        },
      ],
      preview_project_setup: [
        {
          value: {
            revision: 'snapshot-1',
            items: [
              item('agent', 'writer', 'Atlas writer'),
              item('agent', 'qa', 'Reviewer'),
              item('workflow', 'ship', 'Ship a feature', ['agent:writer']),
              item('note', 'rule', 'Naming conventions'),
            ],
          },
        },
      ],
      import_project_setup: [{ value: { imported: ['agent:writer', 'workflow:ship'] } }],
    },
  });
  try {
    await expect
      .poll(async () => (await app.calls()).some((c) => c.cmd === 'list_workspaces'))
      .toBe(true);
    await app.page.locator('[data-workspace-open]').click();
    expect(await app.page.getByRole('button', { name: 'Import setup from project' }).count()).toBe(
      1,
    );
    await app.page.getByRole('button', { name: 'Import setup from project' }).click();
    const dialog = app.page.getByRole('dialog', { name: 'Import setup' });
    await dialog.getByRole('button', { name: 'Atlas', exact: false }).click();
    await expect.poll(() => dialog.locator('[data-setup-item]').count()).toBe(4);
    await dialog.getByRole('checkbox', { name: 'Select Ship a feature' }).check();
    expect(await dialog.getByRole('checkbox', { name: 'Select Atlas writer' }).isChecked()).toBe(
      true,
    );
    expect(await dialog.getByRole('checkbox', { name: 'Select Reviewer' }).isChecked()).toBe(false);
    await dialog.getByRole('button', { name: /^Knowledge/ }).click();
    await dialog.getByRole('textbox', { name: 'Search setup' }).fill('Naming');
    expect(await dialog.locator('[data-setup-item]').count()).toBe(1);
    await dialog.getByRole('textbox', { name: 'Search setup' }).fill('');
    await dialog.getByRole('button', { name: /^All/ }).click();
    expect(await dialog.getByRole('checkbox', { name: 'Select Ship a feature' }).isChecked()).toBe(
      true,
    );
    await dialog.getByRole('button', { name: 'Preview Ship a feature' }).click();
    expect(await dialog.innerText()).toContain('Preview for Ship a feature');
    expect(await dialog.innerText()).toContain('Included with Ship a feature');
    await app.page.screenshot({ path: '/tmp/loadout-project-setup-import.png' });
    await app.page.setViewportSize({ width: 760, height: 850 });
    await app.page.screenshot({ path: '/tmp/loadout-project-setup-narrow.png' });
    expect(await dialog.evaluate((node) => node.scrollWidth <= node.clientWidth + 1)).toBe(true);
    await dialog.getByRole('button', { name: 'Import 2 items' }).click();
    await expect
      .poll(async () => (await app.calls()).some((c) => c.cmd === 'import_project_setup'))
      .toBe(true);
    const call = (await app.calls()).find((c) => c.cmd === 'import_project_setup');
    expect(call?.args).toEqual({
      folder: b,
      sourceFolder: a,
      revision: 'snapshot-1',
      selected: ['agent:writer', 'workflow:ship'],
    });
    await expect.poll(() => dialog.innerText()).toContain('2 items added to Blank project');
    await dialog.getByRole('button', { name: 'Done', exact: true }).click();
    expect(await app.page.getByRole('dialog').count()).toBe(0);
    expect(
      await app.page
        .locator('[data-workspace-open]')
        .evaluate((node) => node === document.activeElement),
    ).toBe(true);
    await app.page.locator('[data-workspace-open]').click();
    await app.page.getByRole('button', { name: 'Import setup from project' }).click();
    await app.page.keyboard.press('Escape');
    expect(await app.page.getByRole('dialog').count()).toBe(0);
  } finally {
    await app.close();
  }
}, 90_000);
