/* Zdanie o CAŁOŚCI: tyle kafelków, ilu agentów naprawdę nadało. Ani jednego więcej.
 *
 * Kryterium 1 tego zadania pyta o gałąź — historia z samego okna, historia z jedną linią
 * agenta, pod-agent spoza planu. Ten plik pyta o strumień, jaki naprawdę widzi człowiek:
 * echo komend z okna, proza lidera, dwie linie dwóch różnych kroków pracy i jedna linia
 * pod-agenta rozpuszczonego w trakcie. Liczba kafelków ma się zgadzać co do sztuki.
 *
 * SŁABA WERSJA: porównanie samej liczby. Przechodzi dla implementacji, która zgubiła
 * pod-agenta i dołożyła widmo — dwa błędy znoszące się w jednej liczbie. Rozróżnia to
 * porównanie CAŁEJ listy nazw, razem z jej kolejnością.
 *
 * KOLEJNOŚĆ JEST TU MIERZONA, a nie zakładana: kafelki stoją w kolejności PIERWSZEGO
 * pojawienia się w strumieniu [roster.ts, komentarz przy `said`], a to jest własność, którą ta
 * naprawa może po cichu zepsuć — na przykład przesiewając historię dopiero po zbudowaniu mapy
 * albo układając listę z powrotem po planie. Plan niżej wymienia agentów w INNEJ kolejności
 * niż strumień, więc lista ułożona po planie wykłada się tutaj i tylko tutaj.
 *
 * Podpis wierszy okna nie jest wpisany z palca: bierzemy go z wiersza, który wydał moduł okna
 * (`../entry/echo`). Wpisany z palca byłby drugą tabelą prawdy o tym samym (niezmiennik 13)
 * i przestałby cokolwiek sądzić w dniu, w którym tamten podpis się zmieni.
 */
import { describe, expect, it } from 'vitest';

import type { Incoming } from '../../../state/run';
import type { WindowLine } from '../entry/echo';
import { echoOf, saidOf } from '../entry/echo';
import { line } from '../feed/fixtures/lines';
import { sealedScroller } from '../feed/fixtures/scroller';
import { createFeed } from '../feed/model';
import type { AgentFacts, RosterInput } from './roster';
import { roster } from './roster';

const LEAD = 'Lead';
const FORGE = 'Forge';
const NEEDLE = 'Needle';

/** Pod-agent. Nadaje, ma numer z pompy i nie ma kroku — żaden plan nie zna go z góry. */
const SCOUT = 'Scout';

/**
 * Plan: trzy kroki, wymienione w kolejności grafu.
 *
 * Ta kolejność jest CELOWO inna niż kolejność nadawania w strumieniu niżej, a Scouta w niej
 * nie ma wcale. Lista ułożona po planie jest nie do odróżnienia od poprawnej dopóty, dopóki
 * oba porządki się zgadzają — a przy pracy równoległej nie zgadzają się prawie nigdy.
 */
const PLAN: readonly AgentFacts[] = [
  { id: FORGE, name: FORGE, role: 'writes code', step: 'running' },
  { id: NEEDLE, name: NEEDLE, role: 'runs checks', step: 'failed' },
  { id: LEAD, name: LEAD, role: 'lead', step: 'running' },
];

/**
 * Czterej agenci, którzy w tym strumieniu naprawdę nadali — w kolejności pierwszego wiersza.
 *
 * Lider jest tu, bo napisał prozą własnymi słowami; pod-agent jest, bo nadał, choć planu nie
 * widział na oczy. Okna nie ma, bo okno nie jest agentem: nikt go nie uruchomił i nic w nim
 * nie pracuje.
 */
const WHO_SAID_SOMETHING: readonly string[] = [LEAD, FORGE, SCOUT, NEEDLE];

/** Wiersz, który okno dopisuje po wpisanej komendzie. */
function typedInTheWindow(command: string): WindowLine {
  const row = echoOf(command);
  if (row === null) {
    throw new Error('the window wrote no row for ' + JSON.stringify(command));
  }
  return row;
}

/**
 * Strumień, jaki widzi człowiek po pierwszym `/run`: pięć wierszy złożonych w oknie
 * i cztery linie czterech różnych agentów, przeplecione tak, jak przeplatają się naprawdę.
 *
 * Wierszy okna jest WIĘCEJ niż agentów i to jest kontrola: implementacja licząca wiersze
 * zamiast agentów nie ma tu ani jednej liczby, w którą mogłaby trafić przypadkiem.
 */
function mixedStream(): RosterInput {
  const feed = createFeed(sealedScroller());
  const rows: readonly Incoming[] = [
    typedInTheWindow('/run ship-it'),
    line.note(1, 1_000, LEAD, 'Three steps; the last one only runs if the checks fail.'),
    saidOf('Nothing is running yet.'),
    line.edit(2, 2_000, FORGE, 'src/parser.rs', 42, 8),
    typedInTheWindow('/open ' + FORGE),
    line.read(3, 3_000, SCOUT, 'docs/csv-edge-cases.md'),
    typedInTheWindow('/stop'),
    line.ran(4, 4_000, NEEDLE, 'Ran the checks', false, ['3 of 40']),
    saidOf('Nothing is running.'),
  ];
  feed.appendLines(rows);
  return { view: feed.view, agents: PLAN };
}

describe('the agents list holds one tile per agent that really said something', () => {
  it('counts the agents in the stream, not the rows in it', () => {
    expect(
      roster(mixedStream()).length,
      'four agents said something here: the lead in its own prose, two steps of the work, and ' +
        'one sub-agent nobody could name in advance. Every other row was written by the window ' +
        'after a slash, and a window is not something that works (invariant 17).',
    ).toBe(WHO_SAID_SOMETHING.length);
  });

  it('gives no tile the name the window signs its rows with', () => {
    const signature = typedInTheWindow('/stop').agent;
    const cards = roster(mixedStream());

    expect(
      cards.map((card) => card.id).includes(signature),
      'the name the window signs with reached the list as an agent: ' +
        JSON.stringify(signature) +
        '. Two mistakes that cancel out in a single count — one agent lost, one ghost added — ' +
        'are why this case asks for names and not for a number.',
    ).toBe(false);
    expect(
      cards.map((card) => card.name).includes(signature),
      'and it must not reach the visible name either. The tile shows a name, not a key, so ' +
        'a fix that only hides the key leaves the same word on the screen.',
    ).toBe(false);
  });

  it('keeps the tiles in the order the agents first appeared', () => {
    expect(
      roster(mixedStream()).map((card) => card.id),
      'first appearance in the stream, not position in the plan — the plan names them Forge, ' +
        'Needle, Lead and never heard of Scout, while the stream heard Lead, Forge, Scout, ' +
        'Needle. Order is what this fix can break in silence: sift the story after the map is ' +
        'built, or rebuild the list from the plan, and the count still looks right.',
    ).toEqual(WHO_SAID_SOMETHING);
  });

  it('is built on a scene with more window rows than agents', () => {
    const history = mixedStream().view.history;
    const written = history.filter((row) => row.id < 0).length;

    expect(
      history.length,
      'every row of the scene has to reach the story. One dropped on the way in and the counts ' +
        'above stop measuring what they name.',
    ).toBe(9);
    expect(
      written,
      'and the rows the window wrote have to outnumber the agents, or an implementation that ' +
        'counts rows instead of agents could land on the right number by accident.',
    ).toBeGreaterThan(WHO_SAID_SOMETHING.length);
  });
});
