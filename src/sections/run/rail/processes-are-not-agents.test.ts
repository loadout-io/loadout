/* Kafelek istnieje dokładnie tak długo, jak rzecz za nim — i stoi obok agentów, nie w nich.
 *
 * SŁABA WERSJA: `toHaveLength(1)` na jednej żywej rzeczy. Przechodzi dla implementacji, która
 * zostawia kafelki po tych, które zeszły — czyli dla „Running" nad komendą zeszłą dwie minuty
 * temu. To jest ta sama klasa wady, którą zamknęły T-66 (widmowy agent z planu) i T-67 (wiersz
 * okna udający agenta), i ta sama, która wraca powierzchnia po powierzchni, bo za każdym razem
 * wchodzi inną. Rozróżnia to przypadek drugi niżej: rzecz, która zeszła, NIE MA kafelka w żadnej
 * z dwóch grup.
 *
 * DRUGA SŁABA WERSJA, mniej widoczna: policzenie samych kafelków. Implementacja, która zgubi
 * agenta i dołoży widmo, ma tę samą liczbę — dlatego wszystkie asercje niżej pytają o NAZWY,
 * a przypadek trzeci porównuje całą listę agentów co do wartości.
 *
 * SCENA JEST PRZESIEWALNA W OBIE STRONY z rozmysłu: jeden agent, jedna rzecz żywa, jedna zeszła.
 * Implementacja, która przepuszcza wszystko, wykłada się na drugiej; ta, która nie przepuszcza
 * niczego, wykłada się na pierwszej; ta, która zlewa oba rodzaje w jedną listę, wykłada się na
 * trzeciej.
 */
import { describe, expect, it } from 'vitest';

import type { RailCard } from './card';
import { railCard } from './card';
import { IDENTITY, STATUS } from './colour';
import type { GroupsInput } from './processes';
import { railGroups } from './processes';

/** Agent, który naprawdę coś nadał — jedyny powód, dla którego agent ma kafelek. */
const AGENT = 'Forge';

/** Wiersz powłoki, który jeszcze biegnie. */
const UP = 'npm run dev';

/** Wiersz powłoki, który zszedł. Kafelka nie ma dostać w ogóle. */
const DOWN = 'python3 -m http.server 8000';

/**
 * Kafelek agenta, złożony tą samą funkcją, którą składa go prawdziwy bieg.
 *
 * Nie ręcznie zapisany obiekt: przypadek trzeci porównuje całą listę agentów co do wartości, więc
 * gdyby ta scena budowała kafelek własnym literałem, porównywałaby go z kształtem, którego lista
 * nigdy nie widzi (niezmiennik 13 — jeden fakt, jedno miejsce).
 */
const AGENT_TILE: RailCard = railCard({
  id: AGENT,
  name: AGENT,
  role: 'writes code',
  status: 'working',
  lines: [{ kind: 'note', text: 'Reading the parser first.' }],
});

/** Jeden z każdego rodzaju plus jeden, który zszedł. */
function scene(): GroupsInput {
  return {
    agents: [AGENT_TILE],
    started: [
      { id: 'up-4213', command: UP, alive: true },
      { id: 'down-4188', command: DOWN, alive: false },
    ],
  };
}

/** Dokładnie jeden kafelek, albo zdanie o tym, ile ich naprawdę było. */
function onlyTile(cards: readonly RailCard[], why: string): RailCard {
  const [first] = cards;
  if (first === undefined || cards.length !== 1) {
    throw new Error(why + ' There are ' + String(cards.length) + ' of them instead.');
  }
  return first;
}

/** Wszystkie nazwy z obu grup, w jednej liście — do pytań „czy gdziekolwiek". */
function everyName(): readonly string[] {
  const groups = railGroups(scene());
  return [...groups.agents, ...groups.started].map((card) => card.name);
}

describe('the agents list keeps agents and started commands apart', () => {
  it('gives the one that is still up a tile of its own, outside the agents', () => {
    const groups = railGroups(scene());

    const tile = onlyTile(
      groups.started,
      'one line was started and is still up, so there is one tile for it.',
    );
    expect(
      tile.name,
      'the tile carries the line the person typed, character for character — that line IS its ' +
        'name. An invented label reads like a fact Loadout knows and is a relation that is not ' +
        'in the data (invariant 17); worse, the person then cannot find on the list the thing ' +
        'they themselves asked for.',
    ).toBe(UP);

    expect(
      groups.agents.map((card) => card.name),
      'and it does not join the agents. Those are two different kinds of thing: one Loadout is ' +
        'leading, the other the person asked it to run, and the stop button under each means ' +
        'something else. A single mixed list is also the version that passes when the only tile ' +
        'on screen belongs to an agent of a run that happens to be going.',
    ).not.toContain(UP);
  });

  it('gives no tile at all to the one that already went down', () => {
    const names = everyName();

    expect(
      names,
      '"Running" over a line that went down two minutes ago is the same lie as a tile for an ' +
        'agent nobody ever started. A tile lives exactly as long as the thing behind it ' +
        '(invariant 17), so this one has no tile in either group — not a greyed-out one, not one ' +
        'that says "done".',
    ).not.toContain(DOWN);

    expect(
      names,
      'and nothing was lost on the way out: the agent that said something keeps its tile, and ' +
        'the line that is still up keeps its own. An implementation that drops one and invents ' +
        'another lands on the same count, which is why this asks for names.',
    ).toEqual([AGENT, UP]);
  });

  it('leaves the agent tile exactly as it was handed in', () => {
    expect(
      railGroups(scene()).agents,
      'the agents group is the one that came in, value for value. This file may only ADD a ' +
        'second group beside it; rebuilding the agent tiles here would put the answer to "what ' +
        'did this agent last say" in two places, and one of them is always the stale one ' +
        '(invariant 13).',
    ).toEqual([AGENT_TILE]);
  });

  it('paints the started line from the quiet colours, never from the four that mean state', () => {
    const tile = onlyTile(
      railGroups(scene()).started,
      'this case needs the one live tile to ask about its square.',
    );

    expect(
      IDENTITY,
      'the square is identity, so it comes from the quiet palette — the same one every agent ' +
        'draws from. What tells the two kinds apart is WHERE the tile stands, never its shade. ' +
        'It picked ' +
        tile.square +
        '.',
    ).toContain(tile.square);
    expect(
      STATUS,
      'and never from the four saturated ones. That is the rule the reference redesign broke: it ' +
        'gave an agent exactly the shade that meant "waiting for your decision" on the tile next ' +
        'to it, and after that nobody trusts any colour on the screen [DESIGN §3].',
    ).not.toContain(tile.square);
  });

  it('is built on a scene with one of each kind and one that went down', () => {
    const built = scene();

    expect(
      built.agents.length,
      'every assertion above compares two groups, so the scene needs something in the first one. ' +
        'Zero agents makes "they do not mix" true for free.',
    ).toBe(1);
    expect(
      built.started.filter((one) => one.alive).length,
      'one line still up, or "it gets a tile" is a statement about nothing.',
    ).toBe(1);
    expect(
      built.started.filter((one) => !one.alive).length,
      'and one that already went down, or "it gets no tile" passes for an implementation that ' +
        'never sifts anything.',
    ).toBe(1);
  });
});
