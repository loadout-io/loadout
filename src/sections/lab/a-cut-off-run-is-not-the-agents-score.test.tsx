import { renderToStaticMarkup } from 'react-dom/server';
import { describe, expect, it } from 'vitest';

import { createLabStore } from '../../state/lab';
import type { CellOutcome, EvalBoard, EvalCell, LabIo, PastEval } from './io';
import LabScreen from './index';

/* „0 of 3 passed" NAD SZEŚCIOMA WIERSZAMI — liczba prawdziwa i bezużyteczna.
 *
 * ── CO ZOBACZYŁ WŁAŚCICIEL, odtworzone z jego zrzutu co do liczby ──────────────────────────
 *
 * Sześć wierszy, jedna kolumna. Trzy `✗`, trzy `·`. Nad tabelą stało `0 of 3 passed`, a pod nią
 * trzy zdania w „What did not pass", które NIE POCHODZĄ OD AGENTA:
 *
 *     „Loadout closed while this step was still running, so the step was cut off with it."
 *
 * To zdanie pisze `commands::reconcile`, kiedy aplikacja ginie w połowie biegu. Krok dostaje
 * wtedy `failed`, a `failed` nie stoi na liście stanów nieosądzonych w `lab::results`, więc
 * **Loadout policzył własne zamknięcie jako trzy porażki agenta i wystawił za to 0%.**
 * Rozkład 3/3 też nie jest przypadkiem: pula ma trzy miejsca naraz, więc dokładnie tyle komórek
 * pracowało w chwili zamknięcia, a trzy nigdy nie ruszyły.
 *
 * ── CZEGO TA WYROCZNIA PILNUJE, A CZEGO NIE ────────────────────────────────────────────────
 *
 * NIE liczy wyniku po raz drugi. Kto przeszedł, a kto nie, rozstrzyga Rust i ma na to własne
 * kryterium; drugie liczenie w oknie rozjechałoby się po pierwszej zmianie tamtej strony.
 * Pyta o dwie rzeczy, które w danych JUŻ SĄ i których ekran nie czytał ani razu:
 *
 *   `run.state`   słowo o CAŁYM przebiegu. `interrupted` wpisuje odzyskiwanie po awarii
 *                 aplikacji. Pole jest na drucie od początku (`io.ts`, `PastEval.state`)
 *                 i do dziś nie miało w oknie ANI JEDNEGO czytelnika.
 *   `cells.length − judged`   ile komórek NIKT nie zmierzył. Arytmetyka na odpowiedzi Rusta,
 *                 nie druga odpowiedź: `judged` jest tym, co Rust policzył.
 *
 * SŁABA WERSJA: `expect(markup).toContain('interrupted')`. Przechodzi ją ekran, który wypisuje
 * słowo z drutu (niezmiennik 14 zabrania), i przechodzi nad liczbą, która dalej kłamie.
 */

const NEVER: LabIo = {
  list: () => Promise.reject(new Error('the screen under test never reads the disk')),
  board: () => Promise.reject(new Error('the screen under test never reads the disk')),
  create: () => Promise.reject(new Error('the screen under test never reads the disk')),
  remove: () => Promise.reject(new Error('the screen under test never reads the disk')),
  propose: () => Promise.reject(new Error('the screen under test never reads the disk')),
  proposeFix: () => Promise.reject(new Error('the screen under test never reads the disk')),
  applyFix: () => Promise.reject(new Error('the screen under test never reads the disk')),
  stopProposing: () => Promise.resolve(),
  decide: () => Promise.reject(new Error('the screen under test never reads the disk')),
  putCase: () => Promise.reject(new Error('the screen under test never reads the disk')),
  putVariant: () => Promise.reject(new Error('the screen under test never reads the disk')),
  dropVariant: () => Promise.reject(new Error('the screen under test never reads the disk')),
};

/** Zdanie, które o tej komórce napisał `commands::reconcile`, a nie agent. */
const CUT_OFF =
  'Loadout closed while this step was still running, so the step was cut off with it.';

function cell(id: string, outcome: CellOutcome, said: string): EvalCell {
  return { case: id, variant: 'as-it-is', outcome, said, costUsd: null };
}

/** Sześć wierszy, jedna kolumna, trzy zmierzone — dokładnie zrzut właściciela. */
function aRun(state: string): PastEval {
  return {
    folder: '20260831-091412__abc',
    when: '2026-08-31 09:14',
    state,
    passed: 0,
    judged: 3,
    costUsd: null,
    cells: [
      cell('one', 'did-not-pass', CUT_OFF),
      cell('two', 'did-not-pass', CUT_OFF),
      cell('three', 'did-not-pass', CUT_OFF),
      cell('four', 'not-judged', 'This never started.'),
      cell('five', 'not-judged', 'This never started.'),
      cell('six', 'not-judged', 'This never started.'),
    ],
  };
}

const NAMES = ['one', 'two', 'three', 'four', 'five', 'six'] as const;

function aBoard(state: string): EvalBoard {
  return {
    set: {
      revision: 'rev-1',
      set: {
        format: 1,
        id: 'adversarial-verifier',
        name: 'adversarial-verifier',
        subject: { kind: 'agent', id: 'a' },
        cases: NAMES.map((id, at) => ({
          id,
          name: 'Case ' + String(at + 1),
          task: 'do the thing',
          expect: [],
          command: '',
          proof: '',
          status: 'in-use' as const,
          because: 'src/one.ts:1',
        })),
        variants: [{ id: 'as-it-is', name: 'As it is', agent: 'a', overrides: {} }],
      },
    },
    runs: [aRun(state)],
    movement: null,
    cannotRun: null,
  };
}

function screen(board: EvalBoard): string {
  const store = createLabStore(NEVER, () => Promise.resolve(null));
  store.setState({
    sets: [board.set.set],
    agents: [{ id: 'a', name: 'Forge' }],
    openId: board.set.set.id,
    board,
    busy: 'idle',
    said: null,
    fix: null,
  });
  return renderToStaticMarkup(<LabScreen store={store} />);
}

function words(markup: string): string {
  return markup
    .replace(/<[^>]*>/g, ' ')
    .replace(/\s+/g, ' ')
    .trim();
}

/** Treść oznaczonego elementu, wycięta po głębokości — nie leniwym wzorcem. */
function region(markup: string, marker: string): string {
  const open = new RegExp('<([a-z]+)[^>]*\\s' + marker + '\\b[^>]*>');
  const hit = open.exec(markup);
  if (hit === null) return '';
  const name = hit[1] ?? '';
  const from = hit.index + hit[0].length;
  const walk = new RegExp('<(/?)' + name + '\\b[^>]*>', 'g');
  walk.lastIndex = from;
  let depth = 1;
  let step = walk.exec(markup);
  while (step !== null) {
    depth += step[1] === '/' ? -1 : 1;
    if (depth === 0) return markup.slice(from, step.index);
    step = walk.exec(markup);
  }
  return markup.slice(from);
}

describe('a run that Loadout cut off by closing', () => {
  const markup = screen(aBoard('interrupted'));
  const said = words(markup);

  it('says the run never finished, in a place that speaks about the run', () => {
    /* PYTAMY O WŁASNE MIEJSCE, nie o cały dokument, i to jest cała ostrożność tego punktu.
     * `contains('Loadout closed')` nad całym ekranem przechodziło NA STARYM KODZIE — bo to
     * zdanie stało już pod tabelą, jako powód JEDNEJ komórki, powtórzony przy każdej z trzech.
     * Fakt o całym przebiegu, wyczytany z trzech zdań o trzech komórkach, jest faktem, który
     * człowiek musi sobie złożyć sam; a przy jednej komórce nie złoży go wcale. */
    expect(
      words(region(markup, 'data-lab-ending')),
      'the screen shows a score of nought over six rows and never once says, as a fact about the ' +
        'whole run, that it was cut off. Every reading of that number is wrong, and the likeliest ' +
        'one blames the agent for the application closing.',
    ).toContain('Loadout closed');
    expect(
      said.includes('interrupted'),
      'the word off the wire reached the screen. Translating it is the window’s job.',
    ).toBe(false);
  });

  it('counts the cells nobody measured, beside the ones that were', () => {
    expect(
      said,
      'the header says "0 of 3 passed" over a table of six rows, with not one word about the ' +
        'three that nobody measured. The three is read as the size of what is on screen, and ' +
        'there is no legend anywhere saying otherwise.',
    ).toContain('3 not measured');
  });

  it('offers the next move, not only the bad news', () => {
    /* W SWOIM MIEJSCU, nie w dokumencie. Zdanie „Run the set again" stoi takze pod tabela,
     * jako zaproszenie do drugiego przebiegu — a warunek postawiony na calym ekranie bral je
     * za nastepny ruch po przebiegu ucietym i przechodzil nad odmowa, ktora go nie mowi.
     * Zmierzone mutacja: skasowanie zdania z `howItEnded` NIE zapalalo tego punktu. */
    expect(
      words(region(markup, 'data-lab-ending')),
      'a refusal that names no next move leaves a person holding a score they cannot act on',
    ).toMatch(/Run (?:it |the set )?again|again to measure|Press Run/i);
  });
});

describe('a run a person stopped on purpose', () => {
  it('says who stopped it, because that is a different fact', () => {
    /* Zawezone do wlasnego miejsca z tego samego powodu, co punkt wyzej: zdanie o zamknieciu
     * aplikacji stoi takze w powodach komorek, wiec pytanie o caly dokument odpowiadaloby
     * o czym innym niz o przebiegu. */
    const ending = words(region(screen(aBoard('cancelled')), 'data-lab-ending'));
    expect(
      ending,
      'a run cut off by the application and a run stopped by a person leave the same numbers ' +
        'behind and mean opposite things',
    ).toMatch(/you stopped|stopped this run|stopped it/i);
    expect(ending.includes('Loadout closed'), 'and they may not share one sentence').toBe(false);
  });
});

describe('a run that finished on its own', () => {
  it('says nothing about how it ended, because there is nothing to say', () => {
    const markup = screen(aBoard('succeeded'));
    const said = words(markup);
    expect(
      words(region(markup, 'data-lab-ending')).includes('Loadout closed'),
      'a finished run wears the sentence written for a cut-off one',
    ).toBe(false);
    expect(
      said,
      'the cells nobody measured are still worth counting: a set that grew after the run has ' +
        'rows that run never saw',
    ).toContain('3 not measured');
  });
});
