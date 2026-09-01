/* Płótno przestaje wyglądać jak paleta języka programowania — trzy pomiary w chromium.
 *
 * PO CO TO ISTNIEJE W PRZEGLĄDARCE, a nie obok, w `src/sections/workflows/canvas/`. Wszystkie
 * trzy wady tego zgłoszenia są wadami UKŁADU I FARBY, a repo nie ma jsdom: `renderToStaticMarkup`
 * nie zna ani jednego piksela i ani jednej reguły arkusza. Kryterium napisane tam odpowiadałoby
 * na pytanie „czy w klasie stoi napis `truncate`", a zgłoszenie właściciela brzmi „nazwa jest
 * ucięta" i „strzałek nie widać". Między jednym a drugim mieszka klasa wady, dla której to repo
 * powstało (niezmiennik 29) — tutaj mierzymy SKUTEK: ile kontrolek, ile pikseli, jaki kontrast.
 *
 * ZMIERZONE PRZED NAPRAWĄ, na tym samym pliku i w tym samym oknie — liczby stoją w komunikatach
 * asercji niżej, bo to je czyta człowiek, który tę czerwień zobaczy.
 *
 * GRANICA ODPOWIADA KSZTAŁTEM, nie treścią (nagłówek `../harness.ts`): `check_workflow` dostaje
 * pustą listę, bo ekran czyta z niej `length`, a domyślne `null` byłoby awarią PRZYRZĄDU,
 * nie produktu.
 */
import { afterAll, beforeAll, describe, expect, it } from 'vitest';
import type { Page } from '@playwright/test';

import type { TauriReply } from '../harness';
import { closeEverything, openApp } from '../harness';

/** Agent o nazwie, którą właściciel widział na zrzucie. Chip bierze z niego nazwę i kolor. */
const AGENT = {
  schema: 1,
  id: 'agent-board-1',
  name: 'ai-systems-architect',
  summary: 'Draws the shape of the system',
  color: 'moss',
  skills: [],
};

/** Krótka nazwa kroku — DOKŁADNIE ten przypadek, który padł: „Reaserch…" na zrzucie. */
const SHORT_NAME = 'Research';

function agentStep(id: string, name: string, y: number): Record<string, unknown> {
  return {
    kind: 'agent',
    id,
    name,
    agent: AGENT.id,
    overrides: {},
    copies: 1,
    instructions: 'Read what came before and write the next piece.',
    skills: 'all',
    folder: { use: 'project' },
    handover: 'notes',
    at: { x: 24, y },
  };
}

const WORKFLOW = {
  path: 'board.json',
  workflow: {
    format: 1,
    id: 'wf-canvas-board',
    name: 'Board',
    steps: [
      agentStep('s_research', SHORT_NAME, 24),
      agentStep('s_write', 'Write it up', 168),
      agentStep('s_judge', 'Say whether it works', 312),
      /* Kafelek z NAJDŁUŻSZYM podpisem rodzaju, jaki ten produkt ma, i z długą nazwą obok —
         czyli najciaśniejszy wiersz, jaki da się na kafelku zbudować. Podpis rodzaju jest
         jedyną rzeczą, po której z płótna widać różnicę między „zostaw uruchomione"
         a „sprawdź", więc to on nie ma prawa się urwać. */
      {
        kind: 'serve',
        id: 's_serve',
        name: 'Start and leave running',
        command: 'npm run dev',
        folder: { use: 'project' },
        at: { x: 24, y: 456 },
      },
    ],
    /* Trzy strzałki: dwie zwykłe i jeden POWRÓT — ta ostatnia była najmniej widoczna z całej
       trójki, bo arkusz maluje ją osobno i słabiej. */
    links: [
      { from: 's_research', to: 's_write' },
      { from: 's_write', to: 's_judge' },
      { from: 's_judge', to: 's_write', max_turns: 3 },
    ],
  },
};

const SCENE: Readonly<Record<string, readonly TauriReply[]>> = {
  list_workflows: Array.from({ length: 12 }, () => ({ value: [WORKFLOW] })),
  load_workflow: Array.from({ length: 6 }, () => ({
    value: { workflow: WORKFLOW.workflow, revision: 'r1' },
  })),
  list_agents: Array.from({ length: 8 }, () => ({ value: [AGENT] })),
  list_skills: Array.from({ length: 8 }, () => ({ value: [] })),
  check_workflow: Array.from({ length: 12 }, () => ({ value: [] })),
};

const SWITCH = '[data-section-switch="workflows"]';
const SCREEN = 'main[data-section="workflows"]';
const LIST_TILE = 'main [data-tile]';
const CANVAS_TILE = 'main [data-step]';
const STEP_EDITOR = 'main [data-step-editor]';
/** Każda kontrolka stojąca na płótnie. Dziś jest ich sześć, ma być dwie. */
const CONTROLS = 'main [data-canvas-area] button';
/** Jedno wejście do stawiania rzeczy. Otwiera listę, nie stawia niczego samo. */
const ADD = 'main [data-add-open]';
/** Lista, która spod niego wychodzi. */
const ADD_LIST = 'main [data-add-list]';

const APPEARS = 6_000;
/** Zapas na w pełni obciążoną maszynę — cała suita `e2e/` podnosi do ośmiu przeglądarek naraz. */
const BUSY_MACHINE = 30_000;

/** Pięć zdań listy — każde mówi, CO POWSTANIE, nie jak nazywa się wariant w kodzie. */
const CHOICES = [
  'A step an agent does',
  'A step that leaves a command running',
  'A step that runs a check',
  'A step that asks you',
  'A way back, to try again',
];

/** Trzy nagłówki: po co człowiek sięga, a nie jakiego kształtu jest to, co powstanie. */
const GOALS = ['Getting work done', 'Checking the work', 'When a check says no'];

beforeAll(async () => {
  const warm = await openApp();
  await warm.close();
}, 180_000);

afterAll(async () => {
  await closeEverything();
}, 30_000);

async function openTheEditor(page: Page): Promise<void> {
  await page.click(SWITCH);
  await page.locator(SCREEN).waitFor({ state: 'attached', timeout: APPEARS });
  await page.locator(LIST_TILE).first().click();
  /* CZEKAMY NA WSZYSTKIE TRZY, bez połykania limitu czasu i z zapasem. Zmierzone 2026-08-31
     w pełnym biegu suity: przy ośmiu workerach, z których każdy podnosi własną przeglądarkę,
     płótno rysowało pierwszy kafelek po ponad sześciu sekundach — a asercja postawiona na
     policzonych zero kafelkach mówi wtedy o obciążeniu maszyny, nie o kodzie (niezmiennik 19). */
  await page.locator(CANVAS_TILE).nth(3).waitFor({ state: 'attached', timeout: BUSY_MACHINE });
}

/** Jasność względna wg WCAG 2.1, z kanałów 0–255. */
function luminance(colour: readonly number[]): number {
  const light = (raw: number): number => {
    const channel = raw / 255;
    return channel <= 0.04045 ? channel / 12.92 : Math.pow((channel + 0.055) / 1.055, 2.4);
  };
  return (
    0.2126 * light(colour[0] ?? 0) + 0.7152 * light(colour[1] ?? 0) + 0.0722 * light(colour[2] ?? 0)
  );
}

/** `rgb(a, b, c)` albo `rgba(a, b, c, alfa)` przeglądarki na cztery liczby. */
function channels(computed: string): readonly number[] {
  const found = computed.match(/-?[\d.]+/g) ?? [];
  return [
    Number(found[0] ?? 0),
    Number(found[1] ?? 0),
    Number(found[2] ?? 0),
    Number(found[3] ?? 1),
  ];
}

/** Farba położona NA tym, co już leży pod spodem. To jest to, co widzi oko. */
function over(paint: readonly number[], under: readonly number[]): readonly number[] {
  const alpha = paint[3] ?? 1;
  return [0, 1, 2].map((at) => (paint[at] ?? 0) * alpha + (under[at] ?? 0) * (1 - alpha));
}

/** Stos teł od okna w górę, sklejony w jeden kolor kryjący. */
function boardColour(stack: readonly string[]): readonly number[] {
  let under = [0, 0, 0];
  for (const layer of [...stack].reverse()) under = [...over(channels(layer), under)];
  return under;
}

/** Kontrast wg WCAG, zaokrąglony do dwóch miejsc — liczba, którą da się zacytować w zgłoszeniu. */
function contrast(ink: string, stack: readonly string[]): number {
  const under = boardColour(stack);
  const one = luminance(over(channels(ink), under));
  const two = luminance(under);
  const ratio = (Math.max(one, two) + 0.05) / (Math.min(one, two) + 0.05);
  return Math.round(ratio * 100) / 100;
}

interface DrawnArrow {
  readonly stroke: string;
  readonly width: number;
  /** Tła wszystkich przodków, od strzałki w górę — bo powierzchnie tej aplikacji są półprzezroczyste. */
  readonly under: readonly string[];
}

/** Farba, grubość i tło POD nią, odczytane z żywego arkusza — nie z nazwy tokenu. */
async function arrowPaint(page: Page, selector: string): Promise<DrawnArrow | null> {
  return page.evaluate((which: string): DrawnArrow | null => {
    const path = document.querySelector(which);
    if (path === null) return null;
    const line = window.getComputedStyle(path);
    const under: string[] = [];
    for (let walk = path.parentElement; walk !== null; walk = walk.parentElement) {
      under.push(window.getComputedStyle(walk).backgroundColor);
    }
    return { stroke: line.stroke, width: Number.parseFloat(line.strokeWidth), under };
  }, selector);
}

const NOTHING_DRAWN: DrawnArrow = { stroke: 'rgb(0, 0, 0)', width: 0, under: ['rgb(0, 0, 0)'] };

describe('the canvas reads as a board, not as a palette of language keywords', () => {
  it('offers one ＋ Add whose list is grouped by what a person wants, and lands the tile', async () => {
    const app = await openApp({ replies: SCENE });
    try {
      const page = app.page;
      await openTheEditor(page);

      const before = await page.locator(CANVAS_TILE).count();
      expect(
        before,
        'the canvas drew no tile, so every measurement below would be about an empty board.',
      ).toBe(4);

      const controls = (await page.locator(CONTROLS).allInnerTexts()).map((one) =>
        one.replace(/\s+/g, ' ').trim(),
      );
      expect(
        controls,
        'the board still offers six same-weight buttons. Four of them put a tile down, one ' +
          'draws an arrow and one rearranges the whole board — three different jobs wearing ' +
          'one family name, and a person has to read all six to find the one they came for.',
      ).toEqual(['＋ Add', 'Rearrange every step']);

      const quiet = await page.locator(SCREEN).innerText();
      for (const said of CHOICES) {
        expect(
          quiet,
          'the list is spelled out on the board before anybody asked for it, so the click ' +
            'below would prove nothing about the control.',
        ).not.toContain(said);
      }

      await page.locator(ADD).click();
      await page
        .locator(ADD_LIST)
        .waitFor({ state: 'visible', timeout: APPEARS })
        .catch(() => undefined);

      const listed = (await page.locator(ADD_LIST).innerText()).replace(/\s+/g, ' ');
      for (const said of CHOICES) {
        expect(
          listed,
          'pressing ＋ Add showed no list, or the list does not say what each pick will make. ' +
            'A name that says the variant instead of the thing leaves a person to press one ' +
            'and find out.',
        ).toContain(said);
      }
      for (const goal of GOALS) {
        expect(
          listed,
          'the list is not grouped by what a person is trying to do, so it is a list of ' +
            'shapes and the reader has to already know which shape they need.',
        ).toContain(goal);
      }
      expect(
        listed.indexOf(GOALS[0] ?? ''),
        'the first heading does not stand above the picks it covers.',
      ).toBeLessThan(listed.indexOf(CHOICES[0] ?? ''));
      expect(
        listed.indexOf(GOALS[2] ?? ''),
        'the way back is listed under a heading that does not say when a person reaches for it.',
      ).toBeLessThan(listed.indexOf(CHOICES[4] ?? ''));

      /* Klikamy ZDANIE, nie znacznik: to je czyta i w nie celuje człowiek. */
      await page
        .locator(ADD_LIST)
        .getByText(CHOICES[2] ?? '')
        .click();
      await page
        .locator(CANVAS_TILE)
        .nth(before)
        .waitFor({ state: 'attached', timeout: APPEARS })
        .catch(() => undefined);

      expect(
        await page.locator(CANVAS_TILE).count(),
        'picking from the list put nothing on the board. A control that does nothing under ' +
          'the pointer is the state this repo has shipped three times (invariant 16).',
      ).toBe(before + 1);
      expect(
        await page.locator(ADD_LIST).count(),
        'the list stayed open over the board after the pick, so it now covers the very tile ' +
          'it just made.',
      ).toBe(0);
      expect(
        await page.locator(STEP_EDITOR).count(),
        'the fresh tile has nothing on it yet and no panel opened on it, so a person is left ' +
          'with a blank card and has to guess that it wants a click.',
      ).toBe(1);
    } finally {
      await app.close();
    }
  }, 90_000);

  it('shows the whole step name on the tile, and lets the agent chip be the thing that yields', async () => {
    const app = await openApp({ replies: SCENE });
    try {
      const page = app.page;
      await openTheEditor(page);

      const measured = await page.evaluate(() => {
        const tile = document.querySelector('[data-step="s_research"]');
        if (tile === null) return null;
        const name = tile.querySelector('b');
        if (name === null) return null;
        return {
          said: name.textContent ?? '',
          wants: Math.round(name.scrollWidth),
          got: Math.round(name.clientWidth),
          whole: tile.textContent ?? '',
        };
      });

      expect(measured, 'the tile for that step is not on the board at all.').not.toBeNull();
      const name = measured ?? { said: '', wants: 0, got: 0, whole: '' };

      expect(
        name.got,
        `the tile clips its own name: "${name.said}" wants ${String(name.wants)} px and was ` +
          `given ${String(name.got)}. The name is the whole content of a tile and the chip ` +
          'beside it is metadata, so this is the wrong one of the two to cut — and there are ' +
          'metres of empty board on either side of it.',
      ).toBeGreaterThanOrEqual(name.wants);

      expect(
        name.whole,
        'the chip naming the agent is gone from the tile. Making room by deleting the ' +
          'metadata answers the complaint by removing the thing it was about.',
      ).toContain('ai-systems');

      /* DRUGA POŁOWA TEJ SAMEJ GRANICY. Nazwa dostaje nie mniej niż połowę wiersza, więc to,
         co stoi obok, dostaje nie więcej — a najdłuższy podpis rodzaju w tym produkcie mieści
         się w tej połowie z zapasem. Kryterium pilnuje, żeby tak zostało: podpis, który się
         urywa, kasuje jedyną rzecz odróżniającą ten kafelek od kafelka „sprawdź". */
      const caption = await page.evaluate(() => {
        const tile = document.querySelector('[data-step="s_serve"]');
        const said = tile?.querySelector('span.label');
        if (said === null || said === undefined) return null;
        return { said: said.textContent ?? '', wants: said.scrollWidth, got: said.clientWidth };
      });
      expect(
        caption,
        'the tile that leaves a command running says nothing about doing so.',
      ).not.toBeNull();
      const kind = caption ?? { said: '', wants: 1, got: 0 };
      expect(
        kind.got,
        `the tile clips the caption that says what kind of step it is: "${kind.said}" wants ` +
          `${String(kind.wants)} px and was given ${String(kind.got)}. That caption is the ` +
          'only thing on the board that tells this tile apart from one that runs a check and ' +
          'waits, and the two differ by the single thing that makes a checking loop mean ' +
          'anything.',
      ).toBeGreaterThanOrEqual(kind.wants);
    } finally {
      await app.close();
    }
  }, 90_000);

  it('draws arrows a person can actually see against the board', async () => {
    const app = await openApp({ replies: SCENE });
    try {
      const page = app.page;
      await openTheEditor(page);
      await page
        .locator('.react-flow__edge-path')
        .first()
        .waitFor({ state: 'attached', timeout: APPEARS })
        .catch(() => undefined);

      const plain = await arrowPaint(
        page,
        '.react-flow__edge:not(.loadout-way-back) .react-flow__edge-path',
      );
      expect(
        plain,
        'the board drew no arrow at all, so there is nothing to measure.',
      ).not.toBeNull();
      const drawn = plain ?? NOTHING_DRAWN;
      const seen = contrast(drawn.stroke, drawn.under);

      expect(
        seen,
        `an arrow is painted ${drawn.stroke} on ${String(boardColour(drawn.under))}, which is ` +
          `${String(seen)}:1. Arrows are the only thing this board says over and above a list ` +
          'of steps, and at nine steps a person cannot tell what follows from what. 3:1 is ' +
          'the floor for graphics that carry meaning (WCAG 1.4.11).',
      ).toBeGreaterThanOrEqual(3);

      expect(
        drawn.width,
        `an arrow is ${String(drawn.width)} px thick — the hairline the library ships by ` +
          'default. A hairline at this contrast disappears into the grid of dots behind it.',
      ).toBeGreaterThan(1);

      const back = await arrowPaint(page, '.loadout-way-back .react-flow__edge-path');
      expect(back, 'the way back was not drawn, so its paint cannot be measured.').not.toBeNull();
      const wayBack = back ?? NOTHING_DRAWN;
      const backSeen = contrast(wayBack.stroke, wayBack.under);

      expect(
        backSeen,
        `the way back is painted ${wayBack.stroke}, which is ${String(backSeen)}:1 — drawn ` +
          'fainter than a plain arrow on purpose, and so faint it is not drawn at all. ' +
          'Something meant to be quieter still has to be there.',
      ).toBeGreaterThanOrEqual(3);

      expect(
        wayBack.width,
        'the way back is as thick as a plain arrow. It means something else than "next", so ' +
          'it has to read as something else — and dashes alone carry that only until two of ' +
          'them cross.',
      ).toBeLessThan(drawn.width);
    } finally {
      await app.close();
    }
  }, 90_000);
});
