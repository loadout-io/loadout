/* Odpowiedź Stopu staje w KARCIE, z której go naciśnięto — i w żadnej innej.
 *
 * # Defekt, który to kryterium zamyka (2026-09, Z-35, runda naprawcza)
 *
 * Zdanie „Nothing is running in <folder>." powstało razem z adresowaniem Stopu folderem
 * i było napisane poprawnie — a nie dochodziło na ekran. `showInStream` w
 * `src/sections/run/index.tsx` dopisywało wiersz do `feedFor(folder)`, czyli do sesji ZAKRESU,
 * podczas gdy widok czyta sesję KARTY na wierzchu (`src/sections/run/feed/live.ts`, `shown()`).
 * Przy karcie biegu te dwa klucze są równe (`id === folder`), więc wady nie było widać; przy
 * karcie założonej `＋` terminal ma własną tożsamość i zdanie lądowało w strumieniu, na który
 * nikt nie patrzy. Z ekranu wygląda to dokładnie jak przycisk bez handlera — czyli ta klasa
 * wady, dla której to repo powstało (niezmiennik 29).
 *
 * # Dlaczego to kryterium stoi TUTAJ, a nie obok `whatStopSaid`
 *
 * Bo czysty moduł dowodzi TREŚCI zdania i nic ponadto: `whatStopSaid(false, 'invoices-ui')`
 * było zielone przez cały czas, w którym produkt milczał. Niezmiennik 29 daje trzy drogi
 * i wymaga jednej z nich; ta jest trzecia — prawdziwa przeglądarka, prawdziwy klawisz,
 * prawdziwe kliknięcie, prawdziwy magazyn (`../harness.ts`). Test czytający wartość funkcji
 * nie odróżnia zdania, które doszło, od zdania, które poszło do niewidocznej sesji.
 *
 * # Dlaczego DWIE karty, a nie jedna
 *
 * Bo „zdanie jest na ekranie" przechodzi także dla implementacji piszącej do WSZYSTKICH sesji
 * naraz — a to jest ta sama rodzina wady, co Stop kończący bieg w każdym folderze. Rozstrzyga
 * przełączenie: zdanie ma być w karcie, która pytała, i ma NIE BYĆ w sąsiedniej.
 *
 * # Dwie drogi, bo są dwie kontrolki
 *
 * `/stop` w wierszu wejścia (`Entry`) i przycisk Stop w pasku (`Start`). Obie kończą się w tym
 * samym `showInStream`, ale wchodzą w niego z dwóch różnych propsów (`onShowInStream` i
 * `onSaid`), więc jedna zielona nie mówi nic o drugiej.
 */
import { afterAll, beforeAll, describe, expect, it } from 'vitest';

import type { RunningApp, TauriReply } from '../harness';
import { closeEverything, openApp } from '../harness';

/** Ekran pracy jest pierwszą sekcją okna (`src/ui/shell/section-store.ts`), więc nikt nie klika. */
const WORK = 'main[data-section="run"]';

/** Pole wiersza wejścia. Ta sama etykieta, po której idzie czytnik ekranu. */
const FIELD = '[aria-label="Command line"]';

/** Karta na pasku; `data-tab` niesie jej tożsamość (`src/sections/run/tabs/tab.tsx`). */
const CARD = '[data-tab]';

/** Znak `＋` na końcu paska kart. */
const PLUS = '[data-add-tab]';

/** Kolumna strumienia — to, co człowiek czyta na tym ekranie (`src/sections/run/index.tsx`). */
const STREAM = '[data-stream-column]';

/** Moduł magazynu zakresów, tak jak serwuje go vite. Ścieżka jest daną, nie importem. */
const WORKSPACES = '/src/state/workspaces.ts';

/** Moduł sesji biegu — stąd bierze się „coś tu idzie", od którego zależy przycisk Stop. */
const RUNS = '/src/state/run.ts';

/**
 * Nazwy, pod którymi zasiew zostawia te dwa magazyny w oknie.
 *
 * Napisane RAZ i przekazywane obu stronom argumentem — z tego samego powodu, co `TAPE`
 * w `../harness.ts`: funkcje biegnące w przeglądarce nie widzą zasięgu tego modułu, więc literał
 * przepisany w dwóch miejscach rozjechałby się po cichu i dałby zdanie o przyrządzie zamiast
 * o produkcie.
 */
const PROJECTS_HANDLE = '__LOADOUT_E2E_PROJECTS__';
const RUNS_HANDLE = '__LOADOUT_E2E_RUNS__';

/** Folder zasianego projektu. Nie jest otwierany ani czytany — jedzie tylko jako napis. */
const FOLDER = '/Users/you/Projects/invoices-ui';

/** Nazwa tego projektu, czyli to, co człowiek widzi na pasku i w menu. */
const NAME = 'invoices-ui';

/** Zdanie, o które chodzi — składa je `whatStopSaid` z nazwy folderu tej karty. */
const ANSWER = 'Nothing is running in ' + NAME + '.';

/**
 * Odpowiedź granicy na Stop: „nie było czego zatrzymać".
 *
 * JAWNA, choć atrapa i tak oddaje `null` dla wszystkiego poza listami: `false` jest tu TREŚCIĄ
 * sceny, nie szczegółem. Kolejka ma dwa wpisy, bo ten sam ekran naciska Stop więcej niż raz.
 */
const NOTHING_STOPPED: readonly TauriReply[] = [{ value: false }, { value: false }];

/** Ile czekamy, aż React dorysuje skutek zdarzenia. Render, nie sieć. */
const SETTLE = 500;

/** Ile czekamy na pierwsze pojawienie się elementu, który ma przyjść po zdarzeniu. */
const APPEARS = 4_000;

/** Otwiera aplikację i czeka na ekran pracy. Ani jednego kliknięcia — praca jest pierwsza. */
async function openWork(): Promise<RunningApp> {
  const app = await openApp({ replies: { stop_run: NOTHING_STOPPED } });
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
 * Wystawia moduł okna pod nazwanym uchwytem w `globalThis`.
 *
 * SKRYPT MODUŁOWY, A NIE `import()` W `page.evaluate`, i to jest pomiar, nie preferencja: vitest
 * przepisuje dynamiczny import w pliku specyfikacji na własny pomocnik
 * (`__vite_ssr_dynamic_import__`), a ten nie istnieje w przeglądarce — kryterium padałoby wtedy
 * na przyrządzie, nie na produkcie. Ten sam zabieg i ten sam powód stoją w
 * `plus-opens-a-terminal.spec.ts`.
 */
async function exposeModule(app: RunningApp, path: string, name: string, handle: string) {
  await app.page
    .addScriptTag({
      type: 'module',
      content:
        'import { ' +
        name +
        " } from '" +
        path +
        "';\nglobalThis[" +
        JSON.stringify(handle) +
        '] = ' +
        name +
        ';\n',
    })
    .catch((cause: unknown) => {
      throw new Error('the module ' + path + ' never loaded into the page: ' + String(cause));
    });
}

/**
 * Zasiewa wybrany projekt w tym samym magazynie, z którego czyta ekran.
 *
 * Atrapa granicy odpowiada KSZTAŁTEM, nie stanem (`../harness.ts`): `list_workspaces` oddaje
 * pustą listę, więc przez okno wyboru folderu nie da się w tym harnessie dojść do zakresu ani
 * razu. Zasiany jest wyłącznie warunek początkowy; klik, przycisk, pasek i strumień są dalej
 * prawdziwe. Odmowa jest GŁOŚNA: zasiew, który nie doszedł, zamieniłby „zdania nie ma" w zdanie
 * o tym harnessie, a nie o produkcie (niezmiennik 20).
 */
async function projectAlreadyChosen(app: RunningApp): Promise<void> {
  await exposeModule(app, WORKSPACES, 'useWorkspaces', PROJECTS_HANDLE);
  const said = await app.page.evaluate(
    (seed: { readonly handle: string; readonly folder: string; readonly name: string }) => {
      const host = globalThis as unknown as Record<string, unknown>;
      const store = host[seed.handle] as { setState?: (next: unknown) => void } | undefined;
      if (store?.setState === undefined)
        return 'the store of saved projects never reached the page';
      store.setState({
        all: [{ id: seed.folder, name: seed.name, folder: seed.folder }],
        activeId: seed.folder,
        said: null,
      });
      return null;
    },
    { handle: PROJECTS_HANDLE, folder: FOLDER, name: NAME },
  );
  if (said !== null) throw new Error('the starting state could not be seeded: ' + said);
  await app.page.waitForTimeout(SETTLE);
}

/**
 * Zasiewa BIEG w sesji tego folderu — bez tego przycisk Stop w ogóle się nie renderuje.
 *
 * Kontrolka startu pyta o jeden fakt: `RunState.workflow !== ''` (`src/sections/run/start.tsx`,
 * `busy`). Zasiew idzie przez `nowRunning`, czyli przez tę samą akcję, którą woła krawędź startu
 * (`src/sections/run/io.ts`), więc pole ustawia się dokładnie tak, jak w produkcie.
 */
async function aRunIsGoingInTheFolder(app: RunningApp): Promise<void> {
  await exposeModule(app, RUNS, 'runFor', RUNS_HANDLE);
  const said = await app.page.evaluate(
    (seed: { readonly handle: string; readonly folder: string }) => {
      const host = globalThis as unknown as Record<string, unknown>;
      const runFor = host[seed.handle] as
        | ((key: string) => { getState: () => { nowRunning: (...args: unknown[]) => void } })
        | undefined;
      if (runFor === undefined) return 'the run session module never reached the page';
      runFor(seed.folder).getState().nowRunning('Ship it', [], seed.folder);
      return null;
    },
    { handle: RUNS_HANDLE, folder: FOLDER },
  );
  if (said !== null) throw new Error('the run could not be seeded: ' + said);
  await app.page.waitForTimeout(SETTLE);
}

/** Tożsamości kart stojących teraz na pasku, w kolejności. */
function cardIds(app: RunningApp): Promise<string[]> {
  return app.page
    .locator(CARD)
    .evaluateAll((nodes) => nodes.map((node) => node.getAttribute('data-tab') ?? ''));
}

/** Zakłada kartę terminalu `＋` i oddaje jej tożsamość. */
async function anotherTerminal(app: RunningApp): Promise<string> {
  const stood = await cardIds(app);
  await app.page.click(PLUS);
  await app.page
    .locator(CARD)
    .nth(stood.length)
    .waitFor({ state: 'attached', timeout: APPEARS })
    .catch(() => undefined);
  await app.page.waitForTimeout(SETTLE);
  const now = await cardIds(app);
  const fresh = now.filter((card) => !stood.includes(card));
  if (fresh.length !== 1) {
    throw new Error(
      'pressing + did not add exactly one terminal, so the two cards below would not be two ' +
        'cards: the bar held ' +
        JSON.stringify(stood) +
        ' and now holds ' +
        JSON.stringify(now),
    );
  }
  return fresh[0] ?? '';
}

/** Przełącza się na tę kartę — kliknięciem w nią, tak jak robi to człowiek. */
async function lookAt(app: RunningApp, card: string): Promise<void> {
  await app.page.click('[data-tab="' + card + '"]');
  await app.page.waitForTimeout(SETTLE);
}

/** Co WIDAĆ w kolumnie strumienia tej karty. Tekst widoczny, nie zawartość DOM-u. */
async function whatThisCardShows(app: RunningApp): Promise<string> {
  const column = app.page.locator(STREAM);
  if ((await column.count()) === 0) return '';
  return (await column.innerText()).replace(/\s+/g, ' ').trim();
}

/** Wpisuje linię i naciska Enter — dokładnie to, co robi człowiek. */
async function send(app: RunningApp, line: string): Promise<void> {
  await app.page.fill(FIELD, line);
  await app.page.press(FIELD, 'Enter');
  await app.page.waitForTimeout(SETTLE);
}

/* Rozruch vite i chromium jest kosztem STAŁYM NA PLIK, nie częścią pierwszego przypadku:
 * `openApp()` jest leniwy, więc bez tego haka pierwszy `it` płaci cały rozruch pod swoim
 * limitem. Ta sama para haków stoi w `plus-opens-a-terminal.spec.ts` i z tego samego powodu. */
beforeAll(async () => {
  await openApp();
}, 180_000);

afterAll(async () => {
  await closeEverything();
}, 30_000);

describe('what Stop answers stands in the card it was pressed in', () => {
  it('/stop answers into the terminal it was typed in, and not into the one beside it', async () => {
    const app = await openWork();
    try {
      await projectAlreadyChosen(app);
      const quiet = await anotherTerminal(app);
      const asking = await anotherTerminal(app);

      expect(
        asking,
        'the two terminals share one identity, so "this card" and "the other card" would be the ' +
          'same session and nothing below could tell them apart',
      ).not.toBe(quiet);

      await send(app, '/stop');

      expect(
        await whatThisCardShows(app),
        'a person typed /stop in this terminal and the screen said nothing. The sentence exists ' +
          'and is correct — it went to the session keyed by the FOLDER, while the screen reads ' +
          'the session of the card on top. A control whose answer lands where nobody looks is ' +
          'indistinguishable from a control with no handler (invariant 29).',
      ).toContain(ANSWER);

      await lookAt(app, quiet);
      expect(
        await whatThisCardShows(app),
        'the answer to /stop turned up in a terminal that never asked. Two cards that share one ' +
          'history are two cards a person stops believing in: they type in one and read it in ' +
          'the other.',
      ).not.toContain(ANSWER);

      await lookAt(app, asking);
      expect(
        await whatThisCardShows(app),
        'the answer disappeared from the card that asked once the person looked away and came ' +
          'back. It belongs to that terminal history, not to whatever was on screen at the time',
      ).toContain(ANSWER);
    } finally {
      await app.close();
    }
  }, 90_000);

  it('the Stop button answers into the card on top, not into the folder session', async () => {
    const app = await openWork();
    try {
      await projectAlreadyChosen(app);
      /* BIEG W FOLDERZE, KARTA TERMINALU NA WIERZCHU — i to jest cała scena tej wady: dopóki
       * karta nazywa się folderem, oba klucze są równe i nie widać niczego. */
      await aRunIsGoingInTheFolder(app);
      const asking = await anotherTerminal(app);

      const stop = app.page.getByRole('button', { name: 'Stop', exact: true });
      expect(
        await stop.count(),
        'the strip has to draw exactly one Stop while a run is going, or the click below is ' +
          'about a control that is not there — or about the wrong one',
      ).toBe(1);

      await stop.click();
      await app.page.waitForTimeout(SETTLE);

      expect(
        await whatThisCardShows(app),
        'the Stop button came back with "there was nothing to stop" and the screen stayed ' +
          'silent. Until 2026-09 this control swallowed the answer entirely; then the answer ' +
          'existed and went to the folder session, which the card on top does not read. Both ' +
          'versions look the same to the person: a button that does nothing (invariant 16).',
      ).toContain(ANSWER);

      expect(
        asking,
        'the card that was on top when Stop was pressed has to be a terminal of its own, not ' +
          'the folder — otherwise both keys agree and this case proves nothing',
      ).not.toBe(FOLDER);
    } finally {
      await app.close();
    }
  }, 90_000);
});
