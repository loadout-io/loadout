/* Ukośnik z nazwą umiejętności jedzie do lidera ZNAK W ZNAK — sądzone w prawdziwym chromium.
 *
 * # Dlaczego to nie może być kryterium bez okna
 *
 * Bo rzecz, która była zepsuta, mieszka w DWÓCH plikach naraz i widać ją dopiero razem: ekran
 * pracy nigdy nie pytał o umiejętności (`list_skills` wołał wyłącznie ekran Knowledge), więc
 * zbiór nazw w wierszu wejścia był pusty, gałąź się nie zapalała, a `send()` odpowiadało
 * „That one is not known here." Test na samej czystej funkcji przechodziłby nad wierszem, który
 * tej listy nie dostaje — czyli dowodziłby mechanizmu przy martwej drodze (niezmiennik 29).
 *
 * # Zdanie człowieka nie ma prawa zmienić się po drodze
 *
 * Zmierzone 2026-09-01 na żywym `claude` 2.1.257: linia ZACZYNAJĄCA SIĘ od ukośnika z nazwą
 * umiejętności rozwija się po stronie klienta, zanim ruszy tura modelu. Ten sam ukośnik
 * postawiony w środku zdania („Please use /harbor-inventory to answer this.") skończył się
 * czternastoma wywołaniami Glob/Grep i odpowiedzią, że takiej umiejętności nie ma. Dlatego
 * asercja jest na RÓWNOŚCI tekstu, nie na `toContain`: owinięcie linii we własne zdanie Loadouta
 * jest dokładnie tym kształtem, który zmierzono jako niedziałający.
 */
import { afterAll, beforeAll, describe, expect, it } from 'vitest';

import type { OpenAppOptions, RunningApp, TauriCall, TauriReply } from '../harness';
import { closeEverything, openApp } from '../harness';

const WORK = 'main[data-section="run"]';
const FIELD = '[aria-label="Command line"]';
const FOLDER = '/Users/somebody/Projects/loadout-skill-line-e2e';
const WORKSPACE = { id: FOLDER, name: 'Skill line fixture', folder: FOLDER };
const SKILL = 'harbor-inventory';
const TYPO = '/harbr-inventory';
const NOT_KNOWN = 'That one is not known here.';
const AGENT = {
  schema: 1,
  id: '0198a1f2-3b4c-7d5e-8f60-99aabbccddee',
  name: 'Lead fixture',
  summary: 'Leads the test conversation',
  color: 'slate',
  instructions: 'Answer the person.',
  runsWith: 'claude-code',
  model: 'sonnet',
  thinking: 'balanced',
  fileAccess: 'look-only',
  giveUpAfterMinutes: 10,
  tools: 'everything',
  skills: [SKILL],
  connections: [],
  writeResultsTo: 'handoffs/lead.md',
};
const APPEARS = 4_000;
const SETTLE = 500;

/* TA SAMA ODPOWIEDŹ NA KAŻDE PYTANIE, a nie jedna w kolejce: atrapa zdejmuje odpowiedzi
 * `shift()`-em i po opróżnieniu kolejki oddaje `[]` na każde `list_*` (`e2e/harness.ts`).
 * Kolejka długości jeden opisywałaby dysk, który ma umiejętność przy pierwszym pytaniu
 * i żadnej przy drugim — a to nie jest dysk, tylko licznik wywołań przebrany za fiksturę.
 * Powód w całości stoi przy `copies` w `pasted-image-reaches-lead.spec.ts`. */
function copies<T>(value: T, count = 24): readonly { readonly value: T }[] {
  return Array.from({ length: count }, () => ({ value }));
}

function withReplies(said: readonly TauriReply[]): OpenAppOptions {
  return {
    replies: {
      list_workspaces: copies([WORKSPACE]),
      list_agents: copies([AGENT]),
      list_skills: copies([{ name: SKILL, fromTheInternet: false, summary: 'Reads the harbor.' }]),
      say_to_orchestrator: said,
    },
  };
}

async function openConversation(said: readonly TauriReply[] = []): Promise<RunningApp> {
  const app = await openApp(withReplies(said));
  await app.page.locator(WORK).waitFor({ state: 'attached', timeout: APPEARS });
  await app.page.locator(FIELD).waitFor({ state: 'attached', timeout: APPEARS });
  const lead = app.page.getByLabel('Lead agent');
  await lead.waitFor({ state: 'attached', timeout: APPEARS });
  await lead.selectOption(AGENT.id);
  return app;
}

function sentToLead(app: RunningApp): Promise<readonly TauriCall[]> {
  return app.calls().then((calls) => calls.filter((call) => call.cmd === 'say_to_orchestrator'));
}

/** Wszystkie wiersze strumienia, sklejone — po to, żeby zapytać, czego w nich NIE MA. */
async function everythingInTheStream(app: RunningApp): Promise<string> {
  const rows = await app.page.locator('[data-line]').allInnerTexts();
  return rows.join('\n');
}

beforeAll(async () => {
  const warm = await openApp();
  await warm.close();
}, 180_000);

afterAll(async () => {
  await closeEverything();
}, 30_000);

describe('a slash that names a skill on the real work screen', () => {
  it('sends a slash that names a skill to the lead, word for word', async () => {
    const app = await openConversation([{ value: null }]);
    try {
      await app.page.fill(FIELD, '/' + SKILL);
      await app.page.press(FIELD, 'Enter');
      await app.page.waitForTimeout(SETTLE);

      const calls = await sentToLead(app);
      expect(
        calls.length,
        'the person typed a slash that names a skill their lead has, and nothing left the ' +
          'window. The work screen never asks what skills are on this machine, so the line ' +
          'bounces off as an unknown command.',
      ).toBe(1);
      expect(
        calls[0]?.args['text'],
        'the line reached the lead with something other than what was typed. A skill only ' +
          'runs when the slash STARTS the line: wrapped in a sentence of ours it is prose, ' +
          'and the model answers that it has no such skill.',
      ).toBe('/' + SKILL);

      expect(
        await everythingInTheStream(app),
        'the line went to the lead and the window still answered as though it did not know ' +
          'the word.',
      ).not.toContain(NOT_KNOWN);
    } finally {
      await app.close();
    }
  }, 90_000);

  it('still bounces a typo back without leaving the window', async () => {
    const app = await openConversation();
    try {
      await app.page.fill(FIELD, TYPO);
      await app.page.press(FIELD, 'Enter');
      await app.page.waitForTimeout(SETTLE);

      expect(
        await everythingInTheStream(app),
        'a misspelled skill name has to read as a typo in a command, not as a message to the ' +
          'lead: sending it as prose looks exactly like a command that was ignored.',
      ).toContain(NOT_KNOWN);
      expect((await sentToLead(app)).length, 'the typo was paid for as a turn with the lead.').toBe(
        0,
      );
    } finally {
      await app.close();
    }
  }, 90_000);
});
