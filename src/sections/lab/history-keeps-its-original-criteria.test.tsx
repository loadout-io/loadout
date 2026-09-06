import { renderToStaticMarkup } from 'react-dom/server';
import { expect, it } from 'vitest';

import type { EvalSet, PastEval } from './io';
import { Matrix } from './matrix';
import { tableFor } from './model';
import { trendOf } from './model';
import { Trend } from './trend';

const original: EvalSet = {
  format: 1,
  id: 'saved',
  name: 'Saved criteria',
  subject: { kind: 'agent', id: 'agent' },
  cases: [
    {
      id: 'case',
      name: 'Original case',
      task: 'Original task',
      status: 'in-use',
      expect: [{ field: 'Answer', contains: 'original answer', describe: '' }],
      command: 'original-check',
      proof: 'original-proof',
      because: '',
    },
  ],
  variants: [{ id: 'variant', name: 'Original variant', agent: 'agent', overrides: {} }],
};

it('renders the measured task, command and column, even after today’s form changes', () => {
  const current: EvalSet = {
    ...original,
    cases: [
      {
        ...original.cases[0]!,
        name: 'Changed case',
        task: 'Changed task',
        command: 'changed-check',
        proof: 'changed-proof',
      },
    ],
    variants: [{ ...original.variants[0]!, name: 'Changed variant' }],
  };
  const run = {
    folder: 'saved',
    when: '2026-09-05',
    state: 'succeeded',
    passed: 1,
    judged: 1,
    costUsd: null,
    definition: original,
    cells: [{ case: 'case', variant: 'variant', outcome: 'passed', said: '', costUsd: null }],
  } as PastEval;
  const markup = renderToStaticMarkup(<Matrix table={tableFor(current, run)} />);
  expect(markup).toContain('Original task');
  expect(markup).toContain('original-check');
  expect(markup).toContain('Original variant');
  expect(markup).not.toContain('Changed task');
  expect(markup).not.toContain('changed-check');
});

it('does not draw a performance trend across different measurement conditions', () => {
  const base = {
    folder: 'saved',
    when: '2026-09-05',
    state: 'succeeded',
    passed: 1,
    judged: 1,
    costUsd: null,
    cells: [],
    definition: original,
  };
  const runs = [
    { ...base, folder: 'new', comparisonFingerprint: 'new-input' },
    { ...base, folder: 'old', comparisonFingerprint: 'old-input' },
  ] as readonly PastEval[];
  expect(renderToStaticMarkup(<Trend shares={trendOf(runs)} />)).not.toContain('data-lab-trend');
});
