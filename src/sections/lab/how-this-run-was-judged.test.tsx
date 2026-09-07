import { renderToStaticMarkup } from 'react-dom/server';
import { describe, expect, it } from 'vitest';

import { createLabStore } from '../../state/lab';
import type { EvalBoard, EvalSet, LabIo, PastEval } from './io';
import LabScreen from './index';

/* Poziom zaufania stoi PRZY WYNIKU, i jest własnością pomiaru, nie dzisiejszego formularza.
 *
 * Zaznaczenie „Restrict file access" i zapisanie zakresu zmieniało jedyne zdanie o ochronie na
 * ekranie — a zdanie to stało nad macierzą zmierzoną na odwrotnych warunkach. Człowiek czytał
 * więc „File access is restricted…" nad wynikiem policzonym bez żadnej granicy, i odwrotnie.
 * „Przeszło" pod pilnowanym dostępem do plików i „przeszło" bez niego to nie jest ten sam fakt,
 * a fakt o zmierzonym biegu jechał na drucie (`PastEval.definition`) i nie miał czytelnika.
 *
 * SŁABA WERSJA: asercja, że gdzieś na ekranie pada słowo o ochronie. Przechodzi ją ekran
 * sprzed naprawy — panel je drukuje. Dlatego oba renderowania mają IDENTYCZNY dzisiejszy
 * zestaw i różnią się wyłącznie zmierzonym biegiem: zdanie ma iść za biegiem.
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

const CASE = {
  id: 'case',
  name: 'Parse data',
  task: 'Parse the file',
  expect: [],
  command: 'trusted-check',
  proof: '',
  status: 'in-use' as const,
  because: '',
};

/** Dzisiejszy zapis: ochrona WŁĄCZONA. Ten sam w obu renderowaniach. */
const SAVED: EvalSet = {
  format: 2,
  id: 'compare-pipeline',
  name: 'My pipeline',
  subject: { kind: 'workflow', id: 'pipeline' },
  cases: [CASE],
  variants: [{ id: 'column', name: 'Two agents', agent: '', overrides: {} }],
  protected: true,
};

function runMeasured(protectedFiles: boolean): PastEval {
  return {
    folder: '20260906-101112__abc',
    when: '2026-09-06 10:11',
    state: 'succeeded',
    passed: 1,
    judged: 1,
    costUsd: 0.5,
    definition: { ...SAVED, protected: protectedFiles },
    cells: [{ case: 'case', variant: 'column', outcome: 'passed', said: '', costUsd: 0.5 }],
  };
}

function screen(today: EvalSet, run: PastEval): string {
  const board: EvalBoard = {
    set: { revision: 'saved', set: today },
    runs: [run],
    movement: null,
    cannotRun: null,
  };
  const store = createLabStore(NEVER, () => Promise.resolve(null));
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

describe('the conditions a result was measured under', () => {
  it('follows the run that was measured, not the scope saved afterwards', () => {
    const afterLoosening = screen(SAVED, runMeasured(false));
    const afterTightening = screen(SAVED, runMeasured(true));

    expect(
      afterLoosening,
      'the saved scope says file access is restricted, but this result was measured without ' +
        'that boundary, and a person reading the marks has no other way to learn it',
    ).toContain('This run was measured as a diagnostic comparison');
    expect(afterLoosening.includes('This run was measured with file access restricted')).toBe(
      false,
    );
    expect(afterTightening).toContain('This run was measured with file access restricted');
    expect(afterTightening.includes('This run was measured as a diagnostic comparison')).toBe(
      false,
    );
  });

  it('leaves the saved scope answering to the form, so both facts keep a carrier', () => {
    const run = runMeasured(false);
    const panelOf = (today: EvalSet): string => {
      const markup = screen(today, run);
      return markup.slice(markup.indexOf('data-workflow-protection'));
    };

    expect(
      panelOf(SAVED),
      'the panel is the control for the next run, so answering it from the measured run would ' +
        'take away the only place the saved scope is readable',
    ).toContain('File access is restricted; trusted external checks judge the result.');
    expect(panelOf({ ...SAVED, protected: false })).toContain('Diagnostic comparison');
  });

  it('says nothing about restrictions where a set has no such choice', () => {
    const forAnAgent: EvalSet = {
      format: 1,
      id: 'review-rubric',
      name: 'Review rubric',
      subject: { kind: 'agent', id: 'a' },
      cases: [CASE],
      variants: [{ id: 'column', name: 'Without', agent: 'a', overrides: {} }],
    };
    const run = { ...runMeasured(false), definition: forAnAgent };

    expect(
      screen(forAnAgent, run).includes('This run was measured'),
      'an agent set has no evaluation scope to speak of, and a sentence about one would be a ' +
        'fact nobody measured',
    ).toBe(false);
  });
});
