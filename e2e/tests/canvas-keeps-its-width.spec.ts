/* Płótno dostaje CAŁE ciało ekranu i nie zmienia szerokości, kiedy panel kroku się otwiera —
 * zmierzone w pikselach, w prawdziwej przeglądarce.
 *
 * PO CO TO, skoro `src/sections/workflows/the-canvas-gets-the-whole-window.test.tsx` pyta
 * o to samo. Bo tamten plik pyta o MECHANIZM: czy panel jest poza układem, czy ramka płótna jest
 * opisana tymi samymi klasami z zaznaczonym krokiem i bez niego. `renderToStaticMarkup` nie zna
 * ani jednego piksela — w repo nie ma jsdom — więc tamto kryterium przechodzi także wtedy, gdy
 * arkusz stylów odbierze płótnu połowę okna w sposób, którego w klasach nie widać. Ten plik pyta
 * o SKUTEK: ile pikseli szerokości naprawdę dostał człowiek. Klasa wady, dla której to repo
 * powstało, mieszka dokładnie między jednym a drugim (niezmiennik 29), a wada, którą to kryterium
 * pilnuje, była wadą UKŁADU — czyli jedyną rodziną, jaką widać wyłącznie w chromium.
 *
 * CO TU BYŁO ŹLE, zgłoszenie właściciela 2026-08-31 ze zrzutu z okna 1512 px. Ciało ekranu stało
 * na `grid-cols-[minmax(0,1fr)_330px]`, czyli kolumna panelu była STAŁA. Bez zaznaczonego kroku
 * rysowała jedno zdanie i 300 px pustki pod nim, a płótno dostawało 974 px z 1304 dostępnych.
 * Piąta część okna szła na powierzchnię, na której nic nie stało.
 *
 * DRUGA POŁOWA JEST RÓWNIE WIĄŻĄCA i to ona broniła tamtej pustki: kolumna, która ZNIKA po
 * kliknięciu w kafelek, przesuwa płótno pod kursorem dokładnie w chwili, w której człowiek w nie
 * celuje. Kryterium, które żąda tylko szerokiego płótna, zamawia tamtą wadę z powrotem — więc
 * oba pomiary stoją tu w jednym `it` i na jednej karcie.
 *
 * TRZECIE `it` JEST O UWAGACH i należy do tej samej naprawy. Wraz z `Run` do nagłówka wyjechała
 * lista rzeczy do poprawienia, a plakietka „N things to fix" przestała być napisem i stała się
 * kontrolką. Kontrolka bez handlera nie wchodzi do repo (niezmiennik 16), a `renderToStaticMarkup`
 * nie odpala ani jednego `onClick` — więc jedynym miejscem, w którym da się tego dowieść, jest
 * prawdziwe kliknięcie. Sprawdzamy całą drogę: plakietka → zdanie uwagi → panel kroku, którego
 * uwaga dotyczy.
 *
 * GRANICA ODPOWIADA KSZTAŁTEM, nie treścią (nagłówek `../harness.ts`). `check_workflow` dostaje
 * własną odpowiedź, bo domyślne `null` jest tu awarią PRZYRZĄDU, a nie produktu: magazyn wpisuje
 * je w `notes`, a ekran czyta z nich `length`.
 */
import { afterAll, beforeAll, describe, expect, it } from 'vitest';
import type { Page } from '@playwright/test';

import type { TauriReply } from '../harness';
import { closeEverything, openApp } from '../harness';

/** Workflow z dwoma krokami: płótno ma co narysować, a walidator ma na czym postawić uwagę. */
const WORKFLOW = {
  path: 'ship.json',
  workflow: {
    format: 1,
    id: 'wf-canvas-width',
    name: 'Ship it',
    steps: [
      {
        kind: 'agent',
        id: 's_build',
        name: 'Build',
        agent: '',
        overrides: {},
        copies: 1,
        instructions: 'Write the smallest change that works.',
        skills: 'all',
        folder: { use: 'project' },
        handover: 'notes',
        at: { x: 24, y: 24 },
      },
      {
        kind: 'agent',
        id: 's_review',
        name: 'Review',
        agent: '',
        overrides: {},
        copies: 1,
        instructions: 'Read the change and say whether it works.',
        skills: 'all',
        folder: { use: 'project' },
        handover: 'notes',
        at: { x: 24, y: 168 },
      },
    ],
    links: [],
  },
};

/** Zdanie walidatora, słowo w słowo — tak, jak przyszłoby z Rusta. Waga `warning`, bo to ono
 * ma dojechać na ekran, a nie zgasić przycisk: gdyby gasiło, kryterium mierzyłoby dwie rzeczy. */
const NOTE_SAID = '"Review" is not connected to the rest of the workflow.';

const NOTES = [{ level: 'warning', stepId: 's_review', message: NOTE_SAID }];

/* Kolejki są zużywalne (`shift()`), a i katalog, i walidator odpowiadają po kilka razy: przy
 * wejściu na sekcję, po zamknięciu edytora i po każdym zapisie. */
const SCENE: Readonly<Record<string, readonly TauriReply[]>> = {
  list_workflows: Array.from({ length: 12 }, () => ({ value: [WORKFLOW] })),
  /* 2026-08-28: otwarcie oddaje plik RAZEM z rewizją, na której okno go czyta. */
  load_workflow: Array.from({ length: 4 }, () => ({
    value: { workflow: WORKFLOW.workflow, revision: 'r1' },
  })),
  check_workflow: Array.from({ length: 12 }, () => ({ value: NOTES })),
};

const SWITCH = '[data-section-switch="workflows"]';
const SCREEN = 'main[data-section="workflows"]';
const LIST_TILE = 'main [data-tile]';
/** Ciało ekranu pod nagłówkiem: cała szerokość, którą jest do czego porównać. */
const BODY = 'main [data-canvas-body]';
/** Ramka płótna. To jej szerokość jest odpowiedzią na „ile dostał człowiek". */
const CANVAS = 'main [data-canvas-area]';
/** Kafelek na płótnie. Ten sam znacznik nosi każdy rodzaj kroku. */
const CANVAS_TILE = 'main [data-step]';
/** Panel kroku — powierzchnia, która ma leżeć NAD płótnem, a nie obok niego. */
const STEP_EDITOR = 'main [data-step-editor]';
/** Plakietka w nagłówku: liczy rzeczy do poprawienia i otwiera ich listę. */
const THINGS_TO_FIX = 'main [data-things-to-fix]';
/** Lista zdań, która spod niej wychodzi. */
const THINGS_TO_FIX_LIST = 'main [data-things-to-fix-list]';

/** Ile czekamy na to, co ma przyjść po kliknięciu. Odpowiedź wraca w tej samej karcie. */
const APPEARS = 6_000;

/** Szerokość pudełka w pikselach urządzenia, zaokrąglona do całych — układ nie jest pomiarem
 * o dokładności subpikselowej, a `boundingBox` oddaje ułamki także tam, gdzie nic się nie rusza. */
async function widthOf(page: Page, selector: string): Promise<number> {
  const box = await page.locator(selector).first().boundingBox();
  return box === null ? -1 : Math.round(box.width);
}

/* Rozruch vite i chromium jest kosztem STAŁYM NA PLIK, nie częścią pierwszego przypadku. */
beforeAll(async () => {
  const warm = await openApp();
  await warm.close();
}, 180_000);

afterAll(async () => {
  await closeEverything();
}, 30_000);

/** Otwiera sekcję i wchodzi w pierwszy workflow z listy — droga człowieka, nie skrót testu. */
async function openTheEditor(page: Page): Promise<void> {
  await page.click(SWITCH);
  await page.locator(SCREEN).waitFor({ state: 'attached', timeout: APPEARS });
  await page.locator(LIST_TILE).first().click();
  await page
    .locator(CANVAS_TILE)
    .first()
    .waitFor({ state: 'visible', timeout: APPEARS })
    .catch(() => undefined);
}

describe('the editor hands the whole window to the canvas and never moves it', () => {
  it('gives the canvas every pixel of the body, and the same pixels once a step is picked', async () => {
    const app = await openApp({ replies: SCENE });
    try {
      const page = app.page;
      await openTheEditor(page);

      const tiles = await page.locator(CANVAS_TILE).count();
      expect(
        tiles,
        'the canvas drew no tile at all, so the click below would land on an empty board and ' +
          'every measurement here would be about a screen nobody could use.',
      ).toBeGreaterThan(0);

      const body = await widthOf(page, BODY);
      const alone = await widthOf(page, CANVAS);
      expect(
        body,
        'the body of the screen has no box, so there is nothing to compare the canvas against.',
      ).toBeGreaterThan(0);
      expect(
        alone,
        'with nothing picked the canvas is narrower than the room it was given. The missing ' +
          'strip is the side column, which was 330 px wide whatever stood in it — on the ' +
          "owner's 1512 px window that is a fifth of the screen spent on one sentence and 300 " +
          'px of nothing underneath it.',
      ).toBe(body);

      expect(
        await page.locator(STEP_EDITOR).count(),
        'the step editor is on the screen before anything was picked, so its appearance below ' +
          'would say nothing about the click.',
      ).toBe(0);

      await page.locator(CANVAS_TILE).first().click();
      await page
        .locator(STEP_EDITOR)
        .waitFor({ state: 'visible', timeout: APPEARS })
        .catch(() => undefined);
      expect(
        await page.locator(STEP_EDITOR).count(),
        'clicking a tile opened no step editor, so the second half of this measurement would ' +
          'compare the screen with itself.',
      ).toBe(1);

      expect(
        await widthOf(page, CANVAS),
        'the canvas changed width the moment a step was picked. That is the defect the empty ' +
          'column was holding at bay: everything on the board slides sideways under the ' +
          'pointer at the exact moment a person is aiming at it, and the tile they meant to ' +
          'press is no longer where they pressed.',
      ).toBe(alone);
    } finally {
      await app.close();
    }
  }, 90_000);

  it('reaches the sentences behind the count, and lands on the step they name', async () => {
    const app = await openApp({ replies: SCENE });
    try {
      const page = app.page;
      await openTheEditor(page);

      await page
        .locator(THINGS_TO_FIX)
        .waitFor({ state: 'visible', timeout: APPEARS })
        .catch(() => undefined);
      expect(
        await page.locator(THINGS_TO_FIX).count(),
        'the validator sent something back and the header says nothing about it, so a person ' +
          'has no sign at all that this workflow needs work before it runs.',
      ).toBe(1);

      const head = await page.locator(THINGS_TO_FIX).innerText();
      expect(
        head.replace(/\s+/g, ' ').trim(),
        'the count in the header does not name what it counts.',
      ).toContain('1 thing to fix');

      expect(
        await page.locator(SCREEN).innerText(),
        'the sentence is spelled out before anybody asked for it. Rolled out on its own it ' +
          'takes the top of the screen for as long as the workflow is unfinished, and that is ' +
          'most of the time a person spends drawing one.',
      ).not.toContain(NOTE_SAID);

      await page.locator(THINGS_TO_FIX).click();
      await page
        .locator(THINGS_TO_FIX_LIST)
        .waitFor({ state: 'visible', timeout: APPEARS })
        .catch(() => undefined);
      expect(
        await page.locator(THINGS_TO_FIX_LIST).innerText(),
        'pressing the count showed nothing. It is a control now, not a caption, and a control ' +
          'that does nothing under the pointer is the state this repo has shipped three times ' +
          '(invariant 16).',
      ).toContain(NOTE_SAID);

      /* Kliknięcie w SAMO ZDANIE — to jest ta droga, przez którą uwaga prowadzi do miejsca,
       * w którym da się ją spełnić. Bez niej lista jest listą skarg bez adresu. */
      await page.locator(THINGS_TO_FIX_LIST).getByText(NOTE_SAID).click();
      await page
        .locator(STEP_EDITOR)
        .waitFor({ state: 'visible', timeout: APPEARS })
        .catch(() => undefined);

      expect(
        await page.locator(`${STEP_EDITOR} #step-name`).inputValue(),
        'pressing the sentence did not open the step it names. The note says which tile is ' +
          'wrong and the only place to put that right is that tile: without this the person ' +
          'reads a complaint and then goes hunting for its subject by hand.',
      ).toBe('Review');
    } finally {
      await app.close();
    }
  }, 90_000);
});
