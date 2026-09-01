import { renderToStaticMarkup } from 'react-dom/server';
import { describe, expect, it } from 'vitest';

import { createLabStore } from '../../state/lab';
import type { EvalBoard, EvalCase, LabIo } from './io';
import LabScreen from './index';

/* Co CZŁOWIEK czyta w tabeli — nie co oddaje funkcja (niezmiennik 29).
 *
 * Wynik komórki liczy Rust i ma na to własne kryterium
 * (`a_cell_needs_three_things_to_pass.rs`). To tutaj pyta o drugą połowę tej samej rzeczy:
 * czy policzony wynik ma DROGĘ NA EKRAN i czy zdanie „dlaczego nie przeszło" da się przeczytać
 * bez otwierania transkryptu. Między jednym a drugim mieszka klasa wady, dla której to repo
 * powstało: kryterium zielone, funkcja martwa.
 *
 * SŁABA WERSJA: asercja, że markup zawiera `✓`. Przechodzi ją ekran, który rysuje znaki
 * w losowych miejscach — i przechodzi na tabeli, w której kandydatka stoi jako wiersz, czyli
 * mierzy coś, czego człowiek nigdy nie zaakceptował. Dlatego liczymy znaki, pytamy o wiersze
 * po identyfikatorze i sprawdzamy, gdzie kandydatka NIE stoi.
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

function aCase(id: string, name: string, status: EvalCase['status']): EvalCase {
  return {
    id,
    name,
    task: 'say which file resolves the tenant',
    expect: [],
    command: '',
    proof: '',
    status,
    because: 'src/guard.ts:14',
  };
}

const BOARD: EvalBoard = {
  set: {
    revision: 'eyJmb3JtYXQiOiAxfQo=',
    set: {
      format: 1,
      id: 'review-rubric',
      name: 'Review rubric',
      subject: { kind: 'agent', id: '0198a1f2-3b4c-7d5e-8f60-112233445566' },
      cases: [
        aCase('one', 'Reads the guard', 'in-use'),
        aCase('two', 'Names the file', 'in-use'),
        aCase('draft', 'Still a draft', 'suggested'),
      ],
      variants: [
        { id: 'without', name: 'Without', agent: 'a', overrides: {} },
        { id: 'with', name: 'With', agent: 'a', overrides: {} },
      ],
    },
  },
  runs: [
    {
      folder: '20260831-091412__abc',
      when: '2026-08-31 09:14',
      state: 'succeeded',
      passed: 3,
      judged: 4,
      costUsd: 1.25,
      cells: [
        { case: 'one', variant: 'without', outcome: 'passed', said: '', costUsd: 0.3 },
        { case: 'one', variant: 'with', outcome: 'passed', said: '', costUsd: 0.3 },
        {
          case: 'two',
          variant: 'without',
          outcome: 'did-not-pass',
          said: '"file" came back as "src/router.ts", and this case asked it to mention "guard".',
          costUsd: 0.35,
        },
        { case: 'two', variant: 'with', outcome: 'passed', said: '', costUsd: 0.3 },
      ],
    },
  ],
  movement: { gained: 1, lost: 0 },
  cannotRun: null,
};

function screen(board: EvalBoard): string {
  const store = createLabStore(NEVER, () => Promise.resolve(null));
  store.setState({
    sets: [board.set.set],
    agents: [{ id: 'a', name: 'Forge' }],
    openId: board.set.set.id,
    board,
    busy: 'idle',
    said: null,
  });
  return renderToStaticMarkup(<LabScreen store={store} />);
}

function occurrences(haystack: string, needle: string): number {
  return haystack.split(needle).length - 1;
}

describe('the table a person reads', () => {
  const markup = screen(BOARD);

  it('draws one row per accepted case and one column per variant', () => {
    expect(occurrences(markup, 'data-lab-row="one"')).toBe(1);
    expect(occurrences(markup, 'data-lab-row="two"')).toBe(1);
    expect(occurrences(markup, 'data-lab-column="without"')).toBe(1);
    expect(occurrences(markup, 'data-lab-column="with"')).toBe(1);
    expect(
      occurrences(markup, 'data-lab-cell='),
      'two accepted rows across two columns is four places a person can look at',
    ).toBe(4);
  });

  it('never puts a case still waiting for a person into the table', () => {
    expect(
      occurrences(markup, 'data-lab-row="draft"'),
      'a suggestion standing as a row means the set measures something nobody accepted, and the ' +
        'number over the table then counts it',
    ).toBe(0);
  });

  it('says which cell did not pass, in the cell and in words below it', () => {
    expect(occurrences(markup, 'data-lab-cell="did-not-pass"')).toBe(1);
    expect(occurrences(markup, 'data-lab-cell="passed"')).toBe(3);
    expect(markup).toContain('data-lab-failures');
    expect(
      markup,
      'the reason has to be readable on this screen; a mark on its own sends a person to the ' +
        'transcript for a sentence the run already wrote',
    ).toContain('and this case asked it to mention');
    expect(
      markup,
      'the failing cell has to be named by the row and the column a person sees, not by ids',
    ).toContain('Names the file · Without');
  });

  it('puts the score, the movement and the spend where the work is', () => {
    expect(markup).toContain('3 of 4 passed');
    expect(markup, 'a person asks whether their change helped, and this is the answer').toContain(
      '+1 since the run before',
    );
    expect(markup).toContain('$1.25');
  });

  it('offers Accept and Discard for the one case still waiting', () => {
    expect(markup).toContain('data-lab-waiting');
    expect(occurrences(markup, 'data-lab-suggestion=')).toBe(1);
    expect(markup).toContain('data-lab-keep="draft"');
    expect(markup).toContain('data-lab-discard="draft"');
    expect(
      markup,
      'a person judges a suggestion by where it came from; without that the only honest answer ' +
        'is to accept all of them or none',
    ).toContain('src/guard.ts:14');
  });

  it('shows no leftover of a value nobody has', () => {
    const words = markup.replace(/<[^>]*>/g, ' ');
    for (const leftover of ['undefined', 'null', 'NaN', '$0.00']) {
      expect(
        words.includes(leftover),
        'the table shows ' +
          leftover +
          ' to a person. A cell with no price simply has none; a zero standing in for one is a ' +
          'number that was never measured.',
      ).toBe(false);
    }
  });

  it('says what is missing instead of offering a run that cannot start', () => {
    const nothing = screen({
      ...BOARD,
      runs: [],
      movement: null,
      cannotRun: 'This set has no columns yet, so there is nothing to compare.',
    });
    expect(nothing).toContain('This set has no columns yet');
    expect(
      nothing,
      'with no run behind it the header may not show a score it does not have',
    ).toContain('Not run yet');
  });
});
