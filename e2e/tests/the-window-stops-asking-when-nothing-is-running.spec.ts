/* Z-25: kiedy zejdzie ostatnia rzecz uruchomiona komendą, okno PRZESTAJE pytać rejestr.
 *
 * ZMIERZONY DEFEKT (audyt 2026-09-02, F-2). `StartedThings` stawiał `setInterval` przy montażu
 * i trzymał go do końca życia okna, więc `list_processes` przechodziło granicę raz na sekundę
 * także wtedy, gdy nic nie biegło i biec nie miało prawa. Jedno przejście granicy na sekundę
 * przez godzinę otwartego okna to 3600 wywołań, z których ani jedno nie zmienia ani jednego
 * piksela — a każde z nich publikowało wtedy nową tablicę i przerysowywało cały ekran Bieg.
 *
 * PO CO TO ISTNIEJE OBOK KRYTERIUM O MAGAZYNIE. Niezmiennik 29: tamto
 * (`../../src/sections/run/rail/a-tick-that-changes-nothing-leaves-the-list-alone.test.ts`)
 * pyta funkcję i mierzy referencję migawki. Ten plik pyta o to, co się STAŁO w prawdziwej
 * przeglądarce, na prawdziwym Reakcie i prawdziwej klawiaturze: czy odstęp naprawdę zszedł
 * razem z ostatnim kafelkiem. Odstęp trzymany przez `useEffect` z pustą listą zależności jest
 * niewidoczny dla każdego kryterium czystego — zdejmuje go dopiero sprzątanie efektu, a efekty
 * nie odpalają się pod `renderToStaticMarkup`.
 *
 * SŁABA WERSJA: „okno nie pyta". Przechodzi ją implementacja, która nie pyta NIGDY — a wtedy
 * kafelek zostaje na ekranie nad rzeczą, która zeszła, bo odświeżanie jest jedyną drogą, którą
 * okno się o tym dowiaduje (`rail/processes.ts`, „SKĄD OKNO WIE, ŻE COŚ ZESZŁO"). Rozróżniają to
 * dwie rzeczy w tej samej karcie: kafelek MUSI zniknąć sam (b), a przed ciszą musi paść co
 * najmniej jedno pytanie (c).
 *
 * CZEGO TEN PLIK NIE DOWODZI. Granica Rusta jest atrapą (`../harness.ts`): `start_process` oddaje
 * liczbę i nic nie uruchamia, `list_processes` oddaje pustą listę. To wystarcza, bo pytanie tego
 * kryterium dotyczy WYŁĄCZNIE tego, ile razy okno przechodzi granicę — a nie tego, co po drugiej
 * stronie naprawdę biegnie.
 *
 * ── DRUGI PRZYPADEK: LICZNIK PRZERYSOWAŃ ────────────────────────────────────────────────────
 *
 * Pierwszy przypadek mówi o oknie, w którym NIC nie biegnie. Drugi mówi o oknie, w którym coś
 * biegnie i nie zmienia się: odstęp pyta rejestr co sekundę, odpowiedź jest wciąż ta sama, a ekran
 * ma się nie przerysować ani razu. To jest druga połowa zgłoszenia F-2 — ta o dwóch tysiącach
 * wypowiedzi w strumieniu, z których każda leksowała markdown od nowa przy każdym tyknięciu.
 *
 * DLACZEGO LICZYMY ZATWIERDZENIA REACTA, A NIE WYWOŁAŃ `Message`. Licznik per komponent wymaga
 * albo jsdom (nie ma go w tym repo, a `package.json` jest niezapisywalny), albo chodzenia po
 * włóknach Reacta wewnątrz przeglądarki — czyli mechanizmu, którego cichą awarią jest ZERO, czyli
 * fałszywa zieleń (niezmiennik 20). Zamiast tego pytamy React o jego własne zatwierdzenia przez
 * `__REACT_DEVTOOLS_GLOBAL_HOOK__`, czyli tę samą drogę, którą pyta go DevTools. Zero zatwierdzeń
 * w oknie, w którym na ekranie stoją wypowiedzi, znaczy że NIC się nie przerysowało — a to jest
 * zdanie mocniejsze niż „licznik renderów `Message` stoi w miejscu", nie słabsze.
 *
 * CICHA AWARIA LICZNIKA JEST TU CZERWONA, i to jest warunek, żeby ten przypadek cokolwiek znaczył:
 * zanim padnie pytanie o ciszę, scena zmienia coś naprawdę (druga linia w strumieniu) i wymaga,
 * żeby licznik DRGNĄŁ. Licznik, którego React nie woła, przewraca ten przypadek zamiast go
 * zazielenić.
 */
import { afterAll, beforeAll, describe, expect, it } from 'vitest';

import type { RunningApp } from '../harness';
import { closeEverything, openApp } from '../harness';

/** Ekran pracy jest pierwszą sekcją okna (`src/ui/shell/section-store.ts`), więc nikt nie klika. */
const WORK = 'main[data-section="run"]';

/** Pole wiersza wejścia. Ta sama etykieta, po której idzie czytnik ekranu. */
const FIELD = '[aria-label="Command line"]';

/** Kafelek rzeczy uruchomionej komendą — własna grupa nad obrazem planu. */
const TILE = '[data-plan-column] [data-started]';

/** To, co otwiera kliknięcie w kafelek: wyjście tej jednej rzeczy. */
const PANEL = '[data-started-output]';

/** Wiersz powłoki, który wpisuje człowiek. */
const COMMAND = 'npm run dev';

/** Cała linia, dokładnie tak, jak stanie w polu. */
const TYPED = '/start ' + COMMAND;

/** Grupa, którą atrapa oddaje na start. Bez niej wpis nigdy nie dostaje adresu i nie schodzi. */
const GROUP = 4242;

/**
 * Uchwyt, pod którym atrapa TRZYMA odpowiedź na start, dopóki test jej nie domknie.
 *
 * Nie `{ value: GROUP }` od razu, i to nie jest ostrożność: wpis bez adresu grupy jest przez
 * odświeżanie NIETYKALNY (`rail/processes.ts`, reguła 1), a wpis z adresem znika w pierwszym
 * odświeżeniu, które go nie znajdzie. Odpowiedź natychmiastowa daje więc kafelek żyjący jedną
 * klatkę — pytanie przy montażu kolumny wraca w tej samej chwili, co adres — i scena mierzyłaby
 * wtedy szeregowanie przeglądarki zamiast produktu. Uchwyt rozdziela te dwie chwile na dwie
 * czynności testu: najpierw kafelek stoi, potem dostaje adres, dopiero potem może zejść.
 */
const STARTING = 'the-start-this-scene-holds';

/** Ile czekamy na pierwsze pojawienie się elementu, który ma przyjść po zdarzeniu. */
const APPEARS = 4_000;

/**
 * Ile trwa cisza, której pilnujemy — w milisekundach.
 *
 * Trzy sekundy przy odstępie jednej: na dzisiejszym kodzie mieszczą się w tym okna DWA albo TRZY
 * pytania, więc różnica między zerem a „dalej pyta" nie jest kwestią jednego tyknięcia w tę czy
 * we w tę. Krócej byłoby wróżeniem z szeregowania, dłużej niczego nie dokłada.
 */
const QUIET = 3_000;

/** Nazwa komendy po tamtej stronie granicy — tej, której liczbę wywołań mierzymy. */
const ASKING = 'list_processes';

/**
 * Wiersz strumienia — czyli JEDNO wywołanie `Message`.
 *
 * `[data-line]` w kolumnie strumienia, a nie `[data-message]`, i to jest różnica o treści:
 * `Message` rysuje wypowiedź jako `article[data-message]`, ale wiersz rodzaju `run` — a takie są
 * wszystkie wiersze, które okno dopisuje samo (`../../src/sections/run/entry/echo.ts`) — jako
 * kreskę z podpisem. Oba przechodzą przez ten sam komponent, więc oba są tym, co ten przypadek
 * liczy. Zawężone do `[data-feed]`, bo `[data-line]` niesie też wiersz transkryptu z szuflady
 * kroku (`feed/line.tsx`), a to jest inna powierzchnia.
 */
const ROW = '[data-feed] [data-line]';

/** Druga linia — ta, która ma czymś ruszyć, żeby dowieść, że licznik żyje. */
const SECOND = '/start echo second';

/** Ile czekamy, aż React dorysuje skutek naciśnięcia klawisza. Render, nie sieć. */
const SETTLE = 500;

/** Nazwa licznika zatwierdzeń w oknie. Napisana raz i przekazywana obu stronom argumentem. */
const COMMITS = '__LOADOUT_E2E_COMMITS__';

/**
 * Licznik ZATWIERDZEŃ Reacta, zasiany przed pierwszym skryptem strony.
 *
 * To jest ta sama droga, którą React oddaje swoje zatwierdzenia narzędziom deweloperskim:
 * `injectInternals` czyta `__REACT_DEVTOOLS_GLOBAL_HOOK__` RAZ, przy inicjalizacji modułu, i od
 * tej chwili woła `onCommitFiberRoot` po każdym zatwierdzeniu. Pola poniżej są dokładnie tymi,
 * których React dotyka — reszta zaczepu narzędzi deweloperskich nas nie interesuje, ale musi
 * istnieć jako nic nierobiąca, bo React sprawdza je po `typeof`.
 *
 * Funkcja jedzie do przeglądarki jako napis, więc nie widzi ani jednego identyfikatora z tego
 * modułu: nazwa licznika przyjeżdża argumentem.
 */
function countReactCommits(slot: string): void {
  const host = globalThis as unknown as Record<string, unknown>;
  host[slot] = 0;
  const bump = (): void => {
    host[slot] = (host[slot] as number) + 1;
  };
  const nothing = (): void => undefined;
  host['__REACT_DEVTOOLS_GLOBAL_HOOK__'] = {
    isDisabled: false,
    supportsFiber: true,
    supportsFlight: true,
    renderers: new Map<number, unknown>(),
    inject: () => 1,
    checkDCE: nothing,
    onCommitFiberRoot: bump,
    onPostCommitFiberRoot: nothing,
    onCommitFiberUnmount: nothing,
    registerInternalModuleStart: nothing,
    registerInternalModuleStop: nothing,
    getFiberRoots: () => new Set<unknown>(),
    setStrictMode: nothing,
    on: nothing,
    off: nothing,
    emit: nothing,
  };
}

/** Ile razy React zatwierdził od otwarcia tej karty. Wykonywane w przeglądarce. */
function readReactCommits(slot: string): number {
  const host = globalThis as unknown as Record<string, unknown>;
  const seen = host[slot];
  if (typeof seen !== 'number') {
    /* GŁOŚNO, a nie zerem. Brak licznika znaczy, że nie zdążył przed skryptami strony — a zero
       zamieniłoby awarię przyrządu w zdanie „nic się nie przerysowało", czyli w fałszywą zieleń
       dokładnie tam, gdzie ten przypadek ma być czerwony (niezmiennik 20). */
    throw new Error(
      'the React commit counter is not on this page: window.' +
        slot +
        ' is missing, so nothing was watching the renderer and "nothing redrew" would be a ' +
        'statement about this scene, not about the screen.',
    );
  }
  return seen;
}

/** Otwiera aplikację i czeka na ekran pracy. Ani jednego kliknięcia — praca jest pierwsza. */
async function openWork(): Promise<RunningApp> {
  const app = await openApp({ replies: { start_process: [{ deferred: STARTING }] } });
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

/* Rozruch vite i chromium jest kosztem STAŁYM NA PLIK, nie częścią pierwszego przypadku:
 * `openApp()` jest leniwy, więc bez tego haka jedyny `it` płaci cały rozruch pod swoim limitem. */
beforeAll(async () => {
  await openApp();
}, 180_000);

afterAll(async () => {
  await closeEverything();
}, 30_000);

describe('the window stops asking about started things once the last one is gone', () => {
  /* JEDEN PRZYPADEK NA CAŁĄ SCENĘ. Kontrola (c) — „przedtem pytało" — ma sens wyłącznie w tej
   * samej karcie, w której padnie (d): to jest jedno zdanie o jednym oknie, a rozbite na dwa `it`
   * każde z nich otwierałoby własną kartę i liczyło pytania czyjegoś innego okna. */
  it('stops asking about started things once the last one is gone', async () => {
    const app = await openWork();
    try {
      // ── (a) COŚ RUSZA I WIDAĆ TO NA EKRANIE ───────────────────────────────────────────
      expect(
        await app.page.locator(TILE).count(),
        'there is no tile of this kind before anything was typed — and there had better not be, ' +
          'or the disappearance below would be about furniture that was already on screen.',
      ).toBe(0);

      await app.page.fill(FIELD, TYPED);
      await app.page.press(FIELD, 'Enter');
      await app.page
        .locator(TILE)
        .first()
        .waitFor({ state: 'attached', timeout: APPEARS })
        .catch(() => undefined);
      expect(
        await app.page.locator(TILE).count(),
        'sending ' +
          JSON.stringify(TYPED) +
          ' left no tile behind, so this window never had anything to stop asking about.',
      ).toBeGreaterThan(0);

      // ── (b) CZŁOWIEK WCHODZI W JEJ WYJŚCIE ────────────────────────────────────────────
      // Scena schodzi tędy z rozmysłu, i to jest naprawa zgłoszona weryfikacją Z-25: rzecz, która
      // kończy się PODCZAS oglądania jej wyjścia, jest jedynym stanem, w którym okno zostaje
      // z otwartym panelem nad czymś, czego nie ma już na liście. Panel gaśnie sam (rysuje go
      // wyszukanie po kluczu w liście), więc oko widzi to samo — a okno, które dalej trzyma ten
      // klucz, dalej ma powód, żeby pytać rejestr co sekundę. Scena bez tego kliknięcia była
      // zielona nad dokładnie tym wyciekiem.
      await app.page.locator(TILE).first().click();
      await app.page.locator(PANEL).waitFor({ state: 'attached', timeout: APPEARS });

      // ── (c) I ZNIKA SAMO, KIEDY REJESTR PRZESTAJE O NIM WIEDZIEĆ ──────────────────────
      // Adres grupy dojeżdża dopiero teraz, więc dopiero teraz jest czym tę rzecz zaadresować.
      // Atrapa odpowiada na pytanie o rejestr pustą listą, czyli tak, jak odpowiada Rust po
      // dowodzie śmierci grupy — a to jest jedyna droga, którą okno się o niej dowiaduje.
      // Zniknięcie kafelka dowodzi więc, że pytanie naprawdę poszło i naprawdę wróciło.
      await app.settle(STARTING, { value: GROUP });
      await app.page.locator(TILE).first().waitFor({ state: 'detached', timeout: APPEARS });
      await app.page.locator(PANEL).waitFor({ state: 'detached', timeout: APPEARS });

      // ── (d) KONTROLA: PRZEDTEM OKNO PYTAŁO ────────────────────────────────────────────
      const asked = (await app.calls()).filter((call) => call.cmd === ASKING).length;
      expect(
        asked,
        'the window never asked at all, which would make the silence below free. Refreshing is ' +
          'the ONLY way this window learns that something it started went down: a window that ' +
          'never asks leaves a tile standing over a line that ended two minutes ago.',
      ).toBeGreaterThan(0);

      // ── (e) I CISZA, KIEDY NIE MA JUŻ O CO PYTAĆ ──────────────────────────────────────
      await app.page.waitForTimeout(QUIET);
      const after = (await app.calls()).filter((call) => call.cmd === ASKING).length;

      expect(
        after - asked,
        'the last started thing is gone from the screen and the window kept crossing the ' +
          'boundary once a second anyway: ' +
          String(after - asked) +
          ' more times in ' +
          String(QUIET) +
          ' ms. Nothing can enter that registry except this window, so an answer to a question ' +
          'nobody has is the same empty list every time — and each one of them used to redraw ' +
          'the whole run screen, the stream under it, and the markdown of every line in it.',
      ).toBe(0);
    } finally {
      await app.close();
    }
  }, 90_000);

  it('redraws nothing while it asks about something that has not changed', async () => {
    const app = await openApp({ replies: { start_process: [{ deferred: STARTING }] } });
    try {
      /* LICZNIK MUSI STANĄĆ PRZED REACTEM, bo `__REACT_DEVTOOLS_GLOBAL_HOOK__` jest czytany raz,
         przy inicjalizacji modułu. `addInitScript` obejmuje kolejne nawigacje tej karty, więc
         przeładowanie JEST tu całym mechanizmem — a przy okazji daje świeżą taśmę wywołań
         i świeżo zasianą kolejkę odpowiedzi atrapy. */
      await app.page.addInitScript(countReactCommits, COMMITS);
      await app.page.reload();
      await app.page.locator(WORK).waitFor({ state: 'attached', timeout: APPEARS });
      await app.page.locator(FIELD).waitFor({ state: 'attached', timeout: APPEARS });

      /* COŚ, CO NIE SCHODZI. Start bez odpowiedzi zostaje na liście na zawsze — wpis bez adresu
         grupy jest dla odświeżania nietykalny (`rail/processes.ts`, reguła 1) — więc odstęp pyta
         rejestr dalej, a rejestr odpowiada wciąż tym samym. To jest tik bez zmiany, powtarzany
         co sekundę: dokładnie ten, który przez godzinę biegu leksował markdown całego strumienia
         od nowa trzy tysiące sześćset razy. */
      await app.page.fill(FIELD, TYPED);
      await app.page.press(FIELD, 'Enter');
      await app.page.locator(TILE).first().waitFor({ state: 'attached', timeout: APPEARS });

      expect(
        await app.page.locator(ROW).count(),
        'there is nothing in the stream, so "the stream did not redraw" would be a sentence ' +
          'about an empty column. Every one of these rows is one call of the component this ' +
          'task memoised, so the scene needs at least one of them on screen before it can say ' +
          'that none of them ran again.',
      ).toBeGreaterThan(0);

      // ── KONTROLA: LICZNIK ŻYJE ────────────────────────────────────────────────────────
      // Zmiana, która NAPRAWDĘ jest zmianą, musi go ruszyć. Bez tego przypadek niżej przechodzi
      // dla zaczepu, którego React nigdy nie zawołał — a jego cichą awarią jest zero.
      const beforeChange = await app.page.evaluate(readReactCommits, COMMITS);
      await app.page.fill(FIELD, SECOND);
      await app.page.press(FIELD, 'Enter');
      await app.page.waitForTimeout(SETTLE);
      const afterChange = await app.page.evaluate(readReactCommits, COMMITS);
      expect(
        afterChange - beforeChange,
        'a second line went into the stream and React committed nothing, which means this scene ' +
          'is not watching the renderer at all. Every number below would then be zero for a ' +
          'reason that has nothing to do with the screen.',
      ).toBeGreaterThan(0);

      // ── CISZA PRZY PRACUJĄCYM ODSTĘPIE ────────────────────────────────────────────────
      const asked = (await app.calls()).filter((call) => call.cmd === ASKING).length;
      await app.page.waitForTimeout(QUIET);
      const askedAgain = (await app.calls()).filter((call) => call.cmd === ASKING).length;
      const drawnAgain = await app.page.evaluate(readReactCommits, COMMITS);

      expect(
        askedAgain - asked,
        'the window stopped asking while something was still up, so the silence below is free ' +
          'and says nothing about redrawing. Something IS on the list here, and its tile has to ' +
          'stop being drawn the moment that thing ends — which is what the asking is for.',
      ).toBeGreaterThanOrEqual(2);

      expect(
        drawnAgain - afterChange,
        'the registry answered ' +
          String(askedAgain - asked) +
          ' times with exactly what the window already knew, and the screen redrew ' +
          String(drawnAgain - afterChange) +
          ' times anyway. Every one of those redraws walks the whole stream — up to two thousand ' +
          'messages, each one reading its markdown from scratch — to put back the same pixels. ' +
          'That is the cost this task exists to remove.',
      ).toBe(0);
    } finally {
      await app.close();
    }
  }, 90_000);
});
