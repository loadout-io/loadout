/* T-34 AC-4: prawdziwy ekran Run, prawdziwy klik i widoczny wynik kopiowania diagnostyki.
 *
 * Niezmiennik 29 zabrania sądzić samą funkcję składającą komunikat. Ten plik otwiera ten sam
 * frontend co aplikacja, pozwala efektowi workspace'ów wybrać prawdziwy aktywny zakres i klika
 * kontrolkę w Chromium. Granica kończy się dokładnie na invoke: harness nagrywa argumenty i
 * daje najpierw odmowę, potem licznikowy receipt — nigdy treść raportu.
 */
import { afterAll, beforeAll, describe, expect, it } from 'vitest';

import type { RunningApp, TauriReply } from '../harness';
import { closeEverything, openApp } from '../harness';

const WORK = 'main[data-section="run"]';
const BUTTON = 'button[aria-label="Copy diagnostics"]';
const FOLDER = '/Users/somebody/Projects/loadout-e2e';
const WORKSPACE = { id: FOLDER, name: 'Diagnostics fixture', folder: FOLDER };
const OTHER_FOLDER = '/Users/somebody/Projects/other-diagnostics-e2e';
const OTHER_WORKSPACE = {
  id: OTHER_FOLDER,
  name: 'Other diagnostics fixture',
  folder: OTHER_FOLDER,
};
const REFUSAL = 'Loadout could not copy diagnostics.';
const COPIED = 'Diagnostics copied';
const PRIVATE_FAILURE = 'PRIVATE stderr must never become UI';
const APPEARS = 4_000;

const COPY_REPLIES: readonly TauriReply[] = [
  { error: PRIVATE_FAILURE },
  { value: { runs: 3, conversations: 2, artifacts: 11 } },
];

async function openWork(
  copyReplies: readonly TauriReply[] = COPY_REPLIES,
  workspaces: readonly (typeof WORKSPACE)[] = [WORKSPACE],
): Promise<RunningApp> {
  const app = await openApp({
    replies: {
      list_workspaces: [{ value: workspaces }],
      copy_diagnostics: copyReplies,
    },
  });
  await app.page.locator(WORK).waitFor({ state: 'attached', timeout: APPEARS });
  /* Kontrola przeciw pytaniu o nieistniejący zakres: przycisk ma dostać wartość, którą ekran
   * naprawdę pokazuje jako aktywną, nie ścieżkę wstawioną bezpośrednio do propsa testowego. */
  await app.page
    .locator('[data-workspace-open]')
    .filter({ hasText: WORKSPACE.name })
    .waitFor({ state: 'attached', timeout: APPEARS });
  return app;
}

async function switchTo(app: RunningApp, workspace: typeof WORKSPACE): Promise<void> {
  await app.page.click('[data-workspace-open]');
  await app.page.click(`[data-workspace-pick="${workspace.id}"]`);
  await app.page
    .locator('[data-workspace-open]')
    .filter({ hasText: workspace.name })
    .waitFor({ state: 'visible', timeout: APPEARS });
}

beforeAll(async () => {
  const warm = await openApp();
  await warm.close();
}, 180_000);

afterAll(async () => {
  await closeEverything();
}, 30_000);

describe('copying diagnostics from the real Run screen', () => {
  it('scopes both attempts to the active workspace and shows refusal, retry and success', async () => {
    const app = await openWork();
    try {
      expect(
        await app.page.locator(BUTTON).count(),
        'the real Run screen has no Copy diagnostics control. A callable IO function would not ' +
          'help the person looking at this screen; the production path has to mount the control.',
      ).toBe(1);

      const button = app.page.locator(BUTTON);
      await button.click();
      await app.page.getByText(REFUSAL, { exact: true }).waitFor({
        state: 'visible',
        timeout: APPEARS,
      });

      expect(
        await app.page.getByText(PRIVATE_FAILURE, { exact: true }).count(),
        'the arbitrary rejection crossed the privacy boundary and became visible. Diagnostics ' +
          'failures use the allowlisted sentence, never raw stderr or server text.',
      ).toBe(0);
      expect(
        await button.isEnabled(),
        'the copy attempt was refused and the only retry control stayed disabled.',
      ).toBe(true);

      await button.click();
      await app.page.getByText(COPIED, { exact: true }).waitFor({
        state: 'visible',
        timeout: APPEARS,
      });

      const calls = (await app.calls()).filter((call) => call.cmd === 'copy_diagnostics');
      expect(
        calls,
        'one click and one retry must cross the Tauri boundary exactly twice, with the active ' +
          'workspace as the complete scope.',
      ).toEqual([
        { cmd: 'copy_diagnostics', args: { folder: FOLDER } },
        { cmd: 'copy_diagnostics', args: { folder: FOLDER } },
      ]);
    } finally {
      await app.close();
    }
  }, 90_000);

  it('resets on a workspace switch and ignores the late result owned by the previous folder', async () => {
    const app = await openWork(
      [
        { deferred: 'copy-from-first-workspace' },
        { value: { runs: 1, conversations: 1, artifacts: 2 } },
      ],
      [WORKSPACE, OTHER_WORKSPACE],
    );
    try {
      const button = app.page.locator(BUTTON);
      await button.click();
      expect(await button.textContent()).toBe('Copying…');

      await switchTo(app, OTHER_WORKSPACE);
      expect(
        await app.page.getByText(COPIED, { exact: true }).count(),
        'the new workspace inherited the previous workspace success state.',
      ).toBe(0);
      expect(await app.page.getByText(REFUSAL, { exact: true }).count()).toBe(0);
      expect(await button.textContent()).toBe('Copy diagnostics');
      expect(await button.isEnabled()).toBe(true);

      await button.click();
      await app.page.getByText(COPIED, { exact: true }).waitFor({
        state: 'visible',
        timeout: APPEARS,
      });

      await app.settle('copy-from-first-workspace', { error: PRIVATE_FAILURE });
      await app.page.waitForTimeout(300);
      expect(await app.page.getByText(COPIED, { exact: true }).count()).toBe(1);
      expect(await app.page.getByText(REFUSAL, { exact: true }).count()).toBe(0);
      expect(await app.page.locator('body').innerText()).not.toContain(PRIVATE_FAILURE);

      expect((await app.calls()).filter((call) => call.cmd === 'copy_diagnostics')).toEqual([
        { cmd: 'copy_diagnostics', args: { folder: FOLDER } },
        { cmd: 'copy_diagnostics', args: { folder: OTHER_FOLDER } },
      ]);
    } finally {
      await app.close();
    }
  }, 90_000);
});
