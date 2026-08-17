/* AC-3 dla T-29: żaden widoczny przycisk na żadnym z pięciu ekranów nie jest martwy.
 *
 * Niezmiennik 16 mówi, że kontrolka bez handlera nie wchodzi do repo. poprzedni prototyp ma trzy martwe
 * przyciski i wszystkie trzy stoją w miejscach, do których nikt nie kliknął DRUGI raz — dlatego
 * ten plik nie wybiera przycisku reprezentatywnego. Klika KAŻDY widoczny `<button>` w `<main>`,
 * każdy na świeżo otwartej aplikacji, i pyta o jedno: czy cokolwiek się stało.
 *
 * ŚWIEŻA APLIKACJA NA PRZYCISK, a nie pięć kliknięć w tej samej karcie. Drugie kliknięcie
 * w tej samej karcie zastaje aplikację w stanie, który zostawiło pierwsze — a wtedy „nic się nie
 * zmieniło" bywa prawdą o przycisku, który już zrobił swoje, zamiast o przycisku martwym.
 * Magazyny zustanda żyją na poziomie modułu, więc nowa karta to nowy stan i nic poza tym.
 *
 * CO ZNACZY „COŚ SIĘ STAŁO". Zmienił się dokument, albo atrapa `__TAURI_INTERNALS__` dostała
 * wywołanie, albo pojawił się dialog. Przycisk, po którym nie zmienia się nic i nie leci nic,
 * jest martwy — i jest dokładnie tym, co widzi człowiek, kiedy mówi, że produkt nie działa.
 *
 * KONTROLA PRZECIW PUSTEJ ASERCJI, DWIE. Pierwsza: zanim ktokolwiek kliknie, dokument musi być
 * CICHY przez to samo okno czasu — inaczej „coś się zmieniło" byłoby prawdą także o własnym
 * ruchu aplikacji i każdy martwy przycisk przechodziłby na cudzej animacji. Druga, niżej:
 * przez pięć sekcji musi znaleźć się przynajmniej jeden widoczny przycisk, bo selektor, który
 * nie łapie niczego, zazielenia „każdy przycisk działa" na całym pustym zbiorze.
 *
 * Czego ta miara NIE łapie i wolę to napisać, niż obiecać więcej, niż mierzę: przycisk, którego
 * jedynym skutkiem jest zmiana wyglądu robiona arkuszem stylów (`:focus`, `:active`), nie rusza
 * ani dokumentu, ani atrapy. Taki przycisk zostanie tu nazwany martwym i to jest wybór — zmiana
 * widoczna wyłącznie w stanie hovera nie jest odpowiedzią na kliknięcie.
 */
import { afterAll, describe, expect, it } from 'vitest';

import type { RunningApp } from '../harness';
import { closeEverything, openApp } from '../harness';

const EXPECTED = ['run', 'workflows', 'agents', 'skills', 'memory'] as const;

type Id = (typeof EXPECTED)[number];

/** Przycisk, po którym wolno, żeby nic się nie stało — i powód, dla którego wolno. */
interface Excused {
  readonly section: Id;
  /** Napis na przycisku, znak w znak. */
  readonly name: string;
  /** Dlaczego ten jeden nie łamie niezmiennika 16. Puste zdanie jest tu zakazane. */
  readonly why: string;
}

/**
 * Lista pusta jest najlepsza z możliwych i taka ma zostać.
 *
 * Wpis wolno dopisać tylko z powodem — lista bez powodów zamienia to kryterium w miejsce,
 * w którym chowa się martwe przyciski zamiast je naprawiać. Pilnuje tego asercja niżej,
 * nie uprzejmość autora.
 */
const EXCUSED: readonly Excused[] = [];

const BUTTONS = 'main button:visible';

/** Okno, w którym skutek kliknięcia ma się pokazać. Render Reacta i mikrozadanie, nie sieć. */
const SETTLE = 300;

/** Ile czekamy na przemontowanie sekcji po kliknięciu w przełącznik. */
const MOUNTS = 4_000;

/** Dokument i taśma wywołań w jednej chwili — to, co porównujemy po obu stronach kliknięcia. */
interface Snapshot {
  readonly html: string;
  readonly calls: number;
}

async function snapshot(app: RunningApp): Promise<Snapshot> {
  return {
    html: await app.page.evaluate(() => document.body.innerHTML),
    calls: (await app.calls()).length,
  };
}

/** Otwiera aplikację i staje na tej sekcji. Zawsze świeża karta, zawsze przez przełącznik. */
async function openAt(id: Id): Promise<RunningApp> {
  const app = await openApp();
  await app.page.click('[data-section-switch="' + id + '"]');
  await app.page
    .locator('main[data-section="' + id + '"]')
    .waitFor({ state: 'attached', timeout: MOUNTS })
    .catch(() => undefined);
  return app;
}

/** Napisy na widocznych przyciskach `<main>`, w kolejności dokumentu. */
async function buttonNames(app: RunningApp): Promise<string[]> {
  const texts = await app.page.locator(BUTTONS).allInnerTexts();
  return texts.map((text) => text.trim().replace(/\s+/g, ' '));
}

function excuseFor(id: Id, name: string): Excused | undefined {
  return EXCUSED.find((entry) => entry.section === id && entry.name === name);
}

afterAll(async () => {
  await closeEverything();
}, 30_000);

describe('no button on any screen is dead', () => {
  it('finds buttons at all, so "every button works" is not a claim about an empty set', async () => {
    const app = await openApp();
    try {
      let found = 0;
      for (const id of EXPECTED) {
        await app.page.click('[data-section-switch="' + id + '"]');
        await app.page
          .locator('main[data-section="' + id + '"]')
          .waitFor({ state: 'attached', timeout: MOUNTS })
          .catch(() => undefined);
        found += await app.page.locator(BUTTONS).count();
      }
      expect(
        found,
        'across all five sections the selector ' +
          JSON.stringify(BUTTONS) +
          ' matched nothing. Either the application draws no controls at all, or this file is ' +
          'asking about elements that do not exist — and then every assertion below is a ' +
          'statement about an empty set.',
      ).toBeGreaterThan(0);
    } finally {
      await app.close();
    }
  }, 60_000);

  for (const id of EXPECTED) {
    it('every visible button on the ' + id + ' screen does something', async () => {
      for (const entry of EXCUSED) {
        expect(
          entry.why.trim().length,
          'the exception for ' +
            JSON.stringify(entry.name) +
            ' on ' +
            entry.section +
            ' carries no reason. An exception without one is where a dead button goes to hide.',
        ).toBeGreaterThan(0);
      }

      /* ── kontrola: dokument stoi w miejscu, kiedy nikt nie klika ───────────────────────── */
      let names: string[] = [];
      const quiet = await openAt(id);
      try {
        names = await buttonNames(quiet);
        const first = await snapshot(quiet);
        await quiet.page.waitForTimeout(SETTLE);
        const second = await snapshot(quiet);
        expect(
          second.html === first.html && second.calls === first.calls,
          'the ' +
            id +
            ' screen changes on its own within ' +
            String(SETTLE) +
            'ms of doing nothing. Until that is true, "the document changed after the click" ' +
            'is a statement about the application moving by itself, and every dead button on ' +
            'this screen passes on it.',
        ).toBe(true);
      } finally {
        await quiet.close();
      }

      /* ── każdy przycisk z osobna, każdy na świeżo otwartej aplikacji ───────────────────── */
      for (let index = 0; index < names.length; index += 1) {
        const name = names[index] ?? '';
        const excuse = excuseFor(id, name);
        if (excuse !== undefined) continue;

        const app = await openAt(id);
        try {
          let sawDialog = false;
          app.page.on('dialog', (dialog) => {
            sawDialog = true;
            void dialog.dismiss();
          });

          const before = await snapshot(app);
          await app.page.locator(BUTTONS).nth(index).click();
          await app.page.waitForTimeout(SETTLE);
          const after = await snapshot(app);

          const happened = after.html !== before.html || after.calls > before.calls || sawDialog;
          expect(
            happened,
            'the button ' +
              JSON.stringify(name === '' ? 'button #' + String(index) : name) +
              ' on the ' +
              id +
              ' screen is dead: after clicking it the document is unchanged, nothing went to ' +
              'Rust and no dialog opened. A control with no handler does not go into this repo ' +
              '(invariant 16) — poprzedni prototyp has three of them, and all three are in places nobody ' +
              'clicked twice. Either wire it up, or write it into EXCUSED with a reason.',
          ).toBe(true);
        } finally {
          await app.close();
        }
      }
    }, 120_000);
  }
});
