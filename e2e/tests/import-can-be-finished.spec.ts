/* Import da się dokończyć, i okno mówi, DOKĄD te pliki trafią.
 *
 * ZGŁOSZENIE WŁAŚCICIELA, 2026-09-15: zaznaczył sześć połączeń w `urc-monorepo`, nacisnął
 * Import kilka razy i „nic się nie stało i nic nie zostało powiedziane". Sprawdzone na dysku:
 * żaden plik nie powstał, więc import nie ruszył ani razu. Przyczyna nie jest w Ruście —
 * przycisk Import był WYŁĄCZONY, bo trzynaście cudzych umiejętności czekało na przeczytanie,
 * a wyłączony przycisk HTML nie wysyła zdarzenia kliknięcia. Strażnik w `apply()` nie miał
 * więc jak się wykonać: człowiek naciskał coś, co nie odpowiada w żaden sposób. Stopka
 * obiecywała dwie drogi („Read each one, or take it out of the import"), a druga z nich
 * istniała wyłącznie po jednej pozycji.
 *
 * DRUGA WADA, ta sama chwila. Import CZYTA folder z pola „Project folder", a ZAPISUJE do
 * biblioteki projektu aktywnego w oknie (`apply_setup` bierze osobny argument `folder`).
 * Aktywny projekt nie przeżywa restartu (`src/state/workspaces.ts`, `pick()` oddaje `all[0]`),
 * więc po świeżym starcie skan czyta `urc-monorepo`, a pliki lądują w `inne-i-zadania` — bez
 * ani jednego zdania. Ta scena stawia dokładnie taki układ: `all[0]` to projekt DOCELOWY,
 * a skanowany folder jest inny.
 *
 * DLACZEGO PRAWDZIWA PRZEGLĄDARKA. Oba zdania powstają PO kliknięciach: hurtowe wyrzucenie
 * umiejętności, a potem Import. `renderToStaticMarkup` nie odpala `onClick`, a wyłączony
 * przycisk jest dokładnie tą klasą wady, której czysty moduł nie widzi — funkcja `apply`
 * jest tam żywa, tylko nikt jej nie woła (niezmiennik 29). Chromium naciska naprawdę.
 *
 * CZEGO TO NIE DOWODZI. Rust zostaje na granicy harnessu: `scan_setup` i `apply_setup`
 * odpowiadają kształtem. Pytanie brzmi „czy człowiek dokończy import i przeczyta, gdzie
 * pliki wylądowały", a nie „czy Rust zapisał je tam naprawdę".
 */
import { afterAll, beforeAll, describe, expect, it } from 'vitest';

import type { ImportItem, ImportPreview } from '../../src/sections/import/setup';
import type { TauriReply } from '../harness';
import { closeEverything, openApp } from '../harness';

/* Projekt, do którego import NAPRAWDĘ pisze: pierwszy na liście, więc po starcie okna jest
 * aktywny (`pick()` oddaje `all[0]`). Nazwa i folder właściciela, nie wymyślone. */
const TARGET_FOLDER = '/Users/somebody/Projects/inne-i-zadania';
const TARGET = { id: TARGET_FOLDER, name: 'Other work', folder: TARGET_FOLDER };
/** I folder, który człowiek wpisuje do skanu. Dwa różne miejsca, i o to w tej scenie chodzi. */
const READING = '/Users/somebody/Projects/urc-monorepo';
const SOURCE = { id: READING, name: 'The monorepo', folder: READING };

function brought(id: string, kind: ImportItem['kind'], path: string, into: string): ImportItem {
  return {
    id,
    kind,
    sources: [{ provider: 'claude', path, hash: `h-${id}`, role: 'definition' }],
    target: into,
    dependencies: [],
    status: 'ready',
    statusMessage: 'Loadout can bring this over as it is.',
    generatedHash: null,
  };
}

/** Cudza umiejętność: gotowa do wniesienia, ale `adjusted`, więc czeka na przeczytanie. */
function unread(id: string): ImportItem {
  return {
    ...brought(id, 'skill', `.claude/skills/${id}/SKILL.md`, `skills/${id}/SKILL.md`),
    statusMessage: 'This skill was normalized and reviewed before import.',
  };
}

const ITEMS: readonly ImportItem[] = [
  brought('lead', 'agent', '.claude/agents/lead.md', 'agents/lead.md'),
  brought('rules', 'memory', '.claude/notes/rules.md', 'memory/rules.md'),
  brought('deploy', 'workflow', '.claude/commands/deploy.md', 'workflows/deploy.md'),
  unread('audit'),
  unread('release'),
];

/** Dwie umiejętności są `adjusted`, reszta `exact` — czyli JEDYNĄ blokadą jest nieczytanie. */
const ADJUSTED = ['audit', 'release'];

const PREVIEW: ImportPreview = {
  snapshot: {
    root: READING,
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
    agents: [{ id: 'lead', name: 'lead' }],
    skills: ADJUSTED.map((name) => ({ name })),
    connections: [],
    workflows: [],
    report: {
      mappings: ITEMS.map((item) => ({
        itemId: item.id,
        compatibility: ADJUSTED.includes(item.id) ? ('adjusted' as const) : ('exact' as const),
        message: ADJUSTED.includes(item.id)
          ? 'This skill was normalized and reviewed before import.'
          : 'The format can be reproduced.',
      })),
    },
  },
};

/* Odpowiedź granicy na prawdziwe kliknięcie Import. Trzy pliki, bo dwie umiejętności właśnie
 * wyszły poza import — paragon mówi, co Rust NAPRAWDĘ zapisał, nie co było zaznaczone. */
const SAVED = {
  id: 'receipt-1',
  written: ['agents/lead.md', 'memory/rules.md', 'workflows/deploy.md'],
  enabledConnections: [],
  filledAgents: [],
};

const SCENE: Readonly<Record<string, readonly TauriReply[]>> = {
  list_workspaces: Array.from({ length: 24 }, () => ({ value: [TARGET, SOURCE] })),
  scan_setup: Array.from({ length: 4 }, () => ({ value: PREVIEW })),
  apply_setup: [{ value: SAVED }],
};

const SWITCH = '[data-section-switch="agents"]';
const SCREEN = 'main[data-section="agents"]';
const OPEN = `${SCREEN} button:has-text("Import setup")`;
const DIALOG = '[role="dialog"]';
const SCAN = `${DIALOG} button:has-text("Scan")`;
const ROW = `${DIALOG} [data-import-items] tbody tr`;
/* Przycisk, który kończy całą tę robotę, i kontrolka, która go odblokowuje. Oba po `data-`,
 * bo słowo „Import" stoi na tym ekranie także w tytule i w ptaszku każdego wiersza. */
const BRING = `${DIALOG} [data-import-now]`;
const DROP = `${DIALOG} [data-drop-unread]`;
const APPEARS = 8_000;

/* Zdania, których szuka człowiek. Literałami, nie importem stałych z produktu: asercja
 * porównująca stałą samą ze sobą przechodzi także wtedy, gdy zdanie nie dojdzie na ekran. */
const LANDING = `Reading ${READING}. These files go into ${TARGET.name} (${TARGET_FOLDER}).`;
const LEFT_OUT =
  'Ready to import. 2 item(s) will not be imported, including 2 skill(s) you did not read.';
const IMPORTED = `3 files imported into ${TARGET.name} (${TARGET_FOLDER}).`;
const NEW_CONVERSATION =
  'An agent that is already talking keeps the connections it started with, so start a new ' +
  'conversation to use anything this import added.';

beforeAll(async () => {
  const warm = await openApp();
  await warm.close();
}, 180_000);

afterAll(async () => {
  await closeEverything();
}, 30_000);

describe('an import that a person can actually finish', () => {
  it('takes every unread skill out in one click, names the project these files go to, and says an agent already talking needs a new conversation', async () => {
    const app = await openApp({ replies: SCENE });
    try {
      const page = app.page;
      await page.setViewportSize({ width: 1280, height: 900 });
      /* Aktywny projekt musi stać w oknie, ZANIM okno importu się zamontuje: `target` czyta
       * się raz, przy otwarciu, tak samo jak dziś `Disk.forProject`. Bez tego czekania scena
       * mierzyłaby wyścig ładowania listy, a nie zdanie o miejscu zapisu. */
      await expect
        .poll(() => page.locator('[data-workspace-open]').innerText(), { timeout: APPEARS })
        .toContain(TARGET.name);

      await page.click(SWITCH);
      await page.locator(SCREEN).waitFor({ state: 'attached', timeout: APPEARS });
      await page.click(OPEN);
      await page.locator(DIALOG).waitFor({ state: 'visible', timeout: APPEARS });

      await page.getByRole('textbox', { name: 'Project folder' }).fill(READING);
      await page.click(SCAN);
      await page.locator(ROW).first().waitFor({ state: 'attached', timeout: APPEARS });

      /* Kontrola przeciw pustej asercji: skan naprawdę coś postawił na ekranie i naprawdę
       * przyjechał z drugiej strony granicy. */
      expect(await page.locator(ROW).count(), 'the scan put no item on the screen').toBe(
        ITEMS.length,
      );
      expect(
        (await app.calls()).some((call) => call.cmd === 'scan_setup'),
        'nothing crossed the boundary, so this scene is about the harness, not the window',
      ).toBe(true);

      expect(
        await page.locator(BRING).isDisabled(),
        'the thirteen unread skills no longer hold the import, so this scene stopped being ' +
          'about the button the owner kept pressing for nothing',
      ).toBe(true);
      expect(
        await page.locator(DIALOG).innerText(),
        'the scan reads one folder and the import writes into another, and the window names ' +
          'only the one it reads. A person presses Import and the files land somewhere else',
      ).toContain(LANDING);

      /* JEDNO kliknięcie za wszystkie trzynaście. Stopka obiecuje „take it out of the
       * import" — do dziś dało się to zrobić wyłącznie po jednej pozycji. */
      await page.click(DROP);
      await expect
        .poll(() => page.locator(DIALOG).innerText(), { timeout: APPEARS })
        .toContain(LEFT_OUT);
      expect(
        await page.locator(BRING).isDisabled(),
        'one click took every unread skill out of the import and the button is still dead, so ' +
          'the person still has nothing to press',
      ).toBe(false);

      await page.locator(BRING).click({ timeout: APPEARS });
      await expect
        .poll(async () => (await app.calls()).some((call) => call.cmd === 'apply_setup'), {
          timeout: APPEARS,
        })
        .toBe(true);
      /* Druga kontrola przeciw pustej asercji: zdanie o projekcie docelowym ma mówić o tym
       * samym miejscu, które poleciało na drut jako `folder`. Rozjazd tych dwóch byłby
       * zdaniem prawdziwym na ekranie i nieprawdziwym na dysku. */
      expect(
        (await app.calls()).find((call) => call.cmd === 'apply_setup')?.args['folder'],
        'the window names one project and writes into another',
      ).toBe(TARGET_FOLDER);

      await expect
        .poll(() => page.locator(DIALOG).innerText(), { timeout: APPEARS })
        .toContain(IMPORTED);
      expect(
        await page.locator(DIALOG).innerText(),
        'a person imports connections, writes to the lead agent that is already open, and gets ' +
          'no tools — the driver was made once, when that conversation started',
      ).toContain(NEW_CONVERSATION);
    } finally {
      await app.close();
    }
  }, 90_000);
});
