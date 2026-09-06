import { renderToStaticMarkup } from 'react-dom/server';
import { afterAll, expect, it } from 'vitest';
import { closeEverything, openApp } from '../../../e2e/harness';
import type { TauriReply } from '../../../e2e/harness';

import type { EvalSet, PastEval } from './io';
import { Matrix } from './matrix';
import { howManyCells, tableFor } from './model';

const set: EvalSet = {
  format: 2,
  id: 'comparison',
  name: 'Comparison',
  subject: { kind: 'workflow', id: 'flow' },
  cases: [
    {
      id: 'case',
      name: 'Parse data',
      task: 'Build a parser',
      status: 'in-use',
      expect: [],
      command: 'trusted-check',
      proof: 'unused',
      because: '',
      repeats: 2,
    },
  ],
  variants: [{ id: 'variant', name: 'Two agents', agent: '', overrides: {} }],
};
const run: PastEval = {
  folder: 'recorded-run',
  when: '2026-09-06',
  state: 'failed',
  passed: 1,
  judged: 1,
  costUsd: 0.25,
  definition: set,
  cells: [
    {
      case: 'case',
      variant: 'variant',
      outcome: 'passed',
      said: '',
      costUsd: 0.25,
      execution: {
        repeat: 0,
        nodes: ['first-builder', 'first-reviewer'],
        output: 'first-reviewer',
        grader: 'first-examiner',
        elapsedMs: 1500,
        costPartial: true,
      },
    },
    {
      case: 'case',
      variant: 'variant',
      outcome: 'not-judged',
      said: 'The assessment could not start.',
      costUsd: null,
      execution: {
        repeat: 1,
        nodes: ['second-builder'],
        output: 'second-builder',
        grader: 'second-examiner',
        elapsedMs: null,
        costPartial: true,
      },
    },
  ],
};

it('shows each recorded repetition without replacing the first result with the last', () => {
  const markup = renderToStaticMarkup(<Matrix table={tableFor(set, run)} />);
  expect(markup.match(/data-lab-cell=/g)).toHaveLength(2);
  expect(markup).toContain('Repeat 1');
  expect(markup).toContain('Repeat 2');
  expect(markup).toContain('data-lab-cell="passed"');
  expect(markup).toContain('data-lab-cell="not-judged"');
  expect(howManyCells(set)).toBe(2);
});

it('lets the person read the reason, elapsed time and partial cost, without guessing missing prices', () => {
  const markup = renderToStaticMarkup(<Matrix table={tableFor(set, run)} />);
  expect(markup).toContain('The assessment could not start.');
  expect(markup).toContain('1.5 s');
  expect(markup).toContain('At least $0.25');
  expect(markup).toContain('Some agents did not report a price.');
  expect(markup).not.toContain('$0.00');
});

afterAll(closeEverything, 30_000);

it('opens the real recorded run from a measured cell, even when its workflow is no longer listed', async () => {
  const workspace = '/work/measured-project';
  const reply = (value: unknown): readonly TauriReply[] =>
    Array.from({ length: 12 }, () => ({ value }));
  const app = await openApp({
    replies: {
      list_workspaces: reply([{ id: workspace, folder: workspace, name: 'Measured project' }]),
      list_eval_sets: reply([set]),
      read_eval_board: reply({
        set: { set, revision: 'saved' },
        runs: [{ ...run, workspace }],
        movement: null,
        cannotRun: null,
      }),
      read_run: reply({
        folder: run.folder,
        title: 'Original measured workflow',
        when: '2026-09-06',
        workflowFile: '',
        state: 'failed',
        steps: [],
        handoffs: [],
        branches: [],
        said: null,
      }),
      list_workflows: reply([]),
    },
  });
  try {
    await app.page.locator('[data-section-switch="lab"]').click();
    await app.page.getByRole('button', { name: set.name, exact: true }).click();
    const cell = app.page.locator('[data-lab-cell="passed"]').first();
    await cell.locator('summary').click();
    const open = cell.getByRole('button', { name: 'Open measured run', exact: true });
    expect(await open.count(), 'a measured cell has no route to its saved execution').toBe(1);
    await open.click();
    await expect
      .poll(async () => (await app.calls()).filter((call) => call.cmd === 'read_run'))
      .toHaveLength(1);
    expect((await app.calls()).find((call) => call.cmd === 'read_run')?.args).toEqual({
      folder: workspace,
      run: run.folder,
    });
    await app.page
      .getByText('Original measured workflow', { exact: true })
      .waitFor({ state: 'visible' });
    expect(
      (await app.calls()).some((call) =>
        ['run_workflow', 'run_eval_set', 'load_workflow'].includes(call.cmd),
      ),
    ).toBe(false);
  } finally {
    await app.close();
  }
}, 90_000);
