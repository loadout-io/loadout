/* PIERWSZE OTWARCIE MIEŚCI SIĘ W OKNIE, W KTÓRYM SIĘ OTWIERA — zmierzone w prawdziwym
 * chromium, w pikselach.
 *
 * CO ZOBACZYŁ CZŁOWIEK, zgłoszenie właściciela 2026-08-31 ze zrzutu okna 1512×950. Ekran, na
 * który pada pierwsze spojrzenie po instalacji, stał PRZEWINIĘTY DO KOŃCA W PRAWO: pierwsze
 * 316 px górnego rzędu chowało się pod bocznym menu, a pierwsza kontrolka czytała się
 * „…agents saved yet" zamiast „No agents saved yet". Łańcuch ramek mówił, gdzie się to bierze:
 *
 *   main[data-section]           w=1180  scrollWidth=1496
 *     div[data-work]             w=1178  scrollWidth=1494
 *       div[data-stream-column]  w=802   scrollWidth=1118
 *         div[data-first-open]   w=1118  scrollWidth=1118
 *
 * Czytaj od dołu: powitanie potrzebowało 1118 px, a dostawało tor `1fr` siatki pracy — 802 px,
 * bo obok niego stała pusta kolumna kroków szeroka na 376 px. Nic tego nie przycinało, więc
 * nadmiar przelewał się aż do `main`.
 *
 * DLACZEGO TO KRYTERIUM MIERZY, A NIE CZYTA KLAS. W repo nie ma jsdom, a `renderToStaticMarkup`
 * nie zna ani jednego piksela: kryterium na klasach przechodzi także wtedy, gdy siatka oddaje
 * powitaniu połowę okna. Wada jest wadą UKŁADU, czyli jedyną rodziną, którą widać wyłącznie
 * w przeglądarce (niezmiennik 29). Ten sam powód i ten sam kształt, co
 * `./canvas-keeps-its-width.spec.ts`.
 *
 * DLACZEGO `openApp()` BEZ ODPOWIEDZI. Atrapa granicy oddaje wtedy pustą listę na każde
 * `list_*` (nagłówek `../harness.ts`) — czyli dokładnie świeżą instalację: ani jednego agenta,
 * ani jednego workflow, ani jednego zakresu. To nie jest scena zbudowana pod ten pomiar, tylko
 * jedyna scena, jaką widzi człowiek, zanim cokolwiek zrobi.
 *
 * TRZY PYTANIA, TRZY `it`, bo to są trzy różne fakty o tym samym ekranie: czy tafla mieści się
 * w oknie, czy naprawdę dostała całą powierzchnię zamiast toru obok pustej kolumny kroków, i czy
 * to, co na niej stoi, nie gniecie się ani nie maluje jedno po drugim. Zlanie ich w jedno
 * kryterium dałoby jedno zdanie porażki na trzy różne wady.
 *
 * DWA OKNA, NIE JEDNO — powód przy [`WINDOWS`]. Każde z trzech pytań jest zadane przy obu.
 */
import { afterAll, beforeAll, describe, expect, it } from 'vitest';

import type { RunningApp } from '../harness';
import { closeEverything, openApp } from '../harness';

/**
 * Dwa okna, w których ten ekran ma się mieścić — i obie liczby są cudze, nie wybrane tutaj.
 *
 * 1512×950 to okno, na którym właściciel zobaczył wadę, i ta sama para, co
 * w `./knowledge-two-shelves-touch.spec.ts`. 1100×700 to NAJMNIEJSZE okno, jakie ta aplikacja
 * w ogóle daje otworzyć (`src-tauri/tauri.conf.json`, `minWidth`/`minHeight`) — a ekran, który
 * przelewa się przy dozwolonym rozmiarze okna, jest zepsuty dokładnie tak samo jak ten
 * z pierwszego zrzutu, tylko trudniej to komuś pokazać. `scripts/density-collect.mjs` mierzy
 * gęstość przy tych samych 1100 i 1512, więc to jest miara tego repo, nie moja.
 */
const WINDOWS = [
  { width: 1512, height: 950 },
  { width: 1100, height: 700 },
] as const;

/** Ekran Run, czyli sekcja, na której powłoka się otwiera (`src/ui/shell/section-store.ts`). */
const RUN = 'main[data-section="run"]';

/** Cała tafla pierwszego otwarcia: droga, powitanie, galeria gotowych i wiersz klawiszy. */
const FIRST_OPEN = '[data-first-open]';

/** Kolumna, w której stoją kroki biegu. */
const STEP_COLUMN = '[data-plan-column]';

/** Obszar pracy ekranu Run — cała powierzchnia pod paskiem, bez chrome. */
const WORK = '[data-work]';

/** Jeden krok narysowany na ekranie. */
const STEP = '[data-step]';

/** Pas drogi z licznikiem i trzema przystankami — pierwsza rzecz na tafli. */
const ROAD = '[data-first-road]';

/** Powitanie: znak, tytuł, zdanie, jedna głośna kontrolka. */
const HERO = '[data-first-hero]';

/** Znak powitania. Ma być kołem, a koło ma dwa równe boki. */
const ORB = '[data-first-orb]';

/** Wiersz wejścia. Stoi na dole tafli i ma tam ZOSTAĆ, także kiedy powitanie jest wysokie. */
const COMMAND = 'input[aria-label="Command line"]';

/** Ile czekamy na powitanie: listy z dysku wracają przez atrapę granicy, czyli po obrocie IPC. */
const SHOWS_UP = 15_000;

/** Jeden piksel luzu: chromium liczy szerokości w ułamkach i zaokrągla je w górę. */
const SLACK = 1;

/** Jedna ramka, tak jak ją widzi człowiek — i tyle, ile trzeba, żeby powiedzieć, co się przelało. */
interface Frame {
  readonly what: string;
  readonly left: number;
  readonly width: number;
  readonly clientWidth: number;
  readonly scrollWidth: number;
  readonly over: number;
}

/** Prostokąt na ekranie — cztery krawędzie, bo pytanie brzmi „czy te dwa na siebie zachodzą". */
interface Box {
  readonly top: number;
  readonly bottom: number;
  readonly width: number;
  readonly height: number;
}

interface Measured {
  readonly window: { readonly width: number; readonly height: number };
  /** Łańcuch od sekcji w dół, po jednej ramce na piętro — do czytania w zdaniu porażki. */
  readonly chain: readonly Frame[];
  readonly section: Frame;
  readonly work: Frame | null;
  readonly firstOpen: Frame | null;
  readonly scrolledRight: number;
  readonly stepColumns: number;
  readonly steps: number;
  readonly road: Box | null;
  readonly hero: Box | null;
  readonly orb: Box | null;
  readonly command: Box | null;
}

async function measure(app: RunningApp): Promise<Measured> {
  return app.page.evaluate(
    ({
      runSelector,
      firstOpenSelector,
      stepColumnSelector,
      stepSelector,
      workSelector,
      roadSelector,
      heroSelector,
      orbSelector,
      commandSelector,
    }) => {
      function frame(what: string, element: Element): Frame {
        const box = element.getBoundingClientRect();
        return {
          what,
          left: box.left,
          width: box.width,
          clientWidth: element.clientWidth,
          scrollWidth: element.scrollWidth,
          over: element.scrollWidth - element.clientWidth,
        };
      }

      function boxOf(selector: string): Box | null {
        const element = document.querySelector(selector);
        if (element === null) return null;
        const box = element.getBoundingClientRect();
        return { top: box.top, bottom: box.bottom, width: box.width, height: box.height };
      }

      const section = document.querySelector(runSelector);
      if (section === null) {
        throw new Error(`the first screen carries no ${runSelector} to measure`);
      }
      const opening = document.querySelector(firstOpenSelector);
      const work = document.querySelector(workSelector);

      /* Łańcuch to każde piętro między sekcją a powitaniem — czyli dokładnie ta droga, po
         której nadmiar wędrował w górę na zrzucie właściciela. Bez powitania w dokumencie
         zostaje sama sekcja i to też jest odpowiedź. */
      const chain: Frame[] = [frame(runSelector, section)];
      if (opening !== null) {
        const upwards: Element[] = [];
        let walk: Element | null = opening;
        while (walk !== null && walk !== section) {
          upwards.push(walk);
          walk = walk.parentElement;
        }
        for (const one of upwards.reverse()) {
          const marker = [...one.attributes]
            .map((attribute) => attribute.name)
            .find((name) => name.startsWith('data-'));
          chain.push(frame(marker === undefined ? one.tagName.toLowerCase() : `[${marker}]`, one));
        }
      }

      return {
        window: { width: window.innerWidth, height: window.innerHeight },
        chain,
        section: frame(runSelector, section),
        work: work === null ? null : frame(workSelector, work),
        firstOpen: opening === null ? null : frame(firstOpenSelector, opening),
        scrolledRight: section.scrollLeft,
        stepColumns: document.querySelectorAll(stepColumnSelector).length,
        steps: document.querySelectorAll(stepSelector).length,
        road: boxOf(roadSelector),
        hero: boxOf(heroSelector),
        orb: boxOf(orbSelector),
        command: boxOf(commandSelector),
      };
    },
    {
      runSelector: RUN,
      firstOpenSelector: FIRST_OPEN,
      stepColumnSelector: STEP_COLUMN,
      stepSelector: STEP,
      workSelector: WORK,
      roadSelector: ROAD,
      heroSelector: HERO,
      orbSelector: ORB,
      commandSelector: COMMAND,
    },
  );
}

/** Łańcuch ramek w jednym napisie — zdanie porażki ma pokazać, KTÓRE piętro się rozepchało. */
function readOut(measured: Measured): string {
  return measured.chain
    .map(
      (one) =>
        `${one.what} x=${String(Math.round(one.left))} w=${String(Math.round(one.width))} ` +
        `content=${String(one.scrollWidth)} over=${String(one.over)}`,
    )
    .join(' | ');
}

/** Pomiar na każde okno z [`WINDOWS`], w tej samej karcie — jedna aplikacja, dwa rozmiary. */
const seen = new Map<number, Measured>();

/** Pomiar spod tej szerokości, albo wyjątek mówiący, że karta go nie oddała. */
function at(width: number): Measured {
  const found = seen.get(width);
  if (found === undefined) {
    throw new Error(`nothing was measured at ${String(width)} px, so there is nothing to judge`);
  }
  return found;
}

let app: RunningApp;

beforeAll(async () => {
  app = await openApp();
  for (const size of WINDOWS) {
    await app.page.setViewportSize({ width: size.width, height: size.height });
    await app.page.locator(RUN).waitFor({ state: 'visible', timeout: SHOWS_UP });
    await app.page.locator(FIRST_OPEN).waitFor({ state: 'visible', timeout: SHOWS_UP });
    seen.set(size.width, await measure(app));
  }
}, 180_000);

afterAll(async () => {
  await closeEverything();
});

describe('the screen a person opens first fits the window it opens in', () => {
  it('keeps the whole welcome inside the surface, with nothing pushed off the left edge', () => {
    for (const size of WINDOWS) {
      const measured = at(size.width);
      const where = `at ${String(size.width)}×${String(size.height)}: `;

      expect(
        measured.firstOpen,
        where +
          'the first open of the application drew no welcome at all, so every measurement ' +
          'below would be describing a screen nobody sees. Frames on screen: ' +
          readOut(measured),
      ).not.toBeNull();

      expect(
        measured.window.width,
        where + 'the window never took the size this measurement is about',
      ).toBe(size.width);

      expect(
        measured.section.scrollWidth,
        where +
          'the first screen of the application is wider than itself, so a part of it can only ' +
          'be reached by scrolling sideways — which is why the first control reads ' +
          '"…agents saved yet" instead of "No agents saved yet". Frames: ' +
          readOut(measured),
      ).toBeLessThanOrEqual(measured.section.clientWidth + SLACK);

      expect(
        measured.scrolledRight,
        where +
          'the first screen opens parked sideways, so the left edge of everything on it is ' +
          'hidden under the side menu. Frames: ' +
          readOut(measured),
      ).toBe(0);

      /* Powitanie ma się mieścić TAM, GDZIE STOI, a nie tylko nie przelewać sekcji: blok
         przycięty przez rodzica z `overflow:hidden` zdałby oba punkty wyżej i dalej gubiłby
         pierwszą kontrolkę. */
      expect(
        measured.firstOpen?.over ?? 0,
        where +
          'the welcome asks for more width than it is given, so its own content overflows the ' +
          'surface it was drawn on. Frames: ' +
          readOut(measured),
      ).toBeLessThanOrEqual(SLACK);
    }
  });

  it('gives the welcome the whole work area, with no column of steps beside it', () => {
    for (const size of WINDOWS) {
      const measured = at(size.width);
      const where = `at ${String(size.width)}×${String(size.height)}: `;

      expect(
        measured.firstOpen,
        where +
          'the first open of the application drew no welcome at all, so the two counts below ' +
          'would be describing a screen nobody sees',
      ).not.toBeNull();

      expect(
        measured.steps,
        where +
          'the first screen draws steps before a single one exists — the stub answers an empty ' +
          'list to every list_* command, so there is nothing on this machine to draw',
      ).toBe(0);

      expect(
        measured.stepColumns,
        where +
          'the welcome shares the screen with an empty column of steps, so the hero of the ' +
          'first open is squeezed into what is left of the work grid instead of taking the ' +
          'whole surface. Frames: ' +
          readOut(measured),
      ).toBe(0);

      /* ZDJĘTA KOLUMNA KROKÓW TO POŁOWA ODPOWIEDZI. Siatka, która została po niej z torem
         `376px minmax(0,1fr)` i jednym dzieckiem, wsadza powitanie w tor pierwszy — czyli
         w 376 px, obok 800 px czerni — i nic z tego nie przelewa ekranu. Zmierzone mutacją:
         bez tego punktu podmiana toru na `WORK_COLUMNS` była zielona na wszystkich trzech
         kryteriach. Powierzchnia jest tym, o co prosi zlecenie, więc jest tu zmierzona. */
      expect(
        Math.round(measured.firstOpen?.width ?? 0),
        where +
          'the welcome does not take the work area it was given: the surface is ' +
          String(Math.round(measured.work?.clientWidth ?? 0)) +
          ' px wide and the welcome takes ' +
          String(Math.round(measured.firstOpen?.width ?? 0)) +
          '. What is left is not another screen — it is black. Frames: ' +
          readOut(measured),
      ).toBeGreaterThanOrEqual(Math.round(measured.work?.clientWidth ?? 0) - SLACK);

      /* I TA SAMA TAFLA MA WYSOKOŚĆ, NIE TYLKO SZEROKOŚĆ. Powitanie jest wysokie — przy oknie
         1100×700 samo powitanie z galerią nie mieści się i przewija w sobie — więc pytanie
         „czy wiersz wejścia dalej jest na ekranie" jest o tej tafli tak samo zasadne, jak
         o widoku pracy pyta je `./t161-long-workflow-stays-inside-run.spec.ts`. Tam pilnuje
         długiego workflow, tutaj wysokiego powitania. */
      expect(
        Math.round(measured.command?.bottom ?? 0),
        where +
          'the command line fell below the bottom edge of the window, so the one control this ' +
          'screen offers a person who wants to type instead of click is not on the screen. It ' +
          'ends at ' +
          String(Math.round(measured.command?.bottom ?? 0)) +
          ' and the window ends at ' +
          String(size.height),
      ).toBeLessThanOrEqual(size.height + SLACK);

      expect(
        Math.round(measured.command?.height ?? 0),
        where + 'the command line has no height at all, so nothing above was measured against it',
      ).toBeGreaterThan(0);
    }
  });

  /* TRZECIE PYTANIE JEST O TĘ SAMĄ WADĘ, TYLKO W PIONIE. Tafla, która dostała całą powierzchnię,
     zaczyna ją sobie dzielić — a dwie rzeczy dzielone z `justify-center` i bez dolnej granicy
     wchodzą jedna na drugą albo gniotą się w pasek. Oba stany zmierzone w chromium przy oknie
     1100×700, najmniejszym, jakie ta aplikacja daje otworzyć. */
  it('keeps the welcome from squashing its mark or painting over the way to the first run', () => {
    for (const size of WINDOWS) {
      const measured = at(size.width);
      const where = `at ${String(size.width)}×${String(size.height)}: `;
      const road = measured.road;
      const hero = measured.hero;
      const orb = measured.orb;

      expect(
        [road, hero, orb].every((one) => one !== null),
        where +
          'the first open is missing its way, its welcome or its mark, so the two questions ' +
          'below would be asked about boxes that are not on the screen',
      ).toBe(true);

      /* RÓŻNICA BOKÓW, nie równość co do piksela: chromium liczy pudełka w ułamkach i zaokrągla
         je w obie strony, więc `88 === 87` przewracałoby ten punkt na zaokrągleniu zamiast na
         gnieceniu. Wada, o którą pytamy, ma rozmiar 88 na 14. */
      expect(
        Math.abs(Math.round(orb?.width ?? 0) - Math.round(orb?.height ?? 0)),
        where +
          'the mark of the welcome is not a circle any more: the column handed it whatever ' +
          'height was left over instead of the size it asks for, and a squashed circle reads ' +
          'as a bar somebody drew by accident. It measures ' +
          String(Math.round(orb?.width ?? 0)) +
          '×' +
          String(Math.round(orb?.height ?? 0)),
      ).toBeLessThanOrEqual(SLACK);

      /* PYTAMY O GÓRĘ ZNAKU, NIE O GÓRĘ POWITANIA, i to jest różnica, na której ten punkt stoi.
         Powitanie ściśnięte poniżej swojej treści ma pudełko dalej NA SWOIM MIEJSCU — wylewa się
         z niego sama treść, w obie strony, bo stoi na `justify-center`. Punkt postawiony na
         `hero.top` był wtedy zielony nad ekranem, na którym znak leżał na drodze; zmierzone
         mutacją przy 1100×700. */
      expect(
        Math.round(orb?.top ?? 0),
        where +
          'the mark of the welcome is painted ON TOP of the three steps to the first run: the ' +
          'welcome was allowed to shrink below its own content and spilled out of its place in ' +
          'both directions. The way ends at ' +
          String(Math.round(road?.bottom ?? 0)) +
          ' and the mark starts at ' +
          String(Math.round(orb?.top ?? 0)) +
          '; the welcome itself measures ' +
          String(Math.round(hero?.height ?? 0)) +
          ' tall',
      ).toBeGreaterThanOrEqual(Math.round(road?.bottom ?? 0) - SLACK);
    }
  });
});
