/* Nowy agent startuje z połączeniami, które ten projekt ma włączone (2026-09-13).
 *
 * PO CO TO ISTNIEJE. Właściciel, 2026-09-11: „domyślnie to powinno wszystko być wypełnione,
 * zwłaszcza connections, a nie że ja mam sam pisać". Jego lider rozmawiał bez Lineara, Figmy
 * i Playwrighta, bo `＋ Create` wpisywał `connections: []`, a jedyną drogą do wypełnienia było
 * pole tekstowe z nazwami serwerów wklepanymi z pamięci — 26 z 32 jego agentów ma tam pustkę.
 *
 * CO SĄDZIMY: napis, który człowiek czyta w polu Connections po `＋ Create` i `More settings`,
 * bez wpisania czegokolwiek (niezmiennik 29). Atrapa oddaje wyłącznie `figma`, bo tak odpowiada
 * Rust — wyłączone połączenia odsiewa `enabled_names` i to sądzi strażnik
 * `the_enabled_connections_are_one_list` w celu `it`. Dokładne `'figma'` mówi więc tu także, że
 * okno nie dołożyło niczego od siebie.
 *
 * CZEKANIE NA ODCZYT NIE JEST ASERCJĄ, i to jest kolejność z rozmysłem: na dzisiejszym kodzie
 * komendy nie ma, więc kryterium ma paść na polu (`expected '' to be 'figma'`), a nie na
 * czekaniu. Że stub był naprawdę pytany, i to o TEN projekt, mówi osobna asercja na końcu.
 */
import { afterAll, expect, it } from 'vitest';
import { closeEverything, openApp } from '../../../e2e/harness';
import type { RunningApp, TauriReply } from '../../../e2e/harness';

const PROJECT = '/work/connections-here';
const replies = (value: unknown): readonly TauriReply[] =>
  Array.from({ length: 12 }, () => ({ value }));

afterAll(closeEverything, 30_000);

/** Czy okno zapytało już o połączenia biblioteki TEGO projektu. */
async function askedHere(app: RunningApp): Promise<boolean> {
  return (await app.calls()).some(
    (one) => one.cmd === 'list_connections' && one.args['folder'] === PROJECT,
  );
}

it('a new agent opens with the connections this project has turned on', async () => {
  const app = await openApp({
    replies: {
      list_workspaces: replies([{ id: PROJECT, folder: PROJECT, name: 'Connections here' }]),
      list_connections: replies(['figma']),
    },
  });
  try {
    await app.page.locator('[data-section-switch="agents"]').click();
    for (let waited = 0; waited < 50 && !(await askedHere(app)); waited += 1) {
      await app.page.waitForTimeout(100);
    }
    await app.page.locator('main [data-create]').first().click();
    await app.page.getByRole('button', { name: 'More settings', exact: true }).click();
    const field = app.page.locator('#agent-connections');
    expect(await field.count(), 'the new agent form has no Connections field at all').toBe(1);
    expect(
      await field.inputValue(),
      'a new agent in a project that has figma turned on opens with an empty Connections ' +
        'field, so the person has to type the name from memory or the agent works without it',
    ).toBe('figma');
    expect(
      await askedHere(app),
      'figma reached the field, but the window never asked for the connections of this ' +
        'project, so it came from somewhere other than the library of the open folder',
    ).toBe(true);
  } finally {
    await app.close();
  }
}, 90_000);
