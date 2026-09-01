/* Prawdziwa mysz zawęża strumień do jednego agenta, a prawdziwy klawisz `1` odpowiada na pytanie.
 *
 * PO CO TO ISTNIEJE OBOK KRYTERIÓW W MARKUPIE. Niezmiennik 29, słowo w słowo: funkcja, którą
 * kryterium woła wprost, dowodzi, że mechanizm istnieje; dopiero prawdziwe kliknięcie dowodzi,
 * że człowiek do niego dochodzi. To repo nie ma jsdom, więc `renderToStaticMarkup` nie odpala
 * `onClick` ani razu, a nasłuch klawiatury zakładany w `useEffect` nie odpala się tam wcale —
 * czyli „chip zawęża" i „jedynka odpowiada" są w tamtym środowisku zdaniami, których nie da się
 * sprawdzić w ogóle. Tutaj naciska je prawdziwa mysz i prawdziwa klawiatura w chromium.
 *
 * DRUGA POŁOWA, KTÓREJ NIE UMIE ŻADEN INNY PLIK. Wiersz wejścia tego ekranu łapie kursor sam
 * (`src/sections/run/index.tsx`, `caretBackToTheField`), więc skrót porzucający każde zdarzenie
 * z pola tekstowego byłby w działającej aplikacji martwy — numer na przycisku obiecywałby
 * klawisz, którego nie ma jak nacisnąć (niezmiennik 16). Tę różnicę widać wyłącznie w oknie,
 * w którym ognisko naprawdę gdzieś stoi.
 *
 * CZEGO TEN PLIK NIE DOWODZI. Granica Rusta jest tu atrapą (`../harness.ts`): `run_workflow`
 * nie uruchamia niczego, a linie wjeżdżają tym samym prawdziwym kanałem Tauri, który zapisało
 * wywołanie. O tym, czy po drugiej stronie wstała praca, mówią kryteria rustowe.
 */
import { afterAll, beforeAll, describe, expect, it } from 'vitest';

import type { RunningApp, TauriCall, TauriReply } from '../harness';
import { closeEverything, openApp } from '../harness';

const FOLDER = '/Users/somebody/Projects/loadout-stream-chips';
const WORKSPACE = { id: FOLDER, name: 'Stream chips', folder: FOLDER };
const AGENT = { id: 'agent-stream-chips', name: 'Forge', summary: 'Writes code', skills: [] };

const SCOUT = 'Scout';
const BUILDER = 'Builder';
const NEEDLE = 'Needle';

const SCOUT_SAID = 'Opened the workspace and reproduced the failure on the first try.';
const BUILDER_SAID = 'Reused the open connection instead of opening a second one.';
const QUESTION = 'Two checks still fail on the migration path. Skip them, or fix them first?';
const SKIP = 'Skip them and continue — Second reader gets the change as it stands';
const FIX = 'Fix them first — Builder gets another try before the checks re-run';

const RUN = 'main[data-section="run"]';
const START = 'button[data-workflow-run="manual"]';
const STREAM = '[data-stream-column]';
const ASKED = '[data-asked]';

const APPEARS = 5_000;
const SETTLE = 250;

function step(id: string, name: string, at: number) {
  return {
    kind: 'agent' as const,
    id,
    name,
    agent: AGENT.id,
    overrides: {},
    copies: 1,
    instructions: 'Do the ' + name + ' part.',
    skills: 'all' as const,
    folder: { use: 'project' as const },
    handover: 'notes' as const,
    at: { x: at * 264, y: 24 },
  };
}

const FILE = {
  path: 'stream-chips.json',
  workflow: {
    format: 1 as const,
    id: 'wf-stream-chips',
    name: 'Two steps',
    steps: [step('s-build', 'Build', 0), step('s-check', 'Check', 1)],
    links: [{ from: 's-build', to: 's-check' }],
  },
};

function copies<T>(value: T, count = 24): readonly { readonly value: T }[] {
  return Array.from({ length: count }, () => ({ value }));
}

const SCENE: Readonly<Record<string, readonly TauriReply[]>> = {
  list_workspaces: copies([WORKSPACE]),
  list_workflows: copies([FILE]),
  load_workflow: copies({ workflow: FILE.workflow, revision: 'r1' }, 4),
  check_workflow: copies([], 8),
  list_agents: copies([AGENT]),
  list_skills: copies([]),
  /* Odroczone, więc bieg nigdy nie schodzi w trakcie tej sceny: `runEnded()` czyści kolejkę
   * pytań, a karta, która zniknęła przez upływ czasu, dałaby zieleń o czasie, nie o klawiszu. */
  run_workflow: [{ deferred: 'stream-chips-run' }],
};

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

describe('the stream narrows under a real click and answers under a real key', () => {
  /* JEDEN PRZYPADEK NA CAŁĄ SCENĘ, i to jest wybór, nie skrót: „przedtem słychać obu",
     „po kliknięciu jednego", „po All znowu obu" i „po jedynce pytania nie ma" to jedno zdanie
     o jednym oknie. Rozbite na cztery, każde otwierałoby własną kartę i sądziło stan sprzed
     swojego własnego kliknięcia. */
  it('shows every voice, keeps one when a chip is pressed, and answers when 1 is pressed', async () => {
    const app = await openApp({ replies: SCENE });
    try {
      await app.page.locator(RUN).waitFor({ state: 'attached', timeout: APPEARS });
      await app.page.locator(START).waitFor({ state: 'visible', timeout: APPEARS });
      await app.page.locator(START).click();
      const call = await runCall(app);

      await say(app, call, [
        { kind: 'note', agent: SCOUT, text: SCOUT_SAID, body: [] },
        { kind: 'note', agent: BUILDER, text: BUILDER_SAID, body: [] },
        { kind: 'asked', agent: NEEDLE, text: QUESTION, options: [SKIP, FIX] },
      ]);
      await app.page.locator(STREAM).waitFor({ state: 'visible', timeout: APPEARS });
      await app.page.waitForTimeout(SETTLE);

      // ── PRZEDTEM SŁYCHAĆ OBU ────────────────────────────────────────────────────────────
      const everyone = await app.page.locator(STREAM).innerText();
      expect(
        everyone,
        'the stream does not carry what ' +
          SCOUT +
          ' said, so nothing below is a statement about narrowing — it is a statement about an ' +
          'empty column',
      ).toContain(SCOUT_SAID);
      expect(everyone, 'nor what ' + BUILDER + ' said').toContain(BUILDER_SAID);

      // ── MYSZ ZAWĘŻA DO JEDNEGO ──────────────────────────────────────────────────────────
      const chip = app.page.locator(STREAM + ' button[data-speaker="' + BUILDER + '"]');
      expect(
        await chip.count(),
        'the head of the stream has no chip for ' +
          BUILDER +
          ', who spoke in this run. The chips are counted from the stream, so a missing one ' +
          'means the head never saw the lines',
      ).toBe(1);
      await chip.click();
      await app.page.waitForTimeout(SETTLE);

      const narrowed = await app.page.locator(STREAM).innerText();
      expect(
        narrowed,
        'the chip changed nothing on screen. A row of chips that only shows who is talking is ' +
          'four controls with no effect, which is worse than no control at all (invariant 16) — ' +
          'and reading one agent out of four voices interleaved by time is the thing it exists ' +
          'to make possible',
      ).not.toContain(SCOUT_SAID);
      expect(
        narrowed,
        'and narrowing to ' + BUILDER + ' has to LEAVE ' + BUILDER + ' on screen',
      ).toContain(BUILDER_SAID);

      // ── I ODDAJE RESZTĘ ─────────────────────────────────────────────────────────────────
      await app.page.locator(STREAM + ' button[data-speaker="All"]').click();
      await app.page.waitForTimeout(SETTLE);
      expect(
        await app.page.locator(STREAM).innerText(),
        'there is no way back to the whole run once you have looked at one agent',
      ).toContain(SCOUT_SAID);

      // ── KLAWISZ ODPOWIADA ───────────────────────────────────────────────────────────────
      expect(
        await app.page.locator(ASKED).count(),
        'the run is not standing on a question at all, so pressing a key below would prove ' +
          'nothing about answering',
      ).toBe(1);

      await app.page.keyboard.press('1');
      await app.page.waitForTimeout(SETTLE);

      expect(
        await app.page.locator(ASKED).count(),
        'the number drawn on the button is not a key. The card says "1 or 2 answer" under it, ' +
          'the run is standing still and costing money, and pressing 1 did nothing — which is ' +
          'a shortcut promised and not delivered (invariant 16). Note the caret: the entry row ' +
          'of this screen takes focus by itself, so a listener that drops every event coming ' +
          'from a text field is dead in the running application even though it passes in a test',
      ).toBe(0);

      const answered = await app.page.locator(STREAM).innerText();
      expect(
        answered,
        'the answer a person gave is nowhere in the stream. An answer that leaves no line ' +
          'behind reads exactly like a key that did nothing, and the option chosen is the only ' +
          'record of which way the run was sent',
      ).toContain('Skip them and continue');
    } finally {
      await app.close();
    }
  }, 90_000);
});
