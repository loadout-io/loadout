/* Wiersz, który mówi „Needs a choice", bo ta sama rzecz leży w dwóch aplikacjach, ma dać się
 * ODDAĆ AGENTOWI — a to, co agent powie, ma stanąć PRZY TEJ POZYCJI.
 *
 * PO CO TO ISTNIEJE. Skan `meetnotes` zostawił 21 decyzji, a siedemnaście z nich to jedno
 * zdanie: „This skill has different copies. Let an agent compare them before import." Zdanie
 * samo prosi o agenta i do dziś nikt go nie woła — jedyną odpowiedzią, jaką ten ekran oferuje,
 * jest pominięcie pozycji. `Leave out all unresolved items` nie jest twierdzeniem, że
 * zachowanie zostało wniesione (docs/PLAN.md §6d).
 *
 * DLACZEGO PRAWDZIWA PRZEGLĄDARKA. Pytanie brzmi „czy kliknięcie człowieka kładzie zdania
 * agenta w komórce TEJ pozycji", a nie „czy funkcja oddaje `Comparison`". Wartość zwrócona
 * dowodzi, że mechanizm istnieje; zdanie na ekranie dowodzi, że produkt działa, i to jest
 * dokładnie ta różnica, dla której powstał niezmiennik 29. `renderToStaticMarkup` nie odpala
 * `onClick`, więc tamtą drogą przycisk bez handlera przechodzi na zielono.
 *
 * CZEGO TO NIE DOWODZI. Rust zostaje na granicy harnessu: `compare_import_copies` odpowiada
 * gotowym porównaniem. Co POJECHAŁO do agenta — polityka plików, sieć, katalogi w zasięgu,
 * treść obu kopii w pytaniu — sądzi drugi plik kryteriów, po tamtej stronie granicy
 * (`src-tauri/tests/it/an_agent_compares_the_copies.rs`).
 */
import { afterAll, beforeAll, describe, expect, it } from 'vitest';

import type { Comparison, ImportItem, ImportPreview } from '../../src/sections/import/setup';
import type { TauriReply } from '../harness';
import { closeEverything, openApp } from '../harness';

const FOLDER = '/Users/somebody/Projects/meetnotes';
const WORKSPACE = { id: FOLDER, name: 'Meetnotes', folder: FOLDER };

/** Zapisany agent, który ma porównać kopie. Jeden, żeby wybór nie był pytaniem tego testu. */
const AGENT = {
  schema: 1,
  id: '019b0000-0000-7000-8000-0000000000a1',
  name: 'Scribe',
  summary: 'Reads and reports',
  color: 'slate',
  instructions: 'Read and report.',
  runsWith: 'claude-code',
  model: 'sonnet',
  thinking: 'quick',
  fileAccess: 'look-only',
  giveUpAfterMinutes: 20,
  tools: 'everything',
  reachesTheWeb: false,
  skills: [],
  connections: [],
  writeResultsTo: '',
};

/** Nazwa rzeczy, o której jest cały ten test. Wiersz szukamy PO NIEJ, nie po `body`. */
const NAME = 'audit';
const ITEM_ID = 'audit-item';

/** Dwie ścieżki, które wiersz nosił przed kliknięciem — i które ma nosić po nim. */
const HERE = '.agents/skills/audit/SKILL.md';
const THERE = '.claude/skills/audit/SKILL.md';

/** Zdanie, którym adapter sam prosi o agenta. */
const ASKS = 'This skill has different copies. Let an agent compare them before import.';

const ITEM: ImportItem = {
  id: ITEM_ID,
  kind: 'skill',
  sources: [
    { provider: 'agent_skills', path: HERE, hash: 'h-here', role: 'definition' },
    { provider: 'claude', path: THERE, hash: 'h-there', role: 'definition' },
  ],
  target: 'skills/audit/SKILL.md',
  dependencies: [],
  status: 'needs_choice',
  statusMessage: ASKS,
  generatedHash: null,
};

/** Drugi wiersz, gotowy do wniesienia. Stoi tu, żeby „ta pozycja" znaczyło coś więcej niż
 *  „jedyna pozycja": zdania agenta mają wylądować w JEDNEJ komórce, nie w tabeli. */
const OTHER: ImportItem = {
  id: 'ship-item',
  kind: 'skill',
  sources: [
    {
      provider: 'claude',
      path: '.claude/skills/ship/SKILL.md',
      hash: 'h-ship',
      role: 'definition',
    },
  ],
  target: 'skills/ship/SKILL.md',
  dependencies: [],
  status: 'ready',
  statusMessage: 'Loadout can bring this over as it is.',
  generatedHash: null,
};

const ITEMS: readonly ImportItem[] = [ITEM, OTHER];

const PREVIEW: ImportPreview = {
  snapshot: {
    root: FOLDER,
    items: ITEMS.map((item) => ({
      id: item.id,
      kind: item.kind,
      path: item.sources[0]?.path ?? '',
      name: item.id === ITEM_ID ? NAME : 'ship',
      summary: 'Found in this project.',
    })),
  },
  draft: {
    sourceHashes: Object.fromEntries(ITEMS.map((item) => [item.id, `h-${item.id}`])),
    items: [...ITEMS],
    agents: [],
    skills: [{ name: NAME }, { name: 'ship' }],
    connections: [],
    workflows: [],
    report: { mappings: [] },
  },
};

/** Co agent powiedział o tych dwóch kopiach. Proza, nie format do sparsowania. */
const SAID =
  'The copy in .agents runs the tests before it reads anything. The copy in .claude reads ' +
  'the code first and never runs the tests, so it is the quieter of the two.';

const COMPARISON: Comparison = {
  itemId: ITEM_ID,
  compared: [HERE, THERE],
  said: SAID,
  keep: HERE,
};

const SCENE: Readonly<Record<string, readonly TauriReply[]>> = {
  list_workspaces: Array.from({ length: 12 }, () => ({ value: [WORKSPACE] })),
  list_agents: Array.from({ length: 12 }, () => ({ value: [AGENT] })),
  scan_setup: Array.from({ length: 4 }, () => ({ value: PREVIEW })),
  compare_import_copies: [{ value: COMPARISON }],
};

/** Nazwa uchwytu, pod którym trzymamy pytanie otwarte, żeby zobaczyć wiersz W TRAKCIE pracy. */
const HELD = 'comparing';

/** Ta sama scena, ale Rust nie odpowiada od razu.
 *
 * To jest kontrola CZASU, nie odpowiedź Rusta napisana w teście: prawdziwy handler Reacta żyje
 * pomiędzy kliknięciem a powrotem `invoke`, a bez tego stan „porównuje teraz" trwałby ułamek
 * sekundy i nie dałoby się o niego zapytać. */
const HOLDING: Readonly<Record<string, readonly TauriReply[]>> = {
  ...SCENE,
  compare_import_copies: [{ deferred: HELD }],
};

const SWITCH = '[data-section-switch="agents"]';
const SCREEN = 'main[data-section="agents"]';
const OPEN = `${SCREEN} button:has-text("Import setup")`;
const DIALOG = '[role="dialog"]';
const SCAN = `${DIALOG} button:has-text("Scan")`;
const ROW = `${DIALOG} [data-import-items] tbody tr`;
/* Przycisk niesie identyfikator swojej pozycji, więc selektor nie zgaduje po napisie —
 * słowo „Compare" ma prawo kiedyś stanąć na tym ekranie także gdzie indziej. */
const COMPARE = `[data-compare-copies="${ITEM_ID}"]`;
const STOP = '[data-stop-comparing]';
/** Zdanie, które ten wiersz mówi, dopóki agent czyta. */
const WORKING = 'An agent is comparing the copies now.';
const APPEARS = 8_000;

beforeAll(async () => {
  const warm = await openApp();
  await warm.close();
}, 180_000);

afterAll(async () => {
  await closeEverything();
}, 30_000);

describe('an agent compares the copies', () => {
  it('puts what an agent said about the two copies at that item', async () => {
    const app = await openApp({ replies: SCENE });
    try {
      const page = app.page;
      await page.setViewportSize({ width: 1280, height: 900 });
      await page.click(SWITCH);
      await page.locator(SCREEN).waitFor({ state: 'attached', timeout: APPEARS });
      await page.click(OPEN);
      await page.locator(DIALOG).waitFor({ state: 'visible', timeout: APPEARS });
      await page.click(SCAN);
      await page.locator(ROW).first().waitFor({ state: 'attached', timeout: APPEARS });

      /* Kontrola przeciw pustej asercji: wiersz, o który tu chodzi, naprawdę stoi na ekranie
       * i naprawdę prosi o agenta. Bez tego wszystko niżej mówiłoby o wierszu, którego nie ma. */
      const row = page.locator(ROW).filter({ hasText: NAME });
      expect(await row.count(), 'the scan put no row for the thing this test is about').toBe(1);
      expect(
        await row.innerText(),
        'the row stopped saying the sentence that asks for an agent in the first place',
      ).toContain(ASKS);

      expect(
        await row.locator(COMPARE).count(),
        'the row that says two apps disagree offers no way to ask an agent about it, so the ' +
          'only answer this screen has for seventeen of those rows is still "leave it out"',
      ).toBe(1);

      await row.locator(COMPARE).click();
      await row.locator(`text=${SAID}`).waitFor({ state: 'attached', timeout: APPEARS });

      const cell = await row.innerText();
      expect(
        cell,
        'what the agent said has to stand at THIS item, in its own cell — a second opinion ' +
          'parked anywhere else is a second opinion about nothing in particular',
      ).toContain(SAID);
      expect(
        cell,
        'and it has to name BOTH files it read, or the person cannot tell which two copies ' +
          'were weighed',
      ).toContain(HERE);
      expect(cell, 'the second file it read is missing from the sentence').toContain(THERE);
      expect(
        cell,
        'the paths this row carried before the click have to stay: no item may lose the files ' +
          'it came from because an agent looked at it',
      ).toContain(THERE);
      expect(
        cell,
        'the answer has to say who decides. The agent advises; the person imports (AGENTS.md §2)',
      ).toContain('This is advice. You choose what to import.');

      /* Status się NIE RUSZA. `Can't be reproduced` i `Needs a choice` zostają widoczne,
       * dopóki człowiek nie rozstrzygnie — analiza jest drugą opinią, nie cichym importem. */
      expect(
        await row.locator('td').nth(3).innerText(),
        'the analysis moved the status of the row, so the screen now claims a decision that ' +
          'nobody made',
      ).toContain('Needs a choice');

      const asked = (await app.calls()).filter((call) => call.cmd === 'compare_import_copies');
      expect(
        asked.length,
        'the click never reached Rust, so the sentences on screen came from somewhere else',
      ).toBe(1);
      expect(
        asked[0]?.args['item'],
        'the question went to Rust about a different item than the row that was clicked',
      ).toBe(ITEM_ID);
      expect(asked[0]?.args['workspace'], 'the question carried no project folder').toBe(FOLDER);
      expect(asked[0]?.args['agent'], 'the question named no agent to ask').toBe(AGENT.id);
    } finally {
      await app.close();
    }
  }, 90_000);

  it('says the agent is working, offers Stop, and takes the row back to its question', async () => {
    const app = await openApp({ replies: HOLDING });
    try {
      const page = app.page;
      await page.setViewportSize({ width: 1280, height: 900 });
      await page.click(SWITCH);
      await page.locator(SCREEN).waitFor({ state: 'attached', timeout: APPEARS });
      await page.click(OPEN);
      await page.locator(DIALOG).waitFor({ state: 'visible', timeout: APPEARS });
      await page.click(SCAN);
      await page.locator(ROW).first().waitFor({ state: 'attached', timeout: APPEARS });

      const row = page.locator(ROW).filter({ hasText: NAME });
      await row.locator(COMPARE).click();

      /* Wiersz, który wygląda tak samo jak przed kliknięciem, jest wierszem, po którym człowiek
       * klika drugi raz — i płaci za drugą turę, nie wiedząc o pierwszej. */
      await row.locator(`text=${WORKING}`).waitFor({ state: 'attached', timeout: APPEARS });
      expect(
        await row.locator(STOP).count(),
        "an agent is reading two files on somebody else's money and this row offers no way to " +
          'end it. Stop is the whole difference between a question and a commitment',
      ).toBe(1);

      await row.locator(STOP).click();
      const stopped = (await app.calls()).filter((call) => call.cmd === 'stop_comparing_copies');
      expect(
        stopped.length,
        'Stop never reached Rust, so the button ends the sentence on screen and leaves the agent ' +
          'reading. That is a control without a handler wearing the mask of one (invariant 16)',
      ).toBe(1);

      /* Rust odpowiada „człowiek to zatrzymał" — wartością, nie odmową (niezmiennik 7). */
      await app.settle(HELD, { value: null });
      await row.locator(COMPARE).waitFor({ state: 'attached', timeout: APPEARS });

      const cell = await row.innerText();
      expect(
        cell,
        'after Stop the row has to be back at its own question, and it is not asking it any more',
      ).toContain(ASKS);
      expect(
        cell,
        'the row still claims an agent is comparing, so Stop left the screen saying something ' +
          'that is not happening',
      ).not.toContain(WORKING);
      expect(
        cell,
        'a stopped comparison left an answer behind. Nobody said anything about these copies, ' +
          'so there is nothing to put under this item',
      ).not.toContain('This is advice. You choose what to import.');
      expect(
        await row.locator('td').nth(3).innerText(),
        'stopping moved the status of the row',
      ).toContain('Needs a choice');
    } finally {
      await app.close();
    }
  }, 90_000);
});
