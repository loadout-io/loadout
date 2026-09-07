/* Ekran Run, kiedy granica oddaje na listę adresatów coś, co listą nie jest (2026-09-07).
 *
 * PO CO TO ISTNIEJE, i to jest pomiar, nie ostrożność. WF-08 dołożyło komendę
 * `step_message_recipients` i zapisuje jej odpowiedź wprost do magazynu biegu, bo typ
 * `Promise<readonly StepSession[]>` w `sections/run/io.ts` jest RZUTOWANIEM (`invoke<…>`),
 * a nie sprawdzeniem. Odpowiedź, która nie jest listą, wchodziła więc do `messageSessions`,
 * a pierwszy render robił na niej `.filter` i rzucał.
 *
 * Zmierzone na ZBUDOWANEJ aplikacji, kolektorem gęstości (`scripts/density-collect.mjs`),
 * którego atrapa nie zna tej komendy i oddaje `null`: cały ekran Run schodził na kartę osłony
 * ze zdaniem „This screen stopped working, so Loadout kept the rest running." i powodem
 * „Cannot read properties of null (reading 'filter')". Strefa pracy `[data-work]` znikała
 * z drzewa, przez co `chromePixels` przestawało być mierzone, a karta awarii dokładała
 * animowany region ponad zapadkę — czyli check gęstości był czerwony, a przyczyną nie był
 * układ, tylko martwy ekran.
 *
 * SŁABA WERSJA: sprawdzić, że nie ma wyjątku. Przechodzi ją każdy `try` dokoła renderu.
 * Kryterium pyta o to, co widzi człowiek: czy strefa pracy dalej stoi i czy karta awarii
 * NIE stoi — bo to ona jest jedyną rzeczą, którą ekran Run pokazywał zamiast pracy.
 */
import { afterAll, expect, it } from 'vitest';

import { closeEverything, openApp } from '../../../e2e/harness';
import type { TauriReply } from '../../../e2e/harness';

const PROJECT = '/Users/somebody/Projects/listless-answer';

/** Ta sama odpowiedź na każde wywołanie: okno pyta o adresatów przy każdej zmianie kroków. */
function always(reply: TauriReply): readonly TauriReply[] {
  return Array.from({ length: 12 }, () => reply);
}

afterAll(async () => {
  await closeEverything();
}, 30_000);

it('keeps the work area standing when the answer about who can be messaged is not a list', async () => {
  const app = await openApp({
    replies: {
      list_workspaces: always({
        value: [{ id: PROJECT, folder: PROJECT, name: 'Listless answer' }],
      }),
      /* Dokładnie to, co oddaje granica, która tej komendy nie zna. */
      step_message_recipients: always({ value: null }),
    },
  });
  try {
    const page = app.page;
    await page.locator('main[data-section="run"]').waitFor({ state: 'attached' });
    /* Odpowiedź doszła: albo strefa pracy stoi, albo ekran już padł. */
    await expect
      .poll(async () => {
        const broke = await page.locator('[data-screen-broke]').count();
        const work = await page.locator('[data-work]').count();
        return broke > 0 || work > 0;
      })
      .toBe(true);

    expect(
      await page.locator('[data-screen-broke]').count(),
      'one answer the window did not understand took the whole Run screen with it: the person ' +
        'sees a failure card where the work area was, and the only way back is to restart the app',
    ).toBe(0);
    expect(
      await page.locator('[data-work]').count(),
      'the work area is gone, so there is nothing on the Run screen to work in',
    ).toBe(1);
  } finally {
    await app.close();
  }
}, 90_000);
