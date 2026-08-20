/* Wiersz złożony w oknie nie ma prawa stać w strefie TERAZ — a do historii wchodzi dalej.
 *
 * ZMIERZONA WADA. `appendLines` zapisuje KAŻDĄ linię trasy `history` do mapy `doing`
 * (`doing.set(line.agent, …)`), a strefa TERAZ jest tą mapą. Po T-58 każda komenda wpisana
 * w wiersz wejścia i każda odpowiedź, którą ten wiersz daje sam sobie, składa wiersz podpisany
 * oknem — więc po pierwszym `/stop` przy niczym niebiegnącym w strefie „co się dzieje teraz"
 * siada wpis „Loadout — Nothing is running.", nieodróżnialny od pracującego agenta, i stoi tam
 * do końca pracy. Agent, który nie pracuje, nie ma prawa stać w tej strefie (niezmiennik 17),
 * a jest to jeden z dwóch regionów, którym ARCHITECTURE §7 pozwala się ruszać — czyli miejsce,
 * w które człowiek patrzy, żeby wiedzieć, czy cokolwiek żyje.
 *
 * DLACZEGO TO KRYTERIUM SĄDZI MODEL, A NIE KOMPONENT. `now.tsx` rysuje `now.rows` bez własnego
 * sita i tak ma zostać: kuracja mieszka w mapowaniu zdarzenie→linia (niezmiennik 15, decyzja D4),
 * a `doing` czytają dwie powierzchnie — ta strefa i szyna agentów przez `../rail/roster.ts`,
 * która na tę samą klasę wady wpadła osobno i osobno ją zamknęła. Sito dopisane w widoku
 * załatałoby objaw i zostawiło mapę pełną widm dla następnego czytającego (niezmiennik 13).
 *
 * NOŚNIK JEST W DANYCH, NIE W NAZWIE. Ujemny numer wydaje wyłącznie `../entry/echo.ts` i wydaje
 * go dlatego, że obie pompy — biegu i rozmowy — stemplują od 1 każda z osobna. Dlatego jeden
 * z przypadków niżej podpisuje wiersz okna nazwą PRAWDZIWEGO agenta: lista zakazanych nazw
 * byłaby drugą tabelą prawdy o tym samym (niezmiennik 13) i myliłaby się w obie strony.
 * Wiersze okna składa tu kod produkcyjny, a nie literał w teście — scena z minusem wpisanym
 * z palca przechodziłaby także wtedy, gdyby tamten moduł przestał go nadawać.
 *
 * SŁABA WERSJA TEGO KRYTERIUM: samo `expect(view.now.rows).toHaveLength(0)` po paczce z okna.
 * Przechodzi dla implementacji, która nie wpuszcza do TERAZ niczego — czyli kasuje strefę
 * będącą jednym z dwóch żywych regionów ekranu. Rozróżnia to przypadek prawdziwego agenta
 * i dlatego stoi w tym samym pliku.
 */
import { describe, expect, it } from 'vitest';

import type { Incoming } from '../../../state/run';
import type { WindowLine } from '../entry/echo';
import { echoOf, saidOf } from '../entry/echo';
import { line } from './fixtures/lines';
import { sealedScroller } from './fixtures/scroller';
import { createFeed } from './model';

/** Agent z planu, który naprawdę nadaje. */
const FORGE = 'Forge';

/** Zdanie, które Forge naprawdę wypowiedział — jedyne, jakie ma prawo stać w strefie TERAZ. */
const FORGE_SAID = 'Rewriting the splitter.';

/** Odpowiedź, którą wiersz wejścia daje sam sobie, kiedy nic nie biegnie. */
const NOTHING_RUNS = 'Nothing is running.';

/** Wiersz, który okno dopisuje po wpisanej komendzie. Numer i podpis wybiera kod produkcyjny. */
function typedInTheWindow(command: string): WindowLine {
  const row = echoOf(command);
  if (row === null) {
    throw new Error('the window wrote no row for ' + JSON.stringify(command));
  }
  return row;
}

/**
 * Ten sam wiersz okna pod cudzym podpisem.
 *
 * Zmieniamy WYŁĄCZNIE nazwę autora: numer, rodzaj i chwila zostają takie, jakie wydał moduł
 * okna. O to w przypadku z podpisem chodzi — pochodzenie wiersza ma rozstrzygać samo, bez
 * oglądania się na to, jak nazywa się ten, kto się pod nim podpisał.
 */
function signedBy(row: WindowLine, agent: string): Incoming {
  return { ...row, agent };
}

/** Scena: te wiersze wchodzą do modelu jedynymi drzwiami, jakie okno do niego ma. */
function heard(rows: readonly Incoming[]) {
  const feed = createFeed(sealedScroller());
  feed.appendLines(rows);
  return feed;
}

describe('a row the window wrote stays out of the NOW zone', () => {
  it('adds not one row to the zone for a batch the window wrote by itself', () => {
    const feed = heard([typedInTheWindow('/stop'), saidOf(NOTHING_RUNS)]);

    expect(
      feed.view.now.rows,
      'nothing was started, so nobody is doing anything. A row here is a relation the data ' +
        'does not have (invariant 17), and it stands in one of the two regions allowed to move ' +
        'on screen — the one place a person looks to find out whether anything is alive.',
    ).toEqual([]);
  });

  it('still puts those very rows into the history', () => {
    const typed = typedInTheWindow('/stop');
    const said = saidOf(NOTHING_RUNS);
    const feed = heard([typed, said]);

    expect(
      feed.view.history.map((row) => row.label),
      'the echo of a typed command stays in the stream, word for word. That echo is the whole ' +
        'reason the window writes a row at all: a terminal where what you typed leaves no ' +
        'trace cannot be told apart from one that never took it. A fix that hides these rows ' +
        'passes the emptiness above and breaks the check that put them there.',
    ).toEqual([typed.text, said.text]);
  });

  it('leaves the row of a real agent in the zone exactly as it is today', () => {
    const feed = heard([
      typedInTheWindow('/run ship-it'),
      line.note(1, 1_000, FORGE, FORGE_SAID),
      saidOf(NOTHING_RUNS),
    ]);

    expect(
      feed.view.now.rows,
      'one real sentence, one row, and it belongs to the agent that said it. Letting nothing ' +
        'into the zone passes the emptiness above and deletes a region the screen is built ' +
        'around — that is the quiet failure this case exists to catch.',
    ).toEqual([{ agent: FORGE, text: FORGE_SAID }]);
  });

  it('asks where the row came from, never what name is under it', () => {
    const feed = heard([
      line.note(1, 1_000, FORGE, FORGE_SAID),
      /* PO zdaniu Forge'a, nie przed nim: wiersz strefy TERAZ jest nadpisywany, więc wiersz
         okna postawiony przed prawdziwym zdaniem nie zmieniłby niczego i ten przypadek
         przechodziłby także dla implementacji, która go liczy. */
      signedBy(typedInTheWindow('/stop'), FORGE),
    ]);

    expect(
      feed.view.now.rows,
      'a row the window wrote is signed with the name of a real agent here, and it must not ' +
        'put a single word into the mouth of that agent. A list of forbidden names would be ' +
        'a second table of truth (invariant 13), and the first agent called Loadout would ' +
        'break it in the other direction.',
    ).toEqual([{ agent: FORGE, text: FORGE_SAID }]);
  });

  it('does not put out Thinking… — a real line from an agent does that', () => {
    const feed = createFeed(sealedScroller());

    feed.appendLines([line.thinking(1, 0, FORGE)]);
    expect(feed.view.now.thinking, 'the slot is live before the window says anything').toBe(FORGE);

    feed.appendLines([typedInTheWindow('/stop'), saidOf(NOTHING_RUNS)]);
    expect(
      feed.view.now.thinking,
      'the slot goes quiet when a real line lands, and the echo of your own Enter is not one. ' +
        'Put out here, it would say the agent stopped thinking because you typed a slash.',
    ).toBe(FORGE);

    feed.appendLines([line.note(2, 2_000, FORGE, FORGE_SAID)]);
    expect(
      feed.view.now.thinking,
      'and a real sentence still does put it out — a slot that never goes quiet says nothing',
    ).toBeNull();
  });

  it('is built on a scene that really carries both kinds of row', () => {
    const history = heard([
      typedInTheWindow('/run ship-it'),
      line.note(1, 1_000, FORGE, FORGE_SAID),
      saidOf(NOTHING_RUNS),
    ]).view.history;

    expect(
      history.filter((row) => row.id < 0).length,
      'the rows the window wrote have to reach the story at all. Dropped on the way in, the ' +
        'empty zone above would be the emptiness of an empty scene and would prove nothing.',
    ).toBe(2);
    expect(
      history.filter((row) => row.id > 0).length,
      'and a row stamped by a pump has to be in the same scene, or the two measurements above ' +
        'are one measurement taken twice.',
    ).toBe(1);
  });
});
