/* Pusta biblioteka połączeń mówi, skąd je wziąć — i przycisk naprawdę tam prowadzi (2026-09-13).
 *
 * PO CO TO ISTNIEJE. Nowy agent startuje od dziś z połączeniami włączonymi w bibliotece
 * projektu. Kiedy tam nie ma ani jednego, pole Connections jest puste tak samo jak przedtem —
 * a biblioteka projektu jest świeża (2026-09-10: zawartość projektu nie korzysta już
 * z domyślnej biblioteki użytkownika), więc właśnie tak wygląda każdy projekt właściciela do
 * pierwszego importu. Puste pole bez słowa zostawia go z pytaniem, skąd te nazwy w ogóle wziąć.
 *
 * CO SĄDZIMY: zdanie i przycisk pod polem, które człowiek widzi po `More settings`, oraz okno
 * importu na ekranie po kliknięciu (niezmiennik 16: kontrolka ma skutek, niezmiennik 29: na
 * ekranie, nie w funkcji).
 *
 * DRUGI PRZYPADEK JEST STRAŻNIKIEM, nie kryterium. Odczyt, który się nie udał, nie wie, czy
 * połączeń jest zero — zdanie „jeszcze nie ma" byłoby wtedy relacją, której nie ma w danych
 * (niezmiennik 17).
 */
import { afterAll, expect, it } from 'vitest';
import { closeEverything, openApp } from '../../../e2e/harness';
import type { RunningApp, TauriReply } from '../../../e2e/harness';

const PROJECT = '/work/no-connections-yet';
const WORKSPACES: readonly TauriReply[] = Array.from({ length: 12 }, () => ({
  value: [{ id: PROJECT, folder: PROJECT, name: 'No connections yet' }],
}));

/** Zdanie spod pola, słowo w słowo — nazywa OBA źródła, bo żaden z dwóch importów nie prowadzi do obu. */
const WHERE =
  'No connections in this project yet: import the tool servers Claude Code or Codex already use ' +
  'here, or copy them from another project or the previous shared library with Import setup ' +
  'from project in the project menu.';

afterAll(closeEverything, 30_000);

/** Otwiera nowego agenta w Agents i rozwija `More settings`, gdy odczyt połączeń już wyszedł. */
async function newAgentWithMoreSettings(app: RunningApp): Promise<void> {
  await app.page.locator('[data-section-switch="agents"]').click();
  for (let waited = 0; waited < 50; waited += 1) {
    const asked = (await app.calls()).some(
      (one) => one.cmd === 'list_connections' && one.args['folder'] === PROJECT,
    );
    if (asked) break;
    await app.page.waitForTimeout(100);
  }
  await app.page.locator('main [data-create]').first().click();
  await app.page.getByRole('button', { name: 'More settings', exact: true }).click();
}

it('says under Connections where they come from when this project has none, and opens the import', async () => {
  const app = await openApp({
    replies: {
      list_workspaces: WORKSPACES,
      list_connections: Array.from({ length: 12 }, () => ({ value: [] })),
    },
  });
  try {
    await newAgentWithMoreSettings(app);
    expect(
      await app.page.locator('#agent-connections').count(),
      'the sentence has to stand under the Connections field, never instead of it',
    ).toBe(1);
    expect(
      await app.page.getByText(WHERE, { exact: true }).count(),
      'this project has no connections at all and the empty field says nothing about where ' +
        'they come from, so the person is left typing server names from memory',
    ).toBe(1);
    const importing = app.page.getByRole('button', { name: 'Import tool servers', exact: true });
    expect(
      await importing.count(),
      'the sentence names a way in and there is no button under it that takes it',
    ).toBe(1);

    await importing.click();
    const dialog = app.page.getByRole('dialog', { name: 'Import setup', exact: true });
    await dialog.waitFor({ state: 'visible', timeout: 5_000 }).catch(() => undefined);
    expect(
      await dialog.isVisible(),
      'pressing Import tool servers left the screen as it was, so the button promises an ' +
        'import it never opens',
    ).toBe(true);
  } finally {
    await app.close();
  }
}, 90_000);

it('says nothing about where connections come from when they could not be read', async () => {
  const app = await openApp({
    replies: {
      list_workspaces: WORKSPACES,
      list_connections: Array.from({ length: 12 }, () => ({
        error: 'Loadout could not read the connections of this project.',
      })),
    },
  });
  try {
    await newAgentWithMoreSettings(app);
    const field = app.page.locator('#agent-connections');
    expect(await field.count(), 'the new agent form has no Connections field at all').toBe(1);
    expect(
      await field.inputValue(),
      'the connections could not be read and the new agent still came with some',
    ).toBe('');
    expect(
      await app.page.getByText(WHERE, { exact: true }).count(),
      'the read failed, so nobody knows the library is empty, and the screen says it is anyway',
    ).toBe(0);
  } finally {
    await app.close();
  }
}, 90_000);
