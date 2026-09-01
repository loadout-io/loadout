/* Boczne menu naprawdę się zwija — po kliknięciu człowieka, w prawdziwej przeglądarce.
 *
 * PO CO TO ISTNIEJE, i to jest niezmiennik 29 wzięty dosłownie. Wszystko inne o dwóch trybach
 * da się osądzić czystym renderem: kryteria w `src/ui/shell/one-nav-two-modes.test.tsx` wołają
 * `collapseNav` wprost i pytają, co powłoka wtedy rysuje. Ani jedno z nich nie dotyka linii,
 * która naprawdę decyduje o tym, czy kontrolka żyje — `onClick` przycisku zwijania. Handler
 * podmieniony na `collapseNav(collapsed)` zamiast `collapseNav(!collapsed)` przechodzi tam
 * ZIELONO i zostawia człowieka z przyciskiem, po którym nic się nie dzieje. To jest dokładnie
 * ta rodzina wady, przez którą to repo w ogóle ma harness przeglądarkowy: kryterium zielone,
 * funkcja martwa.
 *
 * Ten plik klika prawdziwym kliknięciem i naciska prawdziwy klawisz, a potem pyta chromium
 * o SZEROKOŚĆ, którą naprawdę policzył. Mierzy przy tym trzy rzeczy, których render nie widzi:
 * że zwężona nawigacja dalej wpuszcza na sekcje, że wybór jedzie na dysk, i że droga powrotna
 * działa.
 *
 * CZEGO NIE MIERZY: prawdziwego okna Tauri. Na macOS nie ma czym nim wysterować i mówi o tym
 * nagłówek `../harness.ts`. Granica stoi tam, gdzie zaczyna się Rust, a `save_settings` łapie
 * nagrywająca atrapa.
 */
import { afterAll, beforeAll, describe, expect, it } from 'vitest';

import type { RunningApp } from '../harness';
import { closeEverything, openApp } from '../harness';
import { NAV_NARROW, NAV_WIDTH } from '../../src/ui/shell/titlebar';

/** Ile czekamy, aż powłoka stanie w dokumencie. */
const READY = 8_000;

/** Ile czekamy, aż React przerysuje okno po kliknięciu. Render i mikrozadanie, nie sieć. */
const SETTLE = 400;

const FOLD = '[data-nav-fold]';

let app: RunningApp;

/** Szerokość, którą chromium NAPRAWDĘ policzył dla bocznego menu. */
async function navWidth(): Promise<number> {
  const box = await app.page.locator('nav[data-chrome]').boundingBox();
  if (box === null) {
    throw new Error('the side nav was laid out with no box at all, so there is nothing to measure');
  }
  return Math.round(box.width);
}

beforeAll(async () => {
  app = await openApp();
  await app.page.setViewportSize({ width: 1512, height: 950 });
  await app.page.locator(FOLD).waitFor({ state: 'visible', timeout: READY });
}, 180_000);

afterAll(async () => {
  await closeEverything();
});

describe('the side nav folds and unfolds under a real click', () => {
  it('narrows on a click, still reaches every place, and opens again', async () => {
    expect(
      await navWidth(),
      'the window opens on a side nav that is not the wide mode. A fresh library has nobody ' +
        'who chose otherwise, so the mode a person meets first has to be the one where every ' +
        'label, count and reason is readable.',
    ).toBe(NAV_WIDTH);

    await app.page.click(FOLD);
    await app.page.waitForTimeout(SETTLE);

    expect(
      await navWidth(),
      'the fold control was clicked in a real browser and the side nav is still the same ' +
        'width. A handler that is wired up and does nothing looks identical in the markup and ' +
        'identical in a screenshot — this is the one place that can tell the difference.',
    ).toBe(NAV_NARROW);

    /* ZWĘŻONA NAWIGACJA DALEJ WPUSZCZA NA SEKCJE. Bez tego punktu „zwija się" byłoby prawdą
       o szerokości i milczeniem o tym, czy człowiek może jeszcze gdziekolwiek pójść. */
    await app.page.click('[data-section-switch="agents"]');
    await app.page
      .locator('main[data-section="agents"]')
      .waitFor({ state: 'attached', timeout: READY });

    expect(
      await app.page.locator('[data-section-switch="agents"]').count(),
      'the narrowed nav offers more than one control for Agents, or none. One place, one way ' +
        'in — in both modes.',
    ).toBe(1);

    await app.page.click(FOLD);
    await app.page.waitForTimeout(SETTLE);

    expect(
      await navWidth(),
      'the nav folded and would not unfold, so the person is locked into the narrow mode with ' +
        'the list of places, the counts, the reasons and the next step all behind a control ' +
        'that only goes one way',
    ).toBe(NAV_WIDTH);
  }, 60_000);

  it('takes the key it draws, and tells the disk what the person chose', async () => {
    /* Ognisko na `<body>`, nie na przycisku: skrót ma działać z dowolnego miejsca okna, a nie
       tylko wtedy, kiedy człowiek stoi akurat na kontrolce, którą zastępuje. */
    await app.page.locator('body').click({ position: { x: 900, y: 500 } });
    const before = (await app.calls()).filter((call) => call.cmd === 'save_settings').length;

    await app.page.keyboard.press('Meta+b');
    await app.page.waitForTimeout(SETTLE);

    expect(
      await navWidth(),
      'the nav draws ⌘B beside its fold control and the keyboard does nothing with it. A drawn ' +
        'key that does nothing is a control without a handler (invariant 16): the person ' +
        'presses it, the window stands still, and the only thing learned is that they cannot ' +
        'work this app.',
    ).toBe(NAV_NARROW);

    const wrote = (await app.calls()).filter((call) => call.cmd === 'save_settings');
    expect(
      wrote.length,
      'folding the nav sent nothing to Rust, so the choice dies with the window and the person ' +
        'makes it again on every launch',
    ).toBeGreaterThan(before);
    expect(
      wrote[wrote.length - 1]?.args,
      'save_settings reached Rust without saying what the nav mode now is. The file is one, so ' +
        'the write carries the whole entry — a call missing this key remembers everything ' +
        'except the thing that just changed.',
    ).toMatchObject({ navCollapsed: true });

    await app.page.keyboard.press('Meta+b');
    await app.page.waitForTimeout(SETTLE);
    expect(
      await navWidth(),
      'the key folds the nav and will not unfold it, so half of the shortcut is a promise ' +
        'nothing keeps',
    ).toBe(NAV_WIDTH);
  }, 60_000);
});
