/* Wiersz złożony w oknie nie jest agentem: nie ma kroku, nie pracuje, nie ma kafelka.
 *
 * ZMIERZONA WADA. `roster.ts` bije jeden kafelek na każde odrębne `row.agent` w historii,
 * a stan bierze z planu — `statusOf(null, false)` daje `working`. Po T-58 każda komenda
 * wpisana w wiersz wejścia i każda odpowiedź, którą ten wiersz daje sam sobie, składa wiersz
 * podpisany oknem. Skutek widać po pierwszym `/stop`: na liście agentów siada kafelek,
 * którego nikt nie uruchomił, i stoi tam „working" do końca pracy. To jest niezmiennik 17
 * złamany dokładnie tam, gdzie komentarz przy `railCard` się na niego powołuje.
 *
 * NOŚNIK JEST W DANYCH, NIE W NAZWIE. T-58 wymusił ujemny identyfikator dla wiersza składanego
 * w oknie, bo obie pompy stemplują od 1 każda z osobna. „Skład okna" jest więc faktem
 * zapisanym w wierszu, a nie domysłem z podpisu — dlatego przypadek (d) niżej podpisuje wiersz
 * okna nazwą PRAWDZIWEGO agenta. Lista zakazanych nazw byłaby drugą tabelą prawdy
 * (niezmiennik 13) i rozjechałaby się przy pierwszym agencie nazwanym „Loadout".
 *
 * SŁABA WERSJA TEGO KRYTERIUM: samo `toHaveLength(0)` na historii z okna. Przechodzi dla
 * implementacji, która nie rysuje kafelków w ogóle, i — co gorsza — dla tej, która tnie po
 * braku kroku w planie, czyli kasuje pod-agentów. Rozróżniają to przypadki (b) i (c).
 *
 * Wiersze okna składa TU KOD PRODUKCYJNY (`../entry/echo`), a nie literał w teście: ujemny
 * numer jest własnością tamtego modułu i scena, która wpisuje go z palca, przechodziłaby także
 * wtedy, gdyby tamten moduł przestał go nadawać.
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

/** Agent z planu: ma krok, ma rolę, naprawdę nadaje. */
const FORGE = 'Forge';

/** Pod-agent rozpuszczony w trakcie pracy. Żaden plan nie umie go nazwać z góry. */
const SCOUT = 'Scout';

/** Zdanie, które Forge naprawdę wypowiedział — jedyne, jakie ma prawo stać na jego kafelku. */
const FORGE_SAID = 'Rewriting the splitter.';

/**
 * Plan zna wyłącznie Forge'a.
 *
 * Niepusty celowo: lista kafelków zbudowana z planu zamiast ze strumienia przechodzi przypadek
 * (a) tylko wtedy, gdy plan jest pusty — więc pusty plan skasowałby połowę tego kryterium.
 */
const PLAN: readonly AgentFacts[] = [
  { id: FORGE, name: FORGE, role: 'writes code', step: 'running' },
];

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
 * okna. O to w przypadku (d) chodzi — pochodzenie wiersza ma rozstrzygać samo, bez oglądania
 * się na to, jak nazywa się ten, kto się pod nim podpisał.
 */
function signedBy(row: WindowLine, agent: string): Incoming {
  return { ...row, agent };
}

/** Scena: te wiersze wchodzą do historii jedynymi drzwiami, jakie okno do niej ma. */
function heard(rows: readonly Incoming[]): RosterInput {
  const feed = createFeed(sealedScroller());
  feed.appendLines(rows);
  return { view: feed.view, agents: PLAN };
}

/** Sama rozmowa z wierszem wejścia: trzy wiersze złożone w oknie i ani jednego z pompy. */
function windowOnly(): RosterInput {
  return heard([
    typedInTheWindow('/stop'),
    saidOf('Nothing is running.'),
    /* Trzeci podpisany nazwą agenta z planu — „dowolny podpis" jest częścią tego przypadku. */
    signedBy(typedInTheWindow('/open ' + FORGE), FORGE),
  ]);
}

/**
 * To samo plus JEDNA linia prawdziwego agenta.
 *
 * Wiersz okna podpisany Forge'em stoi PO jego zdaniu i to jest cała ostrożność tej sceny:
 * zdanie kafelka wygrywa najnowsze, więc wiersz okna postawiony przed nim nie zmieniłby
 * niczego i przypadek (d) przechodziłby także dla implementacji, która go liczy.
 */
function windowAndOneAgent(): RosterInput {
  return heard([
    typedInTheWindow('/run ship-it'),
    line.note(1, 1_000, FORGE, FORGE_SAID),
    saidOf('Nothing is running.'),
    signedBy(typedInTheWindow('/stop'), FORGE),
  ]);
}

/** Pod-agent: numer z pompy, ani jednego kroku w planie. */
function windowAndOneSubAgent(): RosterInput {
  return heard([
    typedInTheWindow('/stop'),
    line.read(1, 1_000, SCOUT, 'docs/csv-edge-cases.md'),
    saidOf('Nothing is running.'),
  ]);
}

describe('a row the window wrote is not an agent and gets no tile', () => {
  it('has nobody on the list when every row was written by the window', () => {
    expect(
      roster(windowOnly()).length,
      'a story made only of rows the window wrote has nobody in it. Nothing was started, so ' +
        'a tile there is a relation the data does not have (invariant 17) — and it says ' +
        '"working" until you close the app, because the plan has no step to say otherwise.',
    ).toBe(0);
  });

  it('has exactly one tile once one real agent says something, and it is that agent', () => {
    expect(
      roster(windowAndOneAgent()).map((card) => card.id),
      'one real line, one tile, and it belongs to the agent that sent it. The rows the window ' +
        'wrote are still in the same story and they still must not add a name of their own.',
    ).toEqual([FORGE]);
  });

  it('keeps the tile of a sub-agent that has no step in the plan at all', () => {
    const cards = roster(windowAndOneSubAgent());

    expect(
      cards.map((card) => card.id),
      'a sub-agent started in the middle of the work has no step in the plan and never will. ' +
        'A fix that drops every tile without a step passes the count above and deletes the only ' +
        'trace this one leaves — that is the quiet failure this case exists to catch.',
    ).toEqual([SCOUT]);
    expect(
      cards.at(0)?.status,
      'and it still reads "working": it said something and nothing in the story asks it to stop.',
    ).toBe('working');
  });

  it('asks where the row came from, never what name is under it', () => {
    const forge = roster(windowAndOneAgent()).find((card) => card.id === FORGE);

    expect(forge, 'there has to be a tile for Forge before we can ask what it says').toBeDefined();
    expect(
      forge?.say.text,
      'a row the window wrote is signed with the same name as a real agent here, and it must ' +
        'not put a single word into the mouth of that agent. A list of forbidden names would ' +
        'be a second table of truth (invariant 13), and the first agent called Loadout ' +
        'would break it.',
    ).toBe(FORGE_SAID);
    expect(
      forge?.say.who,
      'and the sentence still belongs to the agent, not to Loadout answering a slash.',
    ).toBe('agent');
  });

  it('is built on a scene that really carries both kinds of row', () => {
    const written = windowOnly().view.history;
    expect(
      written.length,
      'the rows the window wrote have to reach the story at all. If they were dropped on the ' +
        'way in, the zero above would be the zero of an empty scene and would prove nothing.',
    ).toBe(3);
    expect(
      written.every((row) => row.id < 0),
      'and every one of them is numbered below zero, which is the whole carrier this fix reads.',
    ).toBe(true);

    const mixed = windowAndOneAgent().view.history;
    expect(
      mixed.some((row) => row.id < 0) && mixed.some((row) => row.id > 0),
      'the mixed scene has to carry both: rows written by the window and rows stamped by ' +
        'a pump. One kind missing turns the two counts above into the same measurement.',
    ).toBe(true);
  });
});
