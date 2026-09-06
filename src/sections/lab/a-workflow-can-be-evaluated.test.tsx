/* WF-19: rzeczywista akcja edytora, bez uruchamiania płatnej pracy podczas otwarcia. */
import { afterAll, expect, it } from 'vitest';
import { closeEverything, openApp } from '../../../e2e/harness';
import type { TauriReply } from '../../../e2e/harness';

const PROJECT = '/work/whole-workflow-lab';
const DOCUMENT = { format: 1, id: 'pipeline', name: 'My pipeline', steps: [], links: [] };
const SET = {
  format: 2,
  id: 'compare-pipeline',
  name: 'My pipeline',
  subject: { kind: 'workflow', id: DOCUMENT.id },
  cases: [],
  variants: [],
};
const BOARD = {
  set: { set: SET, revision: 'saved' },
  runs: [],
  movement: null,
  cannotRun: 'Add a workflow column and accept a case before starting.',
};
const replies = (value: unknown): readonly TauriReply[] =>
  Array.from({ length: 12 }, () => ({ value }));

afterAll(closeEverything, 30_000);

it('Evaluate workflow creates a workflow subject from the actual editor and opens its Lab', async () => {
  const app = await openApp({
    replies: {
      list_workspaces: replies([{ id: PROJECT, folder: PROJECT, name: 'Project' }]),
      list_workflows: replies([
        { kind: 'healthy', value: { path: 'pipeline.json', place: 'project', workflow: DOCUMENT } },
      ]),
      load_workflow: replies({ workflow: DOCUMENT, revision: 'graph-revision' }),
      check_workflow: replies([]),
      create_eval_set: replies({ set: SET, revision: 'saved' }),
      list_eval_sets: replies([SET]),
      read_eval_board: replies(BOARD),
    },
  });
  try {
    await app.page.locator('[data-section-switch="workflows"]').click();
    await app.page.getByRole('button', { name: DOCUMENT.name, exact: false }).first().click();
    const evaluate = app.page.getByRole('button', { name: 'Evaluate workflow', exact: true });
    expect(
      await evaluate.count(),
      'the actual workflow editor has no entry to a workflow comparison',
    ).toBe(1);
    await evaluate.click();
    await expect
      .poll(async () => (await app.calls()).filter((call) => call.cmd === 'create_eval_set'))
      .toHaveLength(1);
    expect((await app.calls()).find((call) => call.cmd === 'create_eval_set')?.args).toMatchObject({
      folder: PROJECT,
      name: DOCUMENT.name,
      subject: { kind: 'workflow', id: DOCUMENT.id },
    });
    await app.page.locator('[data-lab-screen]').waitFor({ state: 'visible' });
    expect((await app.page.locator('[data-lab-screen]').innerText()).toLowerCase()).toContain(
      'workflow',
    );
    expect(
      (await app.calls()).filter((call) =>
        ['run_workflow', 'run_eval_set', 'propose_eval_cases', 'say_to_lead'].includes(call.cmd),
      ),
    ).toHaveLength(0);
  } finally {
    await app.close();
  }
}, 90_000);

const COLUMN = {
  id: 'baseline',
  name: 'Baseline',
  agent: '',
  overrides: {},
  workflow: { id: DOCUMENT.id, outputStep: 'build', overrides: {} },
};
const GRAPH = {
  ...DOCUMENT,
  steps: [
    { id: 'build', name: 'Build', kind: 'agent' },
    { id: 'finish', name: 'Final answer', kind: 'agent' },
  ],
};
const FORM_SET = { ...SET, variants: [COLUMN] };

async function openForms(extra: Readonly<Record<string, readonly TauriReply[]>> = {}) {
  const app = await openApp({
    replies: {
      list_workspaces: replies([{ id: PROJECT, folder: PROJECT, name: 'Project' }]),
      list_workflows: replies([
        {
          kind: 'healthy',
          revision: 'project-revision',
          value: { path: 'pipeline.json', place: 'project', workflow: GRAPH },
        },
        {
          kind: 'healthy',
          revision: 'library-revision',
          value: { path: 'pipeline.json', place: 'library', workflow: GRAPH },
        },
      ]),
      list_eval_sets: replies([FORM_SET]),
      read_eval_board: replies({ ...BOARD, set: { set: FORM_SET, revision: 'set-before-edit' } }),
      put_eval_variant: replies({ set: FORM_SET, revision: 'column-saved' }),
      put_eval_case: replies({ set: FORM_SET, revision: 'case-saved' }),
      ...extra,
    },
  });
  await app.page.locator('[data-section-switch="lab"]').click();
  await app.page.locator('[data-lab-set="compare-pipeline"]').click();
  return app;
}

const READY_SET = {
  ...FORM_SET,
  cases: [
    {
      id: 'ready',
      name: 'Ready case',
      task: 'Do the task',
      expect: [],
      command: 'node test.cjs',
      proof: 'passed: (\\d+)',
      status: 'in-use',
      because: 'Reviewed',
    },
  ],
};
const PREVIEW = {
  set: SET.id,
  revision: 'set-before-edit',
  sourceRevision: 'sources-before-start',
  protected: false,
  size: { cells: 7, nodes: 19, edges: 23, trees: 11 },
  cannotRun: null,
};

it('the actual Lab shows backend preflight counts and sends the reviewed set and source binding with Run', async () => {
  const app = await openForms({
    read_eval_board: replies({
      ...BOARD,
      cannotRun: null,
      set: { set: READY_SET, revision: 'set-before-edit' },
    }),
    preview_eval_run: replies(PREVIEW),
    run_eval_set: replies(null),
  });
  try {
    const preview = app.page.locator('[data-workflow-preview]');
    expect(await preview.count(), 'Run has no shared preflight summary in the actual Lab').toBe(1);
    await expect.poll(() => preview.innerText()).toContain('19 executed steps');
    expect(await preview.innerText()).toContain('7 comparisons');
    expect(await preview.innerText()).toContain('23 connections');
    expect(await preview.innerText()).toContain('11 working folders');
    expect(await app.page.locator('[data-workflow-protection]').innerText()).toContain(
      'Diagnostic comparison',
    );
    expect((await app.calls()).find((call) => call.cmd === 'preview_eval_run')?.args).toEqual({
      folder: PROJECT,
      set: SET.id,
      expectedRevision: 'set-before-edit',
    });
    expect((await app.calls()).filter((call) => call.cmd === 'run_eval_set')).toHaveLength(0);
    await app.page.locator('[data-lab-run]').click();
    await expect
      .poll(async () => (await app.calls()).filter((call) => call.cmd === 'run_eval_set'))
      .toHaveLength(1);
    expect((await app.calls()).find((call) => call.cmd === 'run_eval_set')?.args).toMatchObject({
      folder: PROJECT,
      set: SET.id,
      approval: { revision: PREVIEW.revision, sources: PREVIEW.sourceRevision },
    });
  } finally {
    await app.close();
  }
}, 90_000);

it('an executor preflight refusal is visible once and disables Run without starting a process', async () => {
  const said = 'This agent application cannot restrict file access on this computer.';
  const app = await openForms({
    read_eval_board: replies({
      ...BOARD,
      cannotRun: null,
      set: { set: { ...READY_SET, protected: true }, revision: 'set-before-edit' },
    }),
    preview_eval_run: replies({ ...PREVIEW, protected: true, cannotRun: said }),
  });
  try {
    expect(
      await app.page.getByText(said, { exact: true }).count(),
      'the actual Lab hid the executor refusal',
    ).toBe(1);
    expect(await app.page.locator('[data-lab-run]').isDisabled()).toBe(true);
    expect(
      (await app.calls()).filter((call) => ['run_eval_set', 'run_workflow'].includes(call.cmd)),
    ).toHaveLength(0);
  } finally {
    await app.close();
  }
}, 90_000);

it('Run stays disabled while preview is pending and after a stale preview reply', async () => {
  const app = await openForms({
    read_eval_board: replies({
      ...BOARD,
      cannotRun: null,
      set: { set: READY_SET, revision: 'set-before-edit' },
    }),
    preview_eval_run: [{ deferred: 'comparison-preview' }],
  });
  try {
    await expect
      .poll(async () => (await app.calls()).filter((call) => call.cmd === 'preview_eval_run'))
      .toHaveLength(1);
    expect(await app.page.locator('[data-lab-run]').isDisabled()).toBe(true);
    expect((await app.calls()).filter((call) => call.cmd === 'run_eval_set')).toHaveLength(0);
    await app.settle('comparison-preview', { value: { ...PREVIEW, revision: 'another-revision' } });
    await expect
      .poll(() => app.page.locator('[data-workflow-preview]').innerText())
      .toContain('This preview no longer matches the saved set. Check again before running it.');
    expect(await app.page.locator('[data-lab-run]').isDisabled()).toBe(true);
    expect((await app.calls()).filter((call) => call.cmd === 'run_eval_set')).toHaveLength(0);
  } finally {
    await app.close();
  }
}, 90_000);

it('evaluation scope is saved explicitly without rewriting cases or starting a comparison', async () => {
  const app = await openForms({
    save_eval_protection: replies({
      set: { ...FORM_SET, protected: true },
      revision: 'scope-saved',
    }),
    read_eval_board: [
      { value: { ...BOARD, set: { set: FORM_SET, revision: 'set-before-edit' } } },
      ...replies({
        ...BOARD,
        set: { set: { ...FORM_SET, protected: true }, revision: 'scope-saved' },
      }),
    ],
  });
  try {
    const scope = app.page.locator('[data-workflow-protection]');
    expect(await scope.count(), 'the actual Lab has no control for the evaluation scope').toBe(1);
    expect(await scope.innerText()).toContain('Diagnostic comparison');
    const toggle = scope.getByRole('checkbox', { name: 'Restrict file access', exact: true });
    await toggle.check();
    expect((await app.calls()).filter((call) => call.cmd === 'save_eval_protection')).toHaveLength(
      0,
    );
    await scope.getByRole('button', { name: 'Save evaluation scope', exact: true }).click();
    await expect
      .poll(async () => (await app.calls()).filter((call) => call.cmd === 'save_eval_protection'))
      .toHaveLength(1);
    expect((await app.calls()).find((call) => call.cmd === 'save_eval_protection')?.args).toEqual({
      folder: PROJECT,
      set: SET.id,
      protected: true,
      expectedRevision: 'set-before-edit',
    });
    await expect
      .poll(() => scope.innerText())
      .toContain('File access is restricted; trusted external checks judge the result.');
    expect(await toggle.isChecked()).toBe(true);
    expect(
      await scope.getByRole('button', { name: 'Save evaluation scope', exact: true }).count(),
    ).toBe(0);
    expect(
      (await app.calls()).filter((call) =>
        ['put_eval_case', 'decide_eval_case', 'run_eval_set', 'run_workflow'].includes(call.cmd),
      ),
    ).toHaveLength(0);
  } finally {
    await app.close();
  }
}, 90_000);

it('a refused evaluation scope save keeps the choice without claiming restrictions were saved', async () => {
  const said = 'This set changed on disk. Reopen it before saving.';
  const app = await openForms({ save_eval_protection: [{ error: said }] });
  try {
    const scope = app.page.locator('[data-workflow-protection]');
    const toggle = scope.getByRole('checkbox', { name: 'Restrict file access', exact: true });
    await toggle.check();
    await scope.getByRole('button', { name: 'Save evaluation scope', exact: true }).click();
    await expect.poll(() => app.page.locator('[data-lab-screen]').innerText()).toContain(said);
    expect(await toggle.isChecked()).toBe(true);
    expect(await scope.innerText()).toContain('Diagnostic comparison');
    expect(await scope.innerText()).toContain('Not saved');
    expect(
      (await app.calls()).filter((call) =>
        ['put_eval_case', 'decide_eval_case', 'run_eval_set', 'run_workflow'].includes(call.cmd),
      ),
    ).toHaveLength(0);
  } finally {
    await app.close();
  }
}, 90_000);

it('trusted external code is saved as a candidate, shown for review and accepted only by a separate click', async () => {
  const candidate = {
    id: 'trusted-case',
    name: 'A trusted check',
    task: 'Repair the task',
    expect: [],
    command: '',
    proof: '',
    proofMode: 'external-assessment-v1',
    repeats: 1,
    status: 'suggested',
    because: 'Written by you in Lab',
    examiner: {
      kind: 'python',
      program: '/usr/bin/python3',
      source: 'import json\nprint(json.dumps({"version": 1, "passed": 1}))\n',
    },
  };
  const protectedSet = { ...FORM_SET, protected: true };
  const saved = { ...protectedSet, cases: [candidate] };
  const app = await openForms({
    new_id: replies(candidate.id),
    read_eval_board: [
      { value: { ...BOARD, set: { set: protectedSet, revision: 'set-before-edit' } } },
      ...replies({ ...BOARD, set: { set: saved, revision: 'trusted-saved' } }),
    ],
    put_eval_case: replies({ set: saved, revision: 'trusted-saved' }),
    decide_eval_case: replies({
      set: { ...saved, cases: [{ ...candidate, status: 'in-use' }] },
      revision: 'trusted-accepted',
    }),
  });
  try {
    await app.page.getByRole('button', { name: 'Add case', exact: true }).click();
    const form = app.page.locator('[data-workflow-case="new"]');
    expect(
      await form.getByLabel('Trusted checker code', { exact: true }).count(),
      'protected cases cannot be written in the actual form',
    ).toBe(1);
    await form.getByLabel('Case name', { exact: true }).fill(candidate.name);
    await form.getByLabel('Task', { exact: true }).fill(candidate.task);
    await form.getByLabel('Python interpreter', { exact: true }).fill(candidate.examiner.program);
    await form.getByLabel('Trusted checker code', { exact: true }).fill(candidate.examiner.source);
    await form.getByRole('button', { name: 'Save case', exact: true }).click();
    const waiting = app.page.locator('[data-lab-suggestion="trusted-case"]');
    await waiting.waitFor({ state: 'visible' });
    expect(
      (await app.calls()).find((call) => call.cmd === 'put_eval_case')?.args.case,
    ).toMatchObject(candidate);
    expect(await waiting.innerText()).toContain(candidate.examiner.program);
    expect(await waiting.locator('[data-examiner-source]').textContent()).toBe(
      candidate.examiner.source,
    );
    expect((await app.calls()).filter((call) => call.cmd === 'decide_eval_case')).toHaveLength(0);
    await waiting.getByRole('button', { name: 'Accept', exact: true }).click();
    await expect
      .poll(async () => (await app.calls()).filter((call) => call.cmd === 'decide_eval_case'))
      .toHaveLength(1);
    expect((await app.calls()).find((call) => call.cmd === 'decide_eval_case')?.args).toMatchObject(
      { case: candidate.id, keep: true, expectedRevision: 'trusted-saved' },
    );
    expect(
      (await app.calls()).filter((call) => ['run_eval_set', 'run_workflow'].includes(call.cmd)),
    ).toHaveLength(0);
  } finally {
    await app.close();
  }
}, 90_000);

it('editing accepted trusted code returns it to Waiting for you instead of silently approving new code', async () => {
  const accepted = {
    id: 'accepted-check',
    name: 'Previously accepted',
    task: 'Repair the task',
    expect: [],
    command: '',
    proof: '',
    proofMode: 'external-assessment-v1',
    status: 'in-use',
    because: 'Reviewed earlier',
    examiner: { kind: 'python', program: '/usr/bin/python3', source: 'print("old")' },
  };
  const app = await openForms({
    read_eval_board: replies({
      ...BOARD,
      set: {
        set: { ...FORM_SET, protected: true, cases: [accepted] },
        revision: 'accepted-before-edit',
      },
    }),
  });
  try {
    await app.page
      .locator('[data-lab-workflow-cases]')
      .getByText(accepted.name, { exact: true })
      .click();
    const form = app.page.locator('[data-workflow-case="accepted-check"]');
    const source = form.getByLabel('Trusted checker code', { exact: true });
    expect(
      await source.count(),
      'accepted trusted source cannot be reviewed or edited in the case',
    ).toBe(1);
    await source.fill('print("new code must be reviewed")');
    await form.getByRole('button', { name: 'Save case', exact: true }).click();
    await expect
      .poll(async () => (await app.calls()).filter((call) => call.cmd === 'put_eval_case'))
      .toHaveLength(1);
    expect(
      (await app.calls()).find((call) => call.cmd === 'put_eval_case')?.args.case,
    ).toMatchObject({
      id: accepted.id,
      status: 'suggested',
      examiner: { ...accepted.examiner, source: 'print("new code must be reviewed")' },
    });
    expect(
      (await app.calls()).filter((call) =>
        ['decide_eval_case', 'run_eval_set', 'run_workflow'].includes(call.cmd),
      ),
    ).toHaveLength(0);
  } finally {
    await app.close();
  }
}, 90_000);

it('workflow column saves its exact shelf, file, revision, output and step changes through the real adapter', async () => {
  const app = await openForms();
  try {
    const form = app.page.locator('[data-workflow-column="baseline"]');
    expect(
      await form.count(),
      'the workflow subject still renders the agent model-only editor',
    ).toBe(1);
    await form.getByLabel('Workflow source', { exact: true }).selectOption('library/pipeline.json');
    await form.getByLabel('Output step', { exact: true }).selectOption('finish');
    await form.getByLabel('Column name', { exact: true }).fill('Library candidate');
    await form
      .getByLabel('Step changes (JSON)', { exact: true })
      .fill('{"build":{"model":"chosen-model"}}');
    await form.getByRole('button', { name: 'Save column', exact: true }).click();
    await expect
      .poll(async () => (await app.calls()).filter((call) => call.cmd === 'put_eval_variant'))
      .toHaveLength(1);
    expect((await app.calls()).find((call) => call.cmd === 'put_eval_variant')?.args).toMatchObject(
      {
        folder: PROJECT,
        set: SET.id,
        expectedRevision: 'set-before-edit',
        variant: {
          id: 'baseline',
          name: 'Library candidate',
          workflow: {
            id: DOCUMENT.id,
            place: 'library',
            path: 'pipeline.json',
            revision: 'library-revision',
            outputStep: 'finish',
            overrides: { build: { model: 'chosen-model' } },
          },
        },
      },
    );
    expect(
      (await app.calls()).filter((call) => ['run_eval_set', 'run_workflow'].includes(call.cmd)),
    ).toHaveLength(0);
  } finally {
    await app.close();
  }
}, 90_000);

it('invalid step changes stay in the real form and do not reach the save adapter', async () => {
  const app = await openForms();
  try {
    const form = app.page.locator('[data-workflow-column="baseline"]');
    await form.getByLabel('Workflow source', { exact: true }).selectOption('library/pipeline.json');
    await form.getByLabel('Output step', { exact: true }).selectOption('finish');
    await form.getByLabel('Step changes (JSON)', { exact: true }).fill('[]');
    await form.getByRole('button', { name: 'Save column', exact: true }).click();
    await expect
      .poll(() => form.getByRole('alert').innerText())
      .toContain('Step changes must be an object');
    expect(await form.getByLabel('Step changes (JSON)', { exact: true }).inputValue()).toBe('[]');
    expect((await app.calls()).filter((call) => call.cmd === 'put_eval_variant')).toHaveLength(0);
  } finally {
    await app.close();
  }
}, 90_000);

it('starting files are selected by run title and verified without typing either raw ID', async () => {
  const earlier = {
    folder: '20260906-020000__saved-run-id',
    when: '2026-09-06 02:00',
    title: 'Before the refactor',
    workflowFile: '',
    state: 'succeeded',
    steps: 2,
    costUsd: null,
    said: null,
  };
  const app = await openForms({
    list_runs: replies([earlier]),
    read_run: replies({
      ...earlier,
      steps: [],
      handoffs: [],
      savedInput: { sourceRunId: 'saved-run-id', snapshotId: 'original-input-id' },
      savedInputSaid: null,
    }),
  });
  try {
    await app.page.getByRole('button', { name: 'Add case', exact: true }).click();
    const form = app.page.locator('[data-workflow-case="new"]');
    const picker = form.getByLabel('Starting files', { exact: true });
    expect(
      await picker.count(),
      'starting files still require the person to know internal IDs',
    ).toBe(1);
    await picker.selectOption('saved');
    const beforeSelection = (await app.calls()).filter((call) => call.cmd === 'read_run').length;
    await form.getByLabel('Earlier run', { exact: true }).selectOption(earlier.folder);
    await expect
      .poll(async () => (await app.calls()).filter((call) => call.cmd === 'read_run'))
      .toHaveLength(beforeSelection + 1);
    expect((await app.calls()).filter((call) => call.cmd === 'read_run').at(-1)?.args).toEqual({
      folder: PROJECT,
      run: earlier.folder,
    });
    await form.getByLabel('Case name', { exact: true }).fill('Uses verified starting files');
    await form.getByLabel('Task', { exact: true }).fill('Repeat the task on these files.');
    await form.getByLabel('Check command', { exact: true }).fill('node verify.cjs');
    await form.getByLabel('Passing output', { exact: true }).fill('passed: (\\d+)');
    await form.getByRole('button', { name: 'Save case', exact: true }).click();
    await expect
      .poll(async () => (await app.calls()).filter((call) => call.cmd === 'put_eval_case'))
      .toHaveLength(1);
    expect(
      (await app.calls()).find((call) => call.cmd === 'put_eval_case')?.args.case,
    ).toMatchObject({
      input: { sourceRunId: 'saved-run-id', snapshotId: 'original-input-id' },
      status: 'suggested',
    });
    expect(
      (await app.calls()).filter((call) =>
        ['run_eval_set', 'run_workflow', 'prepare_replay'].includes(call.cmd),
      ),
    ).toHaveLength(0);
  } finally {
    await app.close();
  }
}, 90_000);

it('saving a case reaches Waiting for you and only a separate Accept activates it', async () => {
  const candidate = {
    id: 'case-created',
    name: 'A new case',
    task: 'Do the work',
    expect: [],
    command: 'node check.cjs',
    proof: 'passed: (\\d+)',
    status: 'suggested',
    because: 'Written by you in Lab',
    repeats: 1,
  };
  const saved = { ...FORM_SET, cases: [candidate] };
  const app = await openForms({
    new_id: replies(candidate.id),
    read_eval_board: [
      { value: { ...BOARD, set: { set: FORM_SET, revision: 'set-before-edit' } } },
      ...replies({ ...BOARD, set: { set: saved, revision: 'case-saved' } }),
    ],
    put_eval_case: replies({ set: saved, revision: 'case-saved' }),
    decide_eval_case: replies({
      set: { ...saved, cases: [{ ...candidate, status: 'in-use' }] },
      revision: 'accepted',
    }),
  });
  try {
    await app.page.getByRole('button', { name: 'Add case', exact: true }).click();
    const form = app.page.locator('[data-workflow-case="new"]');
    await form.getByLabel('Case name', { exact: true }).fill(candidate.name);
    await form.getByLabel('Task', { exact: true }).fill(candidate.task);
    await form.getByLabel('Check command', { exact: true }).fill(candidate.command);
    await form.getByLabel('Passing output', { exact: true }).fill(candidate.proof);
    await form.getByRole('button', { name: 'Save case', exact: true }).click();
    const waiting = app.page.locator('[data-lab-suggestion="case-created"]');
    await waiting.waitFor({ state: 'visible' });
    expect((await app.calls()).filter((call) => call.cmd === 'decide_eval_case')).toHaveLength(0);
    await waiting.getByRole('button', { name: 'Accept', exact: true }).click();
    await expect
      .poll(async () => (await app.calls()).filter((call) => call.cmd === 'decide_eval_case'))
      .toHaveLength(1);
    expect((await app.calls()).find((call) => call.cmd === 'decide_eval_case')?.args).toEqual({
      folder: PROJECT,
      set: SET.id,
      case: candidate.id,
      keep: true,
      expectedRevision: 'case-saved',
    });
    expect(
      (await app.calls()).filter((call) => ['run_eval_set', 'run_workflow'].includes(call.cmd)),
    ).toHaveLength(0);
  } finally {
    await app.close();
  }
}, 90_000);

it('an unavailable saved input explains why, blocks Save and never silently selects the current project', async () => {
  const earlier = {
    folder: '20260906-020000__missing-input',
    when: '2026-09-06 02:00',
    title: 'A kept receipt',
    workflowFile: '',
    state: 'succeeded',
    steps: 1,
    costUsd: null,
    said: null,
  };
  const said = 'The saved starting files are missing. Choose another run.';
  const app = await openForms({
    list_runs: replies([earlier]),
    read_run: replies({
      ...earlier,
      steps: [],
      handoffs: [],
      savedInput: null,
      savedInputSaid: said,
    }),
  });
  try {
    await app.page.getByRole('button', { name: 'Add case', exact: true }).click();
    const form = app.page.locator('[data-workflow-case="new"]');
    await form.getByLabel('Case name', { exact: true }).fill('A valid case');
    await form.getByLabel('Task', { exact: true }).fill('Do the work');
    await form.getByLabel('Check command', { exact: true }).fill('node check.cjs');
    await form.getByLabel('Passing output', { exact: true }).fill('passed: (\\d+)');
    await form.getByLabel('Starting files', { exact: true }).selectOption('saved');
    await form.getByLabel('Earlier run', { exact: true }).selectOption(earlier.folder);
    await expect.poll(() => form.getByRole('alert').innerText()).toBe(said);
    expect(await form.getByRole('button', { name: 'Save case', exact: true }).isDisabled()).toBe(
      true,
    );
    expect(await form.getByLabel('Starting files', { exact: true }).inputValue()).toBe('saved');
    expect((await app.calls()).filter((call) => call.cmd === 'put_eval_case')).toHaveLength(0);
    // Zmiana wejścia jest dopiero tym świadomym gestem, nie reakcją na odmowę czytnika.
    await form.getByLabel('Starting files', { exact: true }).selectOption('current');
    await form.getByRole('button', { name: 'Save case', exact: true }).click();
    await expect
      .poll(async () => (await app.calls()).filter((call) => call.cmd === 'put_eval_case'))
      .toHaveLength(1);
    expect(
      (await app.calls()).find((call) => call.cmd === 'put_eval_case')?.args.case,
    ).not.toHaveProperty('input');
  } finally {
    await app.close();
  }
}, 90_000);

it('a case saves starting files, criteria and repeats as a suggestion without accepting or starting it', async () => {
  const app = await openForms();
  try {
    const add = app.page.getByRole('button', { name: 'Add case', exact: true });
    expect(await add.count(), 'the real workflow comparison has no case editor').toBe(1);
    await add.click();
    const form = app.page.locator('[data-workflow-case="new"]');
    await form.getByLabel('Case name', { exact: true }).fill('Starting from an earlier run');
    await form.getByLabel('Task', { exact: true }).fill('Build the feature from these files.');
    await form.getByLabel('Starting files', { exact: true }).selectOption('advanced');
    await form.getByLabel('Source run ID', { exact: true }).fill('saved-source-run');
    await form.getByLabel('Saved input ID', { exact: true }).fill('saved-input-id');
    await form.getByLabel('Repeats', { exact: true }).fill('3');
    await form.getByLabel('Check command', { exact: true }).fill('node check.cjs');
    await form.getByLabel('Passing output', { exact: true }).fill('passed: (\\d+)');
    await form
      .getByLabel('Expected fields (JSON)', { exact: true })
      .fill('[{"field":"summary","contains":"complete","describe":"Summarize the result"}]');
    await form.getByRole('button', { name: 'Save case', exact: true }).click();
    await expect
      .poll(async () => (await app.calls()).filter((call) => call.cmd === 'put_eval_case'))
      .toHaveLength(1);
    expect((await app.calls()).find((call) => call.cmd === 'put_eval_case')?.args).toMatchObject({
      folder: PROJECT,
      set: SET.id,
      expectedRevision: 'set-before-edit',
      case: {
        name: 'Starting from an earlier run',
        task: 'Build the feature from these files.',
        input: { sourceRunId: 'saved-source-run', snapshotId: 'saved-input-id' },
        repeats: 3,
        command: 'node check.cjs',
        proof: 'passed: (\\d+)',
        status: 'suggested',
        expect: [{ field: 'summary', contains: 'complete', describe: 'Summarize the result' }],
      },
    });
    expect(
      (await app.calls()).filter((call) =>
        ['decide_eval_case', 'run_eval_set', 'run_workflow'].includes(call.cmd),
      ),
    ).toHaveLength(0);
  } finally {
    await app.close();
  }
}, 90_000);
