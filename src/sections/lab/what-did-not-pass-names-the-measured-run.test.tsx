import { renderToStaticMarkup } from 'react-dom/server';
import { describe, expect, it } from 'vitest';

import { createLabStore } from '../../state/lab';
import type { EvalBoard, EvalCell, EvalSet, LabIo, PastEval } from './io';
import LabScreen from './index';

/* Lista pod tabelą mówi o TYM SAMYM pomiarze, co tabela — i tym samym językiem.
 *
 * Wynik historyczny nazywa się nazwami z chwili pomiaru; tabela robi to od 2026-09-05, lista
 * „What did not pass" miała własną kopię tej reguły i czytała dzisiejszy formularz. Ta sama
 * komórka miała więc na jednym ekranie dwa podpisy: w tabeli zmierzony, trzy centymetry niżej
 * dzisiejszy — a po usunięciu kolumny z zestawu lista schodziła do identyfikatora z drutu,
 * czyli do napisu, którego człowiek nigdy nie napisał ani nie widział.
 *
 * Kopia nie znała też powtórzeń, które porównanie workflow wprowadziło: dwie nieudane próby
 * tego samego przypadku w tej samej kolumnie dostawały jeden podpis dwa razy, a z takiej listy
 * nie da się dojść, które powtórzenie padło.
 *
 * SŁABA WERSJA: asercja nad całym markupem. Ekran RYSUJE dzisiejsze nazwy w miejscach, w
 * których to jest prawdą — w polach kolumny i w edytorze przypadków — więc pytanie o cały
 * dokument mierzyłoby coś innego. Kryterium czyta wyłącznie sekcję listy.
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
  saveProtection: () => Promise.reject(new Error('the screen under test never reads the disk')),
  previewRun: () => Promise.reject(new Error('the screen under test never reads the disk')),
  putVariant: () => Promise.reject(new Error('the screen under test never reads the disk')),
  dropVariant: () => Promise.reject(new Error('the screen under test never reads the disk')),
};

/** Zestaw taki, jaki był w chwili pomiaru: dwa powtórzenia jednego przypadku w jednej kolumnie. */
const MEASURED: EvalSet = {
  format: 2,
  id: 'compare-pipeline',
  name: 'My pipeline',
  subject: { kind: 'workflow', id: 'pipeline' },
  cases: [
    {
      id: 'case-7',
      name: 'Original case',
      task: 'Parse the file',
      expect: [],
      command: 'trusted-check',
      proof: '',
      status: 'in-use',
      because: '',
      repeats: 2,
    },
  ],
  variants: [{ id: 'column-2', name: 'Original column', agent: '', overrides: {} }],
};

/** Ten sam zestaw po pomiarze: człowiek przemianował i przypadek, i kolumnę. */
const RENAMED: EvalSet = {
  ...MEASURED,
  cases: [{ ...MEASURED.cases[0]!, name: 'Renamed case' }],
  variants: [{ ...MEASURED.variants[0]!, name: 'Renamed column' }],
};

function tried(repeat: number, said: string): EvalCell {
  return {
    case: 'case-7',
    variant: 'column-2',
    outcome: 'did-not-pass',
    said,
    costUsd: 0.25,
    execution: {
      repeat,
      nodes: [],
      output: '',
      grader: '',
      elapsedMs: null,
      costPartial: false,
    },
  };
}

const RUN: PastEval = {
  folder: '20260906-101112__abc',
  when: '2026-09-06 10:11',
  state: 'succeeded',
  passed: 0,
  judged: 2,
  costUsd: 0.5,
  definition: MEASURED,
  cells: [
    tried(0, 'The answer stopped on the first row.'),
    tried(1, 'The answer came back empty.'),
  ],
};

function boardOf(today: EvalSet): EvalBoard {
  return {
    set: { revision: 'saved', set: today },
    runs: [RUN],
    movement: null,
    cannotRun: null,
  };
}

function screen(today: EvalSet): string {
  const store = createLabStore(NEVER, () => Promise.resolve(null));
  const board = boardOf(today);
  store.setState({
    sets: [today],
    agents: [{ id: 'a', name: 'Forge' }],
    openId: today.id,
    board,
    busy: 'idle',
    said: null,
  });
  return renderToStaticMarkup(<LabScreen store={store} />);
}

/** Sama lista porażek. Wszystko nad nią wolno nazywać dzisiejszym formularzem — i tak jest. */
function failures(markup: string): string {
  const at = markup.indexOf('data-lab-failures');
  expect(at, 'the screen drew no list of what did not pass').toBeGreaterThan(-1);
  return markup.slice(at);
}

function occurrences(haystack: string, needle: string): number {
  return haystack.split(needle).length - 1;
}

describe('the list of what did not pass', () => {
  it('names every failing cell by what it was called when it was measured', () => {
    const list = failures(screen(RENAMED));

    expect(
      list,
      'the table above says Original column, so a list saying Renamed column under the same ' +
        'result gives one measured cell two names on one screen',
    ).toContain('Original column');
    expect(occurrences(list, 'Renamed case')).toBe(0);
    expect(occurrences(list, 'Renamed column')).toBe(0);
  });

  it('tells two failed repeats of one case apart, the way the table does', () => {
    const list = failures(screen(RENAMED));

    expect(occurrences(list, 'Original case · Repeat 1 · Original column')).toBe(1);
    expect(
      occurrences(list, 'Original case · Repeat 2 · Original column'),
      'both repeats failed for their own reason; one name twice leaves no way to tell which ' +
        'of them a sentence belongs to',
    ).toBe(1);
  });

  it('keeps a column removed from the set readable instead of falling back to a wire id', () => {
    const list = failures(screen({ ...RENAMED, cases: [], variants: [] }));

    expect(
      list,
      'the button that removes a column promises past results stay readable, and this is where ' +
        'they stop being readable',
    ).toContain('Original case · Repeat 1 · Original column');
    expect(occurrences(list, 'column-2')).toBe(0);
    expect(occurrences(list, 'case-7')).toBe(0);
  });
});
