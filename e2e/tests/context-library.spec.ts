/* CT-01 w PRAWDZIWEJ przeglądarce: nowa pozycja paska, utworzony zestaw i materiał, który po
 * wyjściu z sekcji i powrocie wraca Z GRANICY, a nie z pamięci okna.
 *
 * DLACZEGO POWRÓT IDZIE PRZEZ LISTĘ, A NIE PRZEZ OTWARTY EDYTOR. Magazyn zustanda żyje na
 * poziomie modułu, więc zestaw otwarty przed wyjściem stoi w nim dalej i pokazałby swój materiał
 * także wtedy, gdyby ani jedna komenda nie doszła do Rusta — czyli dokładnie w tej awarii, którą
 * to kryterium ma wykluczyć. Dlatego edytor jest tu ZAMYKANY przed wyjściem, a materiał wraca na
 * ekran dopiero po `read_context_set`: pytamy o drogę do granicy i z powrotem, nie o pamięć karty.
 *
 * DRUGI PRZYPADEK SĄDZI ZDANIE, KTÓRE WIDZI CZŁOWIEK (niezmiennik 29). Odmowa spóźnionego zapisu
 * dowiedziona na wartości `Err` mówi wyłącznie tyle, że mechanizm istnieje; kryterium 4 tego
 * etapu dotyczy zdania na ekranie, więc granica odpowiada tu odmową, a asercja stoi na dokumencie.
 */
import { afterAll, beforeAll, describe, expect, it } from 'vitest';

import type { TauriReply } from '../harness';
import { closeEverything, openApp } from '../harness';

const TITLE = 'Checkout redesign';
const TYPED = 'The checkout drops the promo code when the cart is edited.';

/** Rewizja `draft.json`, którą okno przeczytało. Wraca w ładunku każdego zapisu. */
const OPENED = 'rev-opened';
const AFTER_SAVE = 'rev-saved';

/** Zdanie Rusta, słowo w słowo (`context::Error::Changed`). Ekran nie ma prawa go przepisać. */
const REFUSED =
  'This context set was not saved: it changed on disk after you opened it, so nothing was ' +
  'overwritten. Open it again to see the newer one.';

const SET = {
  schema: 1,
  id: 'ct-checkout',
  title: TITLE,
  description: '',
  archived: false,
  draftRevision: 1,
  latestReadyRevision: null,
  createdAt: '2026-09-07T10:00:00Z',
  changedAt: '2026-09-07T10:00:00Z',
};

/** Zestaw prosto po utworzeniu: jest nazwa, nie ma jeszcze ani jednego źródła. */
const MADE = {
  set: SET,
  draft: { schema: 1, sources: [], excluded: [], howToPrepare: '', requirements: [] },
  revision: OPENED,
};

/** Ten sam zestaw po zapisie — z materiałem, którego szuka asercja na końcu. */
const SAVED = {
  set: SET,
  draft: {
    schema: 1,
    sources: [
      {
        id: 'typed',
        kind: 'text',
        name: 'Typed material',
        description: '',
        text: TYPED,
      },
    ],
    excluded: [],
    howToPrepare: '',
    requirements: [],
  },
  revision: AFTER_SAVE,
};

function copies<T>(value: T, count: number): readonly TauriReply[] {
  return Array.from({ length: count }, () => ({ value }) as TauriReply);
}

/* Pierwsza odpowiedź katalogu jest PUSTA, bo biblioteka na tej scenie zaczyna bez zestawów —
 * bez tego zdanie „nic tu jeszcze nie ma" nie miałoby czego dowieść. Kolejne są już z zestawem
 * i jest ich kilka, bo katalog odpowiada przy każdym wejściu do sekcji. */
const SCENE: Readonly<Record<string, readonly TauriReply[]>> = {
  list_context_sets: [{ value: [] }, ...copies([SET], 8)],
  create_context_set: copies(MADE, 4),
  save_context_draft: copies(SAVED, 4),
  read_context_set: copies(SAVED, 4),
};

/** Ta sama scena, tylko zapis jest odmawiany — i to odmową, którą pisze Rust. */
const REFUSING: Readonly<Record<string, readonly TauriReply[]>> = {
  list_context_sets: [{ value: [] }, ...copies([SET], 8)],
  create_context_set: copies(MADE, 4),
  save_context_draft: Array.from({ length: 4 }, () => ({ error: REFUSED })),
};

const SWITCH = '[data-section-switch="context"]';
const OTHER_SWITCH = '[data-section-switch="knowledge"]';
const SCREEN = 'main[data-section="context"]';
const EMPTY = 'main [data-empty]';
const NAME = '#context-new';
const CREATE = 'main [data-create]';
const EDITOR = 'main [data-context-editor]';
const MATERIAL = '#context-material';
const SAVE = 'main [data-save]';
const BACK = 'main [data-back]';
const CARD = 'main [data-context-set]';
const SAID = 'main [data-refusal]';

/** Ile czekamy na to, co ma przyjść po kliknięciu. Odpowiedź wraca w tej samej karcie. */
const APPEARS = 6_000;

/* Rozruch vite i chromium jest kosztem STAŁYM NA PLIK, nie częścią pierwszego przypadku. */
beforeAll(async () => {
  const warm = await openApp();
  await warm.close();
}, 180_000);

afterAll(async () => {
  await closeEverything();
}, 30_000);

describe('a named context set survives leaving the section', () => {
  it('reaches the new place from the nav, makes a set and reads its material back', async () => {
    const app = await openApp({ replies: SCENE });
    try {
      const page = app.page;

      /* ── nowa pozycja paska jest naprawdę klikalna ─────────────────────────────────────── */
      expect(
        await page.locator(SWITCH).count(),
        'the side nav has no way into Context at all, so everything below would be about a ' +
          'screen a person cannot reach.',
      ).toBe(1);
      await page.locator(SWITCH).click();
      await page.locator(SCREEN).waitFor({ state: 'attached', timeout: APPEARS });

      /* ── przed kliknięciem: pusto, i to jest powiedziane ───────────────────────────────── */
      await page
        .locator(EMPTY)
        .waitFor({ state: 'attached', timeout: APPEARS })
        .catch(() => undefined);
      expect(
        await page.locator(CARD).count(),
        'there must be no set on the list before one is made, or "a set is there" says nothing ' +
          'about the click that made it.',
      ).toBe(0);

      /* ── utworzenie nazwanego zestawu ──────────────────────────────────────────────────── */
      await page.locator(NAME).fill(TITLE);
      await page.locator(CREATE).click();
      await page
        .locator(EDITOR)
        .waitFor({ state: 'visible', timeout: APPEARS })
        .catch(() => undefined);
      expect(
        await page.locator(EDITOR).count(),
        'Create reached nobody, or it reached Rust and drew nothing. A button that talks to the ' +
          'boundary and leaves the screen where it was is the state a person calls broken.',
      ).toBe(1);

      /* ── materiał i zapis ──────────────────────────────────────────────────────────────── */
      await page.locator(MATERIAL).fill(TYPED);
      await page.locator(SAVE).click();

      const deadline = Date.now() + APPEARS;
      let saves = (await app.calls()).filter((call) => call.cmd === 'save_context_draft');
      while (saves.length === 0 && Date.now() < deadline) {
        await page.waitForTimeout(25);
        saves = (await app.calls()).filter((call) => call.cmd === 'save_context_draft');
      }
      expect(
        saves.length,
        'Save reached no command at all, so the material a person typed never left the window.',
      ).toBeGreaterThanOrEqual(1);
      expect(
        JSON.stringify(saves[0]?.args ?? {}),
        'the save carried none of the typed material, so what lands on disk is not what a ' +
          'person wrote. The whole payload was: ' +
          JSON.stringify(saves[0]?.args ?? {}),
      ).toContain(TYPED);
      expect(
        saves[0]?.args['expectedRevision'],
        'the save carried no revision of the draft it was opened from, so Rust has nothing to ' +
          'compare and an older window still wins by writing last.',
      ).toBe(OPENED);

      /* ── wyjście i powrót ──────────────────────────────────────────────────────────────── */
      /* Edytor jest ZAMYKANY, żeby powrót szedł przez listę i przez odczyt granicy, a nie przez
         magazyn, który przeżywa odmontowanie sekcji — powód stoi w nagłówku pliku. */
      await page.locator(BACK).click();
      await page.locator(OTHER_SWITCH).click();
      await page
        .locator('main[data-section="knowledge"]')
        .waitFor({ state: 'attached', timeout: APPEARS });
      await page.locator(SWITCH).click();
      await page.locator(SCREEN).waitFor({ state: 'attached', timeout: APPEARS });

      await page
        .locator(CARD)
        .first()
        .waitFor({ state: 'visible', timeout: APPEARS })
        .catch(() => undefined);
      const onTheList = (await page.locator(CARD).allInnerTexts()).join(' ').replace(/\s+/g, ' ');
      expect(
        onTheList,
        'the set a person made is not on the list after leaving the section and coming back. ' +
          'The list said: ' +
          onTheList,
      ).toContain(TITLE);

      /* ── materiał wraca z granicy ──────────────────────────────────────────────────────── */
      await page.locator(CARD).first().click();
      await page
        .locator(MATERIAL)
        .waitFor({ state: 'visible', timeout: APPEARS })
        .catch(() => undefined);
      expect(
        await page
          .locator(MATERIAL)
          .inputValue()
          .catch(() => ''),
        'opening the set again shows an empty field where the material was. A set that forgets ' +
          'what was typed into it the moment a person looks somewhere else is not a library.',
      ).toBe(TYPED);
    } finally {
      await app.close();
    }
  }, 90_000);

  it('shows the sentence Rust writes when a save arrives with a stale revision', async () => {
    const app = await openApp({ replies: REFUSING });
    try {
      const page = app.page;

      await page.locator(SWITCH).click();
      await page.locator(SCREEN).waitFor({ state: 'attached', timeout: APPEARS });
      await page.locator(NAME).fill(TITLE);
      await page.locator(CREATE).click();
      await page
        .locator(MATERIAL)
        .waitFor({ state: 'visible', timeout: APPEARS })
        .catch(() => undefined);

      await page.locator(MATERIAL).fill(TYPED);
      await page.locator(SAVE).click();

      await page
        .locator(SAID)
        .waitFor({ state: 'visible', timeout: APPEARS })
        .catch(() => undefined);
      const shown = (await page.locator(SAID).allInnerTexts()).join(' ').replace(/\s+/g, ' ');
      expect(
        shown,
        'the refusal never reached the screen. A save turned down in silence leaves a person ' +
          "believing the file holds what the editor shows, and it holds somebody else's newer " +
          'work instead. The screen said: ' +
          shown,
      ).toContain('was not saved');
      expect(
        shown,
        'the screen never says that nothing was overwritten, so a person cannot tell a refused ' +
          'save from a save that took half. The screen said: ' +
          shown,
      ).toContain('nothing was overwritten');
    } finally {
      await app.close();
    }
  }, 90_000);
});
