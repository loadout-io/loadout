/* Kryterium 2: kolor tożsamości i kolor stanu nigdy nie pochodzą z tego samego zbioru.
 *
 * `expect(identityToken('forge')).not.toBe(statusToken('running'))` na kilku parach
 * przechodzi dla implementacji z błędem zawijania, która przy szóstym agencie sięga po
 * `--color-attend` — czyli robi dokładnie ten błąd, przez który ta reguła w ogóle powstała:
 * w referencyjnym redesign poprzedniego prototypu agent Forge miał ten sam hex, co „czeka na twoją
 * decyzję". Kilka par nie ma jak tego zobaczyć, bo tych par nigdy nie ma dość.
 *
 * Rozróżniają to trzy rzeczy naraz:
 *   1. przeliczenie WSZYSTKICH czterdziestu agentów i asercja na ZBIORACH — obraz zawarty
 *      w tożsamości, przecięcie obu palet puste,
 *   2. `statusToken` na wszystkich sześciu stanach: obraz ma dokładnie cztery elementy,
 *      więc żaden stan nie wymyka się do piątego koloru,
 *   3. kwadrat kafelka agenta `failed` — stan jest SŁOWEM, nie kolorem kwadratu [DESIGN §3].
 *
 * Stabilność przydziału sprawdzamy przez `roster`, a nie przez samo `identityToken`, i to
 * jest świadome: przydział liczony z POZYCJI w liście jest jedyną wersją tego błędu, która
 * naprawdę się zdarza, a widać ją tylko tam, gdzie lista istnieje. Ten sam agent ma dostać
 * ten sam kwadrat, kiedy poda się bieg w innej kolejności i kiedy dojdzie nowy agent.
 */
import { describe, expect, it } from 'vitest';
import type { FeedLine } from '../../../state/run';
import { line } from '../feed/fixtures/lines';
import { sealedScroller } from '../feed/fixtures/scroller';
import { createFeed } from '../feed/model';
import type { AgentStatus } from './card';
import { railCard } from './card';
import { identityToken, statusToken } from './colour';
import type { AgentFacts } from './roster';
import { roster } from './roster';

/** Pięć przygaszonych. Literały, nie import — sprawdzenie czytające własne stałe nic nie mówi. */
const IDENTITY = ['--color-id-1', '--color-id-2', '--color-id-3', '--color-id-4', '--color-id-5'];

/** Cztery nasycone. Rozłączne z tamtymi i to jest cała reguła. */
const STATE_COLOURS = ['--color-accent', '--color-attend', '--color-fail', '--color-muted'];

const STATES: readonly AgentStatus[] = [
  'working',
  'waiting',
  'needs you',
  'failed',
  'done',
  'stopped',
];

/** Czterdziestu różnych agentów — osiem pełnych obrotów po pięcioelementowej palecie. */
const FORTY = Array.from({ length: 40 }, (_unused, i) => 'agent-' + String(i));

function facts(id: string): AgentFacts {
  return { id, name: id, role: 'writes code', step: 'running' };
}

/** Kafelki dla biegu, w którym ci agenci nadali po jednej linii, w podanej kolejności. */
function squaresFor(order: readonly string[]): ReadonlyMap<string, string> {
  const feed = createFeed(sealedScroller());
  const lines: FeedLine[] = order.map((id, i) => line.read(i + 1, i * 100, id, 'src/parser.rs'));
  feed.appendLines(lines);

  const cards = roster({ view: feed.view, agents: order.map(facts) });
  return new Map(cards.map((card) => [card.id, card.square]));
}

describe('identity colour and state colour never come from the same set', () => {
  it('sends forty agents into the five quiet colours and nowhere else', () => {
    const given = FORTY.map(identityToken);

    const stray = given.filter((name) => !IDENTITY.includes(name));
    expect(
      stray,
      'the image over forty agents is contained in the five quiet colours. A wrap that ' +
        'reaches for a sixth name is invisible on a handful of pairs and certain over forty',
    ).toEqual([]);
    expect(
      new Set(given).size,
      'and it is not one colour for everyone: a constant answer is contained in the set too, ' +
        'and makes the list of agents unscannable, which is the only reason it has colours',
    ).toBeGreaterThan(1);

    const shared = given.filter((name) => STATE_COLOURS.includes(name));
    expect(
      shared,
      'the two palettes have an empty intersection. Once they share one name, an orange ' +
        'agent and "waiting for you" mean the same thing and people stop trusting colour',
    ).toEqual([]);
  });

  it('answers for all six agent states, out of exactly four colours', () => {
    const given = STATES.map(statusToken);

    expect(given.length, 'the mapping is total: every state has an answer').toBe(6);
    for (const name of given) {
      expect(STATE_COLOURS, 'and every answer comes from the four-colour set').toContain(name);
    }
    expect(
      new Set(given).size,
      'exactly four distinct colours across the six states — a fifth means one state ' +
        'quietly grew its own colour',
    ).toBe(4);
    expect(
      statusToken('done'),
      'a finished thing is quiet [DESIGN §3]. Green means "happening now", not "it worked", ' +
        'and that is what separates Loadout from every dashboard that glows when nothing ' +
        'is going on',
    ).toBe('--color-muted');
  });

  it('never paints the square of a broken agent in the colour of being broken', () => {
    const card = railCard({
      id: 'Needle',
      name: 'Needle',
      role: 'runs checks',
      status: 'failed',
      lines: [line.ran(1, 0, 'Needle', "Ran tests — didn't work", false, ['3 of 40 failed'])],
    });

    expect(card.status, 'the state is a word').toBe('failed');
    expect(
      STATE_COLOURS.includes(card.square),
      'and it is never the colour of the square. The square says who this is; the word says ' +
        'how it went. One surface answering two questions with one colour is the defect this ' +
        'whole rule exists to prevent',
    ).toBe(false);
    expect(IDENTITY).toContain(card.square);
  });

  it('gives an agent the same square when the run is replayed in another order', () => {
    const forward = squaresFor(['Forge', 'Needle', 'Rivet']);
    const backward = squaresFor(['Rivet', 'Needle', 'Forge']);
    const joined = squaresFor(['Orion', 'Forge', 'Needle', 'Rivet']);

    expect(
      backward.get('Forge'),
      'the square belongs to the agent, not to its position. Handing out colours by index ' +
        'looks right on the first screenshot and repaints half the list the moment a ' +
        'sub-agent joins in the middle of a run',
    ).toBe(forward.get('Forge'));
    expect(joined.get('Forge')).toBe(forward.get('Forge'));
    expect(joined.get('Rivet')).toBe(forward.get('Rivet'));
  });
});
