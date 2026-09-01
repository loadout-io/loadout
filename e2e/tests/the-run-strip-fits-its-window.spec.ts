/* Czy pasek ekranu Run MIEŚCI SIĘ w oknie, w którym ten produkt jest projektowany.
 *
 * PO CO PRZEGLĄDARKA, A NIE STATYCZNY RENDER. Przycięcie jest faktem o UKŁADZIE: istnieje
 * dopiero tam, gdzie coś policzyło szerokości. `renderToStaticMarkup` nie ma szerokości, więc
 * o pasku, który wyjeżdża poza kadr, nie wie nic — a to jest dokładnie ta klasa wady, którą
 * człowiek widzi pierwszą sekundą, a bramka nie widziała wcale (niezmiennik 29).
 *
 * CO BYŁO ZMIERZONE 2026-08-31, ZANIM TA WYROCZNIA POWSTAŁA. Przy oknie 1512 px rząd kontrolek
 * chciał 1638 px i dostawał 1205: suwak „ile naraz” i sufit wydatku stały poza kadrem, a jedyną
 * drogą do nich było przewinięcie, o którym nic na ekranie nie mówiło. Nazwa sekcji ustępowała
 * tej samej ciasnocie i czytała się „R..”.
 *
 * DWA PYTANIA, BO PRZYCIĘCIE MIAŁO DWIE OFIARY. Rząd kontrolek niesie CZYNNOŚCI (niezmiennik 16:
 * kontrolka, do której nie da się dosięgnąć, jest kontrolką martwą), a nagłówek niesie
 * odpowiedź na pytanie „na czym stoisz”. Jedno kryterium na obie rzeczy nie umiałoby powiedzieć,
 * która z nich wyjechała.
 */
import { afterAll, describe, expect, it } from 'vitest';

import type { TauriReply } from '../harness';
import { closeEverything, openApp } from '../harness';

/** Okno z makiety. Przycięcie jest funkcją miejsca, więc miejsce musi być znane. */
const WIDE = { width: 1512, height: 950 };

const FOLDER = '/Users/somebody/Projects/client-website-redesign';
const WORKSPACE = { id: FOLDER, name: 'Client website redesign', folder: FOLDER };

/** Prowadzący z nazwą tej długości, co nazwy, które ludzie naprawdę piszą. */
const LEAD = {
  schema: 1,
  id: 'agent-builder',
  name: 'Builder',
  summary: 'Writes the change',
  color: 'green',
  instructions: 'Make the smallest change that makes the failing behaviour pass.',
  runsWith: 'claude',
  model: 'sonnet',
  thinking: 'balanced',
  fileAccess: 'edit-in-folder',
  giveUpAfterMinutes: 45,
  tools: 'all',
  reachesTheWeb: false,
  skills: [],
  connections: [],
  writeResultsTo: 'handoffs/patch-summary.md',
};

const WORKFLOWS = [
  {
    path: 'ship-a-feature.json',
    place: 'project',
    workflow: {
      format: 1,
      id: 'wf-ship',
      name: 'Ship a feature',
      steps: [],
      links: [],
    },
  },
];

function copies<T>(value: T, count = 20): readonly { readonly value: T }[] {
  return Array.from({ length: count }, () => ({ value }));
}

const SCENE: Readonly<Record<string, readonly TauriReply[]>> = {
  list_workspaces: copies([WORKSPACE]),
  list_agents: copies([LEAD]),
  list_workflows: copies(WORKFLOWS),
  list_skills: copies([]),
  list_notes: copies([]),
  list_runs: copies([]),
  read_settings: copies({ defaultLead: LEAD.id, defaultBudgetUsd: 75 }),
};

/** Ile czekamy, aż pasek dostanie to, co przeczytał z dysku. */
const APPEARS = 8_000;

afterAll(async () => {
  await closeEverything();
});

describe('the strip on the work screen at the window this product is drawn for', () => {
  it('keeps every control it carries inside the window', async () => {
    const app = await openApp({ replies: SCENE });
    try {
      await app.page.setViewportSize(WIDE);
      const row = app.page.locator('[data-workflow-controls]');
      await row.waitFor({ state: 'visible', timeout: APPEARS });
      /* Czekamy na wypełniony wybór prowadzącego, nie na sam rząd: pełna szerokość rzędu
       * istnieje dopiero wtedy, gdy stoją w nim wszystkie kontrolki, które ma nieść. */
      await app.page
        .locator('[data-workflow-controls] select option')
        .first()
        .waitFor({ state: 'attached', timeout: APPEARS });

      const measured = await row.evaluate((element) => ({
        wants: element.scrollWidth,
        given: element.clientWidth,
      }));

      expect(
        measured.wants,
        'the row of controls on the work screen wants ' +
          String(measured.wants) +
          ' pixels and the window gives it ' +
          String(measured.given) +
          '. Everything past that edge — how many agents at once, and the amount this run is ' +
          'allowed to spend — is off screen, and the only way to it is a sideways scroll that ' +
          'nothing on the screen mentions. A control a person cannot reach is a dead control',
      ).toBeLessThanOrEqual(measured.given + 1);
    } finally {
      await app.close();
    }
  }, 120_000);

  /* TRZECIE PYTANIE, DOPISANE 2026-09-01, I JEST TO NAPRAWA TEGO PLIKU, NIE ROZSZERZENIE JEGO
   * ZAKRESU. Punkt wyzej pyta o `scrollWidth` CALEGO RZEDU — a rzad, ktorego napisy zostaly
   * skrocone do zera, jest rowny sobie i przechodzi. Zmierzone w chromium 1512x950 z rozwinieta
   * nawigacja: rzad chcial 1562 px, dostawal 1108, a te 454 px roznicy pokrylo w calosci
   * skrocenie DWOCH napisow jednej kontrolki — zdania przy „Learn from this run" (0 px z 400)
   * i reszty jej wlasnej nazwy (57 ze 112). Kryterium bylo nad tym zielone, bo pytalo o pudelko,
   * a nie o to, co w nim widac.
   *
   * ROZNICA JEST TA SAMA, CO MIEDZY „miesci sie" A „da sie przeczytac". Napis ucisniety do zera
   * nie jest metadana, ktora ustapila — jest zdaniem, ktorego produkt nie mowi, a mowi je
   * markup. Dlatego to pytanie idzie po LISCIACH: kazdy element rzedu, ktory sam niesie tekst,
   * ma go pokazac w calosci.
   */
  it('shows every word it carries, instead of fitting by squeezing them to nothing', async () => {
    const app = await openApp({ replies: SCENE });
    try {
      await app.page.setViewportSize(WIDE);
      const row = app.page.locator('[data-workflow-controls]');
      await row.waitFor({ state: 'visible', timeout: APPEARS });
      await app.page
        .locator('[data-workflow-controls] select option')
        .first()
        .waitFor({ state: 'attached', timeout: APPEARS });

      const cut = await row.evaluate((element) => {
        const out: { said: string; wants: number; given: number }[] = [];
        for (const one of Array.from(element.querySelectorAll<HTMLElement>('*'))) {
          /* LISCIE, i nie jest to uproszczenie: rodzic niosacy dwa napisy ma szerokosc obu, wiec
           * o skroceniu jednego z nich nie wie nic. `option` nie jest malowany w rzedzie —
           * chromium daje mu zero szerokosci takze wtedy, gdy wybor jest szeroki. */
          if (one.children.length > 0) continue;
          if (one.tagName.toLowerCase() === 'option') continue;
          const said = (one.textContent ?? '').trim();
          if (said === '') continue;
          if (one.scrollWidth > one.clientWidth + 1 || one.clientWidth === 0) {
            out.push({ said, wants: one.scrollWidth, given: one.clientWidth });
          }
        }
        return out;
      });

      expect(
        cut,
        'the row of controls fits its window only because it ate its own words: ' +
          cut
            .map(
              (one) =>
                JSON.stringify(one.said) +
                ' wants ' +
                String(one.wants) +
                ' pixels and was given ' +
                String(one.given),
            )
            .join('; ') +
          '. A person reads a beginning and a pair of dots, or nothing at all, and the screen ' +
          'says nothing about it. Whatever cannot be read here does not belong on a strip 52 ' +
          'pixels tall',
      ).toEqual([]);
    } finally {
      await app.close();
    }
  }, 120_000);

  it('says the whole name of the screen a person is standing on', async () => {
    const app = await openApp({ replies: SCENE });
    try {
      await app.page.setViewportSize(WIDE);
      /* CELUJEMY W RZECZ, NIE W ZNACZNIK — poprawka z 2026-08-31, ostrzejsza niz to, co bylo.
       * `[data-strip] h1` trafialo w nazwe sekcji tylko tak dlugo, jak dlugo byla ona jedynym
       * naglowkiem pierwszego poziomu w pasku. Nazwa sekcji zeszla do `h2`, bo najwiekszym
       * napisem tego ekranu jest nazwa BIEGU i to ona ma byc `h1` — pilnuje tego
       * `src/sections/run/strip/the-eye-and-the-outline-agree.test.tsx`. Wyrazenie po numerze
       * znacznika trafialoby dzis albo w nic, albo w nazwe biegu, czyli mierzyloby inny napis
       * niz ten, o ktory to kryterium pyta. `data-section-name` nazywa dokladnie ten jeden. */
      const title = app.page.locator('[data-strip] [data-section-name]');
      await title.waitFor({ state: 'visible', timeout: APPEARS });
      await app.page
        .locator('[data-workflow-controls] select option')
        .first()
        .waitFor({ state: 'attached', timeout: APPEARS });

      /* Dwa pomiary, bo „ucięta” ma dwa znaczenia i tylko oba razem mówią, co widzi człowiek.
       * PRZYCIĘTA PRZEZ SIEBIE: pudełko chowa własny tekst i dokłada trzy kropki. NAMALOWANA
       * NA SĄSIEDZIE: tekst wychodzi poza pudełko i wchodzi w kontrolki. Nazwa sekcji ma nie
       * robić ani jednego, ani drugiego — wolno jej zająć pustkę między sobą a rzędem. */
      const seen = await title.evaluate((element) => {
        const range = document.createRange();
        range.selectNodeContents(element);
        const controls = document.querySelector('[data-workflow-controls]');
        return {
          hides: getComputedStyle(element).overflowX !== 'visible',
          wants: element.scrollWidth,
          given: element.clientWidth,
          ends: range.getBoundingClientRect().right,
          neighbour:
            controls === null ? Number.POSITIVE_INFINITY : controls.getBoundingClientRect().left,
          said: element.textContent ?? '',
        };
      });

      expect(
        seen.hides && seen.wants > seen.given,
        'the name of this screen is cut by its own box: "' +
          seen.said +
          '" wants ' +
          String(seen.wants) +
          ' pixels, was given ' +
          String(seen.given) +
          ', and the box hides what it cannot hold — so a person reads two letters and a pair ' +
          'of dots. The controls beside it take their full width first and hand back nothing. ' +
          'The name answers what a person is standing on, and it gives way to nothing',
      ).toBe(false);

      expect(
        seen.ends,
        'the name of this screen is painted over the controls: it ends at ' +
          String(Math.round(seen.ends)) +
          ' and the row of controls starts at ' +
          String(Math.round(seen.neighbour)) +
          '. Two things a person is meant to read separately are drawn on top of each other',
      ).toBeLessThanOrEqual(seen.neighbour);
    } finally {
      await app.close();
    }
  }, 120_000);
});
