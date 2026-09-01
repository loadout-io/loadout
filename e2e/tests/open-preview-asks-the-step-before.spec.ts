/* Kafelek „uruchom i zostaw" prosi krok przed sobą o komendę — klikaniem, nie edycją JSON-a.
 *
 * # Zamówienie, dosłownie
 *
 * Właściciel 2026-08-30: „mozesz w workflows dodac opcje open preview taki kafelek, i agent sam
 * zrobi co trzeba a by odpalic front i backend lub cokolwiek". I wcześniej: „nie chcę w każdym
 * projekcie osobno wpisywać na front i backend command".
 *
 * # Czego to kryterium pilnuje, a czego nie pilnuje nic innego
 *
 * Że kliknięcie DOJEŻDŻA DO PLIKU. Reguła walidatora, czytnik pola w biegu i czysty moduł liczący
 * nazwę mają swoje kryteria i wszystkie są zielone niezależnie od tego, czy człowiek ma jak to
 * ustawić. Kontrolka wpięta w martwy kod wygląda dokładnie tak samo jak wpięta w żywy — do chwili,
 * gdy ktoś jej użyje.
 *
 * Dlatego tu nie ma ani jednego wywołania handlera ani akcji magazynu: klik w prawdziwą kontrolkę,
 * przez prawdziwe `save_workflow`, i pytanie zadane ARGUMENTOWI, który przeszedł przez most.
 *
 * # Dlaczego DWA kafelki, a nie jeden
 *
 * Bo liczba mnoga jest tu treścią zamówienia, nie stylem. Pierwsza wersja mechanizmu miała nazwę
 * pola zaszytą na `command`, więc kafelek frontu i kafelek backendu przeczytałyby TO SAMO — czyli
 * uruchomiłyby dwa razy jedną rzecz i nie powiedziały o tym ani słowa. Jeden kafelek w ławce
 * przepuszcza tamtą wersję bez mrugnięcia.
 *
 * # Dlaczego przycisk, a nie samo zaznaczenie
 *
 * Prośba ląduje na SĄSIEDNIM kafelku. Zaznaczenie, które po cichu zmienia inny krok, jest magią,
 * przez którą przestaje się ufać edytorowi; ostatni przypadek pilnuje, że drugi kafelek nie
 * zmienia się, dopóki nikt nie kliknął.
 */
import { afterAll, beforeAll, describe, expect, it } from 'vitest';

import type { RunningApp, TauriCall, TauriReply } from '../harness';
import { closeEverything, openApp } from '../harness';

const PATH = 'open-preview.json';
const TICK = 'Let the step before this one work out the command';
const AGENT = {
  schema: 1,
  id: 'agent-open-preview',
  name: 'Preview Builder',
  summary: 'Works out how to start the app',
  color: 'slate',
  instructions: 'Work out the one shell line that starts this app.',
  runsWith: 'claude-code',
  model: 'haiku',
  thinking: 'balanced',
  fileAccess: 'look-only',
  giveUpAfterMinutes: 20,
  tools: 'everything',
  reachesTheWeb: false,
  skills: [],
  connections: [],
  writeResultsTo: 'handoffs/result.md',
};

const SWITCH = '[data-section-switch="workflows"]';
const SCREEN = 'main[data-section="workflows"]';
const LIST_TILE = 'main [data-tile]';
const PANEL = 'main [data-step-panel]';
const APPEARS = 6_000;

/** Jeden krok agenta, dwa kafelki „uruchom i zostaw" za nim — kształt z zamówienia. */
function document() {
  return {
    format: 1 as const,
    id: 'wf-open-preview',
    name: 'Open preview',
    steps: [
      {
        kind: 'agent' as const,
        id: 's_build',
        name: 'Build it',
        agent: AGENT.id,
        overrides: {},
        copies: 1,
        instructions: 'Build the app.',
        skills: 'all' as const,
        folder: { use: 'project' as const },
        handover: 'notes' as const,
        at: { x: 24, y: 24 },
      },
      {
        kind: 'serve' as const,
        id: 's_front',
        name: 'Run frontend',
        command: '',
        folder: { use: 'same-copy' as const },
        at: { x: 24, y: 160 },
      },
      {
        kind: 'serve' as const,
        id: 's_back',
        name: 'Run backend',
        command: '',
        folder: { use: 'same-copy' as const },
        at: { x: 24, y: 280 },
      },
    ],
    links: [
      { from: 's_build', to: 's_front' },
      { from: 's_build', to: 's_back' },
    ],
  };
}

const ENTRY = { path: PATH, workflow: document() };

function copies<T>(value: T, count = 20): readonly { readonly value: T }[] {
  return Array.from({ length: count }, () => ({ value }));
}

function scene(): Readonly<Record<string, readonly TauriReply[]>> {
  return {
    list_workflows: copies([ENTRY]),
    load_workflow: copies({ workflow: document(), revision: 'r1' }),
    check_workflow: copies([]),
    list_agents: copies([AGENT]),
    list_skills: copies([]),
    save_workflow: copies('after-the-save'),
  };
}

async function waitForSaves(app: RunningApp, count: number): Promise<readonly TauriCall[]> {
  const deadline = Date.now() + APPEARS;
  while (Date.now() < deadline) {
    const saves = (await app.calls()).filter((call) => call.cmd === 'save_workflow');
    if (saves.length >= count) return saves;
    await app.page.waitForTimeout(25);
  }
  return (await app.calls()).filter((call) => call.cmd === 'save_workflow');
}

/** Kroki z pliku, który przeszedł przez most przy tym zapisie. */
function stepsIn(call: TauriCall | undefined): readonly Record<string, unknown>[] {
  const workflow = call?.args['workflow'];
  if (typeof workflow !== 'object' || workflow === null) return [];
  const steps = (workflow as Record<string, unknown>)['steps'];
  if (!Array.isArray(steps)) return [];
  return steps.filter(
    (step): step is Record<string, unknown> => typeof step === 'object' && step !== null,
  );
}

/** Nazwa pola, na które ten kafelek czeka w zapisanym pliku — pusta, kiedy na żadne. */
function waitsFor(call: TauriCall | undefined, id: string): string {
  const step = stepsIn(call).find((one) => one['id'] === id);
  const from = step?.['commandFrom'];
  if (typeof from !== 'object' || from === null) return '';
  const field = (from as Record<string, unknown>)['field'];
  return typeof field === 'string' ? field : '';
}

/** Nazwy pól, o które poproszono krok agenta w zapisanym pliku. */
function askedOf(call: TauriCall | undefined, id: string): readonly string[] {
  const step = stepsIn(call).find((one) => one['id'] === id);
  const handover = step?.['handover'];
  if (typeof handover !== 'object' || handover === null) return [];
  const fields = (handover as Record<string, unknown>)['fields'];
  if (!Array.isArray(fields)) return [];
  return fields
    .map((one) =>
      typeof one === 'object' && one !== null
        ? (one as Record<string, unknown>)['name']
        : undefined,
    )
    .filter((name): name is string => typeof name === 'string');
}

async function openTile(app: RunningApp, id: string): Promise<void> {
  await app.page.locator(`main [data-step="${id}"]`).click();
  await app.page.locator(PANEL).waitFor({ state: 'visible', timeout: APPEARS });
}

beforeAll(async () => {
  const warm = await openApp();
  await warm.close();
}, 180_000);

afterAll(async () => {
  await closeEverything();
}, 30_000);

describe('a serve tile asks the step before it for the command', () => {
  it('saves a field name of its own, and asks the other tile only on a click', async () => {
    const app = await openApp({ replies: scene() });
    try {
      await app.page.locator(SWITCH).click();
      await app.page.locator(SCREEN).waitFor({ state: 'attached', timeout: APPEARS });
      await app.page.locator(LIST_TILE).first().click();

      // ── (a) ZAZNACZENIE DOJEŻDŻA DO PLIKU ────────────────────────────────────────────────
      await openTile(app, 's_front');
      await app.page.getByLabel(TICK, { exact: true }).check();
      const first = await waitForSaves(app, 1);
      expect(first.length, 'ticking the box never crossed production workflow IO').toBe(1);
      expect(
        waitsFor(first[0], 's_front'),
        'the field name comes from the TILE, so two tiles do not read one command. A fixed name ' +
          'would start one thing twice and say nothing about it',
      ).toBe('run-frontend');

      // ── (b) DRUGI KAFELEK BIERZE INNE POLE ───────────────────────────────────────────────
      await openTile(app, 's_back');
      await app.page.getByLabel(TICK, { exact: true }).check();
      const second = await waitForSaves(app, 2);
      expect(waitsFor(second[1], 's_back')).toBe('run-backend');
      expect(
        waitsFor(second[1], 's_front'),
        'wiring the second tile must not disturb the first one',
      ).toBe('run-frontend');

      // ── (c) DOPÓKI NIKT NIE KLIKNĄŁ, SĄSIEDNI KAFELEK STOI NIETKNIĘTY ────────────────────
      // Prośba ląduje na CUDZYM kroku. Zaznaczenie, które po cichu zmienia sąsiada, jest magią,
      // przez którą przestaje się ufać edytorowi — a ekran mówi wprost, że jeszcze nie poproszono.
      expect(
        askedOf(second[1], 's_build'),
        'ticking a box on one tile must not quietly rewrite another one',
      ).toEqual([]);
      expect(
        await app.page.locator(`${PANEL} [data-field="commandFromState"]`).innerText(),
      ).toContain('does not hand over');

      // ── (d) I KLIKNIĘCIE PROSI, NIE ZASTĘPUJĄC TEGO, O CO JUŻ POPROSZONO ─────────────────
      await app.page.locator(`${PANEL} [data-field="askTheStepBefore"]`).click();
      const third = await waitForSaves(app, 3);
      expect(askedOf(third[2], 's_build'), 'the click did not reach the step before').toEqual([
        'run-backend',
      ]);

      await openTile(app, 's_front');
      await app.page.locator(`${PANEL} [data-field="askTheStepBefore"]`).click();
      const fourth = await waitForSaves(app, 4);
      expect(
        askedOf(fourth[3], 's_build'),
        'one agent hands over both commands. Replacing on the second ask silently unwires the ' +
          'first tile, and the run only says so once it gets there',
      ).toEqual(['run-backend', 'run-frontend']);
    } finally {
      await app.close();
    }
  }, 90_000);
});
