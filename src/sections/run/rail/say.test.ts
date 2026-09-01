/* Kryterium 6: „latest note from this agent" jest cytatem agenta i jest tak oznaczone.
 *
 * `expect(card.say.text).toBe(lastNote.text)` przechodzi dla implementacji, która za
 * „ostatnią wypowiedź agenta" bierze cokolwiek, co przyszło ostatnie — także `problem`
 * i podsumowanie sprawdzeń. Wtedy zdanie Loadouta („3 of 40 tests failed") jest podane jako
 * cytat agenta, czyli `agent said` w rubryce `happened` [FOUNDATIONS §2.2]. To jest ten sam
 * błąd, przed którym stoi kryterium 3, tylko mniejszą czcionką — i dlatego trudniejszy.
 *
 * Rozróżniają to dwie rzeczy:
 *   1. `ran` z `ok: false` PO `note`. Ta sama notatka, ta sama kolejność, jedna linia różnicy
 *      — a odpowiedź ma się zmienić, bo sprawdzenia to Loadout, nie agent. Kontrola obok:
 *      bez tej jednej linii ta sama notatka wraca podpisana `agent`, więc różnicę robi
 *      naprawdę `ran`, a nie to, że notatki nie dało się przeczytać.
 *   2. zamknięty, trzyelementowy zbiór `who`. Czwarte słowo o tym, kto coś powiedział, jest
 *      dokładnie tym, jak w poprzednim prototypie zrobiło się osiem rodzajów „autorytetu".
 */
import { describe, expect, it } from 'vitest';
import type { FeedLine, Who } from '../../../state/run';
import { line } from '../feed/fixtures/lines';
import type { AgentInRun } from './card';
import { railCard } from './card';
import { AUTHORITIES } from './say';

const A = 'Forge';

const NOTE =
  'The quote handling only looks at the first character of a field, which is why an ' +
  'embedded comma inside quotes splits the row.';

function forge(lines: readonly FeedLine[]): AgentInRun {
  return { id: A, name: 'Forge', role: 'writes code', status: 'working', lines };
}

describe('the one sentence on a card says who said it', () => {
  it('quotes the last thing the agent wrote in prose, and marks it as the agent', () => {
    const card = railCard(forge([line.read(1, 0, A, 'src/parser.rs'), line.note(2, 400, A, NOTE)]));

    expect(card.say.text).toBe(NOTE);
    expect(card.say.who, 'prose is the one thing on this card that really is the agent').toBe(
      'agent',
    );
  });

  it('hands a failed check back to Loadout, even when the agent wrote prose first', () => {
    const lines = [
      line.note(1, 0, A, NOTE),
      line.ran(2, 400, A, "Ran tests — didn't work", false, [
        'parser_handles_quoted_commas ... FAILED',
        '3 of 40 failed',
      ]),
    ];

    const card = railCard(forge(lines));

    expect(
      card.say.who,
      'the checks are Loadout speaking, not the agent [FOUNDATIONS §2.2]. Handing this ' +
        'sentence over with the agent name on it is the same defect as putting "I fixed ' +
        'everything" in the block of facts, only smaller',
    ).toBe('loadout');
    expect(
      card.say.text,
      'and it is not the note either. The note is still the last prose this agent wrote; ' +
        'it is simply no longer the latest thing that happened',
    ).not.toBe(NOTE);
    expect(card.say.text.trim().length).toBeGreaterThan(0);

    // Kontrola: ta sama notatka, bez tej jednej linii. Jeśli i tu wyjdzie `loadout`, to
    // różnicy nie robi `ran` — tylko to, że prozy nie widać w ogóle, a wtedy przypadek
    // wyżej niczego nie poświadczył.
    const withoutTheRun = railCard(forge([line.note(1, 0, A, NOTE)]));
    expect(withoutTheRun.say.who).toBe('agent');
    expect(withoutTheRun.say.text).toBe(NOTE);
  });

  it('does not read an error line as something the agent said', () => {
    const card = railCard(
      forge([
        line.note(1, 0, A, NOTE),
        line.problem(2, 400, A, 'Could not reach the model — trying again'),
      ]),
    );

    expect(
      card.say.who,
      'a problem is Loadout reporting, in the same way a check is. An implementation that ' +
        'takes the newest line with text on it gets this one wrong too',
    ).toBe('loadout');
  });

  it('says what the agent is doing when the agent has written no prose at all', () => {
    const card = railCard(
      forge([line.read(1, 0, A, 'tests/parser.rs'), line.edit(2, 400, A, 'src/parser.rs', 42, 8)]),
    );

    expect(card.say.who, 'Loadout is describing the work, so Loadout signs it').toBe('loadout');
    expect(card.say.text.trim().length, 'never the empty string').toBeGreaterThan(0);
    expect(
      card.say.text,
      'and never a made-up sentence: it names the work that is actually in the run',
    ).toContain('src/parser.rs');
  });

  it('still says something when all the agent has done is think', () => {
    const card = railCard(forge([line.thinking(1, 0, A)]));

    expect(
      card.say.text.trim().length,
      'an empty card line reads as a broken agent',
    ).toBeGreaterThan(0);
    expect(card.say.who).toBe('loadout');
  });

  it('never signs a sentence with a fourth name', () => {
    const scenes: readonly (readonly FeedLine[])[] = [
      [line.note(1, 0, A, NOTE)],
      [line.ran(1, 0, A, 'Ran tests — ok', true, [])],
      [line.ran(1, 0, A, "Ran build — didn't work", false, ['1 error'])],
      [line.problem(1, 0, A, 'Could not reach the model — trying again')],
      [line.asked(1, 0, A, 'Which database should it use?', ['Postgres', 'SQLite'])],
      [line.handoff(1, 0, A, 'Forge → Needle')],
      [line.memory(1, 0, A, 'api-conventions.md')],
      [line.done(1, 0, A, 'Finished in 4m 12s')],
    ];

    const signed = scenes.map((lines) => railCard(forge(lines)).say.who);
    for (const who of signed) {
      expect(AUTHORITIES, 'every sentence is signed by one of the three').toContain(who);
    }

    const three: readonly Who[] = ['agent', 'loadout', 'you'];
    expect(
      [...AUTHORITIES].sort(),
      'three names for who said it, in the whole application, not eight. poprzedni prototyp grew ' +
        'eight and then had to explain them; this set is closed so a ninth cannot be added ' +
        'without this line going red',
    ).toEqual([...three].sort());
  });
});
