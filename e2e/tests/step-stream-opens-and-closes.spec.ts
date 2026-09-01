/* Szuflada kroku otwiera się prawdziwym kliknięciem i zamyka prawdziwym Escape.
 *
 * PO CO TO ISTNIEJE OBOK KRYTERIUM W MARKUPIE. Niezmiennik 29, słowo w słowo: funkcja, którą
 * kryterium woła wprost, dowodzi, że mechanizm istnieje; dopiero prawdziwe kliknięcie dowodzi,
 * że człowiek do niego dochodzi. `renderToStaticMarkup` nigdy nie odpala `onClick`, a nasłuch
 * klawiatury zakładany w `useEffect` nie odpala się tam ANI RAZU — więc „Escape zamyka" jest
 * w tamtym środowisku zdaniem, którego nie da się sprawdzić w ogóle. Tutaj naciska je prawdziwa
 * klawiatura w prawdziwym chromium.
 *
 * CZEGO TEN PLIK NIE DOWODZI, i to jest granica, nie kompromis do ukrycia. Granica Rusta jest tu
 * atrapą (`../harness.ts`): `run_workflow` nie uruchamia niczego, a linia agenta wjeżdża tym
 * samym prawdziwym kanałem Tauri, który zapisało wywołanie. O tym, czy po drugiej stronie
 * naprawdę wstała praca, mówią kryteria rustowe — nie ten plik. Ten mówi jedyną rzecz, której
 * tamte nie umieją powiedzieć: że droga od myszy i od klawiatury do tej powierzchni istnieje
 * i jest przejezdna w obie strony.
 *
 * DLACZEGO LINIA MUSI BYĆ PODPISANA NAZWĄ KROKU. Pompa zdarzeń podpisuje linie nazwą kroku
 * (`src-tauri/src/commands/run.rs`, `forward(…, step.name)`), a kafelek dostaje wejście wtedy
 * i tylko wtedy, gdy strumień powiedział o nim cokolwiek. Linia podpisana nazwą AGENTA, a nie
 * kroku, dałaby kafelek bez wejścia — czyli scenę, w której nie ma czego kliknąć, i zielone
 * „nic się nie otworzyło" o harnessie zamiast o produkcie.
 */
import { afterAll, beforeAll, describe, expect, it } from 'vitest';

import type { RunningApp, TauriCall, TauriReply } from '../harness';
import { closeEverything, openApp } from '../harness';

const FOLDER = '/Users/somebody/Projects/loadout-step-stream';
const WORKSPACE = { id: FOLDER, name: 'Step stream', folder: FOLDER };
const AGENT = {
  id: 'agent-step-stream',
  name: 'Forge',
  summary: 'Writes code',
  skills: [],
};

const BUILD = { id: 's-build', name: 'Build' };
const CHECK = { id: 's-check', name: 'Check' };

function step(one: { id: string; name: string }, at: number) {
  return {
    kind: 'agent' as const,
    id: one.id,
    name: one.name,
    agent: AGENT.id,
    overrides: {},
    copies: 1,
    instructions: 'Do the ' + one.name + ' part.',
    skills: 'all' as const,
    folder: { use: 'project' as const },
    handover: 'notes' as const,
    at: { x: at * 264, y: 24 },
  };
}

/**
 * Ten sam plan, rysowany na DWA sposoby, i oba przechodzą tę samą scenę.
 *
 * Bez strzałek obraz uczciwie milczy o kształcie i pokazuje LISTĘ kroków (reguła 17); ze
 * strzałką rysuje PŁÓTNO. To nie jest ten sam kod pod spodem i to jest cały powód, dla którego
 * stoją tu oba: zmierzone 2026-08-31, na płótnie kafelka NIE DAŁO SIĘ KLIKNĄĆ ani razu, bo
 * biblioteka stawia na jego opakowaniu `pointer-events: none`, dopóki nic w nim nie jest
 * wybieralne, przeciągalne ani nie ma `onNodeClick`. Wersja tego kryterium sądząca wyłącznie
 * listę była wtedy zielona nad martwym środkiem każdego kafelka — a płótno jest tą drogą,
 * którą rysuje się PRAWDZIWY plan z pliku.
 */
function workflowFile(joined: boolean) {
  return {
    path: 'step-stream.json',
    workflow: {
      format: 1 as const,
      id: 'wf-step-stream',
      name: 'Two steps',
      steps: [step(BUILD, 0), step(CHECK, 1)],
      links: joined ? [{ from: BUILD.id, to: CHECK.id }] : [],
    },
  };
}

const RUN = 'main[data-section="run"]';
const START = 'button[data-workflow-run="manual"]';
const TILE = '[data-plan-column] [data-step="' + BUILD.id + '"]';
const DRAWER = '[data-step-stream]';
/** Pełny ekran jednego agenta — powierzchnia, do której szuflada jest jedyną drogą. */
const FULL_SCREEN = '[data-agent-screen]';
const MINE = 'Rewriting the quote handling as a small state machine.';
const THEIRS = 'Ran the checks — they did not work';
const WANTED = 'Should the reader keep a trailing comma at the end of a row?';
const OPTIONS = ['Keep it', 'Drop it'] as const;

const APPEARS = 5_000;
const SETTLE = 250;

function copies<T>(value: T, count = 24): readonly { readonly value: T }[] {
  return Array.from({ length: count }, () => ({ value }));
}

function scene(joined: boolean): Readonly<Record<string, readonly TauriReply[]>> {
  const file = workflowFile(joined);
  return {
    list_workspaces: copies([WORKSPACE]),
    list_workflows: copies([file]),
    load_workflow: copies({ workflow: file.workflow, revision: 'r1' }, 4),
    check_workflow: copies([], 8),
    list_agents: copies([AGENT]),
    list_skills: copies([]),
    /* Odroczone, więc bieg nigdy nie schodzi w trakcie tej sceny: `../../src/sections/run/io.ts`
     * woła `runEnded()` w `finally`, a bieg, który zszedł w połowie kliknięcia, dawałby czerwień
     * o czasie, nie o drodze. */
    run_workflow: [{ deferred: 'step-stream-run' }],
  };
}

/** Wywołanie startu, razem z uchwytem prawdziwego kanału Tauri. */
async function runCall(app: RunningApp): Promise<TauriCall> {
  const deadline = Date.now() + APPEARS;
  while (Date.now() < deadline) {
    const call = (await app.calls()).find((one) => one.cmd === 'run_workflow');
    if (call !== undefined) return call;
    await app.page.waitForTimeout(25);
  }
  throw new Error('the visible Run control never reached run_workflow');
}

/** Wpuszcza jedną paczkę linii tym samym kanałem, którym mówi Rust. */
async function say(app: RunningApp, call: TauriCall, said: readonly unknown[]): Promise<void> {
  const channel = call.args['lines'];
  const match = typeof channel === 'string' ? /^__CHANNEL__:(\d+)$/.exec(channel) : null;
  expect(match, 'run_workflow did not carry the real Tauri Channel handle').not.toBeNull();
  const id = match?.[1];
  if (id === undefined) throw new Error('the Channel handle had no callback id');

  await app.page.evaluate(
    ({ slot, message }) => {
      const host = globalThis as unknown as Record<string, unknown>;
      const callback = host[slot];
      if (typeof callback !== 'function') {
        throw new Error(`the live Tauri Channel callback ${slot} is not registered`);
      }
      (callback as (payload: unknown) => void)({ index: 0, message });
    },
    { slot: `_${id}`, message: said },
  );
}

beforeAll(async () => {
  const warm = await openApp();
  await warm.close();
}, 180_000);

afterAll(async () => {
  await closeEverything();
}, 30_000);

describe('a real click opens the stream of one step and a real Escape shuts it', () => {
  /* JEDEN PRZYPADEK NA CAŁĄ SCENĘ, i to jest wybór, nie skrót: „przedtem nie było", „po
   * kliknięciu jest" i „po Escape znowu nie ma" to jedno zdanie o jednym oknie. Rozbite na trzy
   * `it`, każde otwierałoby własną kartę i sądziło stan sprzed swojego własnego kliknięcia.
   *
   * DWA PRZEBIEGI, bo pod spodem są dwa różne rysunki — powód w całości przy [`workflowFile`]. */
  for (const drawing of [
    { name: 'as a list, when the file says nothing about where the steps stand', joined: false },
    { name: 'on the canvas, when the file says where every step stands', joined: true },
  ]) {
    it(`shows nothing before, that one step after, and nothing again after Escape — ${drawing.name}`, async () => {
      const app = await openApp({ replies: scene(drawing.joined) });
      try {
        await app.page.locator(RUN).waitFor({ state: 'attached', timeout: APPEARS });
        await app.page.locator(START).waitFor({ state: 'visible', timeout: APPEARS });
        await app.page.locator(START).click();
        const call = await runCall(app);

        await say(app, call, [
          { kind: 'note', agent: BUILD.name, text: MINE, body: [] },
          { kind: 'note', agent: CHECK.name, text: THEIRS, body: [] },
          { kind: 'asked', agent: BUILD.name, text: WANTED, options: [...OPTIONS] },
        ]);
        await app.page.locator(TILE).waitFor({ state: 'visible', timeout: APPEARS });

        // ── KARTA PYTANIA STOI PRZY TYM KROKU I DA SIĘ W NIĄ WEJŚĆ ───────────────────
        const card = app.page.locator('[data-plan-column] [data-asked]');
        expect(
          await card.count(),
          'the way to answer is not in the plan column at all. It stood at the foot of the ' +
            'other one, and with four steps going at once nothing on screen said which of them ' +
            'was waiting.',
        ).toBe(1);
        const choice = card.locator(`button:has-text("${OPTIONS[0]}")`);
        /* Próbne kliknięcie dowodzi OSIĄGALNOŚCI, nie zmieniając sceny. To jest ta jedna
           asercja, którą przewracał `pointer-events: none` na płótnie: karta rysowała się
           w markupie i nie przyjmowała ani jednego naciśnięcia. */
        await choice.click({ trial: true, timeout: APPEARS });

        // ── KONTROLA PRZECIW PUSTEMU PRZEJŚCIU ────────────────────────────────────────────
        expect(
          await app.page.locator(DRAWER).count(),
          'the panel stood open before anything was clicked. Without this line "the click opened ' +
            'it" says nothing at all: a column that always draws one passes the whole rest of ' +
            'this case on furniture that was already on screen.',
        ).toBe(0);

        // ── MYSZ OTWIERA ──────────────────────────────────────────────────────────────────
        await app.page.locator(TILE).click();
        await app.page.waitForTimeout(SETTLE);

        expect(
          await app.page.locator(DRAWER).count(),
          'clicking the card changed nothing in the document. A control whose handler has no ' +
            'effect is worse than no control at all (invariant 16), and this is the only way from ' +
            'the picture into what one step is saying.',
        ).toBe(1);
        const opened = await app.page.locator(DRAWER).innerText();
        expect(
          opened,
          'what ' + BUILD.name + ' said is missing from the panel that opened for it',
        ).toContain(MINE);
        expect(
          opened,
          'a line belonging to ' +
            CHECK.name +
            ' reached the panel of ' +
            BUILD.name +
            '. A panel that opens without narrowing to one step is the same panel for every card.',
        ).not.toContain(THEIRS);

        // ── OBRAZ ZOSTAJE, i to jest cała różnica wobec ekranu zakrywającego okno ─────────
        expect(
          await app.page.locator('[data-plan-column] [data-step="' + CHECK.id + '"]').isVisible(),
          'opening one step hid the others. Work that goes on in parallel is ordinary work ' +
            '(invariant 11): the price of asking about one may not be losing sight of the rest.',
        ).toBe(true);

        // ── KLAWIATURA ZAMYKA ─────────────────────────────────────────────────────────────
        await app.page.keyboard.press('Escape');
        await app.page.waitForTimeout(SETTLE);

        expect(
          await app.page.locator(DRAWER).count(),
          'Escape left the panel standing. A surface that appears over the work has to go away ' +
            'without hunting for a control: the hands are on the keyboard, because this screen is ' +
            'a place where a person types.',
        ).toBe(0);

        // ── I DA SIĘ JĄ OTWORZYĆ ZNOWU, I ZAMKNĄĆ MYSZĄ ──────────────────────────────────
        await app.page.locator(TILE).click();
        await app.page.waitForTimeout(SETTLE);
        expect(
          await app.page.locator(DRAWER).count(),
          'the card opened once and never again, so shutting it cost the way back in',
        ).toBe(1);

        // ── DROGA DALEJ: PEŁNY EKRAN TEGO AGENTA ────────────────────────────────────────
        expect(
          await app.page.locator(FULL_SCREEN).count(),
          'the full screen of one agent stood open with nobody having asked for it, so the ' +
            'press below would be a statement about something already there',
        ).toBe(0);
        await app.page.locator(DRAWER + ' button:has-text("Open this agent")').click();
        await app.page.waitForTimeout(SETTLE);
        expect(
          await app.page.locator(FULL_SCREEN).count(),
          'the way on from the panel does nothing. It is the only one left: what this agent was ' +
            'given and what it left behind are two blocks of facts read off the disk, and the ' +
            'stream carries neither. Without a live press here that whole screen keeps its ' +
            'tests and loses its callers (invariant 16).',
        ).toBe(1);
        await app.page.locator(FULL_SCREEN + ' button[aria-label="Back to the run"]').click();
        await app.page.waitForTimeout(SETTLE);

        await app.page.locator(DRAWER + ' button:has-text("Close")').click();
        await app.page.waitForTimeout(SETTLE);
        expect(
          await app.page.locator(DRAWER).count(),
          'the visible way out did nothing. Escape is the shortcut; a control a person can ' +
            'point at is the way that has to work without knowing the shortcut.',
        ).toBe(0);
      } finally {
        await app.close();
      }
    }, 90_000);
  }
});
