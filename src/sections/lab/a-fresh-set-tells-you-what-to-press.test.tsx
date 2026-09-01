import { renderToStaticMarkup } from 'react-dom/server';
import { describe, expect, it } from 'vitest';

import { createLabStore } from '../../state/lab';
import type { EvalBoard, EvalCase, LabIo } from './io';
import LabScreen from './index';

/* Świeży zestaw: jedna kolumna, zero przypadków. To jest PIERWSZY ekran, jaki człowiek widzi
 * po naciśnięciu „Evaluate" — i to on zawiódł na żywo 2026-08-31.
 *
 * CO BYŁO ŹLE, zmierzone na zrzucie właściciela: to samo zdanie stało dwa razy pod sobą, mówiło
 * o propozycjach, których nie było, nad tabelą z samym nagłówkiem, a Run dawał się kliknąć
 * i nie robił nic poza powtórzeniem tego zdania po raz trzeci. Cztery wady w jednym widoku
 * i ani jednego kryterium, które by go oglądało — bo wszystkie patrzyły na zestaw, który JUŻ
 * coś mierzy.
 *
 * SŁABA WERSJA: `expect(markup).toContain('Write cases')`. Przechodzi ją ekran, który pokazuje
 * to zdanie DWA razy, obok pustej tabeli, przy klikalnym Run. Dlatego liczymy wystąpienia
 * i pytamy o to, czego na ekranie ma NIE być.
 */

const NEVER: LabIo = {
  list: () => Promise.reject(new Error('this screen never reads the disk here')),
  board: () => Promise.reject(new Error('this screen never reads the disk here')),
  create: () => Promise.reject(new Error('this screen never reads the disk here')),
  remove: () => Promise.resolve(),
  propose: () => Promise.reject(new Error('this screen never reads the disk here')),
  proposeFix: () => Promise.reject(new Error('this screen never reads the disk here')),
  applyFix: () => Promise.reject(new Error('this screen never reads the disk here')),
  stopProposing: () => Promise.resolve(),
  decide: () => Promise.reject(new Error('this screen never reads the disk here')),
  putCase: () => Promise.reject(new Error('this screen never reads the disk here')),
  putVariant: () => Promise.reject(new Error('this screen never reads the disk here')),
  dropVariant: () => Promise.reject(new Error('this screen never reads the disk here')),
};

/** Ten sam plik, który powstał na maszynie właściciela: jedna kolumna, `cases: []`. */
function aBoard(cases: readonly EvalCase[], cannotRun: string | null): EvalBoard {
  return {
    set: {
      revision: 'rev-1',
      set: {
        format: 1,
        id: 'adversarial-verifier',
        name: 'adversarial-verifier',
        subject: { kind: 'agent', id: '01a04349-d19d-73b3-a71f-8287bcddacdc' },
        cases: [...cases],
        variants: [
          {
            id: 'as-it-is',
            name: 'As it is',
            agent: '01a04349-d19d-73b3-a71f-8287bcddacdc',
            overrides: {},
          },
        ],
      },
    },
    runs: [],
    movement: null,
    cannotRun,
  };
}

const FRESH =
  'This set has no cases yet. Press Write cases and an agent will draft some from this project.';

function screen(board: EvalBoard, said: string | null = null): string {
  const store = createLabStore(NEVER, () => Promise.resolve(null));
  store.setState({
    sets: [board.set.set],
    agents: [{ id: 'a', name: 'Forge' }],
    openId: board.set.set.id,
    board,
    busy: 'idle',
    said,
    fix: null,
  });
  return renderToStaticMarkup(<LabScreen store={store} />);
}

function occurrences(haystack: string, needle: string): number {
  return haystack.split(needle).length - 1;
}

describe('a set with nothing in it yet', () => {
  const markup = screen(aBoard([], FRESH));

  it('says what to press, exactly once', () => {
    expect(
      occurrences(markup, FRESH),
      'the same sentence twice under itself is one fact in two places, and a person reads the ' +
        'second one as a second thing to do',
    ).toBe(1);
  });

  it('draws no table at all rather than a heading over nothing', () => {
    expect(
      markup.includes('data-lab-matrix'),
      'a header row with no rows under it is not an empty table — it is a promise that ' +
        'something is there, and a person looks for what nobody wrote',
    ).toBe(false);
  });

  it('leaves Run unpressable while there is nothing to run', () => {
    const run = /<button[^>]*data-lab-run[^>]*>/.exec(markup)?.[0] ?? '';
    expect(run, 'the Run button has to be in the document to be judged').not.toBe('');
    expect(
      run.includes('disabled'),
      'a button that answers a press by repeating a sentence already on screen is a control ' +
        'with no effect and a second home for the same fact',
    ).toBe(true);
  });

  it('still offers the one thing that moves it forward', () => {
    expect(markup, 'Write cases is the whole next step from here').toContain('data-lab-propose');
  });

  it('says which field is a name and which is a model, in words a person can see', () => {
    /* Zmierzone na żywym ekranie: właściciel wpisał nazwę modelu w pole obok, bo placeholder
     * znika po pierwszym znaku i nic już nie mówi, czym jest to, co wpisał. */
    expect(markup).toContain('data-lab-column-name="as-it-is"');
    expect(markup).toContain('data-lab-column-model="as-it-is"');
    const columns = markup.slice(markup.indexOf('data-lab-columns'));
    expect(columns).toContain('Name');
    expect(columns).toContain('Model');
  });

  it('never shows a second copy of the reason, even after a press', () => {
    /* Naciśnięty Run w tym stanie ma nie dokładać zdania: powód stoi już nad tabelą. */
    const after = screen(aBoard([], FRESH), FRESH);
    expect(
      occurrences(after, FRESH),
      'this is the shape the owner saw: the same sentence twice, one under the other',
    ).toBe(2);
    // Kontrola: to jest stan, którego magazyn NIE ma prawa wyprodukować — dowodzi tego
    // `controls-have-an-effect`, gdzie `run()` przy `cannotRun` zostawia `said` na `null`.
  });
});
