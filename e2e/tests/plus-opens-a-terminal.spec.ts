/* Plusik otwiera terminal w projekcie, który już wybrano — i widzi to człowiek.
 *
 * ZGŁOSZENIE (właściciel, 2026-08-20): „jak klikam plusik to powinno po prostu odpalać nowy nasz
 * terminal i sobie tam możemy kolejne workflow w naszym scope co mamy zaznaczone, a nie tak jak
 * teraz że scope wybieramy znowu".
 *
 * STAN JEST GORSZY, NIŻ BRZMI ZGŁOSZENIE, i to zmierzone z kodu: `＋` woła `openFolder`, czyli
 * systemowe okno wyboru katalogu, a wybór kończy się dołożeniem nowego ZAKRESU — który od razu
 * staje się aktywny. Pasek pokazuje karty aktywnego zakresu, a w świeżym nie ma żadnej. Kliknięcie
 * w `＋` nie dokłada więc karty NIGDY: wymienia projekt i pasek robi się pusty.
 *
 * DLACZEGO TO KRYTERIUM STOI TUTAJ, A NIE OBOK MAGAZYNU (niezmiennik 29). Skutek `＋` musi być
 * widoczny tam, gdzie patrzy człowiek. Kryterium pytające magazyn kart o jego zawartość byłoby
 * zielone także wtedy, gdyby przycisk nie miał handlera, gdyby handler wisiał na propsie, którego
 * nikt nie podaje, albo gdyby karta powstawała poza filtrem paska — a to są cztery różne wady
 * z jednej rodziny, wszystkie znalezione na ZIELONEJ bramce w fali z 2026-08-20.
 *
 * SŁABA WERSJA: sam „kart jest o jedną więcej". Przechodzi dla implementacji, która dokłada kartę
 * i PRZY OKAZJI nadal otwiera okno wyboru folderu — czyli zostawia dokładnie tę wadę, którą
 * zgłosił właściciel. Rozstrzygają dwie rzeczy naraz: taśma wywołań (żadnego pytania o folder,
 * kiedy projekt jest wybrany) i przypadek odwrotny (pytanie o folder, kiedy projektu nie ma —
 * bo terminal bez miejsca pracy nie ma gdzie stanąć).
 *
 * DLACZEGO ZAKRES JEST ZASIANY, A NIE WYKLIKANY. Atrapa granicy odpowiada KSZTAŁTEM, nie stanem
 * (`../harness.ts`): `list_workspaces` oddaje pustą listę, a `save_workspace` oddaje `null`, więc
 * przez okno wyboru folderu nie da się w tym harnessie dojść do zakresu — ani razu. Zasiewamy
 * więc stan wyjściowy przez ten sam magazyn, z którego czyta ekran, i sprawdzamy zaraz potem, że
 * ekran naprawdę go zobaczył (zaproszenie „wybierz folder" znika). Klik, przycisk, pasek i taśma
 * są dalej prawdziwe — zasiany jest wyłącznie warunek początkowy.
 */
import { afterAll, beforeAll, describe, expect, it } from 'vitest';

import type { RunningApp } from '../harness';
import { closeEverything, openApp } from '../harness';

/** Ekran pracy jest pierwszą sekcją okna (`src/ui/shell/section-store.ts`), więc nikt nie klika. */
const WORK = 'main[data-section="run"]';

/** Pole wiersza wejścia. Ta sama etykieta, po której idzie czytnik ekranu. */
const FIELD = '[aria-label="Command line"]';

/** Karta na pasku; `data-tab` niesie jej tożsamość (`src/sections/run/tabs/tab.tsx`). */
const CARD = '[data-tab]';

/** Karta na wierzchu — to samo pole, z którego bierze się podświetlenie. */
const ON_TOP = '[data-tab][aria-current="true"]';

/** Znak `＋` na końcu paska kart. */
const PLUS = '[data-add-tab]';

/** Zaproszenie „wybierz folder", które ekran rysuje TYLKO wtedy, gdy nie ma ani jednego zakresu. */
const INVITE = '[data-add-workspace]';

/** Moduł magazynu zakresów, tak jak serwuje go vite. Ścieżka jest daną, nie importem. */
const WORKSPACES = '/src/state/workspaces.ts';

/**
 * Nazwa, pod którą zasiew zostawia ten magazyn w oknie.
 *
 * Napisana RAZ i przekazywana obu stronom argumentem — z tego samego powodu, co `TAPE`
 * w `../harness.ts`: funkcje biegnące w przeglądarce nie widzą zasięgu tego modułu, więc literał
 * przepisany w dwóch miejscach rozjechałby się po cichu i dałby zdanie o produkcie zamiast
 * o przyrządzie.
 */
const HANDLE = '__LOADOUT_E2E_PROJECTS__';

/** Folder zasianego projektu. Nie jest otwierany ani czytany — jedzie tylko jako napis. */
const FOLDER = '/Users/you/Projects/ledger-ui';

/** Ile czekamy, aż React dorysuje skutek kliknięcia. Render, nie sieć. */
const SETTLE = 400;

/** Ile czekamy na pierwsze pojawienie się elementu, który ma przyjść po zdarzeniu. */
const APPEARS = 4_000;

/** Co trzyma teraz kursor — w postaci, którą da się wpisać w komunikat porażki. */
interface Focused {
  readonly tag: string;
  readonly label: string | null;
}

function focused(app: RunningApp): Promise<Focused> {
  return app.page.evaluate(() => {
    const on = document.activeElement;
    if (on === null) return { tag: 'nothing', label: null };
    return { tag: on.tagName.toLowerCase(), label: on.getAttribute('aria-label') };
  });
}

/** Otwiera aplikację i czeka na ekran pracy. Ani jednego kliknięcia — praca jest pierwsza. */
async function openWork(): Promise<RunningApp> {
  const app = await openApp();
  await app.page
    .locator(WORK)
    .waitFor({ state: 'attached', timeout: APPEARS })
    .catch(() => undefined);
  await app.page
    .locator(FIELD)
    .waitFor({ state: 'attached', timeout: APPEARS })
    .catch(() => undefined);
  return app;
}

/**
 * Ile razy okno poprosiło system o wybór folderu.
 *
 * Wtyczka dialogu jedzie tą samą drogą, co każda komenda — przez `__TAURI_INTERNALS__.invoke` —
 * więc taśma harnessu widzi ją tak samo dobrze jak `run_workflow`. Dopasowanie po `dialog`, a nie
 * po pełnej nazwie: numer wersji wtyczki w nazwie komendy nie jest faktem o naszym produkcie.
 */
async function folderQuestions(app: RunningApp): Promise<number> {
  const calls = await app.calls();
  return calls.filter((call) => call.cmd.toLowerCase().includes('dialog')).length;
}

/** Tożsamości kart stojących teraz na pasku, w kolejności. */
function cardIds(app: RunningApp): Promise<string[]> {
  return app.page
    .locator(CARD)
    .evaluateAll((nodes) => nodes.map((node) => node.getAttribute('data-tab') ?? ''));
}

/** Tożsamość karty na wierzchu — pusta lista znaczy, że żadna nią nie jest. */
function cardsOnTop(app: RunningApp): Promise<string[]> {
  return app.page
    .locator(ON_TOP)
    .evaluateAll((nodes) => nodes.map((node) => node.getAttribute('data-tab') ?? ''));
}

/**
 * Zasiewa wybrany projekt w tym samym magazynie, z którego czyta ekran.
 *
 * SKRYPT MODUŁOWY, A NIE `import()` W `page.evaluate`, i to jest pomiar, nie preferencja: vitest
 * przepisuje dynamiczny import w tym pliku na własny pomocnik (`__vite_ssr_dynamic_import__`),
 * a ten nie istnieje w przeglądarce — kryterium padało wtedy na przyrządzie, nie na produkcie.
 * Treść `addScriptTag` idzie do strony nietknięta, a przeglądarka rozwiązuje ścieżkę wobec adresu
 * strony, czyli trafia w ten sam moduł, z którego rysuje się okno.
 *
 * Odmowa jest tu GŁOŚNA, a nie cicha: zasiew, który nie doszedł, zamieniłby „przycisk nie dokłada
 * karty" w zdanie o tym harnessie, a nie o produkcie (niezmiennik 20).
 */
async function projectAlreadyChosen(app: RunningApp, folder: string): Promise<void> {
  await app.page
    .addScriptTag({
      type: 'module',
      content:
        "import { useWorkspaces } from '" +
        WORKSPACES +
        "';\nglobalThis[" +
        JSON.stringify(HANDLE) +
        '] = useWorkspaces;\n',
    })
    .catch((cause: unknown) => {
      throw new Error(
        'the module of saved projects never loaded from ' + WORKSPACES + ': ' + String(cause),
      );
    });

  const said = await app.page.evaluate(
    (seed: { readonly handle: string; readonly folder: string }) => {
      const host = globalThis as unknown as Record<string, unknown>;
      const store = host[seed.handle] as { setState?: (next: unknown) => void } | undefined;
      if (store?.setState === undefined) return 'the store of saved projects never reached the page';
      store.setState({
        all: [{ id: seed.folder, name: 'ledger-ui', folder: seed.folder }],
        activeId: seed.folder,
        said: null,
      });
      return null;
    },
    { handle: HANDLE, folder },
  );
  if (said !== null) {
    throw new Error(
      'the starting state could not be seeded: ' +
        said +
        '. Nothing below would be a statement about the product.',
    );
  }
  await app.page.waitForTimeout(SETTLE);
}

/* Rozruch vite i chromium jest kosztem STAŁYM NA PLIK, nie częścią pierwszego przypadku:
 * `openApp()` jest leniwy, więc bez tego haka pierwszy `it` płaci cały rozruch pod swoim
 * limitem. Ta sama para haków stoi w `terminal-behaves.spec.ts` i z tego samego powodu. */
beforeAll(async () => {
  await openApp();
}, 180_000);

afterAll(async () => {
  await closeEverything();
}, 30_000);

describe('the plus sign opens a terminal where the person already works', () => {
  it('asks for a folder only when there is no project, and adds a card when there is', async () => {
    const app = await openWork();
    try {
      expect(
        await app.page.locator(PLUS).count(),
        'the tab bar has to draw the + at all, or every question below is about a control that ' +
          'is not there',
      ).toBe(1);

      // ── KONTROLA: BEZ PROJEKTU `＋` DALEJ PYTA O FOLDER ─────────────────────────────────
      //
      // Terminal bez miejsca pracy nie ma gdzie stanąć, więc pytanie o folder jest tu jedyną
      // uczciwą odpowiedzią. Bez tego przypadku „nie pyta o folder" przechodziłoby także dla
      // przycisku, który nie robi zupełnie nic.
      expect(
        await app.page.locator(INVITE).count(),
        'this half of the case needs an application with NO project chosen, and the screen is ' +
          'not offering to choose one — so the fixture is not what it says it is',
      ).toBe(1);

      const askedBefore = await folderQuestions(app);
      await app.page.click(PLUS);
      await app.page.waitForTimeout(SETTLE);
      expect(
        await folderQuestions(app),
        'with no project chosen, + has to ask which folder to work in. A + that quietly opens ' +
          'a terminal with nowhere to stand leaves the person with a card whose work has no home.',
      ).toBe(askedBefore + 1);

      // ── PROJEKT JUŻ WYBRANY ────────────────────────────────────────────────────────────
      await projectAlreadyChosen(app, FOLDER);
      expect(
        await app.page.locator(INVITE).count(),
        'the screen still offers to choose a project, so the seeded one never reached it and ' +
          'the rest of this case would be measuring the same empty state twice',
      ).toBe(0);

      const stood = await cardIds(app);
      const asked = await folderQuestions(app);
      await app.page.click(PLUS);
      await app.page
        .locator(CARD)
        .nth(stood.length)
        .waitFor({ state: 'attached', timeout: APPEARS })
        .catch(() => undefined);

      // ── (a) ŻADNEGO PYTANIA O FOLDER ───────────────────────────────────────────────────
      expect(
        await folderQuestions(app),
        'the project was already chosen and + asked for a folder anyway. This is the report ' +
          'word for word: "a nie tak jak teraz ze scope wybieramy znowu". Choosing where to ' +
          'work is a decision made once, in the side menu, not a chore repeated before every ' +
          'piece of work.',
      ).toBe(asked);

      // ── (b) O JEDNĄ KARTĘ WIĘCEJ, I WIDAĆ JĄ NA PASKU ──────────────────────────────────
      const now = await cardIds(app);
      expect(
        now.length,
        'pressing + left the bar exactly as it was. Today the click swaps the project instead ' +
          'of adding anything, so the bar goes EMPTY — a control that takes an instruction and ' +
          'carries out another one is worse than no control at all (invariant 16). The bar ' +
          'holds: ' +
          JSON.stringify(now),
      ).toBe(stood.length + 1);

      // ── (c) NOWA KARTA NA WIERZCHU, KURSOR DALEJ W POLU ────────────────────────────────
      const fresh = now.filter((card) => !stood.includes(card));
      expect(
        await cardsOnTop(app),
        'the terminal that was just opened is not the one on top. A person who asks for a new ' +
          'place to work is looking at it, not at the one that was there before; a bar where no ' +
          'card is open while several stand is a screen nobody can read.',
      ).toEqual(fresh);

      const on = await focused(app);
      expect(
        on.label,
        'after opening a terminal the caret sits on ' +
          JSON.stringify(on) +
          ' instead of the command line. Opening a terminal is asking to type in it, and a ' +
          'browser leaves the caret on whatever button was pressed — so this costs one more ' +
          'click before every first line, which is the defect the owner reported on 2026-08-20.',
      ).toBe('Command line');
    } finally {
      await app.close();
    }
  }, 90_000);
});
