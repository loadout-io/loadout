/* 2026-09-09: mierzymy pustkę pod instrukcjami i ucięte przyciski ze zrzutu właściciela.
 * Prawdziwy ekran i CSS; atrapa IPC izoluje bibliotekę użytkownika od testu formularza. */
import { afterAll, expect, it } from 'vitest';
import { closeEverything, openApp } from '../harness';

afterAll(closeEverything);

it('keeps editing compact and generation controls inside the sidebar', async () => {
  const app = await openApp({
    replies: {
      list_agents: Array.from({ length: 8 }, () => ({
        value: [{ id: 'layout-agent', name: 'Code reviewer', instructions: 'Review the changes.' }],
      })),
      save_agent: [{ value: 'saved-revision' }],
    },
  });
  const page = app.page;
  try {
    await page.setViewportSize({ width: 1440, height: 1100 });
    await page.locator('[data-section-switch="agents"]').click();
    const sheet = page.locator('[data-role-sheet]');
    await sheet.waitFor();
    expect(await page.locator('#agent-description').isVisible()).toBe(false);
    const instructions = await page.locator('[data-field="instructions"]').boundingBox();
    const brain = await page.locator('[data-brain]').boundingBox();
    expect(instructions).not.toBeNull();
    expect(brain).not.toBeNull();
    expect(brain!.y - instructions!.y - instructions!.height).toBeLessThan(100);

    await page.getByText('Create from description', { exact: true }).click();
    for (const width of [1440, 1000]) {
      await page.setViewportSize({ width, height: 900 });
      const sidebar = await page.locator('[data-agent-index]').boundingBox();
      expect(sidebar).not.toBeNull();
      for (const vendor of ['codex', 'claude']) {
        const button = page.locator(`[data-field="create-with-${vendor}"]`);
        expect(await button.isVisible()).toBe(true);
        const box = await button.boundingBox();
        expect(box).not.toBeNull();
        expect(box!.x + box!.width).toBeLessThanOrEqual(sidebar!.x + sidebar!.width);
      }
      expect(await sheet.evaluate((node) => node.scrollWidth <= node.clientWidth)).toBe(true);
    }
    await page.locator('[data-brain]').click();
    await page.locator('[data-field="model"]').fill('sonnet');
    await page.locator('[data-brain]').click();
    await page.locator('[data-field="name"]').fill('Updated reviewer');
    await page.locator('[data-save]').click();
    await page.getByText('Saved', { exact: true }).waitFor();
    const saved = (await app.calls()).find((call) => call.cmd === 'save_agent');
    expect(saved?.args['agent']).toMatchObject({ name: 'Updated reviewer', model: 'sonnet' });
    await page.setViewportSize({ width: 1440, height: 1000 });
    await page.getByText('Create from description', { exact: true }).click();
    await sheet.evaluate((node) => {
      node.scrollTop = 0;
    });
    const name = await page.locator('#agent-name').boundingBox();
    const summary = await page.locator('#agent-summary').boundingBox();
    expect(name!.y).toBe(summary!.y);
    const save = await page.locator('[data-save]').boundingBox();
    expect(save!.y + save!.height).toBeLessThan(1000);
    await page.screenshot({ path: '/tmp/loadout-agent-editor-after.png' });
  } finally {
    await app.close();
  }
}, 60_000);
