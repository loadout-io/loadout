/* Kafelek „sprawdź" staje na płótnie po PRAWDZIWYM kliknięciu, i po drugim kliknięciu daje się
 * wypełnić.
 *
 * PO CO TO, skoro trzy kryteria obok mierzą te same funkcje. Bo one mierzą FUNKCJE. Ten plik
 * niczego nie renderuje ręcznie i nie woła ani jednej funkcji stanu: otwiera aplikację, wchodzi
 * do sekcji przełącznikiem, w który klika człowiek, otwiera workflow z listy i naciska przycisk
 * płótna. Sześć razy 2026-08-16 kryterium było zielone, a produkt nie działał — mechanizm
 * istniał i nikt go nie zamontował. Tutaj jedynym wejściem jest kliknięcie.
 *
 * TRZY KONTROLE PRZECIW PUSTEJ ASERCJI, bo bez nich „pole komendy jest na ekranie" przechodzi
 * na ekranie, który je rysuje zawsze:
 *   przed kliknięciem tego pola NIE MA nigdzie w dokumencie;
 *   płótno naprawdę rysuje kafelki, zanim czegokolwiek szukamy — inaczej „doszedł jeden" byłoby
 *     zdaniem o pustym płótnie;
 *   między postawieniem kafelka a kliknięciem w niego zaglądamy do SĄSIEDNIEGO kafelka, żeby
 *     panel odjechał. Bez tego ostatnie kliknięcie nie mierzy niczego: przycisk dodawania sam
 *     otwiera panel nowego kafelka, więc pole byłoby na ekranie jeszcze przed nim.
 *
 * GRANICA ODPOWIADA KSZTAŁTEM, nie treścią (nagłówek `../harness.ts`). `check_workflow` dostaje
 * własną odpowiedź, bo domyślne `null` jest tu awarią PRZYRZĄDU, a nie produktu: magazyn wpisuje
 * je w `notes`, a ekran czyta z nich `length` i cała sekcja ginie na `null`.
 */
import { afterAll, beforeAll, describe, expect, it } from 'vitest';
import type { Page } from '@playwright/test';

import type { TauriReply } from '../harness';
import { closeEverything, openApp } from '../harness';

/** Workflow z jednym krokiem: płótno ma wtedy co narysować, zanim cokolwiek dołożymy. */
const WORKFLOW = {
  path: 'ship.json',
  workflow: {
    format: 1,
    id: 'wf-check-tile',
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
    ],
    links: [],
  },
};

/* Kolejki są zużywalne (`shift()`), a i katalog, i walidator odpowiadają po kilka razy: przy
 * wejściu na sekcję, po zamknięciu edytora i po każdym zapisie. Karta, która wyczerpie kolejkę,
 * dostaje domyślną odpowiedź atrapy — dla `check_workflow` jest nią `null`, czyli awaria
 * przyrządu przebrana za wadę produktu. */
const SCENE: Readonly<Record<string, readonly TauriReply[]>> = {
  list_workflows: Array.from({ length: 12 }, () => ({ value: [WORKFLOW] })),
  /* 2026-08-28: otwarcie oddaje plik RAZEM z rewizją, na której okno go czyta — bez niej
   * zapis nie ma czego porównać z dyskiem (`commands::workflows::OpenWorkflow`). */
  load_workflow: Array.from({ length: 4 }, () => ({
    value: { workflow: WORKFLOW.workflow, revision: 'r1' },
  })),
  check_workflow: Array.from({ length: 12 }, () => ({ value: [] })),
};

const SWITCH = '[data-section-switch="workflows"]';
const SCREEN = 'main[data-section="workflows"]';
const LIST_TILE = 'main [data-tile]';
/** Kafelek na płótnie. Ten sam znacznik nosi każdy rodzaj kroku. */
const CANVAS_TILE = 'main [data-step]';
/** Pole komendy w panelu sprawdzenia — jedyna rzecz, dla której ten kafelek istnieje. */
const COMMAND_FIELD = 'main #check-command';
/** Zdanie, którym ekran odpowiada, kiedy NIC nie jest zaznaczone. */
const NOTHING_PICKED = 'Pick a step to see what it was given.';

/** Ile czekamy na to, co ma przyjść po kliknięciu. Odpowiedź wraca w tej samej karcie. */
const APPEARS = 6_000;

/** Identyfikatory kafelków stojących teraz na płótnie. */
async function tilesOn(page: Page): Promise<string[]> {
  return page
    .locator(CANVAS_TILE)
    .evaluateAll((cards) => cards.map((card) => card.getAttribute('data-step') ?? ''));
}

/* Rozruch vite i chromium jest kosztem STAŁYM NA PLIK, nie częścią pierwszego przypadku:
 * `openApp()` jest leniwy, więc bez tego haka pierwszy `it` płaci cały rozruch pod swoim
 * limitem i pada na nim, mierząc start narzędzia zamiast produktu. */
beforeAll(async () => {
  const warm = await openApp();
  await warm.close();
}, 180_000);

afterAll(async () => {
  await closeEverything();
}, 30_000);

describe('a person puts a check on the canvas with one click and fills it in with another', () => {
  it('draws the new tile, says what it is, and opens its command field when clicked', async () => {
    const app = await openApp({ replies: SCENE });
    try {
      const page = app.page;

      await page.click(SWITCH);
      await page.locator(SCREEN).waitFor({ state: 'attached', timeout: APPEARS });
      await page.locator(LIST_TILE).first().click();

      /* ── scena jest prawdziwym płótnem, a nie ekranem w połowie wczytanym ───────────────── */
      await page
        .locator(CANVAS_TILE)
        .first()
        .waitFor({ state: 'visible', timeout: APPEARS })
        .catch(() => undefined);
      const before = await tilesOn(page);
      expect(
        before,
        'the canvas drew no tile for the workflow that was opened, so everything below would be ' +
          'measured on an empty board and "one more tile" would mean nothing.',
      ).toEqual(['s_build']);
      expect(
        await page.locator(COMMAND_FIELD).count(),
        'the field for the command is on the screen before anything was added, so its presence ' +
          'afterwards would say nothing about the click.',
      ).toBe(0);

      /* ── jedyna kontrolka, która stawia sprawdzenie ────────────────────────────────────── */
      const add = page.getByRole('button', { name: /run a check/i });
      expect(
        await add.count(),
        'the canvas offers no way to put a check on it. Rust has had this kind of step in full ' +
          'since T-23 and the canvas already draws one when a file arrives carrying it — but ' +
          'nothing here puts one down, so every loop a person builds by hand is a loop about ' +
          'what an agent said.',
      ).toBe(1);

      await add.click();

      /* ── co zobaczył człowiek ──────────────────────────────────────────────────────────── */
      await page
        .locator(CANVAS_TILE)
        .nth(before.length)
        .waitFor({ state: 'visible', timeout: APPEARS })
        .catch(() => undefined);
      const after = await tilesOn(page);
      const fresh = after.filter((id) => !before.includes(id));
      expect(
        fresh.length,
        'the click put down no new tile. A button that reaches the document and draws nothing ' +
          'is the state a person calls broken, and it is the state this repo has shipped before.',
      ).toBe(1);

      const card = page.locator(`main [data-step="${fresh[0] ?? ''}"]`);
      const said = (await card.innerText()).replace(/\s+/g, ' ');
      expect(
        said.toLowerCase(),
        'the new tile does not say on its face that it is a check. Two kinds of tile carry a ' +
          'shell command, and they differ in the one thing that matters: this one waits for the ' +
          'command and reads the answer out of its output. Told apart only by opening them, they ' +
          'are told apart by nobody. The tile reads: ' +
          said,
      ).toContain('check');

      /* Zaglądamy do sąsiada, żeby panel odjechał: przycisk dodawania sam otwiera panel nowego
       * kafelka, więc bez tego kliknięcie niżej nie mierzyłoby niczego. */
      await page.locator('main [data-step="s_build"]').click();
      await page
        .locator(COMMAND_FIELD)
        .waitFor({ state: 'detached', timeout: APPEARS })
        .catch(() => undefined);

      await card.click();
      await page
        .locator(COMMAND_FIELD)
        .waitFor({ state: 'visible', timeout: APPEARS })
        .catch(() => undefined);

      const screen = await page.locator('main').innerText();
      expect(
        screen,
        'clicking the new tile answered with the sentence the screen keeps for "nothing is ' +
          'picked". A tile that can be put down and never set up is exactly the defect this ' +
          'repo has already shipped three times.',
      ).not.toContain(NOTHING_PICKED);
      expect(
        await page.locator(COMMAND_FIELD).count(),
        'the panel for this tile carries no field for the command, so the one thing it does ' +
          'cannot be typed in anywhere. The run then refuses on an empty command, after the ' +
          'person has waited out every step before it.',
      ).toBe(1);
    } finally {
      await app.close();
    }
  }, 90_000);
});
