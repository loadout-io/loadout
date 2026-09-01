/* Lista pozycji importu nie ma prawa zniknąć, kiedy skan znajdzie ich dużo.
 *
 * ZGŁOSZENIE WŁAŚCICIELA, 2026-08-29 (zrzut ekranu). Skan `~/Projects/meetnotes` znalazł
 * 86 pozycji. Na ekranie zostały cztery liczniki, trzy przyciski filtra, długa lista ścieżek
 * i zdanie „37 item(s) will not be imported." — a TABELI POZYCJI nie było wcale. Między
 * filtrem a nagłówkiem „Proposed files" stała sama kreska: ramka kontenera ściśniętego
 * do zera pikseli. Skutek dla człowieka jest podwójny i oba są ciężkie: filtr steruje czymś,
 * czego nie widać, a tych 37 pominiętych pozycji nie da się ani obejrzeć, ani odznaczyć.
 *
 * DLACZEGO TO MUSI BYĆ PRAWDZIWA PRZEGLĄDARKA. Wada jest w układzie, nie w danych: okno modala
 * jest kolumną fleksa z `max-h-full`, a kontener tabeli był JEDYNYM dzieckiem, które pozwalało
 * się ścisnąć poniżej własnej treści. Rosnąca lista „Proposed files" zabierała mu całą
 * wysokość. Ani czysty moduł, ani `renderToStaticMarkup` nie liczą układu — obie te drogi
 * dałyby zielone na kodzie, na którym właściciel widział pusty ekran. Chromium liczy.
 *
 * CZEGO TO NIE DOWODZI. Rust zostaje na granicy harnessu: `scan_setup` odpowiada kształtem
 * planu, dokładnie w liczbach ze zrzutu. Pytanie brzmi „czy człowiek zobaczy to, co skan
 * znalazł", a nie „czy skan liczy dobrze" — to drugie sądzą kryteria po stronie Rusta.
 */
import type { Page } from '@playwright/test';
import { afterAll, beforeAll, describe, expect, it } from 'vitest';

import type { ImportItem, ImportPreview, ItemKind } from '../../src/sections/import/setup';
import type { TauriReply } from '../harness';
import { closeEverything, openApp } from '../harness';

const FOLDER = '/Users/somebody/Projects/meetnotes';
const WORKSPACE = { id: FOLDER, name: 'Meetnotes', folder: FOLDER };

/** Liczby ze zrzutu właściciela: tyle skan wniesie, po rodzajach. */
const BRINGS: readonly (readonly [ItemKind, number])[] = [
  ['skill', 17],
  ['agent', 11],
  ['memory', 16],
  ['workflow', 1],
  ['connection', 4],
];
/** I tyle zostaje poza importem, bo nie są gotowe. Zdanie w stopce mówi dokładnie tę liczbę. */
const LEFT_OUT = 37;
const BROUGHT = BRINGS.reduce((sum, [, count]) => sum + count, 0);

function ready(kind: ItemKind, at: number): ImportItem {
  const name = `${kind}-${String(at).padStart(2, '0')}`;
  return {
    id: `ready-${name}`,
    kind,
    sources: [
      {
        provider: 'claude',
        path: `.claude/${kind}s/${name}.md`,
        hash: `h-${name}`,
        role: 'definition',
      },
    ],
    target: `${kind}s/${name}/FILE.md`,
    dependencies: [],
    status: 'ready',
    statusMessage: 'Loadout can bring this over as it is.',
    generatedHash: null,
  };
}

function held(at: number): ImportItem {
  const name = `held-${String(at).padStart(2, '0')}`;
  return {
    id: name,
    kind: at % 2 === 0 ? 'skill' : 'agent',
    sources: [
      { provider: 'codex', path: `.codex/${name}.toml`, hash: `h-${name}`, role: 'definition' },
    ],
    target: null,
    dependencies: [],
    status: at % 2 === 0 ? 'unsupported' : 'needs_choice',
    statusMessage: 'Loadout has nothing that behaves like this.',
    generatedHash: null,
  };
}

const ITEMS: readonly ImportItem[] = [
  ...BRINGS.flatMap(([kind, count]) => Array.from({ length: count }, (_, at) => ready(kind, at))),
  ...Array.from({ length: LEFT_OUT }, (_, at) => held(at)),
];

const PREVIEW: ImportPreview = {
  snapshot: {
    root: FOLDER,
    items: ITEMS.map((item) => ({
      id: item.id,
      kind: item.kind,
      path: item.sources[0]?.path ?? '',
      name: item.id,
      summary: 'Found in this project.',
    })),
  },
  draft: {
    sourceHashes: Object.fromEntries(ITEMS.map((item) => [item.id, `h-${item.id}`])),
    items: [...ITEMS],
    agents: ITEMS.filter((item) => item.kind === 'agent').map((item) => ({
      id: item.id,
      name: item.id,
    })),
    skills: ITEMS.filter((item) => item.kind === 'skill').map((item) => ({ name: item.id })),
    connections: [],
    workflows: [],
    report: { mappings: [] },
  },
};

const SCENE: Readonly<Record<string, readonly TauriReply[]>> = {
  list_workspaces: Array.from({ length: 12 }, () => ({ value: [WORKSPACE] })),
  scan_setup: Array.from({ length: 4 }, () => ({ value: PREVIEW })),
};

const SWITCH = '[data-section-switch="agents"]';
const SCREEN = 'main[data-section="agents"]';
const OPEN = `${SCREEN} button:has-text("Import setup")`;
const DIALOG = '[role="dialog"]';
const SCAN = `${DIALOG} button:has-text("Scan")`;
const ITEM_LIST = `${DIALOG} [data-import-items]`;
const ROW = `${ITEM_LIST} tbody tr`;
const INCLUDE = 'input[aria-label="Import this item"]';
const NEEDS_ATTENTION = `${DIALOG} button:has-text("Needs attention")`;
/* Przycisk, który kończy całą tę robotę. `data-` zamiast tekstu, bo słowo „Import" stoi
 * na tym ekranie także w tytule i w ptaszku każdego wiersza. */
const BRING = `${DIALOG} [data-import-now]`;
const APPEARS = 8_000;

interface Box {
  readonly top: number;
  readonly bottom: number;
  readonly height: number;
}

interface Measured {
  readonly viewportHeight: number;
  readonly listClientHeight: number;
  readonly listScrollHeight: number;
  readonly list: Box;
  readonly firstRow: Box;
  readonly dialogBottom: number;
  readonly bringBottom: number;
}

async function measure(page: Page): Promise<Measured> {
  return page.evaluate(
    ({ listSelector, rowSelector, dialogSelector, bringSelector }) => {
      function required(selector: string): HTMLElement {
        const found = document.querySelector<HTMLElement>(selector);
        if (found === null) throw new Error(`the open window is missing ${selector}`);
        return found;
      }
      function box(element: Element): Box {
        const rectangle = element.getBoundingClientRect();
        return { top: rectangle.top, bottom: rectangle.bottom, height: rectangle.height };
      }
      const list = required(listSelector);
      const firstRow = required(rowSelector);
      return {
        viewportHeight: window.innerHeight,
        listClientHeight: list.clientHeight,
        listScrollHeight: list.scrollHeight,
        list: box(list),
        firstRow: box(firstRow),
        dialogBottom: box(required(dialogSelector)).bottom,
        bringBottom: box(required(bringSelector)).bottom,
      };
    },
    { listSelector: ITEM_LIST, rowSelector: ROW, dialogSelector: DIALOG, bringSelector: BRING },
  );
}

/** Jedno kryterium, wypowiedziane o tym, co widzi człowiek, a nie o klasach CSS. */
function expectFirstItemOnScreen(measured: Measured, label: string): void {
  const slack = 1;
  expect(
    measured.listClientHeight,
    `${label}: the list of found items was flattened to nothing, so no item can be read or unticked`,
  ).toBeGreaterThan(0);
  expect(
    measured.firstRow.height,
    `${label}: the first found item has no height of its own`,
  ).toBeGreaterThan(0);
  expect(
    measured.listClientHeight,
    `${label}: the list is shorter than the first item standing in it`,
  ).toBeGreaterThanOrEqual(measured.firstRow.height);
  expect(
    measured.firstRow.bottom,
    `${label}: the first found item is cut off by the bottom of its own list`,
  ).toBeLessThanOrEqual(measured.list.bottom + slack);
  expect(
    measured.firstRow.top,
    `${label}: the first found item sits above its own list`,
  ).toBeGreaterThanOrEqual(measured.list.top - slack);
  expect(
    measured.list.bottom,
    `${label}: the list runs off the bottom of the screen`,
  ).toBeLessThanOrEqual(measured.viewportHeight + slack);
  expect(
    measured.listScrollHeight,
    `${label}: 86 found items should overflow the list and make it scroll`,
  ).toBeGreaterThan(measured.listClientHeight);
  /* Lista, która wypchnęła przycisk pod krawędź ekranu, jest tą samą wadą co lista ściśnięta
   * do zera: człowiek stoi nad gotowym planem i nie ma czym go zatwierdzić. */
  expect(
    measured.bringBottom,
    `${label}: the button that brings everything over was pushed off the bottom of the screen`,
  ).toBeLessThanOrEqual(measured.viewportHeight + slack);
  expect(
    measured.bringBottom,
    `${label}: the button that brings everything over has no place on the screen`,
  ).toBeGreaterThan(0);
}

beforeAll(async () => {
  const warm = await openApp();
  await warm.close();
}, 180_000);

afterAll(async () => {
  await closeEverything();
}, 30_000);

describe('a big scan still shows the person every item it found', () => {
  it('keeps the item list on screen, tickable and scrollable, at both supported sizes', async () => {
    const app = await openApp({ replies: SCENE });
    try {
      const page = app.page;
      await page.setViewportSize({ width: 1280, height: 720 });
      await page.click(SWITCH);
      await page.locator(SCREEN).waitFor({ state: 'attached', timeout: APPEARS });
      await page.click(OPEN);
      await page.locator(DIALOG).waitFor({ state: 'visible', timeout: APPEARS });
      await page.click(SCAN);
      await page.locator(ROW).first().waitFor({ state: 'attached', timeout: APPEARS });

      /* Kontrola przeciw pustej asercji: mierzymy ekran, na którym skan NAPRAWDĘ coś znalazł
       * i naprawdę coś pominął — inaczej „widać pierwszą pozycję" byłoby zdaniem o niczym. */
      expect(await page.locator(ROW).count(), 'the scan put no item on the screen').toBe(
        BROUGHT + LEFT_OUT,
      );
      expect(
        await page.locator(`${DIALOG} p:has-text("will not be imported")`).innerText(),
        'the window stopped saying how many items stay out',
      ).toContain(`${String(LEFT_OUT)} item(s) will not be imported.`);

      for (const size of [
        { width: 1280, height: 720 },
        { width: 1440, height: 900 },
      ]) {
        await page.setViewportSize(size);
        /* Próbne kliknięcie samo przewija do celu, więc bez tego drugi rozmiar mierzyłby okno
         * przewinięte przez pierwszy — i „przycisk jest na ekranie" znaczyłoby co innego. */
        await page.locator(DIALOG).evaluate((window_) => {
          window_.scrollTop = 0;
        });
        await page.waitForTimeout(50);
        const label = `${String(size.width)}x${String(size.height)}`;
        expectFirstItemOnScreen(await measure(page), label);

        /* Widać to za mało: ptaszek ma dać się nacisnąć. Próbne kliknięcie Playwrighta pyta
         * o to, czy trafi w niego kursor, i nie zmienia stanu okna pod następnym rozmiarem. */
        const tick = page.locator(`${ROW} ${INCLUDE}`).first();
        await tick.click({ trial: true, timeout: APPEARS });

        /* Filtr, który steruje niewidoczną listą, jest kontrolką bez skutku (niezmiennik 16).
         * Po nim na ekranie mają stać wyłącznie pominięte pozycje — i mają być widoczne. */
        await page.click(NEEDS_ATTENTION);
        expect(
          await page.locator(ROW).count(),
          `${label}: asking for the items that need attention showed a different set`,
        ).toBe(LEFT_OUT);
        expectFirstItemOnScreen(await measure(page), `${label} needing attention`);
        await page.click(`${DIALOG} button:has-text("All")`);
        await page.locator(ROW).nth(BROUGHT).waitFor({ state: 'attached', timeout: APPEARS });

        /* A przycisk, który kończy całą tę robotę, ma dać się nacisnąć tam, gdzie stoi. */
        await page.locator(BRING).click({ trial: true, timeout: APPEARS });
      }
    } finally {
      await app.close();
    }
  }, 90_000);
});
