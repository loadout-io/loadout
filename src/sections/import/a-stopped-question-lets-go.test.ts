/* „Stop" przy porównaniu kopii ma ZAWSZE zwolnić ekran — także wtedy, kiedy pytanie nigdy
 * nie wróci.
 *
 * ZMIERZONE, 2026-08-31. `stopComparing()` wołało Rusta i nic poza tym: lokalne `comparing`
 * czyściła dopiero odpowiedź `compareCopies`, w `.finally()`. Kiedy ta odpowiedź nie wracała
 * — agent zawieszony, kanał zerwany, `stop_comparing_copies` odrzucone — wiersz mówił
 * „An agent is comparing the copies now." BEZ KOŃCA, a każdy inny wiersz miał wyłączone
 * pytanie, bo warunek patrzy na `comparing !== null`. Limitu czasu nie ma nigdzie, więc
 * jedynym wyjściem było zamknięcie okna i utrata całego planu.
 *
 * DLACZEGO CZYSTY MODUŁ, a nie klik. To jest przejście stanu między kliknięciem a odpowiedzią
 * z drugiej strony granicy — w tym repo nie ma jsdom, a `renderToStaticMarkup` nie odpala
 * `onClick`. Niezmiennik 29 daje na to trzy drogi i wybieramy pierwszą: czysty moduł dowodzi
 * TREŚCI zdania, a `e2e/tests/an-agent-compares-the-copies.spec.ts` dowodzi, że ten sam
 * mechanizm stoi na prawdziwej ścieżce po prawdziwym kliknięciu.
 */
import { describe, expect, it } from 'vitest';

import {
  IDLE,
  STOPPED,
  answered,
  asking,
  askFailed,
  refused,
  stopFailed,
  stopped,
} from './comparing';
import type { Comparison } from './setup';

const ITEM = 'audit-item';
const OTHER = 'ship-item';

const SAID: Comparison = {
  itemId: ITEM,
  compared: ['.agents/skills/audit/SKILL.md', '.claude/skills/audit/SKILL.md'],
  said: 'The two copies differ in one line.',
  keep: '.agents/skills/audit/SKILL.md',
};

describe('a comparison the person stopped', () => {
  it('frees the screen even when the question never comes back', () => {
    const working = asking(IDLE, ITEM);
    /* Kontrola przeciw pustej asercji: przed Stopem wiersz NAPRAWDĘ trzyma ekran. */
    expect(working.at, 'nothing was holding the screen, so Stop has nothing to prove').toBe(ITEM);

    const free = stopped(working);

    expect(
      free.at,
      'Stop leaves the row saying an agent is working. The person waits for an answer that is ' +
        'never coming, and every other row keeps its question switched off',
    ).toBeNull();
    expect(
      free.said?.sentence,
      'the row goes quiet after Stop, so nothing on screen says what just happened',
    ).toBe(STOPPED);
    expect(free.said?.item, 'the sentence landed at some other item than the one stopped').toBe(
      ITEM,
    );
    expect(
      STOPPED,
      'the sentence states a fact and stops there. A person reading it still does not know ' +
        'what they may do next',
    ).toMatch(/ask again/);
  });

  it('refuses the answer that arrives after it was stopped', () => {
    const working = asking(IDLE, ITEM);
    const free = stopped(working);

    const late = answered(free, working.ask, ITEM, SAID);

    expect(
      Object.keys(late.answers),
      'an agent answered a question nobody is waiting for any more, and the screen put its ' +
        'sentences under an item the person had already moved on from',
    ).toEqual([]);

    /* A pytanie zadane PO Stopie ma dojść normalnie: unieważnienie dotyczy tego jednego
     * pytania, nie każdego następnego (niezmiennik 7 — monotoniczna generacja, nie flaga). */
    const again = asking(free, OTHER);
    const arrived = answered(again, again.ask, OTHER, { ...SAID, itemId: OTHER });
    expect(
      Object.keys(arrived.answers),
      'one Stop switched off every later question too, so the person can never ask again',
    ).toEqual([OTHER]);
    expect(arrived.at, 'the screen stayed busy after the answer arrived').toBeNull();
  });

  it('names the next move when Loadout could not reach the agent at all', () => {
    /* Tak, jak woła to ekran: Stop zwalnia wiersz OD RAZU, a dopiero potem wraca odmowa
     * z drugiej strony granicy — i wtedy podmienia zdanie przy tej samej pozycji. */
    const letGo = stopped(asking(IDLE, ITEM));
    const free = refused(letGo, letGo.ask, ITEM, stopFailed('Loadout is not there.'));

    expect(
      free.at,
      'a failed Stop left the row claiming an agent is still comparing the copies',
    ).toBeNull();
    expect(
      free.said?.sentence,
      'the sentence repeats what Rust said and stops there, so the person is told a fact and ' +
        'left with no move',
    ).toBe(
      'Loadout is not there. This row is free again — ask again, or decide about it yourself.',
    );
    expect(
      askFailed('Loadout could not ask an agent about those copies.'),
      'a question that failed says nothing about what to try instead',
    ).toBe(
      'Loadout could not ask an agent about those copies. Ask another agent, or decide about ' +
        'it yourself.',
    );
  });
});
